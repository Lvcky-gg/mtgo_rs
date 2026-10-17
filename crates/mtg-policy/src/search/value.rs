//! The learned evaluation: a small network over [`encode`](super::encode)d positions.
//!
//! Trained by `tools/learn/train_value.py` on self-play recorded by the `selfplay` example, and
//! read here from the `value.bin` that script writes. The forward pass mirrors the script's
//! `Value` module exactly:
//!
//! ```text
//! token  = [features | one-hot slot | name embedding]
//! h      = relu(T2 relu(T1 token + t1) + t2)            per visible object
//! pooled = sum of h per slot                            order-free, any board size
//! value  = sigmoid(v2 · relu(V1 [global | pooled / 4] + v1) + b)
//! ```
//!
//! Plain loops over `f32`, no dependencies: the network is small enough that a playout's
//! single leaf evaluation costs far less than the playout.

use super::encode::{Encoded, NC, NG, SCHEMA_VERSION, SLOTS};

const MAGIC: &[u8; 4] = b"MTGV";

/// A trained value network.
#[derive(Clone, Debug)]
pub struct ValueNet {
    name_buckets: usize,
    embed: usize,
    hidden: usize,
    width: usize,
    names: Vec<f32>,
    /// The first token layer stored by input column, `[input][hidden]`.
    t1_cols: Vec<f32>,
    t1_b: Vec<f32>,
    /// The second token layer by input column, `[hidden][width]`.
    t2_cols: Vec<f32>,
    t2_b: Vec<f32>,
    v1_w: Vec<f32>,
    v1_b: Vec<f32>,
    v2_w: Vec<f32>,
    v2_b: f32,
}

impl ValueNet {
    /// Read a `value.bin`. Refuses a file written for another encoding schema.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.get(..4) != Some(MAGIC) {
            return Err("not a value network file".into());
        }
        let mut at = 4;
        let mut word = || -> Result<u32, String> {
            let w = bytes
                .get(at..at + 4)
                .ok_or("value network file is truncated")?;
            at += 4;
            Ok(u32::from_le_bytes(w.try_into().unwrap()))
        };
        let (format, schema) = (word()?, word()?);
        let dims: Vec<usize> = (0..7)
            .map(|_| word().map(|w| w as usize))
            .collect::<Result<_, _>>()?;
        let [ng, nc, slots, name_buckets, embed, hidden, width] = dims[..] else {
            unreachable!()
        };
        if format != 1 || schema != SCHEMA_VERSION || ng != NG || nc != NC || slots != SLOTS {
            return Err(format!(
                "value network is for encoding schema {schema} ({ng} global, {nc} card \
                 features); this build encodes schema {SCHEMA_VERSION} ({NG}, {NC})"
            ));
        }
        let mut floats = bytes[at..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c));
        let mut take = |n: usize| -> Result<Vec<f32>, String> {
            let v: Vec<f32> = floats.by_ref().take(n).collect();
            if v.len() == n {
                Ok(v)
            } else {
                Err("value network file is truncated".into())
            }
        };
        let token_in = nc + slots + embed;
        let value_in = ng + slots * width;
        Ok(Self {
            name_buckets,
            embed,
            hidden,
            width,
            names: take(name_buckets * embed)?,
            t1_cols: transpose(&take(hidden * token_in)?, hidden, token_in),
            t1_b: take(hidden)?,
            t2_cols: transpose(&take(width * hidden)?, width, hidden),
            t2_b: take(width)?,
            v1_w: take(hidden * value_in)?,
            v1_b: take(hidden)?,
            v2_w: take(hidden)?,
            v2_b: take(1)?[0],
        })
    }

    /// The probability that the player `e` was encoded for wins.
    pub fn evaluate(&self, e: &Encoded) -> f64 {
        let mut pooled = vec![0f32; SLOTS * self.width];
        let mut a = vec![0f32; self.hidden];
        let mut h = vec![0f32; self.width];
        for t in &e.tokens {
            let slot = t.slot as usize;
            let name =
                &self.names[(t.name as usize % self.name_buckets) * self.embed..][..self.embed];
            // Column by column: most features are zero and are skipped, the one-hot slot is a
            // single column, and each step is a vector add over the hidden width.
            let column = |i: usize| &self.t1_cols[i * self.hidden..][..self.hidden];
            a.copy_from_slice(&self.t1_b);
            add(&mut a, column(NC + slot), 1.0);
            for (i, f) in t.features.iter().enumerate() {
                if *f != 0.0 {
                    add(&mut a, column(i), *f);
                }
            }
            for (j, v) in name.iter().enumerate() {
                add(&mut a, column(NC + SLOTS + j), *v);
            }
            for v in &mut a {
                *v = v.max(0.0);
            }
            // Same for the second layer: about half of `a` is zero after the ReLU.
            h.copy_from_slice(&self.t2_b);
            for (i, v) in a.iter().enumerate() {
                if *v != 0.0 {
                    add(&mut h, &self.t2_cols[i * self.width..][..self.width], *v);
                }
            }
            let out = &mut pooled[slot * self.width..][..self.width];
            for (p, v) in out.iter_mut().zip(&h) {
                *p += v.max(0.0);
            }
        }
        let value_in = NG + SLOTS * self.width;
        let mut x = Vec::with_capacity(value_in);
        x.extend_from_slice(&e.global);
        x.extend(pooled.iter().map(|p| p / 4.0));
        let mut logit = self.v2_b;
        for o in 0..self.hidden {
            let w = &self.v1_w[o * value_in..][..value_in];
            logit += self.v2_w[o] * (self.v1_b[o] + dot(w, &x)).max(0.0);
        }
        1.0 / (1.0 + (-f64::from(logit)).exp())
    }
}

/// A row-major `[rows][cols]` matrix as `[cols][rows]`.
fn transpose(m: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut t = vec![0f32; m.len()];
    for r in 0..rows {
        for c in 0..cols {
            t[c * rows + r] = m[r * cols + c];
        }
    }
    t
}

fn add(acc: &mut [f32], column: &[f32], scale: f32) {
    for (a, c) in acc.iter_mut().zip(column) {
        *a += scale * c;
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(schema: u32) -> Vec<u8> {
        let (buckets, embed, hidden, width) = (4usize, 2usize, 3usize, 2usize);
        let mut b = MAGIC.to_vec();
        for w in [1, schema, NG as u32, NC as u32, SLOTS as u32, 4, 2, 3, 2] {
            b.extend(w.to_le_bytes());
        }
        let floats = buckets * embed
            + hidden * (NC + SLOTS + embed)
            + hidden
            + width * hidden
            + width
            + hidden * (NG + SLOTS * width)
            + hidden
            + hidden
            + 1;
        b.extend(std::iter::repeat_n(0u8, floats * 4));
        b
    }

    #[test]
    fn a_zero_network_is_a_coin_flip_and_other_schemas_are_refused() {
        let net = ValueNet::from_bytes(&file(SCHEMA_VERSION)).unwrap();
        let e = Encoded {
            global: [1.0; NG],
            tokens: Vec::new(),
        };
        assert_eq!(net.evaluate(&e), 0.5);
        assert!(ValueNet::from_bytes(&file(SCHEMA_VERSION + 1)).is_err());
        let mut short = file(SCHEMA_VERSION);
        short.truncate(short.len() - 4);
        assert!(ValueNet::from_bytes(&short).is_err());
    }
}
