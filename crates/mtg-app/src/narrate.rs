//! The game log, told in words: what changed between one view of the game and the next.
//!
//! The window never sees the engine's event log — a raw event names cards the viewer may not be
//! entitled to see, which is why only projected views cross to the UI (see `mtg_session`). So
//! the log is reconstructed from views instead: whatever differs between two consecutive views
//! is what happened, and every name in it is one the viewer could see anyway.
//!
//! Pure, so the wording is tested without a window.

use std::collections::BTreeSet;

use mtg_core::{ObjectId, PlayerId, Zone};
use mtg_engine::{PlayerView, view::ObjectView};

use crate::cards_text::CardTexts;

/// What happened between `before` and `after`, oldest first, in plain sentences.
pub fn changes(before: &PlayerView, after: &PlayerView, texts: &CardTexts) -> Vec<String> {
    let mut out = Vec::new();
    if after.prevent_combat_damage != before.prevent_combat_damage {
        out.push(if after.prevent_combat_damage {
            "All combat damage is prevented this turn".into()
        } else {
            "Combat damage prevention ended".into()
        });
    }
    let who = |p: PlayerId| crate::format::player_name(after, p);
    let name = |o: &ObjectView| {
        texts
            .object_text(o)
            .map_or_else(|| "(hidden)".into(), |text| text.name.clone())
    };
    let target_name = |target: &mtg_core::Target, view: &PlayerView| match target {
        mtg_core::Target::Player(player) => who(*player),
        mtg_core::Target::Object(object) => view
            .visible
            .get(object)
            .map_or_else(|| "(hidden)".into(), name),
    };
    for target in &after.prevent_damage_to {
        if !before.prevent_damage_to.contains(target) {
            out.push(format!(
                "All damage to {} is prevented this turn",
                target_name(target, after)
            ));
        }
    }
    for target in &before.prevent_damage_to {
        if !after.prevent_damage_to.contains(target) {
            out.push(format!(
                "Damage prevention for {} ended",
                target_name(target, before)
            ));
        }
    }

    if after.turn != before.turn || after.active_player != before.active_player {
        let whose = if after.active_player == after.viewer {
            "Your turn".to_string()
        } else {
            format!("{}'s turn", who(after.active_player))
        };
        out.push(format!("— Turn {}: {whose} —", after.turn));
    }

    let ids_in = |v: &PlayerView, zone: Zone| -> BTreeSet<ObjectId> {
        v.visible
            .values()
            .filter(|o| o.zone.zone == zone)
            .map(|o| o.id)
            .collect()
    };
    let (bf_before, bf_after) = (
        ids_in(before, Zone::Battlefield),
        ids_in(after, Zone::Battlefield),
    );

    // New on the stack: a spell cast or an ability triggered or activated.
    for id in &after.stack {
        if before.stack.contains(id) {
            continue;
        }
        let Some(o) = after.visible.get(id) else {
            continue;
        };
        out.push(if o.is_ability {
            format!("{}'s ability goes on the stack", name(o))
        } else {
            format!("{} cast {}", who(o.controller), name(o))
        });
    }

    // Left the stack without becoming a permanent: resolved or countered.
    for id in &before.stack {
        if after.stack.contains(id) {
            continue;
        }
        let Some(o) = before.visible.get(id) else {
            continue;
        };
        let became_permanent = !o.is_ability
            && o.card.is_some()
            && after.visible.values().any(|n| {
                n.zone.zone == Zone::Battlefield && !bf_before.contains(&n.id) && n.card == o.card
            });
        if !became_permanent {
            out.push(format!("{} left the stack", name(o)));
        }
    }

    // New on the battlefield.
    for id in bf_after.difference(&bf_before) {
        let o = &after.visible[id];
        out.push(format!(
            "{} entered the battlefield ({})",
            name(o),
            who(o.controller).to_lowercase()
        ));
    }

    // Gone from the battlefield: died, or went somewhere else.
    for id in bf_before.difference(&bf_after) {
        let o = &before.visible[id];
        let card = o.card;
        let to_graveyard = after.players.values().any(|p| {
            let previous = before.players.get(&p.id);
            card.is_some()
                && p.graveyard.iter().any(|id| {
                    !previous.is_some_and(|player| player.graveyard.contains(id))
                        && after
                            .visible
                            .get(id)
                            .is_some_and(|object| object.card == card)
                })
        });
        let creature = texts.object_text(o).is_some_and(|text| text.is_creature);
        out.push(match (to_graveyard, creature) {
            (true, true) => format!("{} died", name(o)),
            (true, false) => format!("{} was put into a graveyard", name(o)),
            (false, _) => format!("{} left the battlefield", name(o)),
        });
    }

    // Persistent battlefield objects can change controller, attachment, or public counters.
    for id in bf_before.intersection(&bf_after) {
        let old = &before.visible[id];
        let current = &after.visible[id];
        if old.controller != current.controller {
            out.push(format!(
                "{} gained control of {}",
                who(current.controller),
                name(current)
            ));
        }
        if old.attached_to != current.attached_to {
            out.push(if let Some(target) = current.attached_to {
                format!(
                    "{} became attached to {}",
                    name(current),
                    target_name(&mtg_core::Target::Object(target), after)
                )
            } else {
                format!("{} became unattached", name(current))
            });
        }
        let kinds: BTreeSet<_> = old
            .counters
            .keys()
            .chain(current.counters.keys())
            .copied()
            .collect();
        for kind in kinds {
            let previous = old.counters.get(&kind).copied().unwrap_or(0);
            let total = current.counters.get(&kind).copied().unwrap_or(0);
            let delta = i64::from(total) - i64::from(previous);
            if delta != 0 {
                let verb = if delta > 0 { "gained" } else { "lost" };
                let noun = if delta.abs() == 1 {
                    "counter"
                } else {
                    "counters"
                };
                out.push(format!(
                    "{} {verb} {} {} {noun} ({total})",
                    name(current),
                    delta.abs(),
                    crate::format::counter_name(kind)
                ));
            }
        }
    }

    // Attacks and blocks, once each as they are declared.
    let attackers: Vec<&ObjectView> = after
        .visible
        .values()
        .filter(|o| o.attacking && !before.visible.get(&o.id).is_some_and(|b| b.attacking))
        .collect();
    if let Some(first) = attackers.first() {
        let names: Vec<String> = attackers
            .iter()
            .map(|o| {
                if let Some(target) = o.attacking_target {
                    let defender = match target {
                        mtg_core::Target::Player(player) if player == after.viewer => "you".into(),
                        _ => target_name(&target, after),
                    };
                    format!("{} at {defender}", name(o))
                } else {
                    name(o)
                }
            })
            .collect();
        out.push(format!(
            "{} attacked with {}",
            who(first.controller),
            names.join(", ")
        ));
    }
    for o in after.visible.values() {
        let Some(attacker) = o.blocking else { continue };
        if before
            .visible
            .get(&o.id)
            .is_some_and(|b| b.blocking == Some(attacker))
        {
            continue;
        }
        let blocked = after
            .visible
            .get(&attacker)
            .map_or_else(|| "an attacker".into(), name);
        out.push(format!("{} blocked {blocked}", name(o)));
    }

    // Newly visible hand cards: views do not distinguish draws, tutors, and returns.
    let hand = |v: &PlayerView| -> BTreeSet<ObjectId> {
        v.visible
            .values()
            .filter(|o| o.zone.zone == Zone::Hand && o.zone.player == Some(v.viewer))
            .map(|o| o.id)
            .collect()
    };
    let added: Vec<String> = hand(after)
        .difference(&hand(before))
        .map(|id| name(&after.visible[id]))
        .collect();
    if !added.is_empty() {
        out.push(format!("Your hand gained {}", added.join(", ")));
    }

    // Public player totals.
    for p in after.players.values() {
        let Some(old) = before.players.get(&p.id) else {
            continue;
        };
        for (label, previous, current) in [
            ("energy", old.energy, p.energy),
            ("poison", old.poison, p.poison),
        ] {
            let delta = i64::from(current) - i64::from(previous);
            if delta != 0 {
                let verb = match (delta > 0, p.id == after.viewer) {
                    (true, true) => "gain",
                    (true, false) => "gains",
                    (false, true) => "lose",
                    (false, false) => "loses",
                };
                out.push(format!(
                    "{} {verb} {} {label} ({current})",
                    who(p.id),
                    delta.abs()
                ));
            }
        }
        let delta = i64::from(p.life) - i64::from(old.life);
        if delta != 0 {
            let verb = match (p.id == after.viewer, delta > 0) {
                (true, true) => "gain",
                (true, false) => "lose",
                (false, true) => "gains",
                (false, false) => "loses",
            };
            out.push(format!(
                "{} {verb} {} life ({})",
                who(p.id),
                delta.abs(),
                p.life
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{CardId, ZoneRef};
    use mtg_engine::view::PlayerSummary;
    use std::collections::BTreeMap;

    const ME: PlayerId = PlayerId(0);

    #[test]
    fn attacks_name_player_and_permanent_defenders() {
        let before = view(
            vec![
                obj(2, ZoneRef::shared(Zone::Battlefield), THEM, 1),
                obj(3, ZoneRef::shared(Zone::Battlefield), ME, 0),
            ],
            [20, 20],
        );
        for (target, defender) in [
            (mtg_core::Target::Player(ME), "you"),
            (mtg_core::Target::Player(THEM), "Opponent"),
            (mtg_core::Target::Object(ObjectId(3)), "Quiet Field"),
            (mtg_core::Target::Object(ObjectId(99)), "(hidden)"),
        ] {
            let mut after = before.clone();
            let attacker = after.visible.get_mut(&ObjectId(2)).unwrap();
            attacker.attacking = true;
            attacker.attacking_target = Some(target);
            assert_eq!(
                changes(&before, &after, &texts()),
                vec![format!("Opponent attacked with Stone Bear at {defender}")]
            );
            assert!(changes(&after, &after, &texts()).is_empty());
        }
    }

    #[test]
    fn changed_blocking_assignments_are_reported_once() {
        let mut blocker = obj(2, ZoneRef::shared(Zone::Battlefield), ME, 1);
        blocker.blocking = Some(ObjectId(3));
        let before = view(
            vec![
                blocker,
                obj(3, ZoneRef::shared(Zone::Battlefield), THEM, 1),
                obj(4, ZoneRef::shared(Zone::Battlefield), THEM, 0),
            ],
            [20, 20],
        );
        let mut after = before.clone();
        after.visible.get_mut(&ObjectId(2)).unwrap().blocking = Some(ObjectId(4));
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Stone Bear blocked Quiet Field"]
        );
        assert!(changes(&after, &after, &texts()).is_empty());
        after.visible.get_mut(&ObjectId(2)).unwrap().blocking = Some(ObjectId(99));
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Stone Bear blocked an attacker"]
        );
    }

    #[test]
    fn unrelated_hidden_arrivals_do_not_hide_stack_departures() {
        let mut spell = obj(2, ZoneRef::shared(Zone::Stack), ME, 1);
        spell.card = None;
        let mut permanent = obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1);
        permanent.card = None;
        assert_eq!(
            changes(
                &view(vec![spell], [20, 20]),
                &view(vec![permanent], [20, 20]),
                &texts()
            ),
            vec![
                "(hidden) left the stack",
                "(hidden) entered the battlefield (you)"
            ]
        );
    }

    #[test]
    fn an_ability_does_not_become_a_permanent_with_its_sources_card_id() {
        let mut ability = obj(2, ZoneRef::shared(Zone::Stack), ME, 1);
        ability.is_ability = true;
        let permanent = obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1);
        assert_eq!(
            changes(
                &view(vec![ability], [20, 20]),
                &view(vec![permanent], [20, 20]),
                &texts()
            ),
            vec![
                "Stone Bear left the stack",
                "Stone Bear entered the battlefield (you)"
            ]
        );
    }

    #[test]
    fn attachment_changes_are_reported() {
        let before = view(
            vec![
                obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1),
                obj(4, ZoneRef::shared(Zone::Battlefield), ME, 0),
            ],
            [20, 20],
        );
        let mut after = before.clone();
        after.visible.get_mut(&ObjectId(4)).unwrap().attached_to = Some(ObjectId(3));
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Quiet Field became attached to Stone Bear"]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["Quiet Field became unattached"]
        );
        assert!(changes(&after, &after, &texts()).is_empty());

        let mut moved = after.clone();
        moved.visible.get_mut(&ObjectId(4)).unwrap().attached_to = Some(ObjectId(99));
        assert_eq!(
            changes(&after, &moved, &texts()),
            vec!["Quiet Field became attached to (hidden)"]
        );
    }

    #[test]
    fn permanent_counter_additions_and_removals_are_reported() {
        let before = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        let mut after = before.clone();
        after
            .visible
            .get_mut(&ObjectId(3))
            .unwrap()
            .counters
            .insert(mtg_core::CounterKind::PlusOnePlusOne, 2);
        after
            .visible
            .get_mut(&ObjectId(3))
            .unwrap()
            .counters
            .insert(mtg_core::CounterKind::Stun, 1);
        assert_eq!(
            changes(&before, &after, &texts()),
            vec![
                "Stone Bear gained 2 +1/+1 counters (2)",
                "Stone Bear gained 1 stun counter (1)",
            ]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec![
                "Stone Bear lost 2 +1/+1 counters (0)",
                "Stone Bear lost 1 stun counter (0)",
            ]
        );
        assert!(changes(&after, &after, &texts()).is_empty());
    }

    #[test]
    fn a_spell_put_in_the_graveyard_is_not_claimed_to_have_resolved() {
        let before = view(vec![obj(3, ZoneRef::shared(Zone::Stack), ME, 1)], [20, 20]);
        let after = view(
            vec![obj(4, ZoneRef::of(Zone::Graveyard, ME), ME, 1)],
            [20, 20],
        );
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Stone Bear left the stack"]
        );
    }

    #[test]
    fn control_changes_are_reported_without_a_second_battlefield_entry() {
        let before = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), THEM, 1)],
            [20, 20],
        );
        let after = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["You gained control of Stone Bear"]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["Opponent gained control of Stone Bear"]
        );
    }

    #[test]
    fn extreme_life_changes_are_reported_without_overflow() {
        let before = view(vec![], [i32::MIN, 20]);
        let after = view(vec![], [i32::MAX, 20]);
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["You gain 4294967295 life (2147483647)"]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["You lose 4294967295 life (-2147483648)"]
        );
    }

    #[test]
    fn public_energy_and_poison_gains_and_losses_are_reported() {
        let mut before = view(vec![], [20, 20]);
        before.players.get_mut(&ME).unwrap().energy = 5;
        let mut after = before.clone();
        after.players.get_mut(&ME).unwrap().energy = 2;
        after.players.get_mut(&THEM).unwrap().poison = 3;
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["You lose 3 energy (2)", "Opponent gains 3 poison (3)",]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["You gain 3 energy (5)", "Opponent loses 3 poison (0)",]
        );
        assert!(changes(&after, &after, &texts()).is_empty());
    }
    const THEM: PlayerId = PlayerId(1);

    fn texts() -> CardTexts {
        use mtg_core::{CardType, ManaCost};
        use mtg_ir::CardFace;
        struct Two(Vec<CardFace>);
        impl mtg_ir::PrintedCards for Two {
            fn face(&self, c: CardId, _: u8) -> Option<&CardFace> {
                self.0.get(c.0 as usize)
            }
            fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
                None
            }
        }
        let face = |name: &str, t: CardType| CardFace {
            name: name.into(),
            mana_cost: ManaCost::FREE,
            card_types: vec![t],
            subtypes: Vec::new(),
            supertypes: Vec::new(),
            power: Some(2),
            toughness: Some(2),
            loyalty: None,
            abilities: Vec::new(),
            oracle_text: None,
            colors: None,
        };
        let cards = Two(vec![
            face("Quiet Field", CardType::Land),
            face("Stone Bear", CardType::Creature),
        ]);
        CardTexts::snapshot(&cards, [CardId(0), CardId(1)])
    }

    fn obj(id: u32, zone: ZoneRef, controller: PlayerId, card: u32) -> ObjectView {
        ObjectView {
            id: ObjectId(id),
            zone,
            controller,
            card: Some(CardId(card)),
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
        }
    }

    fn view(objects: Vec<ObjectView>, life: [i32; 2]) -> PlayerView {
        let summary = |id: PlayerId, life: i32, graveyard: Vec<ObjectId>| PlayerSummary {
            id,
            life,
            poison: 0,
            energy: 0,
            hand_size: objects
                .iter()
                .filter(|o| o.zone.zone == Zone::Hand && o.zone.player == Some(id))
                .count() as u32,
            library_size: 30,
            graveyard,
            mana: [0; 6],
            commander_damage: Default::default(),
        };
        let graveyard = |p: PlayerId| {
            objects
                .iter()
                .filter(|o| o.zone.zone == Zone::Graveyard && o.zone.player == Some(p))
                .map(|o| o.id)
                .collect()
        };
        let players = BTreeMap::from([
            (ME, summary(ME, life[0], graveyard(ME))),
            (THEM, summary(THEM, life[1], graveyard(THEM))),
        ]);
        let stack = objects
            .iter()
            .filter(|o| o.zone.zone == Zone::Stack)
            .map(|o| o.id)
            .collect();
        PlayerView {
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer: ME,
            turn: 3,
            active_player: THEM,
            step: mtg_core::Step::PrecombatMain,
            priority: Some(ME),
            players,
            visible: objects.into_iter().map(|o| (o.id, o)).collect(),
            stack,
        }
    }

    #[test]
    fn a_land_arrival_and_a_cast_are_reported() {
        let before = view(vec![], [20, 20]);
        let after = view(
            vec![
                obj(1, ZoneRef::shared(Zone::Battlefield), THEM, 0),
                obj(2, ZoneRef::shared(Zone::Stack), THEM, 1),
            ],
            [20, 20],
        );
        let log = changes(&before, &after, &texts());
        assert!(
            log.contains(&"Opponent cast Stone Bear".to_string()),
            "{log:?}"
        );
        assert!(
            log.contains(&"Quiet Field entered the battlefield (opponent)".to_string()),
            "{log:?}"
        );
    }

    #[test]
    fn a_returned_land_is_not_claimed_to_have_been_played() {
        let before = view(
            vec![obj(2, ZoneRef::of(Zone::Graveyard, ME), ME, 0)],
            [20, 20],
        );
        let after = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 0)],
            [20, 20],
        );
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Quiet Field entered the battlefield (you)"]
        );
    }

    #[test]
    fn a_resolved_creature_enters_rather_than_just_resolving() {
        let before = view(
            vec![obj(2, ZoneRef::shared(Zone::Stack), THEM, 1)],
            [20, 20],
        );
        let after = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), THEM, 1)],
            [20, 20],
        );
        let log = changes(&before, &after, &texts());
        assert_eq!(
            log,
            vec!["Stone Bear entered the battlefield (opponent)".to_string()]
        );
    }

    #[test]
    fn a_creature_that_goes_to_the_graveyard_died() {
        let before = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        let after = view(
            vec![obj(4, ZoneRef::of(Zone::Graveyard, ME), ME, 1)],
            [20, 20],
        );
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Stone Bear died".to_string()]
        );
    }

    #[test]
    fn an_old_graveyard_copy_does_not_make_an_exiled_creature_die() {
        let old_copy = obj(4, ZoneRef::of(Zone::Graveyard, ME), ME, 1);
        let before = view(
            vec![
                obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1),
                old_copy.clone(),
            ],
            [20, 20],
        );
        let after = view(
            vec![
                old_copy,
                obj(5, ZoneRef::of(Zone::Graveyard, ME), ME, 0),
                obj(6, ZoneRef::shared(Zone::Exile), ME, 1),
            ],
            [20, 20],
        );
        let log = changes(&before, &after, &texts());
        assert!(log.contains(&"Stone Bear left the battlefield".to_string()));
        assert!(!log.contains(&"Stone Bear died".to_string()));
    }

    #[test]
    fn attacks_and_life_changes_are_reported() {
        let mut attacker = obj(3, ZoneRef::shared(Zone::Battlefield), THEM, 1);
        let before = view(vec![attacker.clone()], [20, 20]);
        attacker.attacking = true;
        let after = view(vec![attacker], [18, 20]);
        let log = changes(&before, &after, &texts());
        assert!(
            log.contains(&"Opponent attacked with Stone Bear".to_string()),
            "{log:?}"
        );
        assert!(log.contains(&"You lose 2 life (18)".to_string()), "{log:?}");
    }

    #[test]
    fn new_hand_cards_are_named() {
        let before = view(vec![], [20, 20]);
        let after = view(vec![obj(5, ZoneRef::of(Zone::Hand, ME), ME, 1)], [20, 20]);
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Your hand gained Stone Bear".to_string()]
        );
    }

    #[test]
    fn hand_arrivals_are_reported_even_when_hand_size_does_not_increase() {
        let before = view(vec![obj(5, ZoneRef::of(Zone::Hand, ME), ME, 0)], [20, 20]);
        let after = view(vec![obj(6, ZoneRef::of(Zone::Hand, ME), ME, 1)], [20, 20]);
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["Your hand gained Stone Bear"]
        );
    }

    #[test]
    fn a_returned_permanent_is_not_claimed_to_be_drawn() {
        let before = view(
            vec![obj(5, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        let after = view(vec![obj(6, ZoneRef::of(Zone::Hand, ME), ME, 1)], [20, 20]);
        assert_eq!(
            changes(&before, &after, &texts()),
            vec![
                "Stone Bear left the battlefield",
                "Your hand gained Stone Bear",
            ]
        );
    }

    #[test]
    fn nothing_changed_is_nothing_said() {
        let v = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        assert!(changes(&v, &v, &texts()).is_empty());
    }

    #[test]
    fn combat_prevention_start_and_expiry_are_publicly_narrated() {
        let before = view(vec![], [20, 20]);
        let mut after = before.clone();
        after.prevent_combat_damage = true;
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["All combat damage is prevented this turn"]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["Combat damage prevention ended"]
        );
    }

    #[test]
    fn targeted_prevention_names_only_visible_targets() {
        let before = view(
            vec![obj(3, ZoneRef::shared(Zone::Battlefield), ME, 1)],
            [20, 20],
        );
        let mut after = before.clone();
        after.prevent_damage_to = vec![mtg_core::Target::Object(ObjectId(3))];
        assert_eq!(
            changes(&before, &after, &texts()),
            vec!["All damage to Stone Bear is prevented this turn"]
        );
        assert_eq!(
            changes(&after, &before, &texts()),
            vec!["Damage prevention for Stone Bear ended"]
        );
    }
}
