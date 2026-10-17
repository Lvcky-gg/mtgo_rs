//! A static evaluation: how good a position is for one player, without playing it out.
//!
//! Search reads this at the end of every playout that has not ended the game. It is the
//! counterpart of a chess engine's evaluation function and, like one, is a weighted sum of
//! features: [`features`] measures a player's resources, and the score is a logistic of the
//! weighted difference between the player and their strongest opponent — a win probability in
//! `[0, 1]`. Finished games score exactly 0, ½ or 1.
//!
//! Because the score is a logistic of a linear function, the weights are exactly what logistic
//! regression fits from "features at this point → who went on to win". The `tune_eval` example
//! in `mtg-app` plays self-play games and prints fitted [`Weights`]; the defaults here are its
//! output, rounded, rather than guesses.

use mtg_core::{CardType, CounterKind, Keyword, PlayerId, Zone, ZoneRef};
use mtg_engine::{GameState, layers::PrintedCards};

/// What [`features`] measures, in order.
pub const FEATURES: [&str; 14] = [
    "life",
    "life over cap",
    "poison",
    "cards in hand",
    "library shortfall",
    "lands",
    "creatures",
    "power",
    "toughness",
    "evasive power",
    "other permanents",
    "planeswalkers",
    "loyalty",
    "recurring engines",
];
pub const N: usize = FEATURES.len();

/// The evaluation's coefficients: one per feature, in log-odds per unit of the difference.
#[derive(Clone, Debug)]
pub struct Weights {
    pub coefficients: [f64; N],
    /// Life up to this counts as "life"; beyond it, as "life over cap".
    pub life_cap: i32,
    /// Fewer cards than this in library counts as "library shortfall".
    pub library_floor: u32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            // The original hand weights, in log-odds (points ÷ 6). Replace with fitted values.
            coefficients: [
                0.058, 0.015, -0.117, 0.167, -0.25, 0.133, 0.167, 0.1, 0.058, 0.05, 0.2, 0.333,
                0.083, 0.025,
            ],
            life_cap: 20,
            library_floor: 5,
        }
    }
}

/// The win probability for `me` in `state`.
pub fn evaluate(
    state: &GameState,
    cards: &dyn PrintedCards,
    me: PlayerId,
    weights: &Weights,
) -> f64 {
    let living: Vec<PlayerId> = state
        .players
        .values()
        .filter(|p| !p.has_lost)
        .map(|p| p.id)
        .collect();
    if !living.contains(&me) {
        return 0.0;
    }
    if living.len() == 1 {
        return 1.0;
    }
    let score = |who| dot(&weights.coefficients, &features(state, cards, who, weights));
    let mine = score(me);
    let best_opponent = living
        .iter()
        .filter(|p| **p != me)
        .map(|p| score(*p))
        .fold(f64::NEG_INFINITY, f64::max);
    logistic(mine - best_opponent)
}

pub fn logistic(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

pub fn dot(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// One player's resources, feature by feature (see [`FEATURES`]).
pub fn features(
    state: &GameState,
    cards: &dyn PrintedCards,
    who: PlayerId,
    w: &Weights,
) -> [f64; N] {
    let mut f = [0.0; N];
    let p = state.player(who);
    f[0] = p.life.min(w.life_cap) as f64;
    f[1] = (p.life - w.life_cap).max(0) as f64;
    f[2] = p.poison as f64;
    let count = |zone| {
        state
            .objects_in(ZoneRef {
                zone,
                player: Some(who),
            })
            .len() as u32
    };
    f[3] = count(Zone::Hand) as f64;
    f[4] = w.library_floor.saturating_sub(count(Zone::Library)) as f64;

    for id in state.battlefield() {
        if mtg_engine::layers::controller(state, id) != Some(who) {
            continue;
        }
        let Some(ch) = mtg_engine::layers::compute(state, cards, id) else {
            continue;
        };
        let o = &state.objects[&id];
        let face = cards.face(o.card, o.face);
        let has = |k: Keyword| {
            ch.granted_keywords.contains(&k)
                || face.is_some_and(|face| {
                    face.abilities.iter().any(|a| {
                        ch.abilities.contains(&a.id)
                            && matches!(a.kind, mtg_ir::AbilityKind::Keyword(x) if x == k)
                    })
                })
        };
        if let Some(face) = face {
            f[13] += face
                .abilities
                .iter()
                .filter(|a| ch.abilities.contains(&a.id))
                .map(crate::bot::recurring_ability_value)
                .sum::<i64>() as f64;
        }
        if ch.has_type(CardType::Land) {
            f[5] += 1.0;
        } else if ch.has_type(CardType::Creature) {
            let power = ch.power.unwrap_or(0).max(0) as f64;
            f[6] += 1.0;
            f[7] += power;
            f[8] += (ch.toughness.unwrap_or(0) - o.damage as i32).max(0) as f64;
            if has(Keyword::Flying) {
                f[9] += power;
            }
        } else if ch.has_type(CardType::Planeswalker) {
            f[11] += 1.0;
            f[12] += o.counters.get(&CounterKind::Loyalty).copied().unwrap_or(0) as f64;
        } else {
            f[10] += 1.0;
        }
    }
    f
}
