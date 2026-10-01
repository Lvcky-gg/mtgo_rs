//! Making a peer-to-peer shuffle trustworthy.
//!
//! One peer has to be authoritative, because hidden information has to live
//! somewhere. That peer knows both libraries — which means, absent something like
//! this module, the host can look at their opponent's deck order and can rig their
//! own shuffle. Among friends this may not matter; designing as if it does not
//! matter is still the wrong call, because "you have to trust whoever pressed
//! host" is a bad property to bake into the format.
//!
//! # Commit, then reveal
//!
//! Before the first shuffle:
//!
//! 1. Each player picks a random 32-byte seed and a decklist, and sends
//!    `H(seed ‖ decklist ‖ salt)` — a commitment. Neither the seed nor the list
//!    is disclosed.
//! 2. Once both commitments are in, each player reveals only their **seed**.
//! 3. The shuffle RNG for both libraries is seeded with `seed_a ⊕ seed_b`.
//!
//! Neither player controls the result, because neither knew the other's seed when
//! committing. The host cannot rig the shuffle even though the host performs it.
//!
//! # Why the decklist stays committed until the end
//!
//! Anyone who knows a seed *and* a decklist can derive that library's order. So
//! the decklist is committed at the start and revealed only when the game ends.
//! Until then the host holds your library order but cannot have chosen it, and you
//! hold a proof of what your deck was.
//!
//! # Verification after the fact
//!
//! At game end both players reveal decklists and salts. Either side can then
//! check the commitments, re-derive both shuffles, replay the event log through
//! the engine, and confirm the final state matches. A host that peeked leaves no
//! trace — peeking is passive and unprovable — but a host that *acted* on it, by
//! drawing a card it should not have or shuffling to a stacked order, produces a
//! log that does not replay. That is the honest boundary of what this achieves,
//! and it is worth stating rather than implying more.
//!
//! Full mental poker, where no peer ever holds the other's library, is possible
//! and is deliberately out of scope: it needs a commutative-encryption shuffle
//! protocol and a round trip per hidden card, which is a poor trade for a game
//! played among people who know each other.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain separation, so a commitment made here can never be confused with a hash this
/// program computes for any other purpose.
const COMMIT_DOMAIN: &[u8] = b"mtgors-shuffle-commit-v1";
const STREAM_DOMAIN: &[u8] = b"mtgors-shuffle-stream-v1";

/// Fill a buffer with operating-system randomness.
pub fn fill_random(buf: &mut [u8]) {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom")
        .expect("no /dev/urandom: cannot generate a seed safely");
    f.read_exact(buf).expect("could not read entropy");
}

/// Published before any seed is revealed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Commitment(pub [u8; 32]);

impl Commitment {
    /// Commit to a seed, a salt, and a decklist.
    ///
    /// The salt matters: a decklist is low-entropy and guessable, so a commitment over the
    /// seed and list alone would be brute-forceable once the seed is revealed. The salt
    /// makes the commitment hiding rather than merely binding.
    ///
    /// The decklist is length-prefixed and its entries are encoded fixed-width, so two
    /// different lists cannot produce the same preimage.
    pub fn of(seed: &Seed, salt: &[u8; 16], decklist: &[(u32, u8)]) -> Self {
        let mut h = Sha256::new();
        h.update(COMMIT_DOMAIN);
        h.update(seed.0);
        h.update(salt);
        h.update((decklist.len() as u32).to_le_bytes());
        for (oracle, count) in decklist {
            h.update(oracle.to_le_bytes());
            h.update([*count]);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.finalize());
        Commitment(out)
    }
}

/// Held privately until the commit phase ends.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seed(pub [u8; 32]);

impl core::fmt::Debug for Seed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never log a seed: before reveal it is the whole secret.
        f.write_str("Seed(<redacted>)")
    }
}

impl Seed {
    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        fill_random(&mut bytes);
        Seed(bytes)
    }

    /// Combine both players' seeds.
    ///
    /// XOR is sufficient for the threat model: the result is unpredictable to either party
    /// as long as *one* seed was honestly random, and each player distrusts the other rather
    /// than distrusting themselves. Neither can steer the outcome, because neither knew the
    /// other's seed when committing.
    pub fn combine(a: Seed, b: Seed) -> Seed {
        let mut out = [0u8; 32];
        for (o, (x, y)) in out.iter_mut().zip(a.0.iter().zip(b.0.iter())) {
            *o = x ^ y;
        }
        Seed(out)
    }
}

/// What a player reveals at game end so the other can verify.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Disclosure {
    pub seed: Seed,
    pub salt: [u8; 16],
    /// Oracle ids with counts, in the canonical order used for the commitment.
    pub decklist: Vec<(u32, u8)>,
}

impl Disclosure {
    /// Whether this disclosure matches what was committed to.
    pub fn matches(&self, commitment: &Commitment) -> bool {
        Commitment::of(&self.seed, &self.salt, &self.decklist) == *commitment
    }
}

/// Outcome of checking a game after it ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Commitments match and the log replays to the recorded final state.
    Consistent,
    /// A revealed seed or decklist does not match what was committed.
    CommitmentMismatch,
    /// The log does not replay — some event could not have happened.
    ReplayDiverged,
}

/// A deterministic permutation of `n` items from a seed.
///
/// Fisher–Yates driven by a SHA-256 counter stream. Both peers derive the same order from
/// the same combined seed, which is what makes the shuffle verifiable — and it is why the
/// engine takes a seed in its command rather than owning an RNG.
///
/// A stream cipher would be the conventional choice; SHA-256 in counter mode is used to
/// avoid another dependency, and is adequate here because the requirement is
/// *unpredictability to the other player*, not cryptographic stream quality.
pub fn shuffle_order(seed: &Seed, n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    if n < 2 {
        return order;
    }

    let mut stream = ByteStream::new(seed);
    // Standard Fisher–Yates, from the top down.
    for i in (1..n).rev() {
        let j = stream.below(i as u64 + 1) as usize;
        order.swap(i, j);
    }
    order
}

/// An endless byte stream from a seed: `SHA-256(domain ‖ seed ‖ counter)`, concatenated.
struct ByteStream {
    seed: [u8; 32],
    counter: u64,
    block: [u8; 32],
    used: usize,
}

impl ByteStream {
    fn new(seed: &Seed) -> Self {
        Self {
            seed: seed.0,
            counter: 0,
            block: [0u8; 32],
            used: 32,
        }
    }

    fn next_byte(&mut self) -> u8 {
        if self.used >= 32 {
            let mut h = Sha256::new();
            h.update(STREAM_DOMAIN);
            h.update(self.seed);
            h.update(self.counter.to_le_bytes());
            self.block.copy_from_slice(&h.finalize());
            self.counter += 1;
            self.used = 0;
        }
        let b = self.block[self.used];
        self.used += 1;
        b
    }

    /// A uniform value below `bound`, by rejection sampling.
    ///
    /// Rejection rather than modulo: modulo would bias low indices, which in a shuffle
    /// means a detectably non-uniform deck order.
    fn below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound > 0);
        let zone = u64::MAX - (u64::MAX % bound) - 1;
        loop {
            let mut buf = [0u8; 8];
            for b in buf.iter_mut() {
                *b = self.next_byte();
            }
            let v = u64::from_le_bytes(buf);
            if v <= zone {
                return v % bound;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decklist() -> Vec<(u32, u8)> {
        vec![(1, 4), (2, 20), (3, 2)]
    }

    #[test]
    fn a_disclosure_matches_its_own_commitment() {
        let seed = Seed::random();
        let salt = [9u8; 16];
        let c = Commitment::of(&seed, &salt, &decklist());
        let d = Disclosure {
            seed,
            salt,
            decklist: decklist(),
        };
        assert!(d.matches(&c));
    }

    #[test]
    fn changing_the_seed_breaks_the_commitment() {
        let salt = [9u8; 16];
        let c = Commitment::of(&Seed::random(), &salt, &decklist());
        let d = Disclosure {
            seed: Seed::random(),
            salt,
            decklist: decklist(),
        };
        assert!(
            !d.matches(&c),
            "a different seed must not satisfy the commitment"
        );
    }

    #[test]
    fn changing_the_decklist_breaks_the_commitment() {
        // This is the property that stops a player swapping decks after seeing the shuffle.
        let seed = Seed::random();
        let salt = [9u8; 16];
        let c = Commitment::of(&seed, &salt, &decklist());

        let mut other = decklist();
        other[0].1 = 3;
        let d = Disclosure {
            seed,
            salt,
            decklist: other,
        };
        assert!(!d.matches(&c));
    }

    #[test]
    fn reordering_the_decklist_breaks_the_commitment() {
        // The encoding is order-sensitive, so both sides must agree on canonical order.
        let seed = Seed::random();
        let salt = [9u8; 16];
        let c = Commitment::of(&seed, &salt, &decklist());

        let mut other = decklist();
        other.reverse();
        assert!(
            !Disclosure {
                seed,
                salt,
                decklist: other
            }
            .matches(&c)
        );
    }

    #[test]
    fn two_lists_cannot_collide_through_ambiguous_encoding() {
        // Without length prefixes and fixed widths, (1,2),(3,4) and (1,2,3,4) as different
        // groupings could hash the same.
        let seed = Seed([0u8; 32]);
        let salt = [0u8; 16];
        let a = Commitment::of(&seed, &salt, &[(0x0102, 3), (0x0405, 6)]);
        let b = Commitment::of(&seed, &salt, &[(0x0102, 3), (0x0405, 6), (0, 0)]);
        assert_ne!(a, b);
    }

    #[test]
    fn combining_is_symmetric_and_neither_side_controls_it() {
        let a = Seed::random();
        let b = Seed::random();
        assert_eq!(Seed::combine(a, b), Seed::combine(b, a));
        assert_ne!(Seed::combine(a, b), a, "the result is not either input");
        assert_ne!(Seed::combine(a, b), b);
    }

    #[test]
    fn combining_with_a_known_seed_still_depends_on_the_other() {
        // The guarantee: even if one player picks their seed adversarially — say all
        // zeroes — the outcome is the other player's seed, which they could not predict.
        let honest = Seed::random();
        let adversarial = Seed([0u8; 32]);
        assert_eq!(Seed::combine(honest, adversarial), honest);
    }

    #[test]
    fn two_seeds_produce_different_shuffles() {
        let a = shuffle_order(&Seed::random(), 60);
        let b = shuffle_order(&Seed::random(), 60);
        assert_ne!(a, b);
    }

    #[test]
    fn the_same_seed_produces_the_same_shuffle() {
        // Both peers must derive the same order, or the game desynchronises.
        let seed = Seed::random();
        assert_eq!(shuffle_order(&seed, 60), shuffle_order(&seed, 60));
    }

    #[test]
    fn a_shuffle_is_a_permutation() {
        let order = shuffle_order(&Seed::random(), 60);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            (0..60).collect::<Vec<_>>(),
            "every index appears exactly once"
        );
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        assert!(shuffle_order(&Seed::random(), 0).is_empty());
        assert_eq!(shuffle_order(&Seed::random(), 1), vec![0]);
    }

    #[test]
    fn a_shuffle_actually_moves_things() {
        // A "shuffle" that returned the identity would pass the permutation test.
        let order = shuffle_order(&Seed::random(), 60);
        let fixed = order.iter().enumerate().filter(|(i, v)| i == *v).count();
        assert!(
            fixed < 20,
            "{fixed} of 60 cards did not move; that is not a shuffle"
        );
    }

    #[test]
    fn the_shuffle_is_roughly_uniform() {
        // A crude check that rejection sampling did its job: over many shuffles, each card
        // should reach the top sometimes. A modulo bias would skew this badly.
        let mut tops = std::collections::BTreeMap::new();
        for _ in 0..2000 {
            let order = shuffle_order(&Seed::random(), 10);
            *tops.entry(order[0]).or_insert(0u32) += 1;
        }
        assert_eq!(
            tops.len(),
            10,
            "every position should reach the top at least once"
        );
        let min = *tops.values().min().unwrap();
        let max = *tops.values().max().unwrap();
        assert!(max < min * 3, "distribution looked skewed: {tops:?}");
    }

    #[test]
    fn a_seed_is_not_printed_in_debug_output() {
        let seed = Seed([0xAB; 32]);
        assert_eq!(format!("{seed:?}"), "Seed(<redacted>)");
    }
}
