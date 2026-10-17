//! Lookahead: choosing an answer by playing the game out.
//!
//! [`crate::bot::Bot`] decides from the board in front of it. This decides by trying things:
//! for each sensible answer to the question in front of it, play the game forward on copies of
//! the engine and keep the answer whose futures go best. It is the first rung of the ladder that
//! ends at a solver — the same shape as Monte Carlo search in Go before AlphaGo, or a chess
//! engine with a weak evaluation and no pruning yet:
//!
//! - **Candidates** — the answers worth comparing ([`candidates`]): every priority action, a
//!   handful of attack and block plans around the heuristic one.
//! - **Determinization** — each future starts from a copy whose hidden cards have been dealt
//!   again ([`determinize`]), so the search cannot see what the player cannot.
//! - **Playouts** — after the candidate, everyone (the searcher included) plays on with the
//!   heuristic [`Bot`] until a turn horizon, and [`eval::evaluate`] scores where it ended.
//! - **Allocation** — a UCB1 bandit spends the playout budget on candidates still in contention.
//!   Every candidate's k-th playout uses the same deal and seed (common random numbers), so
//!   candidates are compared on the same futures rather than on their luck.
//!
//! The engine is the only rules authority throughout: a candidate the engine refuses is dropped,
//! and playouts go through `advance`/`answer` like any game. The searcher holds no rules.
//!
//! Target choices compare bounded substitutions around the heuristic's targets.
//! Modal choices compare bounded combinations of effects.
//! Optional effects compare accepting and declining.
//! Questions without a candidate generator yet (ordering, mulligans) go to the
//! heuristic bot. They are the next things to search.

pub mod determinize;
pub mod encode;
pub mod eval;
mod modes;
pub mod value;

use mtg_core::{ObjectId, PlayerId};
use mtg_engine::{
    Choice, Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
    state::Rng,
};

use crate::bot::Bot;

pub use determinize::determinize;
pub use eval::{Weights, evaluate};

/// How hard to think.
#[derive(Clone, Debug)]
pub struct Budget {
    /// Playouts per decision, shared among its candidates.
    pub playouts: u32,
    /// Playouts stop this many turns after the decision's turn and are scored there.
    pub horizon_turns: u32,
    /// A playout that asks more questions than this is scored where it stands — a loop.
    pub max_choices: u32,
    /// UCB1's exploration constant, for scores in `[0, 1]`.
    pub exploration: f64,
    /// How much better than the heuristic's answer, in win probability, another answer must
    /// score before it replaces it. The paired standard error is the floor, so noise between
    /// equivalent moves — land before spell, or after — keeps the heuristic's choice.
    pub margin: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            playouts: 96,
            horizon_turns: 2,
            // A turn of ordinary play asks well under a hundred questions; past this a playout is
            // looping (flicker engines, untap combos) and is scored where it stands.
            max_choices: 400,
            exploration: 0.4,
            margin: 0.02,
        }
    }
}

/// A bot that searches the decisions it can and defers the rest to the heuristic bot.
pub struct Searcher {
    pub bot: Bot,
    pub budget: Budget,
    pub weights: Weights,
    /// A learned evaluation for playout leaves; the linear [`evaluate`] when absent.
    pub value: Option<std::sync::Arc<value::ValueNet>>,
    encoder: std::cell::RefCell<encode::Encoder>,
    seed: u64,
}

/// What a search found, for logging and tuning.
#[derive(Clone, Debug)]
pub struct Considered {
    pub answer: Answer,
    pub visits: u32,
    pub mean: f64,
}

impl Searcher {
    pub fn new(bot: Bot, budget: Budget, seed: u64) -> Self {
        Self {
            bot,
            budget,
            weights: Weights::default(),
            value: None,
            encoder: Default::default(),
            seed,
        }
    }

    /// Answer `choice`, which `engine` is waiting on.
    pub fn decide(&self, engine: &Engine, cards: &dyn PrintedCards, choice: &Choice) -> Answer {
        let view = engine.view_for(choice.who);
        let heuristic = self.bot.decide(choice, &view);
        match self.search(engine, cards, choice, heuristic.clone()) {
            Some(best) => best.answer,
            None => heuristic,
        }
    }

    /// The search itself: `None` when there was nothing to compare.
    pub fn search(
        &self,
        engine: &Engine,
        cards: &dyn PrintedCards,
        choice: &Choice,
        heuristic: Answer,
    ) -> Option<Considered> {
        let me = choice.who;
        let view = engine.view_for(me);
        let heuristic_key = format!("{heuristic:?}");
        let mut options = candidates(&self.bot, choice, &view, heuristic);
        // Only what the engine accepts is a candidate; each is kept as the engine it leads to.
        // `Answer` has no equality; its debug form is a faithful key for duplicates.
        let mut seen = std::collections::BTreeSet::new();
        let mut roots: Vec<(Answer, Engine)> = Vec::new();
        for answer in options.drain(..) {
            if !seen.insert(format!("{answer:?}")) {
                continue;
            }
            let mut fork = engine.clone();
            if fork.answer(cards, choice.id, answer.clone()).is_ok() {
                roots.push((answer, fork));
            }
        }
        if roots.len() < 2 {
            return None;
        }
        let heuristic_root = roots
            .iter()
            .position(|(a, _)| format!("{a:?}") == heuristic_key);

        let horizon = engine.state.turn + self.budget.horizon_turns;
        // Decisions differ by turn and choice so the same seat does not replay one future.
        let base = self.seed ^ u64::from(engine.state.turn) << 32 ^ choice.id;
        let mut visits = vec![0u32; roots.len()];
        let mut totals = vec![0f64; roots.len()];
        // Every score in playout order, so candidates can be compared deal by deal.
        let mut scores: Vec<Vec<f64>> = vec![Vec::new(); roots.len()];
        let budget = self.budget.playouts.max(roots.len() as u32 * 2);
        // Racing: an alternative that can no longer clearly beat the heuristic's answer stops
        // receiving playouts, and once none is left in contention the search stops. Most
        // decisions are ones the heuristic already gets right, so most end early.
        let mut alive = vec![true; roots.len()];
        for spent in 0..budget {
            let pick = if (spent as usize) < roots.len() * 2 {
                spent as usize % roots.len()
            } else {
                let ln = f64::from(spent).ln();
                (0..roots.len())
                    .filter(|i| alive[*i])
                    .max_by(|a, b| {
                        let ucb = |i: usize| {
                            totals[i] / f64::from(visits[i])
                                + self.budget.exploration * (ln / f64::from(visits[i])).sqrt()
                        };
                        ucb(*a).total_cmp(&ucb(*b))
                    })
                    .unwrap_or(0)
            };
            // Common random numbers: the k-th playout of every candidate shares a deal.
            let mut rng = rng_from(base ^ u64::from(visits[pick]).wrapping_mul(0x9E37_79B9));
            let mut fork = roots[pick].1.clone();
            determinize(&mut fork, me, &mut rng);
            let score = self.play_out(&mut fork, cards, me, horizon);
            visits[pick] += 1;
            totals[pick] += score;
            scores[pick].push(score);
            if let Some(h) = heuristic_root
                && (spent as usize) + 1 >= roots.len() * 2
            {
                for c in 0..roots.len() {
                    if c != h
                        && alive[c]
                        && scores[c].len().min(scores[h].len()) >= 3
                        && !could_be_better(&scores[c], &scores[h], self.budget.margin)
                    {
                        alive[c] = false;
                    }
                }
                if (0..roots.len()).all(|c| c == h || !alive[c]) {
                    break;
                }
            }
        }

        // The most-visited candidate, as UCB recommends; mean breaks ties.
        let mut best = (0..roots.len()).max_by(|a, b| {
            visits[*a].cmp(&visits[*b]).then(
                (totals[*a] / f64::from(visits[*a]))
                    .total_cmp(&(totals[*b] / f64::from(visits[*b]))),
            )
        })?;
        // It replaces the heuristic's answer only if it is clearly better on the same deals.
        if let Some(h) = heuristic_root
            && best != h
            && !clearly_better(&scores[best], &scores[h], self.budget.margin)
        {
            best = h;
        }
        Some(Considered {
            answer: roots.swap_remove(best).0,
            visits: visits[best],
            mean: totals[best] / f64::from(visits[best]),
        })
    }

    /// Play heuristically until the horizon or the end of the game; score it for `me`.
    fn play_out(
        &self,
        engine: &mut Engine,
        cards: &dyn PrintedCards,
        me: PlayerId,
        horizon: u32,
    ) -> f64 {
        let mut asked = 0;
        // Steps without a question count too: a loop of mandatory triggers asks no one.
        let mut steps = 0u64;
        loop {
            steps += 1;
            if steps > u64::from(self.budget.max_choices) * 64 {
                break;
            }
            match engine.advance(cards) {
                Progress::Continue => {}
                Progress::GameOver { .. } => {
                    return evaluate(&engine.state, cards, me, &self.weights);
                }
                Progress::NeedsChoice(c) => {
                    asked += 1;
                    if engine.state.turn > horizon || asked > self.budget.max_choices {
                        break;
                    }
                    if !answer_with(&self.bot, engine, cards, &c) {
                        break;
                    }
                }
            }
        }
        match &self.value {
            Some(net) if !engine.state.player(me).has_lost => {
                net.evaluate(&self.encoder.borrow_mut().encode(&engine.state, cards, me))
            }
            _ => evaluate(&engine.state, cards, me, &self.weights),
        }
    }
}

/// Whether `a` beats `b` by more than `margin` and by more than twice the standard error of
/// their paired differences. Samples pair up by index: the k-th playouts share a deal.
fn clearly_better(a: &[f64], b: &[f64], margin: f64) -> bool {
    let (mean, se) = paired(a, b);
    mean > margin.max(2.0 * se)
}

/// Whether `a` might still beat `b` by `margin`: the upper end of a two-standard-error interval
/// on their paired differences reaches it.
fn could_be_better(a: &[f64], b: &[f64], margin: f64) -> bool {
    let (mean, se) = paired(a, b);
    mean + 2.0 * se >= margin
}

/// Mean and standard error of the paired differences `a[k] - b[k]`.
fn paired(a: &[f64], b: &[f64]) -> (f64, f64) {
    let n = a.len().min(b.len());
    if n < 2 {
        return (0.0, f64::INFINITY);
    }
    let diffs: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    let mean = diffs.iter().sum::<f64>() / n as f64;
    let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    (mean, (var / n as f64).sqrt())
}

/// Answer `c` with the heuristic bot, falling back to the plainest legal answer. `false` when
/// even that is refused — the game cannot go on.
pub fn answer_with(bot: &Bot, engine: &mut Engine, cards: &dyn PrintedCards, c: &Choice) -> bool {
    let view = engine.view_for(c.who);
    let answer = bot.decide(c, &view);
    engine.answer(cards, c.id, answer).is_ok()
        || engine
            .answer(cards, c.id, crate::well_formed(c, &view))
            .is_ok()
}

fn rng_from(seed: u64) -> Rng {
    let mut bytes = [0u8; 32];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&(seed.wrapping_add(i as u64)).to_le_bytes());
    }
    Rng::from_seed(&bytes)
}

/// The answers worth comparing, the heuristic one first. Empty when the question is not one
/// search handles yet.
pub fn candidates(
    bot: &Bot,
    choice: &Choice,
    view: &mtg_engine::PlayerView,
    heuristic: Answer,
) -> Vec<Answer> {
    let mut out = vec![heuristic.clone()];
    match &choice.kind {
        ChoiceKind::Confirm => {
            out.extend([Answer::Bool(false), Answer::Bool(true)]);
        }
        ChoiceKind::ChooseModes {
            available,
            count,
            min,
        } => {
            out.extend(modes::alternatives(
                available.len(),
                min.unwrap_or(*count),
                *count,
            ));
        }
        ChoiceKind::ChooseTargets { slots, .. } => {
            if let Answer::Targets(picks) = &heuristic {
                // Change one target at a time, retaining every slot's target count
                // and the other slots. The engine validates cross-slot restrictions.
                // Bound branching for large battlefields and multiplayer games.
                let mut added = 0;
                'slots: for (slot, legal) in slots.iter().enumerate() {
                    let Some(selected) = picks.get(slot) else {
                        continue;
                    };
                    for (index, _) in selected.iter().enumerate() {
                        for target in legal {
                            if selected.contains(target) {
                                continue;
                            }
                            if added >= 24 {
                                break 'slots;
                            }
                            let mut alternative = picks.clone();
                            alternative[slot][index] = *target;
                            out.push(Answer::Targets(alternative));
                            added += 1;
                        }
                    }
                }
            }
        }
        ChoiceKind::Priority { legal } => {
            out.push(Answer::Pass);
            out.extend(
                legal
                    .actions
                    .iter()
                    .filter(|a| !matches!(a, Action::Pass | Action::Concede))
                    .cloned()
                    .map(Answer::Action),
            );
        }
        ChoiceKind::DeclareAttackers {
            eligible,
            defenders,
        } => {
            let chosen: Vec<ObjectId> = match &heuristic {
                Answer::Attackers(pairs) => pairs.iter().map(|(a, _)| *a).collect(),
                Answer::Objects(ids) => ids.clone(),
                _ => Vec::new(),
            };
            let plan = |set: &[ObjectId]| bot.attack_plan(set, defenders, view);
            out.push(plan(&[]));
            out.push(plan(eligible));
            // One creature more or fewer than the heuristic sends.
            if eligible.len() <= 8 {
                for id in eligible {
                    let set: Vec<ObjectId> = if chosen.contains(id) {
                        chosen.iter().copied().filter(|c| c != id).collect()
                    } else {
                        chosen.iter().copied().chain([*id]).collect()
                    };
                    out.push(plan(&set));
                }
            }
        }
        ChoiceKind::DeclareBlockers { eligible, .. } => {
            let chosen = match &heuristic {
                Answer::Blocks(pairs) => pairs.clone(),
                _ => Vec::new(),
            };
            out.push(Answer::Blocks(Vec::new()));
            // The heuristic's blocks with one block taken away or added.
            for pair in &chosen {
                out.push(Answer::Blocks(
                    chosen.iter().copied().filter(|p| p != pair).collect(),
                ));
            }
            let mut added = 0;
            for (blocker, attackers) in eligible {
                if chosen.iter().any(|(b, _)| b == blocker) {
                    continue;
                }
                for attacker in attackers {
                    if added >= 12 {
                        break;
                    }
                    added += 1;
                    out.push(Answer::Blocks(
                        chosen
                            .iter()
                            .copied()
                            .chain([(*blocker, *attacker)])
                            .collect(),
                    ));
                }
            }
        }
        _ => return Vec::new(),
    }
    out
}
