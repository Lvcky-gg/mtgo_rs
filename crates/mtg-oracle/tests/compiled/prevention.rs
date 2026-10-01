//! Global combat-damage prevention, compiled from invented cards.
use super::harness::*;
use mtg_core::{Event, Step, Target, Zone};
use mtg_engine::{actions::Action, view::project};

fn fog(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{G}",
        "Instant",
        None,
        "Prevent all combat damage that would be dealt this turn.",
    )
}

fn shield(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all damage that would be dealt to any target this turn.",
    )
}

#[test]
fn targeted_prevention_stops_spell_damage_to_objects_and_players() {
    for protect_player in [false, true] {
        let mut t = Table::default();
        let shield = shield(&mut t);
        let bear = t.bear();
        let burn = t.card(
            "{R}",
            "Instant",
            None,
            "Lifelink\n~ deals 3 damage to any target.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let creature = g.put(bear, P1, Zone::Battlefield);
        let protection = g.put(shield, P0, Zone::Hand);
        let burn = g.put(burn, P0, Zone::Hand);
        let target = if protect_player {
            Target::Player(P1)
        } else {
            Target::Object(creature)
        };
        g.main();
        g.act(Action::Cast { object: protection }, &[target], &[]);
        assert_eq!(project(&g.engine.state, P1).prevent_damage_to, vec![target]);
        g.act(Action::Cast { object: burn }, &[target], &[]);
        assert_eq!(g.life(P0), 20);
        assert_eq!(g.life(P1), 20);
        assert_eq!(g.engine.state.objects[&creature].damage, 0);
    }
}

#[test]
fn targeted_prevention_stops_only_damage_to_the_shielded_combatant() {
    let mut t = Table::default();
    let shield = shield(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let blocker = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(shield, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(attacker)],
        &[],
    );
    g.combat(&[attacker], &[(blocker, attacker)], &[], &[]);
    assert!(g.engine.state.objects.contains_key(&attacker));
    assert_eq!(g.engine.state.objects[&attacker].damage, 0);
    assert!(!g.engine.state.objects.contains_key(&blocker));
}

#[test]
fn player_prevention_stops_combat_poison_and_expires_at_cleanup() {
    let mut t = Table::default();
    let shield = shield(&mut t);
    let beast = t.card("", "Creature", Some((3, 3)), "Infect, lifelink");
    let mut g = Game::new(t);
    g.lands(1);
    let attacker = g.put(beast, P0, Zone::Battlefield);
    let spell = g.put(shield, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: spell }, &[Target::Player(P1)], &[]);
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P0), 20);
    assert_eq!(g.engine.state.player(P1).poison, 0);
    g.until(P1, Step::PrecombatMain);
    assert!(g.engine.state.prevent_damage_to.is_empty());
    g.main();
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(g.engine.state.player(P1).poison, 3);
}

#[test]
fn targeted_prevention_does_not_follow_a_bounced_card() {
    let mut t = Table::default();
    let shield = shield(&mut t);
    let bear = t.bear();
    let bounce = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let burn = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(6);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let shield = g.put(shield, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    let burn = g.put(burn, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: shield },
        &[Target::Object(creature)],
        &[],
    );
    g.act(
        Action::Cast { object: bounce },
        &[Target::Object(creature)],
        &[],
    );
    assert!(g.engine.state.prevent_damage_to.is_empty());
    let hand = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == bear)
        .unwrap()
        .id;
    g.act(Action::Cast { object: hand }, &[], &[]);
    let returned = g.find(bear).unwrap();
    assert_ne!(returned, creature);
    g.act(
        Action::Cast { object: burn },
        &[Target::Object(returned)],
        &[],
    );
    assert!(!g.engine.state.objects.contains_key(&returned));
}

#[test]
fn removing_the_target_before_resolution_creates_no_prevention() {
    let mut t = Table::default();
    let shield = shield(&mut t);
    let bear = t.bear();
    let kill = t.card("{R}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(2);
    let creature = g.put(bear, P1, Zone::Battlefield);
    let shield = g.put(shield, P0, Zone::Hand);
    let kill = g.put(kill, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: shield }, &[Target::Object(creature)]);
    g.act(
        Action::Cast { object: kill },
        &[Target::Object(creature)],
        &[],
    );
    assert!(g.engine.state.prevent_damage_to.is_empty());
}

#[test]
fn mass_damage_and_lifelink_count_only_unprotected_recipients() {
    let mut t = Table::default();
    let shield = shield(&mut t);
    let bear = t.bear();
    let burn = t.card(
        "{R}",
        "Instant",
        None,
        "Lifelink\n~ deals 1 damage to each creature.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let protected = g.put(bear, P0, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    let shield = g.put(shield, P0, Zone::Hand);
    let burn = g.put(burn, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: shield },
        &[Target::Object(protected)],
        &[],
    );
    g.act(Action::Cast { object: burn }, &[], &[]);
    assert_eq!(g.engine.state.objects[&protected].damage, 0);
    assert_eq!(g.engine.state.objects[&other].damage, 1);
    assert_eq!(g.life(P0), 21);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn creature_damage_does_not_implicitly_include_controllers() {
    for include_players in [false, true] {
        let mut t = Table::default();
        let bear = t.bear();
        let burn = t.card(
            "{R}",
            "Instant",
            None,
            if include_players {
                "~ deals 1 damage to each creature and each player."
            } else {
                "~ deals 1 damage to each creature."
            },
        );
        let mut g = Game::new(t);
        g.lands(1);
        let creatures = [
            g.put(bear, P0, Zone::Battlefield),
            g.put(bear, P1, Zone::Battlefield),
        ];
        let spell = g.put(burn, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: spell }, &[], &[]);
        for creature in creatures {
            assert_eq!(g.engine.state.objects[&creature].damage, 1);
        }
        for player in [P0, P1] {
            assert_eq!(g.life(player), if include_players { 19 } else { 20 });
        }
    }
}

fn cast_fog(g: &mut Game, card: mtg_core::CardId) {
    let spell = g.put(card, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: spell }, &[], &[]);
    assert!(g.engine.state.prevent_combat_damage);
    assert!(project(&g.engine.state, P0).prevent_combat_damage);
    assert!(project(&g.engine.state, P1).prevent_combat_damage);
}

#[test]
fn fog_prevents_both_damage_steps_lifelink_and_combat_damage_triggers() {
    let mut t = Table::default();
    let fog = fog(&mut t);
    let creature = t.card(
        "",
        "Creature",
        Some((3, 3)),
        "Double strike, lifelink\nWhenever ~ deals combat damage to a player, draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let attacker = g.put(creature, P0, Zone::Battlefield);
    cast_fog(&mut g, fog);
    let hand = g.count(Zone::Hand, P0);
    let log_len = g.engine.log.len();
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P0), 20);
    assert_eq!(g.life(P1), 20);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    assert!(!g.engine.log[log_len..].iter().any(|e| matches!(
        e.event,
        Event::DamageMarked { .. } | Event::DamageDealtToPlayer { .. }
    )));
}

#[test]
fn fog_prevents_attacker_and_blocker_damage_deathtouch_and_infect() {
    let mut t = Table::default();
    let fog = fog(&mut t);
    let attacker_card = t.card("", "Creature", Some((4, 4)), "Trample, infect, deathtouch");
    let blocker_card = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let attacker = g.put(attacker_card, P0, Zone::Battlefield);
    let blocker = g.put(blocker_card, P1, Zone::Battlefield);
    cast_fog(&mut g, fog);
    g.combat(&[attacker], &[(blocker, attacker)], &[], &[]);
    for object in [attacker, blocker] {
        assert_eq!(g.engine.state.objects[&object].damage, 0);
        assert!(g.engine.state.objects[&object].counters.is_empty());
        assert!(!g.engine.state.objects[&object].dealt_deathtouch_damage);
    }
    assert_eq!(g.engine.state.player(P1).poison, 0);
}

#[test]
fn fog_preserves_planeswalker_loyalty() {
    let mut t = Table::default();
    let fog = fog(&mut t);
    let walker = t.loyalty("", "Planeswalker", 3, "0: You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let walker = g.put(walker, P1, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&walker)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 3);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    cast_fog(&mut g, fog);
    g.combat_at(&[(attacker, Target::Object(walker))], &[]);
    assert_eq!(
        g.engine.state.objects[&walker].counters[&mtg_core::CounterKind::Loyalty],
        3
    );
}

#[test]
fn fog_does_not_prevent_noncombat_damage_and_expires_at_cleanup() {
    let mut t = Table::default();
    let fog = fog(&mut t);
    let burn = t.card("{R}", "Instant", None, "~ deals 2 damage to target player.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let burn = g.put(burn, P0, Zone::Hand);
    cast_fog(&mut g, fog);
    g.act(Action::Cast { object: burn }, &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 18);
    g.until(P1, Step::PrecombatMain);
    assert!(!g.engine.state.prevent_combat_damage);
    assert!(!project(&g.engine.state, P0).prevent_combat_damage);
    g.main();
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P1), 16);
}

#[test]
fn countering_fog_does_not_start_prevention() {
    let mut t = Table::default();
    let fog = fog(&mut t);
    let counter = t.card("{U}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    g.lands(2);
    let fog = g.put(fog, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: fog }, &[]);
    let spell = g.stack()[0];
    g.act(
        Action::Cast { object: counter },
        &[Target::Object(spell)],
        &[],
    );
    assert!(!g.engine.state.prevent_combat_damage);
}

// ---- shields: the next N damage, damage by a source, and static prevention -----------

fn burn(t: &mut Table, n: u32) -> mtg_core::CardId {
    t.card(
        "{R}",
        "Instant",
        None,
        &format!("~ deals {n} damage to any target."),
    )
}

#[test]
fn a_counting_shield_prevents_only_the_next_n_damage() {
    let mut t = Table::default();
    let healer = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "{T}: Prevent the next 2 damage that would be dealt to any target this turn.",
    );
    let bear = t.card("{3}{G}", "Creature — Bear", Some((4, 4)), "");
    let bolt = burn(&mut t, 3);
    let shock = burn(&mut t, 1);
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(healer, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let bolt = g.put(bolt, P0, Zone::Hand);
    let shock = g.put(shock, P0, Zone::Hand);
    g.main();
    g.act(activate(h, 0), &[Target::Object(b)], &[]);
    assert_eq!(g.engine.state.damage_shields.len(), 1);
    g.cast(shock, &[Target::Object(b)]);
    assert_eq!(g.engine.state.objects[&b].damage, 0, "1 of the 2 prevented");
    assert_eq!(g.engine.state.damage_shields[0].remaining, Some(1));
    g.cast(bolt, &[Target::Object(b)]);
    assert_eq!(
        g.engine.state.objects[&b].damage, 2,
        "the last 1 prevented, 2 dealt"
    );
    assert!(g.engine.state.damage_shields.is_empty(), "used up");
}

#[test]
fn lifelink_counts_only_damage_a_shield_let_through() {
    let mut t = Table::default();
    let ward = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent the next 2 damage that would be dealt to target player this turn.",
    );
    let drain = t.card(
        "{R}",
        "Instant",
        None,
        "Lifelink\n~ deals 3 damage to any target.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let ward = g.put(ward, P0, Zone::Hand);
    let drain = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(ward, &[Target::Player(P1)]);
    g.cast(drain, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.life(P0), 21, "lifelink gains what was dealt");
}

#[test]
fn shields_end_at_cleanup() {
    let mut t = Table::default();
    let ward = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent the next 5 damage that would be dealt to you this turn.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let ward = g.put(ward, P0, Zone::Hand);
    g.main();
    g.cast(ward, &[]);
    assert_eq!(g.engine.state.damage_shields.len(), 1);
    g.until(P1, Step::PrecombatMain);
    assert!(g.engine.state.damage_shields.is_empty());
}

#[test]
fn a_static_shield_stops_combat_damage_but_not_burn() {
    let mut t = Table::default();
    let wall = t.card(
        "{1}{W}",
        "Creature — Spirit",
        Some((0, 2)),
        "Defender\nPrevent all combat damage that would be dealt to this creature.",
    );
    let giant = t.card("{4}{R}", "Creature — Goblin", Some((5, 5)), "");
    let bolt = burn(&mut t, 3);
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(wall, P1, Zone::Battlefield);
    let a = g.put(giant, P0, Zone::Battlefield);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.combat(&[a], &[(w, a)], &[], &[]);
    assert!(
        g.engine.state.objects.contains_key(&w),
        "combat damage prevented"
    );
    assert_eq!(g.engine.state.objects[&w].damage, 0);
    g.cast(bolt, &[Target::Object(w)]);
    assert!(
        !g.engine.state.objects.contains_key(&w),
        "burn is not combat damage"
    );
}

#[test]
fn damage_by_creatures_is_prevented_but_not_by_spells() {
    let mut t = Table::default();
    let ghost = t.card(
        "{2}{W}",
        "Creature — Spirit",
        Some((1, 1)),
        "Prevent all damage that would be dealt to this creature by creatures.",
    );
    let giant = t.card("{4}{R}", "Creature — Goblin", Some((5, 5)), "");
    let bolt = burn(&mut t, 1);
    let mut g = Game::new(t);
    g.lands(1);
    let gh = g.put(ghost, P1, Zone::Battlefield);
    let a = g.put(giant, P0, Zone::Battlefield);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.combat(&[a], &[(gh, a)], &[], &[]);
    assert!(g.engine.state.objects.contains_key(&gh));
    g.cast(bolt, &[Target::Object(gh)]);
    assert!(!g.engine.state.objects.contains_key(&gh));
}

#[test]
fn an_aura_stops_damage_to_and_by_its_creature() {
    let mut t = Table::default();
    let pacifism = t.card(
        "{1}{W}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nPrevent all combat damage that would be dealt to and dealt by enchanted creature.",
    );
    let giant = t.card("{4}{R}", "Creature — Goblin", Some((5, 5)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let a = g.put(giant, P0, Zone::Battlefield);
    let blocker = g.put(bear, P1, Zone::Battlefield);
    let aura = g.put(pacifism, P0, Zone::Hand);
    g.main();
    g.cast(aura, &[Target::Object(a)]);
    g.combat(&[a], &[(blocker, a)], &[], &[]);
    assert!(
        g.engine.state.objects.contains_key(&blocker),
        "no damage by it"
    );
    assert_eq!(g.engine.state.objects[&a].damage, 0, "no damage to it");
}

#[test]
fn a_targeted_attacker_deals_no_combat_damage() {
    let mut t = Table::default();
    let holy = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all combat damage that would be dealt by target creature this turn.",
    );
    let giant = t.card("{4}{R}", "Creature — Goblin", Some((5, 5)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let a = g.put(giant, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let holy = g.put(holy, P0, Zone::Hand);
    g.main();
    g.cast(holy, &[Target::Object(a)]);
    g.combat(&[a, b], &[], &[], &[]);
    assert_eq!(g.life(P1), 18, "only the bear connects");
}

#[test]
fn a_phantom_prevents_damage_and_loses_one_counter_per_batch() {
    let mut t = Table::default();
    let phantom = t.card(
        "{2}{G}",
        "Creature — Spirit",
        Some((0, 0)),
        "This creature enters with three +1/+1 counters on it.\nIf damage would be dealt to \
         this creature, prevent that damage. Remove a +1/+1 counter from this creature.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let p = g.put(phantom, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&p)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 3);
    let a = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(p)]);
    assert_eq!(g.pt(p), (2, 2), "the bolt was prevented, one counter gone");
    assert_eq!(g.engine.state.objects[&p].damage, 0);
    // Two blockers' damage at once costs one counter.
    g.combat(&[p], &[(a, p), (b, p)], &[], &[]);
    assert_eq!(g.pt(p), (1, 1));
    assert!(g.find(phantom).is_some());
}
