"""Train the value network: position (from one seat) -> probability that seat wins.

    ~/.local/share/mtgo_rs/venv/bin/python tools/learn/train_value.py RUN_DIR [--epochs N] [--out FILE]

Architecture (mirrored exactly by the Rust evaluator, so keep them in step):

    token  = [features (NC) | one-hot slot (SLOTS) | name embedding (EMBED)]
    h      = relu(W2 relu(W1 token + b1) + b2)                 per visible object, width H
    pooled = sum of h per slot  -> SLOTS * H                    order-free, any board size
    x      = [global (NG) | pooled / 4]
    value  = sigmoid(w4 . relu(W3 x + b3) + b4)

Games are split by id (every tenth held out), so no game appears in both halves. The report
compares held-out log loss with a coin flip and with logistic regression on the global features
alone; only the arena decides whether the net is adopted.
"""
import argparse
import json
import os
import struct
import sys
import time

import numpy as np
import torch
import torch.nn as nn

sys.path.insert(0, os.path.dirname(__file__))
import data  # noqa: E402

NAME_BUCKETS = 4096
EMBED = 16
H = 64
HIDDEN = 128


class Value(nn.Module):
    def __init__(self, ng, nc, slots):
        super().__init__()
        self.slots = slots
        self.names = nn.Embedding(NAME_BUCKETS, EMBED)
        self.t1 = nn.Linear(nc + slots + EMBED, HIDDEN)
        self.t2 = nn.Linear(HIDDEN, H)
        self.v1 = nn.Linear(ng + slots * H, HIDDEN)
        self.v2 = nn.Linear(HIDDEN, 1)

    def forward(self, glob, feats, slot, name, owner, n):
        token = torch.cat(
            [feats, nn.functional.one_hot(slot, self.slots).float(), self.names(name % NAME_BUCKETS)], 1
        )
        h = torch.relu(self.t2(torch.relu(self.t1(token))))
        pooled = torch.zeros(n * self.slots, H)
        pooled.index_add_(0, owner * self.slots + slot, h)
        x = torch.cat([glob, pooled.view(n, -1) / 4.0], 1)
        return self.v2(torch.relu(self.v1(x))).squeeze(1)


def batch(p, t, idx):
    """Tensors for positions `idx`: globals, their tokens, and each token's position in the batch."""
    starts = p["token_start"][idx].astype(np.int64)
    counts = p["token_count"][idx].astype(np.int64)
    owner = np.repeat(np.arange(len(idx)), counts)
    offsets = np.arange(counts.sum()) - np.repeat(np.cumsum(counts) - counts, counts)
    rows = np.repeat(starts, counts) + offsets
    tok = t[rows]
    return (
        torch.from_numpy(p["global"][idx].astype(np.float32)),
        torch.from_numpy(tok["features"].astype(np.float32)),
        torch.from_numpy(tok["slot"].astype(np.int64)),
        torch.from_numpy((tok["name"] % NAME_BUCKETS).astype(np.int64)),
        torch.from_numpy(owner),
        len(idx),
    ), torch.from_numpy(p["outcome"][idx].astype(np.float32))


def log_loss(prob, y):
    prob = np.clip(prob, 1e-6, 1 - 1e-6)
    return float(-(y * np.log(prob) + (1 - y) * np.log(1 - prob)).mean())


def evaluate(model, p, t, idx, size=4096):
    model.eval()
    out = []
    with torch.no_grad():
        for k in range(0, len(idx), size):
            x, _ = batch(p, t, idx[k : k + size])
            out.append(torch.sigmoid(model(*x)).numpy())
    model.train()
    return np.concatenate(out)


def global_logistic(p, train, test):
    """Baseline: logistic regression on the global features alone."""
    x = torch.from_numpy(p["global"].astype(np.float32))
    y = torch.from_numpy(p["outcome"].astype(np.float32))
    lin = nn.Linear(x.shape[1], 1)
    opt = torch.optim.LBFGS(lin.parameters(), max_iter=200)

    def closure():
        opt.zero_grad()
        loss = nn.functional.binary_cross_entropy_with_logits(lin(x[train]).squeeze(1), y[train])
        loss = loss + 1e-4 * (lin.weight**2).sum()
        loss.backward()
        return loss

    opt.step(closure)
    with torch.no_grad():
        return torch.sigmoid(lin(x[test]).squeeze(1)).numpy()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("run")
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--batch", type=int, default=512)
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    torch.manual_seed(0)
    schema, p, t = data.load(args.run)
    # Draws carry no signal for "who wins"; keep decided positions only.
    p = p[p["outcome"] != 0.5]
    test_mask = p["game"] % 10 == 0
    train, test = np.flatnonzero(~test_mask), np.flatnonzero(test_mask)
    y_test = p["outcome"][test]
    print(f"{len(train)} training / {len(test)} held-out positions, "
          f"{len(np.unique(p['game']))} games, {len(t)} tokens")
    print(f"coin flip          {log_loss(np.full(len(test), 0.5), y_test):.4f}")
    base = global_logistic(p, train, test)
    print(f"global logistic    {log_loss(base, y_test):.4f}  "
          f"accuracy {((base > 0.5) == (y_test > 0.5)).mean():.3f}")

    model = Value(schema["ng"], schema["nc"], schema["slots"])
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    steps = args.epochs * (len(train) // args.batch)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.lr, total_steps=max(steps, 1))
    rng = np.random.default_rng(0)
    best = (float("inf"), None)
    for epoch in range(args.epochs):
        started = time.time()
        order = rng.permutation(train)
        for k in range(0, len(order) - args.batch + 1, args.batch):
            x, y = batch(p, t, order[k : k + args.batch])
            loss = nn.functional.binary_cross_entropy_with_logits(model(*x), y)
            opt.zero_grad()
            loss.backward()
            opt.step()
            sched.step()
        prob = evaluate(model, p, t, test)
        ll = log_loss(prob, y_test)
        acc = ((prob > 0.5) == (y_test > 0.5)).mean()
        print(f"epoch {epoch + 1}: held-out log loss {ll:.4f}, accuracy {acc:.3f} ({time.time() - started:.0f}s)")
        if ll < best[0]:
            best = (ll, {k: v.detach().clone() for k, v in model.state_dict().items()})

    model.load_state_dict(best[1])
    out = args.out or os.path.join(args.run, "value.bin")
    export(model, schema, out)
    json.dump({"held_out_log_loss": best[0], "positions": len(p)}, open(out + ".json", "w"))
    # Reference outputs for `value_check`, which runs the Rust forward pass on the same rows.
    rows = test[: min(16, len(test))]
    cases = []
    for row, value in zip(rows, evaluate(model, p, t, rows)):
        start, count = int(p["token_start"][row]), int(p["token_count"][row])
        tok = t[start : start + count]
        cases.append({
            "global": p["global"][row].tolist(),
            "tokens": [[int(x["slot"]), int(x["name"]), x["features"].astype(np.float32).tolist()] for x in tok],
            "value": float(value),
        })
    json.dump(cases, open(out + ".check.json", "w"))
    print(f"best held-out log loss {best[0]:.4f} -> {out}")


def export(model, schema, path):
    """`value.bin` for crates/mtg-policy/src/search/value.rs: a header, then f32 arrays."""
    sd = {k: v.detach().numpy().astype("<f4") for k, v in model.state_dict().items()}
    header = struct.pack(
        "<4s9I", b"MTGV", 1, schema["schema_version"], schema["ng"], schema["nc"],
        schema["slots"], NAME_BUCKETS, EMBED, HIDDEN, H,
    )
    order = ["names.weight", "t1.weight", "t1.bias", "t2.weight", "t2.bias",
             "v1.weight", "v1.bias", "v2.weight", "v2.bias"]
    with open(path, "wb") as f:
        f.write(header)
        for k in order:
            f.write(sd[k].tobytes())  # row-major: [out][in], as the Rust side reads


if __name__ == "__main__":
    main()
