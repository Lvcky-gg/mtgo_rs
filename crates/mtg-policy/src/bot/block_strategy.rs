//! Block decision logic for the bot.
//!
//! The bot blocks to minimize incoming damage, prioritizing defense of the player over planeswalkers.
//!
//! ## Block Priority
//!
//! The [`Bot::blocks`] method applies two phases:
//!
//! ### Phase 1: Good Blocks First
//!
//! Kill attackers when possible: find blockers that would destroy the attacker without dying.
//! This prioritizes profitable trades (1-for-1 kill, 2-for-1 kill, etc.).
//!
//! ### Phase 2: Chump Blocks
//!
//! Block remaining attackers aimed at the player or planeswalkers to reduce incoming damage:
//! 1. Prioritize saving the **player** first (20 life).
//! 2. Then save threatened **planeswalkers** (in ascending loyalty order).
//! 3. Use only the minimum blockers needed (accounting for menace, flying, etc.).
//!
//! ## Combat Nuances
//!
//! - **Menace**: Requires 2 blockers (instead of 1); handled via `min_blockers`.
//! - **Trample**: If blocked, excess damage bypasses to the defending player/planeswalker.
//! - **Deathtouch**: Treats any creature as having 1 toughness for the purpose of lethal.
//! - **Double strike**: Damage is counted twice when evaluating lethal and prevention.
//!
//! Example:
//! - A 5/3 creature with menace aimed at the player requires 2 blockers.
//! - If the player has 10 life and can block with [2/2, 3/3, 1/1], the bot blocks with [3/3, 1/1]
//!   and lets the 2/2 stay back (more likely needed elsewhere).

use mtg_core::{CounterKind, ObjectId, Target};
use mtg_engine::PlayerView;

use super::{Bot, Stats};

impl Bot {
    /// Safe kills first, then chump to prevent lethal damage to the player or a planeswalker.
    ///
    /// The player gets first use of scarce blockers. This method assesses incoming damage,
    /// prioritizes blocks that kill the attacker, and uses chump blocks to save life or
    /// planeswalker loyalty when necessary.
    pub fn blocks(
        &self,
        eligible: &[(ObjectId, Vec<ObjectId>)],
        view: &PlayerView,
    ) -> Vec<(ObjectId, ObjectId)> {
        let mut blocks: Vec<(ObjectId, ObjectId)> = Vec::new();
        // Include un-blockable attackers when measuring danger. The eligibility
        // list alone omits them and would underestimate lethal incoming damage.
        let mut ids: std::collections::BTreeSet<ObjectId> = eligible
            .iter()
            .flat_map(|(_, atts)| atts.iter().copied())
            .collect();
        ids.extend(view.visible.values().filter(|o| o.attacking).map(|o| o.id));
        let mut attackers: Vec<(ObjectId, Stats)> = ids
            .into_iter()
            .filter_map(|id| Some((id, self.stats(view, id)?)))
            .collect();
        attackers.sort_by_key(|(id, s)| (std::cmp::Reverse(s.power), *id));

        let can_block = |blocker: ObjectId, attacker: ObjectId| {
            eligible
                .iter()
                .any(|(b, atts)| *b == blocker && atts.contains(&attacker))
        };
        let used = |blocks: &[(ObjectId, ObjectId)], b: ObjectId| {
            blocks.iter().any(|(x, _)| *x == b)
        };
        let blocked = |blocks: &[(ObjectId, ObjectId)], a: ObjectId| {
            blocks.iter().any(|(_, x)| *x == a)
        };

        // Good blocks first.
        let needed = |a: &Stats| usize::from(a.min_blockers).max(if a.menace { 2 } else { 1 });
        for (attacker, a) in &attackers {
            if needed(a) > 1 {
                continue;
            }
            let good = eligible.iter().find_map(|(blocker, _)| {
                if used(&blocks, *blocker) || !can_block(*blocker, *attacker) {
                    return None;
                }
                let b = self.stats(view, *blocker)?;
                let (blocker_kills, attacker_kills) = super::fight(&b, a);
                (blocker_kills && !attacker_kills).then_some(*blocker)
            });
            if let Some(blocker) = good {
                blocks.push((blocker, *attacker));
            }
        }

        let destination = |attacker: ObjectId| {
            view.visible
                .get(&attacker)
                .and_then(|o| o.attacking_target)
                .unwrap_or(Target::Player(view.viewer))
        };
        let incoming = |target: Target, blocks: &[(ObjectId, ObjectId)]| -> i32 {
            attackers
                .iter()
                .filter(|(a, _)| destination(*a) == target)
                .map(|(id, a)| {
                    let damage = a
                        .power
                        .max(0)
                        .saturating_mul(if a.double_strike { 2 } else { 1 });
                    if !blocked(blocks, *id) {
                        return damage;
                    }
                    if !a.trample {
                        return 0;
                    }
                    let absorbed: i32 = blocks
                        .iter()
                        .filter(|(_, attacker)| attacker == id)
                        .filter_map(|(b, _)| self.stats(view, *b))
                        .map(|b| if a.deathtouch { 1 } else { b.toughness.max(0) })
                        .sum();
                    (damage - absorbed).max(0)
                })
                .sum()
        };
        // Save the player first, then planeswalkers. Damage aimed at a walker
        // never contributes to the estimate of damage to its controller.
        let mut threatened = vec![(
            Target::Player(view.viewer),
            view.players.get(&view.viewer).map_or(0, |p| p.life),
        )];
        let walker_ids: std::collections::BTreeSet<_> = attackers
            .iter()
            .filter_map(|(a, _)| match destination(*a) {
                Target::Object(o) => Some(o),
                _ => None,
            })
            .collect();
        for id in walker_ids {
            if let Some(o) = view
                .visible
                .get(&id)
                .filter(|o| o.controller == view.viewer)
                && let Some(loyalty) = o.counters.get(&CounterKind::Loyalty)
            {
                threatened.push((Target::Object(id), *loyalty));
            }
        }
        for (target, health) in threatened {
            for (attacker, stats) in &attackers {
                if incoming(target, &blocks) < health {
                    break;
                }
                if destination(*attacker) != target || blocked(&blocks, *attacker) {
                    continue;
                }
                let needed = needed(stats);
                let available: Vec<_> = eligible
                    .iter()
                    .map(|(b, _)| *b)
                    .filter(|b| !used(&blocks, *b) && can_block(*b, *attacker))
                    .take(needed)
                    .collect();
                if available.len() == needed {
                    blocks.extend(available.into_iter().map(|b| (b, *attacker)));
                }
            }
        }
        if let [(only, _)] = blocks[..] {
            if self.stats(view, only).is_some_and(|s| s.cant_block_alone) {
                blocks.clear();
            }
        }
        blocks
    }
}
