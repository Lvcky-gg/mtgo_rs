//! Attack decision logic for the bot.
//!
//! The bot attacks with creatures that maximize damage without losing value.
//! If lethal damage is available, it attacks with everything (ignoring bad trades).
//!
//! ## Attack Selection
//!
//! The [`Bot::attackers`] method filters eligible creatures:
//! 1. **Lethal path**: If total damage >= opponent's life (even if blockers kill one big attacker), send all.
//! 2. **Safe path**: Send only creatures that can't be killed for free (no blocker can 1-for-1 them).
//!
//! ## Attack Assignment
//!
//! The [`Bot::attack_plan`] method distributes attackers to targets:
//! 1. **Calculate lethal**: Is the total damage minus preventable damage >= opponent life?
//! 2. **If lethal**: Send all attackers to the player.
//! 3. **If not lethal**:
//!    a. Assign attackers to planeswalkers in ascending loyalty order.
//!    b. Assign remaining attackers to the player.
//!
//! This pressure strategy ensures planeswalkers die before the player does (when possible).

use mtg_core::{ObjectId, PlayerId, Target};
use mtg_engine::PlayerView;

use super::{Bot, Stats};

impl Bot {
    /// Which creatures should attack, using optimal prioritization.
    ///
    /// Attack with everything when that is lethal even if every blocker stops one of the biggest
    /// attackers; otherwise with each creature no blocker can kill for free.
    ///
    /// This method filters the eligible attackers using the bot's combat assessment heuristics,
    /// avoiding trades that lose value while pursuing lethal damage opportunities.
    pub fn attackers(&self, eligible: &[ObjectId], view: &PlayerView) -> Vec<ObjectId> {
        let blockers = self.potential_blockers(view);
        let attackers: Vec<(ObjectId, Stats)> = eligible
            .iter()
            .filter_map(|id| Some((*id, self.stats(view, *id)?)))
            .collect();

        let their_life = opponent(view).map_or(i32::MAX, |p| view.players[&p].life);
        let total: i64 = attackers.iter().map(|(_, s)| s.combat_damage()).sum();
        if blockers.is_empty()
            || total >= i64::from(their_life) + blockers.len() as i64 * max_damage(&attackers)
        {
            return attackers
                .iter()
                .filter(|(_, s)| s.power > 0)
                .map(|(id, _)| *id)
                .collect();
        }

        attackers
            .iter()
            .filter(|(_, a)| a.power > 0)
            .filter(|(_, a)| {
                !blockers.iter().any(|b| {
                    if !a.blockable_by(b) {
                        return false;
                    }
                    let (attacker_kills, blocker_kills) = super::fight(a, b);
                    blocker_kills && !attacker_kills
                })
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Assign attackers to defenders, prioritizing lethal damage to the player.
    ///
    /// Takes lethal player damage first; otherwise pressures the lowest-loyalty
    /// planeswalker and sends spare attackers to the next destination.
    ///
    /// This method implements the bot's attack planning heuristics, targeting the player
    /// when damage is lethal, otherwise distributing attackers to finish weak planeswalkers.
    pub fn attack_plan(
        &self,
        attackers: &[ObjectId],
        defenders: &[Target],
        view: &PlayerView,
    ) -> mtg_engine::choice::Answer {
        use mtg_engine::choice::Answer;

        let Some(player) = defenders
            .iter()
            .find(|t| matches!(t, Target::Player(_)))
            .copied()
        else {
            return match defenders.first() {
                Some(target) => {
                    Answer::Attackers(attackers.iter().map(|a| (*a, *target)).collect())
                }
                None => Answer::Objects(attackers.to_vec()),
            };
        };
        let mut ranked: Vec<_> = attackers
            .iter()
            .map(|id| {
                let stats = self.stats(view, *id).unwrap_or_default();
                (*id, stats.combat_damage(), stats)
            })
            .collect();
        ranked.sort_by_key(|(id, damage, _)| (std::cmp::Reverse(*damage), *id));
        let blockers = self.potential_blockers(view);
        let mut stoppable: Vec<i64> = ranked
            .iter()
            .filter(|(_, _, a)| blockers.iter().any(|b| a.blockable_by(b)))
            .map(|(_, damage, _)| *damage)
            .collect();
        stoppable.sort_by_key(|damage| std::cmp::Reverse(*damage));
        let total: i64 = ranked.iter().map(|(_, damage, _)| *damage).sum();
        let prevented: i64 = stoppable.iter().take(blockers.len()).sum();
        let life = match player {
            Target::Player(p) => view.players.get(&p).map_or(i32::MAX, |p| p.life),
            _ => i32::MAX,
        };
        if total - prevented >= i64::from(life) {
            return Answer::Attackers(attackers.iter().map(|a| (*a, player)).collect());
        }
        let mut walkers: Vec<_> = defenders
            .iter()
            .filter_map(|target| {
                let Target::Object(id) = target else {
                    return None;
                };
                let loyalty = view
                    .visible
                    .get(id)?
                    .counters
                    .get(&mtg_core::CounterKind::Loyalty)
                    .copied()?;
                (loyalty > 0).then_some((*id, loyalty))
            })
            .collect();
        walkers.sort_by_key(|(id, loyalty)| (*loyalty, *id));
        let mut assigned = Vec::new();
        let mut remaining = ranked.into_iter();
        for (walker, loyalty) in walkers {
            let mut damage = 0;
            while damage < i64::from(loyalty) {
                let Some((attacker, power, _)) = remaining.next() else {
                    break;
                };
                assigned.push((attacker, Target::Object(walker)));
                damage += power;
            }
        }
        assigned.extend(remaining.map(|(attacker, _, _)| (attacker, player)));
        Answer::Attackers(assigned)
    }

    /// The opponent's untapped creatures: who could block.
    pub(super) fn potential_blockers(&self, view: &PlayerView) -> Vec<Stats> {
        view.visible
            .values()
            .filter(|o| o.controller != view.viewer && !o.tapped)
            .filter(|o| o.zone.zone == mtg_core::Zone::Battlefield)
            .filter_map(|o| self.stats(view, o.id))
            .filter(|s| s.creature)
            .collect()
    }
}

fn opponent(view: &PlayerView) -> Option<PlayerId> {
    view.players.keys().copied().find(|p| *p != view.viewer)
}

fn max_damage(creatures: &[(ObjectId, Stats)]) -> i64 {
    creatures
        .iter()
        .map(|(_, s)| s.combat_damage())
        .max()
        .unwrap_or(0)
}
