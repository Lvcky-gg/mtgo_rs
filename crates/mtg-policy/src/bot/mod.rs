//! A simple opponent: plays lands, casts spells, attacks and blocks.
//!
//! [`crate::Policy`] only answers what is *not* a decision, and with no one at the keyboard the
//! rest falls to each question's default — which for priority is to pass. That is right for a
//! seat whose player stepped away, and useless as an opponent: it never plays a card. This makes
//! the decisions instead.
//!
//! Like every other answerer it holds **no rules**. It picks from the engine's legal-action list
//! and the view the engine projected for its seat, so it cannot cheat or play illegally — the
//! worst a bad heuristic can do is play badly. The only thing it knows beyond the view is each
//! card's printed stats and ability metadata, snapshotted up front.
//!
//! ## Strategy Overview
//!
//! The bot's decision tree prioritizes maximizing damage to the opponent:
//!
//! 1. **Lethal damage** — Attack/cast spells that deal lethal damage directly to the opponent.
//! 2. **Planeswalker pressure** — If lethal is not available, pressure the lowest-loyalty planeswalker.
//! 3. **Defense** — Block incoming damage to save the player or threatened planeswalkers.
//! 4. **Resource management** — Save blockers and spells for valuable effects; avoid bad trades.
//!
//! ### On its own turn:
//! - Play lands for mana acceleration (highest priority).
//! - Activate planeswalker abilities that build loyalty (positive delta first).
//! - Cast the most expensive spell (highest mana value).
//! - Hold pure prevention spells (like Fog) for declared attacks.
//!
//! ### During combat:
//! - **Attack**: Send creatures that won't lose value in combat; attack with everything if lethal.
//! - **Block**: Kill attackers when possible; chump block to save life or planeswalker loyalty.
//!
//! ### Combat assessment:
//! Printed stats plus visible counters and damage are used to evaluate threats. Continuous stat
//! changes and unfamiliar effect shapes remain outside these heuristics; the bot's worst case
//! is poor play, never illegal play.

mod attack_strategy;
mod block_strategy;
mod cast_strategy;

use std::collections::{BTreeMap, BTreeSet};

use mtg_core::{AbilityId, CardId, CardType, CounterKind, ObjectId, Target};
use mtg_engine::{
    Choice, PlayerView,
    choice::{Answer, ChoiceKind},
};
use mtg_ir::{
    AbilityKind, AdditionalCost, Effect, PrintedCards, Selector,
    ability::Keyword,
    effect::{Modification, Restriction},
};

use crate::Policy;

/// Combat attributes snapshotted from a card's printed characteristics plus board state.
///
/// The bot uses these stats to evaluate trades, blocks, and whether an attack is lethal.
/// This includes power/toughness, evasion keywords (flying/reach), and combat modifiers
/// (menace, deathtouch, trample, double strike).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Stats {
    pub mana_value: u32,
    pub creature: bool,
    pub land: bool,
    pub power: i32,
    pub toughness: i32,
    pub flying: bool,
    pub reach: bool,
    pub first_strike: bool,
    pub deathtouch: bool,
    pub menace: bool,
    pub trample: bool,
    pub double_strike: bool,
    /// The fewest creatures that may block it beyond menace's two ("except by three or
    /// more creatures"); 0 when nothing says so.
    pub min_blockers: u8,
    /// "Can't attack alone".
    pub cant_attack_alone: bool,
    /// "Can't block alone".
    pub cant_block_alone: bool,
}

impl Stats {
    /// Unblocked damage across both combat damage steps.
    fn combat_damage(&self) -> i64 {
        i64::from(self.power.max(0)) * if self.double_strike { 2 } else { 1 }
    }

    /// Read a card's front face.
    ///
    /// Returns the printed stats of a card if the face is defined. This is used by the bot
    /// to make decisions about attacks and blocks.
    pub fn of(cards: &dyn PrintedCards, card: CardId) -> Option<Self> {
        Self::of_face(cards, card, 0)
    }

    fn of_face(cards: &dyn PrintedCards, card: CardId, index: u8) -> Option<Self> {
        let face = cards.face(card, index)?;
        let ch = face.printed_characteristics();
        let has = |k: Keyword| {
            face.abilities
                .iter()
                .any(|a| matches!(a.kind, AbilityKind::Keyword(x) if x == k))
        };
        let restriction = |want: fn(&Restriction) -> Option<u8>| {
            face.abilities.iter().find_map(|a| match &a.kind {
                AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(r),
                    condition: None,
                } => want(r),
                _ => None,
            })
        };
        Some(Self {
            min_blockers: restriction(|r| match r {
                Restriction::MinimumBlockers(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(0),
            cant_attack_alone: restriction(|r| {
                matches!(r, Restriction::CantAttackAlone).then_some(0)
            })
            .is_some(),
            cant_block_alone: restriction(|r| {
                matches!(r, Restriction::CantBlockAlone).then_some(0)
            })
            .is_some(),
            mana_value: face.mana_cost.mana_value(),
            creature: ch.has_type(CardType::Creature),
            land: ch.has_type(CardType::Land),
            power: face.power.unwrap_or(0),
            toughness: face.toughness.unwrap_or(0),
            flying: has(Keyword::Flying),
            reach: has(Keyword::Reach),
            first_strike: has(Keyword::FirstStrike) || has(Keyword::DoubleStrike),
            deathtouch: has(Keyword::Deathtouch),
            menace: has(Keyword::Menace),
            trample: has(Keyword::Trample),
            double_strike: has(Keyword::DoubleStrike),
        })
    }

    /// Whether this creature's accumulated damage destroys `other`.
    ///
    /// Positive damage is lethal when it reaches the other's remaining toughness,
    /// or when its source has deathtouch.
    ///
    /// Example: A 2/2 creature with deathtouch kills any creature by damage.
    fn kills_with_damage(&self, other: &Stats, damage: i64) -> bool {
        damage > 0 && (self.deathtouch || damage >= i64::from(other.toughness))
    }

    /// Whether `blocker` may block this creature, as far as flying goes. The engine's
    /// eligibility list is the real answer; this is only for weighing threats on the board.
    ///
    /// A flyer can only be blocked by a blocker that also has flying or reach.
    /// Non-flying creatures can be blocked by any creature.
    ///
    /// Example: A 2/2 flying creature cannot be blocked by a 3/3 ground creature.
    fn blockable_by(&self, blocker: &Stats) -> bool {
        !self.flying || blocker.flying || blocker.reach
    }
}

/// How one creature fares fighting another, accounting for first and double strike.
///
/// Returns `(a_kills_b, b_kills_a)` where each bool indicates if that creature destroys the other.
///
/// First strike is modeled: if one creature has first strike and the other doesn't, only the
/// first striker's lethal damage survives (if the other creature dies to first strike damage,
/// its own damage doesn't happen).
///
/// Examples:
/// - **3/3 vs 2/2**: `(true, false)` — 3/3 survives.
/// - **2/2 first strike vs 3/3**: `(false, false)` — Neither dies (2 damage doesn't kill 3/3).
/// - **3/3 first strike vs 2/2**: `(true, false)` — 3/3 kills 2/2 with first strike (2's damage never resolves).
fn fight(a: &Stats, b: &Stats) -> (bool, bool) {
    let first_damage = |s: &Stats| {
        if s.first_strike || s.double_strike {
            i64::from(s.power.max(0))
        } else {
            0
        }
    };
    let a_first = first_damage(a);
    let b_first = first_damage(b);
    let a_kills_first = a.kills_with_damage(b, a_first);
    let b_kills_first = b.kills_with_damage(a, b_first);
    if a_kills_first || b_kills_first {
        return (a_kills_first, b_kills_first);
    }
    let total_damage = |s: &Stats, first| {
        first
            + if !s.first_strike || s.double_strike {
                i64::from(s.power.max(0))
            } else {
                0
            }
    };
    (
        a.kills_with_damage(b, total_damage(a, a_first)),
        b.kills_with_damage(a, total_damage(b, b_first)),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum TargetPreference {
    #[default]
    Unknown,
    Friendly,
    Hostile,
}

#[derive(Clone, Copy)]
struct LoyaltyAbility {
    delta: i32,
}

/// The bot's decision-making engine.
///
/// Holds precomputed metadata (stats, abilities, targeting preferences) to make fast decisions
/// about attacks, blocks, casting, and targeting. The bot queries this metadata combined with
/// the current board state to execute its heuristics.
pub struct Bot {
    stats: BTreeMap<(CardId, u8), Stats>,
    layouts: BTreeMap<CardId, mtg_ir::Layout>,
    loyalty: BTreeMap<(CardId, u8, AbilityId), LoyaltyAbility>,
    targeting: BTreeMap<(CardId, u8, Option<AbilityId>), TargetPreference>,
    combat_prevention: BTreeSet<(CardId, u8)>,
    policy: Policy,
}

impl Bot {
    /// A bot that knows the given cards. `ids` should cover every card in play — both decks.
    ///
    /// The bot builds a lookup table of card stats and abilities to make fast decisions.
    /// All card IDs from both players' decks should be provided.
    pub fn new(cards: &dyn PrintedCards, ids: impl IntoIterator<Item = CardId>) -> Self {
        let mut stats = BTreeMap::new();
        let mut layouts = BTreeMap::new();
        let mut loyalty = BTreeMap::new();
        let mut targeting = BTreeMap::new();
        let mut combat_prevention = BTreeSet::new();
        for id in ids {
            layouts.insert(id, cards.layout(id));
            let count = if matches!(
                cards.layout(id),
                mtg_ir::Layout::ModalDfc | mtg_ir::Layout::Adventure | mtg_ir::Layout::Split
            ) {
                2
            } else {
                1
            };
            for index in 0..count {
                if let Some(s) = Stats::of_face(cards, id, index) {
                    stats.insert((id, index), s);
                }
                let Some(face) = cards.face(id, index) else {
                    continue;
                };
                for a in &face.abilities {
                    match &a.kind {
                        AbilityKind::Activated {
                            cost,
                            effect,
                            is_loyalty_ability,
                            ..
                        } => {
                            targeting.insert((id, index, Some(a.id)), target_preference(effect));
                            if *is_loyalty_ability {
                                let delta = cost
                                    .additional
                                    .iter()
                                    .filter_map(|c| match c {
                                        AdditionalCost::Loyalty { delta } => Some(*delta),
                                        _ => None,
                                    })
                                    .sum();
                                loyalty.insert((id, index, a.id), LoyaltyAbility { delta });
                            }
                        }
                        AbilityKind::SpellEffect(effect) => {
                            if matches!(effect, Effect::PreventAllCombatDamage) {
                                combat_prevention.insert((id, index));
                            }
                            targeting.insert((id, index, None), target_preference(effect));
                        }
                        AbilityKind::Triggered { effect, .. } => {
                            targeting.insert((id, index, Some(a.id)), target_preference(effect));
                        }
                        _ => {}
                    }
                }
            }
        }
        Self {
            stats,
            layouts,
            loyalty,
            targeting,
            combat_prevention,
            policy: Policy::default(),
        }
    }

    /// Answer a question for the seat `view` belongs to.
    ///
    /// Makes a decision using the bot's heuristics. The decision flow:
    ///
    /// 1. Check if the policy has an automatic answer (policy-driven exceptions).
    /// 2. Dispatch to the appropriate decision handler:
    ///    - **Priority** — play lands, then loyalty activations, then expensive spells
    ///    - **DeclareAttackers** — send creatures that won't lose value; attack with everything if lethal
    ///    - **DeclareBlockers** — kill attackers when possible; chump block to save life or planeswalkers
    ///    - **ChooseTargets** — target friendly effects on friendly creatures/players; hostile on opponents
    ///    - **KeepOrMulligan** — keep hands with 2–5 lands (mulligan up to twice)
    ///    - **ChooseObjects** — discard the worst cards (spare lands, then expensive spells)
    /// 3. Fall back to `well_formed` for unrecognized choice types (engine defaults).
    ///
    /// # Examples
    ///
    /// **Priority decision with creatures and spells:**
    /// - If a land is available, play it (mana acceleration).
    /// - Else if a planeswalker loyalty ability is available, activate it (prefer +loyalty).
    /// - Else cast the most expensive spell in hand.
    /// - Else pass (stack is empty and nothing else to do).
    ///
    /// **Attack decision with lethal damage available:**
    /// - If 12 damage is available and opponent has 12 life, send all creatures.
    /// - Else, send only creatures that can't be killed for free (don't lose value).
    pub fn decide(&self, choice: &Choice, view: &PlayerView) -> Answer {
        if let Some(answer) = self.policy.may_auto_answer(choice, view) {
            return answer;
        }
        let answer = match &choice.kind {
            ChoiceKind::Priority { legal } => Some(self.priority(&legal.actions, view)),
            ChoiceKind::DeclareAttackers {
                eligible,
                defenders,
            } => {
                let mut attackers = self.attackers(eligible, view);
                if let Some(Answer::Objects(required)) = &choice.default {
                    for id in required {
                        if eligible.contains(id) && !attackers.contains(id) {
                            attackers.push(*id);
                        }
                    }
                }
                if let [only] = attackers[..]
                    && self.stats(view, only).is_some_and(|s| s.cant_attack_alone)
                {
                    attackers.clear();
                }
                Some(self.attack_plan(&attackers, defenders, view))
            }
            ChoiceKind::ChooseTargets { slots } => self.targets(choice, slots, view),
            // Blocks something must make (lure, CR 509.1c) come as the engine's default,
            // which the bot's own choice would not know to include.
            ChoiceKind::DeclareBlockers { .. } if matches!(&choice.default, Some(Answer::Blocks(b)) if !b.is_empty()) => {
                choice.default.clone()
            }
            ChoiceKind::DeclareBlockers { eligible, .. } => {
                Some(Answer::Blocks(self.blocks(eligible, view)))
            }
            ChoiceKind::KeepOrMulligan { mulligans_taken } => {
                Some(Answer::Bool(self.keeps(view, *mulligans_taken)))
            }
            // Choosing cards out of its own hand — to bottom after a mulligan, or to discard —
            // gives up the least useful ones.
            ChoiceKind::ChooseObjects { from, min, .. } if self.all_in_hand(from, view) => {
                Some(Answer::Objects(self.worst_cards(from, *min as usize, view)))
            }
            _ => None,
        };
        answer.unwrap_or_else(|| crate::well_formed(choice, view))
    }

    fn hand(&self, view: &PlayerView) -> Vec<ObjectId> {
        view.visible
            .values()
            .filter(|o| o.zone.zone == mtg_core::Zone::Hand && o.zone.player == Some(view.viewer))
            .map(|o| o.id)
            .collect()
    }

    fn is_land(&self, view: &PlayerView, id: ObjectId) -> bool {
        self.stats(view, id).is_some_and(|s| s.land)
    }

    /// Keep two to five lands in a seven-card hand; mulligan at most twice.
    ///
    /// A hand with 2–5 lands is considered keepable (enough mana without flooding).
    /// After two mulligans, the bot keeps any hand (five-card hands are worse than almost any seven).
    ///
    /// Example: With 1 land in hand, mulligan. With 2 lands, keep. With 6 lands, mulligan.
    fn keeps(&self, view: &PlayerView, mulligans_taken: u32) -> bool {
        if mulligans_taken >= 2 {
            return true;
        }
        let lands = self
            .hand(view)
            .into_iter()
            .filter(|id| self.is_land(view, *id))
            .count();
        (2..=5).contains(&lands)
    }

    fn all_in_hand(&self, from: &[ObjectId], view: &PlayerView) -> bool {
        let hand = self.hand(view);
        !from.is_empty() && from.iter().all(|o| hand.contains(o))
    }

    /// The `n` cards the hand can best spare: surplus lands when it has plenty, otherwise the
    /// most expensive spells.
    ///
    /// Prioritizes discarding in order:
    /// 1. Spare lands (when the hand has >3 lands).
    /// 2. Most expensive spells (high mana value = more likely to be unnecessary).
    /// 3. Remaining lands (the hand needs some mana).
    ///
    /// Example: With hand [Island, Mountain, Swamp, Shock, Fireball, Draw, Counterspell],
    /// to discard 2 cards: pick Swamp and Mountain (spare 3rd and 4th land), not Shock (cheap removal).
    fn worst_cards(&self, from: &[ObjectId], n: usize, view: &PlayerView) -> Vec<ObjectId> {
        let lands = from.iter().filter(|id| self.is_land(view, **id)).count();
        let mut ranked: Vec<ObjectId> = from.to_vec();
        ranked.sort_by_key(|id| {
            let s = self.stats(view, *id).unwrap_or_default();
            let spare_land = s.land && lands > 3;
            // Lower sorts first: spare lands, then spells from most to least expensive, then
            // the lands the hand needs.
            (
                u32::from(!spare_land),
                s.land,
                std::cmp::Reverse(s.mana_value),
                id.0,
            )
        });
        ranked.truncate(n);
        ranked
    }

    /// Get the effective combat stats of a creature on the board, including counters and damage.
    ///
    /// Starts with printed stats and adjusts for:
    /// - Mana value of split cards (adds both halves).
    /// - +1/+1 and -1/-1 counters (apply to power and toughness).
    /// - Damage marked on the creature (reduces effective toughness).
    ///
    /// Returns `None` if the object is not in the visible zone or its card is unknown.
    fn stats(&self, view: &PlayerView, id: ObjectId) -> Option<Stats> {
        view.visible
            .get(&id)
            .and_then(|o| self.stats.get(&(o.card?, o.face)))
            .copied()
            .map(|mut stats| {
                let o = &view.visible[&id];
                if let Some(card) = o.card
                    && o.zone.zone != mtg_core::Zone::Stack
                    && self.layouts.get(&card) == Some(&mtg_ir::Layout::Split)
                    && let Some(other) = self.stats.get(&(card, 1))
                {
                    stats.mana_value = stats.mana_value.saturating_add(other.mana_value);
                }
                let plus = o
                    .counters
                    .get(&CounterKind::PlusOnePlusOne)
                    .copied()
                    .unwrap_or(0);
                let minus = o
                    .counters
                    .get(&CounterKind::MinusOneMinusOne)
                    .copied()
                    .unwrap_or(0);
                let delta = i64::from(plus) - i64::from(minus);
                stats.power = (i64::from(stats.power) + delta)
                    .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                    as i32;
                stats.toughness = (i64::from(stats.toughness) + delta - i64::from(o.damage))
                    .clamp(0, i64::from(i32::MAX)) as i32;
                stats
            })
    }

    /// Choose targets for spells and abilities based on their effect.
    ///
    /// The bot ranks targets using a preference learned from the spell/ability's effect:
    /// - **Friendly effects** (gain life, draw, prevent damage) target friendly creatures/players.
    /// - **Hostile effects** (deal damage, destroy, discard) target opponents.
    /// - **Unknown effects** defer to the engine's default.
    ///
    /// Targets are sorted by:
    /// 1. Preference (hostile target? friendly target?).
    /// 2. Value (mana value for objects, 1 for players).
    /// 3. ID (for stable, consistent choices).
    ///
    /// Example: A `Shock` (Deal 2 damage) targets a hostile creature/player.
    /// Example: A `Healing Touch` targets a friendly creature or the controller.
    fn targets(&self, choice: &Choice, slots: &[Vec<Target>], view: &PlayerView) -> Option<Answer> {
        let stack = view.visible.get(view.stack.last()?)?;
        let preference = self
            .targeting
            .get(&(stack.card?, stack.face, stack.ability))
            .copied()?;
        if preference == TargetPreference::Unknown {
            return None;
        }
        let counts: Vec<usize> = match &choice.default {
            Some(Answer::Targets(picks)) => picks.iter().map(Vec::len).collect(),
            _ => vec![1; slots.len()],
        };
        Some(Answer::Targets(
            slots
                .iter()
                .enumerate()
                .map(|(i, legal)| {
                    let mut ranked = legal.clone();
                    ranked.sort_by_key(|target| {
                        let (controller, value, id) = match target {
                            Target::Player(p) => (Some(*p), 1, u32::from(p.0)),
                            Target::Object(o) => (
                                view.visible.get(o).map(|o| o.controller),
                                self.stats(view, *o)
                                    .map_or(0, |s| s.mana_value.saturating_add(2)),
                                o.0,
                            ),
                        };
                        let friendly = controller == Some(view.viewer);
                        (
                            std::cmp::Reverse(match preference {
                                TargetPreference::Friendly => friendly,
                                TargetPreference::Hostile => controller.is_some() && !friendly,
                                TargetPreference::Unknown => false,
                            }),
                            std::cmp::Reverse(value),
                            id,
                        )
                    });
                    ranked.truncate(counts.get(i).copied().unwrap_or(1));
                    ranked
                })
                .collect(),
        ))
    }
}

/// A small heuristic for effects whose targets are consistently helpful or harmful.
/// Mixed or unfamiliar effects retain the engine's default target answer.
///
/// Recognizes common patterns:
/// - **Damage, destroy, discard, lose life** → target hostile creatures/players.
/// - **Gain life, draw** → target friendly creatures/players.
/// - **Prevent damage** → target friendlies (protects the controller).
/// - **Sequences** → if all effects in a sequence have the same preference, use that.
///
/// Example: `Lightning Bolt` (Deal 3 damage) returns `Hostile`.
/// Example: `Fog` (Prevent all combat damage) returns `Friendly`.
fn target_preference(effect: &Effect) -> TargetPreference {
    use TargetPreference::{Friendly, Hostile, Unknown};
    let targeted = |s: &Selector| matches!(s, Selector::Target { .. });
    match effect {
        Effect::DealDamage { to, .. } if targeted(to) => Hostile,
        Effect::Destroy { what } if targeted(what) => Hostile,
        Effect::LoseLife { who, .. } | Effect::Discard { who, .. } if targeted(who) => Hostile,
        Effect::GainLife { who, .. } | Effect::Draw { who, .. } if targeted(who) => Friendly,
        Effect::PreventDamage { to } if targeted(to) => Friendly,
        Effect::PreventDamageShield { to: Some(to), .. } if targeted(to) => Friendly,
        Effect::PreventDamageShield { by: Some(by), .. } if targeted(by) => Hostile,
        Effect::Sequence(items) => {
            let known: Vec<_> = items
                .iter()
                .map(target_preference)
                .filter(|p| *p != Unknown)
                .collect();
            known
                .first()
                .copied()
                .filter(|first| known.iter().all(|p| p == first))
                .unwrap_or(Unknown)
        }
        _ => Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{PlayerId, Step, ZoneRef};
    use mtg_engine::{
        actions::{Action, LegalActions},
        view::{ObjectView, PlayerSummary},
    };

    const ME: PlayerId = PlayerId(1);
    const THEM: PlayerId = PlayerId(0);

    fn creature(power: i32, toughness: i32) -> Stats {
        Stats {
            creature: true,
            power,
            toughness,
            mana_value: 2,
            ..Default::default()
        }
    }

    #[test]
    fn large_attack_totals_and_blocker_estimates_do_not_overflow() {
        let b = bot(&[creature(i32::MAX, 1), creature(1, i32::MAX)]);
        let v = view(
            ME,
            Step::DeclareAttackers,
            &[(1, 0, ME), (2, 0, ME), (3, 0, ME), (4, 1, THEM)],
        );
        assert_eq!(
            b.attackers(&[ObjectId(1), ObjectId(2), ObjectId(3)], &v),
            vec![ObjectId(1), ObjectId(2), ObjectId(3)]
        );
    }

    #[test]
    fn large_double_strike_damage_is_lethal_after_one_attacker_is_blocked() {
        let b = bot(&[
            Stats {
                double_strike: true,
                ..creature(i32::MAX, 1)
            },
            creature(1, 1),
            Stats::default(),
        ]);
        let mut v = view(
            ME,
            Step::DeclareAttackers,
            &[(1, 0, ME), (2, 0, ME), (3, 1, THEM), (4, 2, THEM)],
        );
        v.players.get_mut(&THEM).unwrap().life = i32::MAX;
        loyalty(&mut v, 4, 1);
        let Answer::Attackers(plan) = b.attack_plan(
            &[ObjectId(1), ObjectId(2)],
            &[Target::Player(THEM), Target::Object(ObjectId(4))],
            &v,
        ) else {
            panic!("attack plan")
        };
        assert_eq!(
            plan,
            vec![
                (ObjectId(1), Target::Player(THEM)),
                (ObjectId(2), Target::Player(THEM))
            ]
        );
    }

    #[test]
    fn large_incoming_damage_still_requests_a_life_saving_block() {
        let b = bot(&[creature(1, 1), creature(i32::MAX, i32::MAX)]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 1, THEM), (3, 1, THEM)],
        );
        attacking(&mut v, 2, Target::Player(ME));
        attacking(&mut v, 3, Target::Player(ME));
        assert_eq!(
            b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v),
            vec![(ObjectId(1), ObjectId(2))]
        );
    }

    #[test]
    fn large_marked_damage_never_increases_effective_toughness() {
        let b = bot(&[creature(2, 2)]);
        let mut v = view(ME, Step::DeclareAttackers, &[(1, 0, ME)]);
        v.visible.get_mut(&ObjectId(1)).unwrap().damage = u32::MAX;
        assert_eq!(b.stats(&v, ObjectId(1)).unwrap().toughness, 0);
    }

    #[test]
    fn extreme_counters_are_combined_before_clamping_stats() {
        let b = bot(&[creature(i32::MIN, i32::MIN)]);
        let mut v = view(ME, Step::DeclareAttackers, &[(1, 0, ME)]);
        let o = v.visible.get_mut(&ObjectId(1)).unwrap();
        o.counters.insert(CounterKind::PlusOnePlusOne, i32::MAX);
        o.counters.insert(CounterKind::MinusOneMinusOne, i32::MIN);
        let stats = b.stats(&v, ObjectId(1)).unwrap();
        assert_eq!(stats.power, i32::MAX);
        assert_eq!(stats.toughness, i32::MAX);
    }

    #[test]
    fn discarding_a_maximum_mana_value_spell_does_not_overflow() {
        let b = bot(&[
            Stats {
                mana_value: u32::MAX,
                ..Default::default()
            },
            creature(1, 1),
        ]);
        let mut v = view(ME, Step::Cleanup, &[(1, 0, ME), (2, 1, ME)]);
        in_hand(&mut v, &[1, 2]);
        assert_eq!(
            b.worst_cards(&[ObjectId(1), ObjectId(2)], 1, &v),
            vec![ObjectId(1)]
        );
    }

    /// A bot whose card N has the given stats.
    fn bot(cards: &[Stats]) -> Bot {
        Bot {
            layouts: BTreeMap::new(),
            stats: cards
                .iter()
                .enumerate()
                .map(|(i, s)| ((CardId(i as u32), 0), *s))
                .collect(),
            loyalty: BTreeMap::new(),
            targeting: BTreeMap::new(),
            combat_prevention: BTreeSet::new(),
            policy: Policy::default(),
        }
    }

    /// A view for the bot: `objects` are (object id, card index, controller).
    fn view(active: PlayerId, step: Step, objects: &[(u32, u32, PlayerId)]) -> PlayerView {
        let summary = |id| PlayerSummary {
            id,
            life: 20,
            poison: 0,
            energy: 0,
            hand_size: 3,
            library_size: 20,
            graveyard: Vec::new(),
            mana: [0; 6],
            commander_damage: Default::default(),
        };
        PlayerView {
            revealed_cards: Vec::new(),
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer: ME,
            turn: 3,
            active_player: active,
            step,
            priority: Some(ME),
            players: [(ME, summary(ME)), (THEM, summary(THEM))].into(),
            visible: objects
                .iter()
                .map(|(id, card, owner)| {
                    let o = ObjectView {
                        id: ObjectId(*id),
                        zone: ZoneRef::shared(mtg_core::Zone::Battlefield),
                        controller: *owner,
                        card: Some(CardId(*card)),
                        face: 0,
                        adventure_player: None,
                        tapped: false,
                        damage: 0,
                        counters: Default::default(),
                        attached_to: None,
                        targets: Vec::new(),
                        is_ability: false,
                        ability: None,
                        attacking: false,
                        attacking_target: None,
                        blocking: None,
                    };
                    (o.id, o)
                })
                .collect(),
            stack: Vec::new(),
        }
    }

    fn priority(actions: Vec<Action>) -> Choice {
        Choice {
            id: 1,
            who: ME,
            kind: ChoiceKind::Priority {
                legal: LegalActions {
                    who: Some(ME),
                    actions,
                    ..Default::default()
                },
            },
            because: "test".into(),
            default: Some(Answer::Pass),
            undo: false,
        }
    }

    fn in_hand(view: &mut PlayerView, ids: &[u32]) {
        for id in ids {
            if let Some(o) = view.visible.get_mut(&ObjectId(*id)) {
                o.zone = ZoneRef::of(mtg_core::Zone::Hand, ME);
            }
        }
    }

    #[test]
    fn it_keeps_a_hand_with_a_few_lands_and_sends_back_a_landless_one() {
        let land = Stats {
            land: true,
            ..Default::default()
        };
        let b = bot(&[land, creature(2, 2)]);
        let mut good = view(
            ME,
            Step::Untap,
            &[
                (1, 0, ME),
                (2, 0, ME),
                (3, 0, ME),
                (4, 1, ME),
                (5, 1, ME),
                (6, 1, ME),
                (7, 1, ME),
            ],
        );
        in_hand(&mut good, &[1, 2, 3, 4, 5, 6, 7]);
        assert!(b.keeps(&good, 0));
        let mut bad = view(
            ME,
            Step::Untap,
            &[
                (1, 1, ME),
                (2, 1, ME),
                (3, 1, ME),
                (4, 1, ME),
                (5, 1, ME),
                (6, 1, ME),
                (7, 1, ME),
            ],
        );
        in_hand(&mut bad, &[1, 2, 3, 4, 5, 6, 7]);
        assert!(!b.keeps(&bad, 0), "no lands");
        assert!(b.keeps(&bad, 2), "but not below five cards");
    }

    #[test]
    fn it_bottoms_surplus_lands_or_else_its_most_expensive_spell() {
        let land = Stats {
            land: true,
            ..Default::default()
        };
        let cheap = creature(1, 1);
        let dear = Stats {
            mana_value: 6,
            ..creature(6, 6)
        };
        let b = bot(&[land, cheap, dear]);
        let ids = [
            (1, 0, ME),
            (2, 0, ME),
            (3, 0, ME),
            (4, 0, ME),
            (5, 0, ME),
            (6, 1, ME),
            (7, 2, ME),
        ];
        let mut v = view(ME, Step::Untap, &ids);
        in_hand(&mut v, &[1, 2, 3, 4, 5, 6, 7]);
        let all: Vec<ObjectId> = (1..=7).map(ObjectId).collect();
        let bottom = b.worst_cards(&all, 1, &v);
        assert!(b.is_land(&v, bottom[0]), "five lands: a land goes");

        let ids = [(1, 0, ME), (2, 0, ME), (3, 1, ME), (4, 2, ME)];
        let mut v = view(ME, Step::Untap, &ids);
        in_hand(&mut v, &[1, 2, 3, 4]);
        let four: Vec<ObjectId> = (1..=4).map(ObjectId).collect();
        assert_eq!(
            b.worst_cards(&four, 1, &v),
            vec![ObjectId(4)],
            "two lands: the six-drop goes"
        );
    }

    #[test]
    fn a_land_comes_before_a_spell() {
        let b = bot(&[Stats::default(), creature(2, 2)]);
        let v = view(ME, Step::PrecombatMain, &[(1, 0, ME), (2, 1, ME)]);
        let choice = priority(vec![
            Action::Cast {
                object: ObjectId(2),
            },
            Action::PlayLand {
                object: ObjectId(1),
            },
            Action::Pass,
        ]);
        assert!(matches!(
            b.decide(&choice, &v),
            Answer::Action(Action::PlayLand { .. })
        ));
    }

    #[test]
    fn pure_fog_is_held_for_a_threatening_attack_and_not_cast_twice() {
        let mut b = bot(&[Stats::default(), creature(3, 3)]);
        b.combat_prevention.insert((CardId(0), 0));
        let action = Action::Cast {
            object: ObjectId(1),
        };
        let choice = priority(vec![action.clone(), Action::Pass]);
        let mut v = view(ME, Step::PrecombatMain, &[(1, 0, ME), (2, 1, THEM)]);
        in_hand(&mut v, &[1]);
        assert!(matches!(b.decide(&choice, &v), Answer::Pass));
        v.active_player = THEM;
        v.step = Step::DeclareBlockers;
        assert!(matches!(b.decide(&choice, &v), Answer::Pass));
        let attacker = v.visible.get_mut(&ObjectId(2)).unwrap();
        attacker.attacking = true;
        attacker.attacking_target = Some(Target::Player(ME));
        assert!(matches!(b.decide(&choice, &v), Answer::Action(a) if a == action));
        v.prevent_combat_damage = true;
        assert!(matches!(b.decide(&choice, &v), Answer::Pass));
    }

    #[test]
    fn the_most_expensive_spell_is_cast() {
        let cheap = creature(1, 1);
        let dear = Stats {
            mana_value: 5,
            ..creature(5, 5)
        };
        let b = bot(&[cheap, dear]);
        let v = view(ME, Step::PrecombatMain, &[(1, 0, ME), (2, 1, ME)]);
        let choice = priority(vec![
            Action::Cast {
                object: ObjectId(1),
            },
            Action::Cast {
                object: ObjectId(2),
            },
            Action::Pass,
        ]);
        assert!(matches!(
            b.decide(&choice, &v),
            Answer::Action(Action::Cast {
                object: ObjectId(2)
            })
        ));
    }

    #[test]
    fn nothing_is_cast_on_the_opponents_turn_or_over_a_stack() {
        let b = bot(&[creature(2, 2)]);
        let choice = priority(vec![
            Action::Cast {
                object: ObjectId(1),
            },
            Action::Pass,
        ]);

        let theirs = view(THEM, Step::PrecombatMain, &[(1, 0, ME)]);
        assert!(matches!(b.decide(&choice, &theirs), Answer::Pass));

        let mut busy = view(ME, Step::PrecombatMain, &[(1, 0, ME)]);
        busy.stack.push(ObjectId(9));
        assert!(matches!(b.decide(&choice, &busy), Answer::Pass));
    }

    #[test]
    fn it_attacks_when_nothing_can_block_profitably() {
        // My 3/3 against their 2/2: blocking would only lose them the 2/2.
        let b = bot(&[creature(3, 3), creature(2, 2)]);
        let v = view(ME, Step::DeclareAttackers, &[(1, 0, ME), (2, 1, THEM)]);
        assert_eq!(b.attackers(&[ObjectId(1)], &v), vec![ObjectId(1)]);
    }

    #[test]
    fn it_holds_back_a_creature_that_would_die_for_nothing() {
        // My 2/2 against their 3/3, which would block and eat it.
        let b = bot(&[creature(2, 2), creature(3, 3)]);
        let v = view(ME, Step::DeclareAttackers, &[(1, 0, ME), (2, 1, THEM)]);
        assert!(b.attackers(&[ObjectId(1)], &v).is_empty());
    }

    #[test]
    fn a_flyer_ignores_ground_blockers() {
        let flyer = Stats {
            flying: true,
            ..creature(2, 2)
        };
        let b = bot(&[flyer, creature(5, 5)]);
        let v = view(ME, Step::DeclareAttackers, &[(1, 0, ME), (2, 1, THEM)]);
        assert_eq!(b.attackers(&[ObjectId(1)], &v), vec![ObjectId(1)]);
    }

    #[test]
    fn first_strike_wins_a_fight_it_would_otherwise_trade() {
        let striker = Stats {
            first_strike: true,
            ..creature(2, 2)
        };
        assert_eq!(fight(&striker, &creature(2, 2)), (true, false));
        assert_eq!(fight(&creature(2, 2), &creature(2, 2)), (true, true));
    }

    #[test]
    fn double_strike_accounts_for_both_hits_in_creature_fights() {
        let striker = Stats {
            double_strike: true,
            ..creature(2, 5)
        };
        assert_eq!(fight(&striker, &creature(3, 4)), (true, false));
        assert_eq!(fight(&creature(3, 4), &striker), (false, true));
        assert_eq!(fight(&striker, &creature(5, 4)), (true, true));
        let first = Stats {
            first_strike: true,
            ..creature(3, 4)
        };
        assert_eq!(fight(&striker, &first), (true, false));
        let lethal_first = Stats {
            first_strike: true,
            ..creature(5, 4)
        };
        assert_eq!(fight(&striker, &lethal_first), (false, true));
        let both = Stats {
            double_strike: true,
            ..creature(3, 4)
        };
        assert_eq!(fight(&striker, &both), (true, true));
    }

    #[test]
    fn it_takes_a_safe_double_strike_block() {
        let b = bot(&[
            Stats {
                double_strike: true,
                ..creature(2, 5)
            },
            creature(3, 4),
        ]);
        let v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        assert_eq!(
            b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v),
            vec![(ObjectId(1), ObjectId(2))]
        );
    }

    #[test]
    fn double_strike_can_make_a_lethal_attack_worth_losing_creatures() {
        let b = bot(&[
            Stats {
                double_strike: true,
                ..creature(2, 1)
            },
            creature(5, 5),
        ]);
        let mut v = view(
            ME,
            Step::DeclareAttackers,
            &[(1, 0, ME), (2, 0, ME), (3, 0, ME), (4, 1, THEM)],
        );
        v.players.get_mut(&THEM).unwrap().life = 8;
        assert_eq!(
            b.attackers(&[ObjectId(1), ObjectId(2), ObjectId(3)], &v),
            vec![ObjectId(1), ObjectId(2), ObjectId(3)]
        );
    }

    #[test]
    fn it_blocks_where_the_blocker_kills_and_survives() {
        let b = bot(&[creature(3, 3), creature(2, 2)]);
        // Their 2/2 attacks; my 3/3 can block it.
        let v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        let blocks = b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v);
        assert_eq!(blocks, vec![(ObjectId(1), ObjectId(2))]);
    }

    #[test]
    fn it_does_not_throw_away_a_creature_when_not_in_danger() {
        let b = bot(&[creature(1, 1), creature(4, 4)]);
        let v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        assert!(b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v).is_empty());
    }

    #[test]
    fn it_chump_blocks_when_the_damage_would_be_lethal() {
        let b = bot(&[creature(1, 1), creature(4, 4)]);
        let mut v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        v.players.get_mut(&ME).unwrap().life = 4;
        assert_eq!(
            b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v),
            vec![(ObjectId(1), ObjectId(2))]
        );
    }

    #[test]
    fn a_menace_attacker_is_never_blocked_alone() {
        let menace = Stats {
            menace: true,
            ..creature(2, 2)
        };
        let b = bot(&[creature(3, 3), menace]);
        let v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        assert!(b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v).is_empty());
    }

    #[test]
    fn it_never_attacks_or_blocks_alone_with_a_creature_that_cant() {
        let loner = Stats {
            cant_attack_alone: true,
            cant_block_alone: true,
            ..creature(3, 3)
        };
        let b = bot(&[loner, creature(2, 2)]);
        let v = view(ME, Step::DeclareAttackers, &[(1, 0, ME), (2, 1, THEM)]);
        let attack = Choice {
            id: 1,
            who: ME,
            kind: ChoiceKind::DeclareAttackers {
                eligible: vec![ObjectId(1)],
                defenders: vec![Target::Player(THEM)],
            },
            because: "test".into(),
            default: Some(Answer::Objects(Vec::new())),
            undo: false,
        };
        assert!(
            matches!(b.decide(&attack, &v), Answer::Attackers(a) if a.is_empty()),
            "a lone 3/3 would attack, but this one can't"
        );
        // Their 2/2 attacks; my 3/3 would block it, but not alone.
        let v = view(THEM, Step::DeclareBlockers, &[(1, 0, ME), (2, 1, THEM)]);
        assert!(b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v).is_empty());
    }

    #[test]
    fn it_blocks_a_creature_needing_three_blockers_with_three_or_none() {
        let giant = Stats {
            min_blockers: 3,
            ..creature(9, 9)
        };
        let b = bot(&[creature(1, 1), giant]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 0, ME), (3, 0, ME), (9, 1, THEM)],
        );
        attacking(&mut v, 9, Target::Player(ME));
        v.players.get_mut(&ME).unwrap().life = 5;
        let eligible: Vec<_> = [1, 2, 3].map(|i| (ObjectId(i), vec![ObjectId(9)])).to_vec();
        assert_eq!(
            b.blocks(&eligible, &v).len(),
            3,
            "chump with all three to live"
        );
        assert!(
            b.blocks(&eligible[..2], &v).is_empty(),
            "two can't block it"
        );
    }

    fn attacking(view: &mut PlayerView, attacker: u32, destination: Target) {
        let o = view.visible.get_mut(&ObjectId(attacker)).unwrap();
        o.attacking = true;
        o.attacking_target = Some(destination);
    }

    fn loyalty(view: &mut PlayerView, id: u32, amount: i32) {
        view.visible
            .get_mut(&ObjectId(id))
            .unwrap()
            .counters
            .insert(CounterKind::Loyalty, amount);
    }

    #[test]
    fn it_uses_an_offered_loyalty_ability_and_prefers_building_loyalty() {
        let mut b = bot(&[Stats::default()]);
        for (id, delta) in [(0, -2), (1, 1), (2, 0)] {
            b.loyalty
                .insert((CardId(0), 0, AbilityId(id)), LoyaltyAbility { delta });
        }
        let v = view(ME, Step::PrecombatMain, &[(1, 0, ME)]);
        let action = |id| Action::ActivateAbility {
            source: ObjectId(1),
            ability: AbilityId(id),
        };
        let c = priority(vec![action(0), action(1), action(2), Action::Pass]);
        assert!(matches!(b.decide(&c, &v), Answer::Action(a) if a == action(1)));
        let c = priority(vec![action(0), Action::Pass]);
        assert!(matches!(b.decide(&c, &v), Answer::Action(a) if a == action(0)));
        assert!(matches!(
            b.decide(&priority(vec![Action::Pass]), &v),
            Answer::Pass
        ));
    }

    #[test]
    fn it_splits_attacks_between_planeswalkers_and_the_player() {
        let b = bot(&[creature(3, 3), Stats::default()]);
        let mut v = view(
            ME,
            Step::DeclareAttackers,
            &[
                (1, 0, ME),
                (2, 0, ME),
                (3, 0, ME),
                (4, 1, THEM),
                (5, 1, THEM),
            ],
        );
        loyalty(&mut v, 4, 2);
        loyalty(&mut v, 5, 3);
        let Answer::Attackers(plan) = b.attack_plan(
            &[ObjectId(1), ObjectId(2), ObjectId(3)],
            &[
                Target::Player(THEM),
                Target::Object(ObjectId(5)),
                Target::Object(ObjectId(4)),
            ],
            &v,
        ) else {
            panic!("attack plan")
        };
        assert_eq!(
            plan,
            vec![
                (ObjectId(1), Target::Object(ObjectId(4))),
                (ObjectId(2), Target::Object(ObjectId(5))),
                (ObjectId(3), Target::Player(THEM))
            ]
        );
    }

    #[test]
    fn lethal_damage_takes_priority_over_attacking_planeswalkers() {
        let b = bot(&[creature(3, 3), Stats::default()]);
        let mut v = view(
            ME,
            Step::DeclareAttackers,
            &[(1, 0, ME), (2, 0, ME), (3, 1, THEM)],
        );
        loyalty(&mut v, 3, 1);
        v.players.get_mut(&THEM).unwrap().life = 6;
        let Answer::Attackers(plan) = b.attack_plan(
            &[ObjectId(1), ObjectId(2)],
            &[Target::Player(THEM), Target::Object(ObjectId(3))],
            &v,
        ) else {
            panic!("attack plan")
        };
        assert!(plan.iter().all(|(_, t)| *t == Target::Player(THEM)));
    }

    #[test]
    fn it_uses_only_offered_attack_destinations_and_preserves_required_attackers() {
        let b = bot(&[creature(1, 1), creature(5, 5)]);
        let v = view(ME, Step::DeclareAttackers, &[(1, 0, ME), (2, 1, THEM)]);
        let c = Choice {
            id: 1,
            who: ME,
            kind: ChoiceKind::DeclareAttackers {
                eligible: vec![ObjectId(1)],
                defenders: vec![Target::Player(THEM)],
            },
            because: "must attack".into(),
            default: Some(Answer::Objects(vec![ObjectId(1)])),
            undo: false,
        };
        assert!(
            matches!(b.decide(&c, &v), Answer::Attackers(plan) if plan == vec![(ObjectId(1), Target::Player(THEM))])
        );
    }

    #[test]
    fn it_chumps_to_save_a_planeswalker() {
        let b = bot(&[creature(1, 1), creature(4, 4), Stats::default()]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 1, THEM), (3, 2, ME)],
        );
        loyalty(&mut v, 3, 4);
        attacking(&mut v, 2, Target::Object(ObjectId(3)));
        assert_eq!(
            b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v),
            vec![(ObjectId(1), ObjectId(2))]
        );
        loyalty(&mut v, 3, 5);
        assert!(b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v).is_empty());
    }

    #[test]
    fn it_saves_the_player_before_a_planeswalker_and_separates_incoming_damage() {
        let b = bot(&[creature(1, 1), creature(4, 4), Stats::default()]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 1, THEM), (3, 1, THEM), (4, 2, ME)],
        );
        loyalty(&mut v, 4, 4);
        attacking(&mut v, 2, Target::Object(ObjectId(4)));
        attacking(&mut v, 3, Target::Player(ME));
        v.players.get_mut(&ME).unwrap().life = 4;
        let eligible = [(ObjectId(1), vec![ObjectId(2), ObjectId(3)])];
        assert_eq!(b.blocks(&eligible, &v), vec![(ObjectId(1), ObjectId(3))]);
        // The player's four incoming damage is safe at five life; preserve the walker.
        v.players.get_mut(&ME).unwrap().life = 5;
        assert_eq!(b.blocks(&eligible, &v), vec![(ObjectId(1), ObjectId(2))]);
    }

    #[test]
    fn unblockable_damage_is_included_in_survival_decisions() {
        let b = bot(&[creature(1, 1), creature(3, 3)]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 1, THEM), (3, 1, THEM)],
        );
        attacking(&mut v, 2, Target::Player(ME));
        attacking(&mut v, 3, Target::Player(ME));
        v.players.get_mut(&ME).unwrap().life = 5;
        assert_eq!(
            b.blocks(&[(ObjectId(1), vec![ObjectId(2)])], &v),
            vec![(ObjectId(1), ObjectId(2))]
        );
    }

    #[test]
    fn menace_is_blocked_with_two_creatures_when_needed_to_survive() {
        let menace = Stats {
            menace: true,
            ..creature(4, 4)
        };
        let b = bot(&[creature(1, 1), menace]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 0, ME), (3, 1, THEM)],
        );
        attacking(&mut v, 3, Target::Player(ME));
        v.players.get_mut(&ME).unwrap().life = 4;
        assert_eq!(
            b.blocks(
                &[
                    (ObjectId(1), vec![ObjectId(3)]),
                    (ObjectId(2), vec![ObjectId(3)])
                ],
                &v
            ),
            vec![(ObjectId(1), ObjectId(3)), (ObjectId(2), ObjectId(3))]
        );
    }

    #[test]
    fn it_targets_an_enemy_with_a_harmful_loyalty_ability() {
        let mut b = bot(&[Stats::default(), creature(2, 2), creature(3, 3)]);
        b.targeting.insert(
            (CardId(0), 0, Some(AbilityId(2))),
            TargetPreference::Hostile,
        );
        let mut v = view(
            ME,
            Step::PrecombatMain,
            &[(1, 0, ME), (2, 1, ME), (3, 2, THEM)],
        );
        v.visible.get_mut(&ObjectId(1)).unwrap().ability = Some(AbilityId(2));
        v.stack.push(ObjectId(1));
        let c = Choice {
            id: 1,
            who: ME,
            kind: ChoiceKind::ChooseTargets {
                slots: vec![vec![
                    Target::Object(ObjectId(2)),
                    Target::Object(ObjectId(3)),
                ]],
            },
            because: "target".into(),
            default: Some(Answer::Targets(vec![vec![Target::Object(ObjectId(2))]])),
            undo: false,
        };
        assert!(
            matches!(b.decide(&c, &v), Answer::Targets(picks) if picks == vec![vec![Target::Object(ObjectId(3))]])
        );
    }

    #[test]
    fn target_heuristics_preserve_required_target_counts() {
        let mut b = bot(&[Stats::default(), creature(2, 2)]);
        b.targeting
            .insert((CardId(0), 0, None), TargetPreference::Hostile);
        let mut v = view(
            ME,
            Step::PrecombatMain,
            &[(1, 0, ME), (2, 1, THEM), (3, 1, THEM)],
        );
        v.stack.push(ObjectId(1));
        let c = Choice {
            id: 1,
            who: ME,
            kind: ChoiceKind::ChooseTargets {
                slots: vec![vec![
                    Target::Object(ObjectId(2)),
                    Target::Object(ObjectId(3)),
                ]],
            },
            because: "two targets".into(),
            default: Some(Answer::Targets(vec![vec![
                Target::Object(ObjectId(2)),
                Target::Object(ObjectId(3)),
            ]])),
            undo: false,
        };
        assert!(
            matches!(b.decide(&c, &v), Answer::Targets(picks) if picks[0].len() == 2 && picks[0][0] != picks[0][1])
        );
    }

    #[test]
    fn trample_is_not_treated_as_fully_stopped_by_a_chump() {
        let trample = Stats {
            trample: true,
            ..creature(5, 5)
        };
        let b = bot(&[creature(1, 1), trample, creature(2, 2)]);
        let mut v = view(
            THEM,
            Step::DeclareBlockers,
            &[(1, 0, ME), (2, 0, ME), (3, 1, THEM), (4, 2, THEM)],
        );
        attacking(&mut v, 3, Target::Player(ME));
        attacking(&mut v, 4, Target::Player(ME));
        v.players.get_mut(&ME).unwrap().life = 5;
        let blocks = b.blocks(
            &[
                (ObjectId(1), vec![ObjectId(3), ObjectId(4)]),
                (ObjectId(2), vec![ObjectId(3), ObjectId(4)]),
            ],
            &v,
        );
        assert_eq!(
            blocks,
            vec![(ObjectId(1), ObjectId(3)), (ObjectId(2), ObjectId(4))]
        );
    }

    #[test]
    fn split_card_value_is_combined_in_hand_and_half_specific_on_stack() {
        let mut b = bot(&[Stats {
            mana_value: 2,
            ..Default::default()
        }]);
        b.layouts.insert(CardId(0), mtg_ir::Layout::Split);
        b.stats.insert(
            (CardId(0), 1),
            Stats {
                mana_value: 3,
                ..Default::default()
            },
        );
        let mut v = view(ME, Step::PrecombatMain, &[(1, 0, ME)]);
        v.visible.get_mut(&ObjectId(1)).unwrap().zone = ZoneRef::of(mtg_core::Zone::Hand, ME);
        assert_eq!(b.stats(&v, ObjectId(1)).unwrap().mana_value, 5);
        v.visible.get_mut(&ObjectId(1)).unwrap().zone = ZoneRef::shared(mtg_core::Zone::Stack);
        assert_eq!(b.stats(&v, ObjectId(1)).unwrap().mana_value, 2);
        v.visible.get_mut(&ObjectId(1)).unwrap().face = 1;
        assert_eq!(b.stats(&v, ObjectId(1)).unwrap().mana_value, 3);
    }
}
