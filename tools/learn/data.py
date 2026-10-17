"""Read self-play shards written by the `selfplay` example (mtg-app/examples/selfplay.rs).

A run directory holds `schema.json` and, per thread, `NN-positions.bin` and `NN-tokens.bin`.
`load(run)` returns one dict of numpy arrays for the whole run, with each shard's token
offsets rebased so `tokens[start:start+count]` are a position's visible objects.
"""
import glob
import json
import os

import numpy as np

TYPES = {"f16": "<f2", "f32": "<f4", "u32": "<u4", "u64": "<u8"}


def dtype(layout):
    return np.dtype([(n, TYPES[t], (k,)) if k > 1 else (n, TYPES[t]) for n, t, k in layout])


def load(run):
    schema = json.load(open(os.path.join(run, "schema.json")))
    pos_t, tok_t = dtype(schema["positions"]), dtype(schema["tokens"])
    positions, tokens, base = [], [], 0
    for path in sorted(glob.glob(os.path.join(run, "*-positions.bin"))):
        p = np.fromfile(path, dtype=pos_t)
        t = np.fromfile(path.replace("positions", "tokens"), dtype=tok_t)
        p["token_start"] += base
        base += len(t)
        positions.append(p)
        tokens.append(t)
    return schema, np.concatenate(positions), np.concatenate(tokens)


if __name__ == "__main__":
    import sys

    schema, p, t = load(sys.argv[1])
    assert (p["token_start"] + p["token_count"]).max() <= len(t), "token offsets out of range"
    print(f"schema v{schema['schema_version']}: {len(p)} positions, {len(t)} tokens, "
          f"{len(np.unique(p['game']))} games")
    print(f"outcome mean {p['outcome'].mean():.3f}, tokens/position {p['token_count'].mean():.1f}, "
          f"turn mean {p['turn'].mean():.1f}")
    print("slot counts", np.bincount(t["slot"], minlength=schema["slots"]))
    print("distinct card names", len(np.unique(t["name"])))
    g = p["global"]
    for i, name in enumerate(schema["global"][:22]):
        print(f"  {name:<22} mean {g[:, i].mean():6.3f}  max {g[:, i].max():6.3f}")
