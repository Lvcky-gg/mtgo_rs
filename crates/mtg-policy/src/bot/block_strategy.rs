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
        let destination = |attacker: ObjectId| {
            view.visible
                .get(&attacker)
                .and_then(|o| o.attacking_target)
                .unwrap_or(Target::Player(view.viewer))
        };
        attackers.sort_by_key(|(id, s)| {
            (
                destination(*id) != Target::Player(view.viewer),
                std::cmp::Reverse(s.power),
                *id,
            )
        });

        let can_block = |blocker: ObjectId, attacker: ObjectId| {
            eligible
                .iter()
                .any(|(b, atts)| *b == blocker && atts.contains(&attacker))
        };
        let used =
            |blocks: &[(ObjectId, ObjectId)], b: ObjectId| blocks.iter().any(|(x, _)| *x == b);
        let blocked =
            |blocks: &[(ObjectId, ObjectId)], a: ObjectId| blocks.iter().any(|(_, x)| *x == a);

        // Good blocks first.
        let needed = |a: &Stats| usize::from(a.min_blockers).max(if a.menace { 2 } else { 1 });
        for (attacker, a) in &attackers {
            if needed(a) > 1 {
                continue;
            }
            let good = eligible
                .iter()
                .filter_map(|(blocker, atts)| {
                    if used(&blocks, *blocker) || !can_block(*blocker, *attacker) {
                        return None;
                    }
                    let b = self.stats(view, *blocker)?;
                    let (blocker_kills, attacker_kills) = super::fight(&b, a);
                    // Preserve flexible blockers (for example, our only flyer) for
                    // attackers that the more restricted blockers cannot handle.
                    (blocker_kills && !attacker_kills).then_some((atts.len(), *blocker))
                })
                .min();
            if let Some((_, blocker)) = good {
                blocks.push((blocker, *attacker));
            }
        }

        let contributes = |stats: &Stats, poison: bool| {
            if poison {
                stats.infect || stats.toxic > 0
            } else {
                !stats.infect
            }
        };
        let incoming = |target: Target, poison: bool, blocks: &[(ObjectId, ObjectId)]| -> i64 {
            attackers
                .iter()
                .filter(|(a, stats)| {
                    destination(*a) == target
                        && (!matches!(target, Target::Player(_)) || contributes(stats, poison))
                })
                .map(|(id, a)| {
                    let damage = a.combat_damage();
                    let damage = if !blocked(blocks, *id) {
                        damage
                    } else if !a.trample {
                        0
                    } else {
                        let absorbed: i64 = blocks
                            .iter()
                            .filter(|(_, attacker)| attacker == id)
                            .filter_map(|(b, _)| self.stats(view, *b))
                            .map(|b| {
                                if a.deathtouch {
                                    1
                                } else {
                                    i64::from(b.toughness.max(0))
                                }
                            })
                            .sum();
                        (damage - absorbed).max(0)
                    };
                    if poison {
                        // Toxic applies only when combat damage reaches the player.
                        // Infect replaces life damage; toxic adds poison per hit.
                        if damage == 0 {
                            return 0;
                        }
                        let hits = if a.double_strike { 2 } else { 1 };
                        (if a.infect { damage } else { 0 }) + i64::from(a.toxic) * hits
                    } else {
                        damage
                    }
                })
                .sum()
        };
        // Save the player from poison, then life loss, then planeswalkers.
        // Infect damage to players does not reduce life. Damage aimed at a walker
        // never contributes to the estimate of damage to its controller.
        let player = view.players.get(&view.viewer);
        let mut threatened = vec![
            (
                Target::Player(view.viewer),
                player.map_or(10, |p| 10u32.saturating_sub(p.poison)) as i64,
                true,
            ),
            (
                Target::Player(view.viewer),
                player.map_or(0, |p| i64::from(p.life)),
                false,
            ),
        ];
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
                threatened.push((Target::Object(id), i64::from(*loyalty), false));
            }
        }
        for (target, health, poison) in threatened {
            for (attacker, stats) in &attackers {
                if incoming(target, poison, &blocks) < health {
                    break;
                }
                if destination(*attacker) != target
                    || blocked(&blocks, *attacker)
                    || (matches!(target, Target::Player(_)) && !contributes(stats, poison))
                {
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
        if let [(only, _)] = blocks[..]
            && self.stats(view, only).is_some_and(|s| s.cant_block_alone)
        {
            blocks.clear();
        }
        blocks
    }
}
