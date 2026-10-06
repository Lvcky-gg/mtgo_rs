//! "Can't cast" restrictions: "your opponents can't cast creature spells", "each player
//! can't cast more than one spell each turn", and one-shot "this turn" versions.
use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

fn offers_cast(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

#[test]
fn each_player_can_cast_only_one_spell_each_turn() {
    let mut t = Table::default();
    let rule = t.card(
        "{1}{W}",
        "Enchantment",
        None,
        "Each player can't cast more than one spell each turn.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(rule, P1, Zone::Battlefield);
    let a = g.put(shock, P0, Zone::Hand);
    let b = g.put(shock, P0, Zone::Hand);
    let actions = g.main();
    assert!(offers_cast(&actions, a) && offers_cast(&actions, b));
    g.cast(a, &[Target::Player(P1)]);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(!offers_cast(&actions, b), "a second spell this turn");
}

#[test]
fn your_opponents_cant_cast_creature_spells() {
    for theirs in [false, true] {
        let mut t = Table::default();
        let rule = t.card(
            "{1}{W}",
            "Enchantment",
            None,
            "Your opponents can't cast creature spells.",
        );
        let bear = t.bear();
        let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
        let mut g = Game::new(t);
        g.lands(2);
        g.put(rule, if theirs { P1 } else { P0 }, Zone::Battlefield);
        let b = g.put(bear, P0, Zone::Hand);
        let s = g.put(shock, P0, Zone::Hand);
        let actions = g.main();
        assert_eq!(offers_cast(&actions, b), !theirs, "the bear");
        assert!(offers_cast(&actions, s), "a noncreature spell is fine");
    }
}

#[test]
fn your_opponents_cant_cast_spells_this_turn() {
    let mut t = Table::default();
    let silence = t.card(
        "{W}",
        "Instant",
        None,
        "Your opponents can't cast spells this turn.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(silence, P0, Zone::Hand);
    let theirs = g.put(shock, P1, Zone::Hand);
    let mine = g.put(shock, P0, Zone::Hand);
    g.main();
    let forbidden =
        |g: &Game, id, who| mtg_engine::cost::cast_forbidden(&g.engine.state, &g.table, id, who);
    assert!(!forbidden(&g, theirs, P1));
    g.cast(s, &[]);
    assert!(forbidden(&g, theirs, P1), "after it resolves");
    assert!(!forbidden(&g, mine, P0), "only opponents");
    // It lasts for this turn only.
    g.until(P1, mtg_core::Step::Upkeep);
    assert!(!forbidden(&g, theirs, P1));
}

// ---- cost reductions counted in odd ways ------------------------------------------------------

#[test]
fn it_costs_less_for_each_creature_in_your_party() {
    // Two Wizards and a Warrior make a party of two: one creature per role.
    for party in [false, true] {
        let mut t = Table::default();
        let spell = t.card(
            "{4}{W}",
            "Sorcery",
            None,
            "This spell costs {1} less to cast for each creature in your party.\nYou gain 1 \
             life.",
        );
        let wizard = t.card("{U}", "Creature — Wizard", Some((1, 1)), "");
        let warrior = t.card("{R}", "Creature — Warrior", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(3);
        let s = g.put(spell, P0, Zone::Hand);
        if party {
            g.put(wizard, P0, Zone::Battlefield);
            g.put(wizard, P0, Zone::Battlefield);
            g.put(warrior, P0, Zone::Battlefield);
        }
        let actions = g.main();
        assert_eq!(offers_cast(&actions, s), party, "5 less 2 is 3");
    }
}

#[test]
fn undaunted_costs_one_less_per_opponent() {
    let mut t = Table::default();
    let knight = t.card("{3}{W}", "Creature — Soldier", Some((3, 3)), "Undaunted");
    let mut g = Game::new(t);
    g.lands(3);
    let k = g.put(knight, P0, Zone::Hand);
    let actions = g.main();
    assert!(offers_cast(&actions, k), "one opponent: {{2}}{{W}}");
    g.cast(k, &[]);
    let on = g.find(knight).expect("on the battlefield");
    assert_eq!(g.engine.state.objects[&on].zone.zone, Zone::Battlefield);
}

#[test]
fn domain_costs_less_for_each_basic_land_type() {
    for diverse in [false, true] {
        let mut t = Table::default();
        let spell = t.card(
            "{4}{G}",
            "Sorcery",
            None,
            "Domain — This spell costs {1} less to cast for each basic land type among lands \
             you control.\nYou gain 1 life.",
        );
        let forest = t.card("", "Basic Land — Forest", None, "");
        let island = t.card("", "Basic Land — Island", None, "");
        let mountain = t.card("", "Basic Land — Mountain", None, "");
        let mut g = Game::new(t);
        g.put(forest, P0, Zone::Battlefield);
        for land in if diverse {
            [island, mountain]
        } else {
            [forest, forest]
        } {
            g.put(land, P0, Zone::Battlefield);
        }
        let s = g.put(spell, P0, Zone::Hand);
        let actions = g.main();
        assert_eq!(offers_cast(&actions, s), diverse, "5 less 3 is 2");
    }
}

// ---- other one-shot effects ----------------------------------------------------------------

#[test]
fn target_player_shuffles_their_graveyard_into_their_library() {
    let mut t = Table::default();
    let spell = t.card(
        "{U}",
        "Sorcery",
        None,
        "Target player shuffles their graveyard into their library.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Graveyard);
    }
    g.main();
    let library = g.count(Zone::Library, P1);
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(g.count(Zone::Library, P1), library + 3);
}

#[test]
fn each_player_shuffles_hand_and_graveyard_away_then_draws_seven() {
    let mut t = Table::default();
    let spell = t.card(
        "{2}{U}",
        "Sorcery",
        None,
        "Each player shuffles their hand and graveyard into their library, then draws seven \
         cards.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(spell, P0, Zone::Hand);
    g.put(bear, P1, Zone::Hand);
    g.put(bear, P1, Zone::Graveyard);
    g.main();
    g.cast(s, &[]);
    for p in [P0, P1] {
        assert_eq!(g.count(Zone::Hand, p), 7);
        assert_eq!(g.count(Zone::Graveyard, p), if p == P0 { 1 } else { 0 });
    }
}

#[test]
fn activated_abilities_of_artifacts_cant_be_activated() {
    for locked in [false, true] {
        let mut t = Table::default();
        let rod = t.card(
            "{2}",
            "Artifact",
            None,
            "Activated abilities of artifacts can't be activated.",
        );
        let relic = t.card("{1}", "Artifact", None, "{T}: You gain 1 life.");
        let mut g = Game::new(t);
        if locked {
            g.put(rod, P1, Zone::Battlefield);
        }
        let r = g.put(relic, P0, Zone::Battlefield);
        let actions = g.main();
        assert_eq!(offers(&actions, r), !locked);
    }
}

#[test]
fn a_spell_countered_this_way_is_exiled() {
    for exile in [false, true] {
        let mut t = Table::default();
        let text = if exile {
            "Counter target spell. If that spell is countered this way, exile it instead of \
             putting it into its owner's graveyard."
        } else {
            "Counter target spell."
        };
        let dissipate = t.card("{1}{U}{U}", "Instant", None, text);
        let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
        let mut g = Game::new(t);
        g.lands(4);
        let d = g.put(dissipate, P0, Zone::Hand);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: s }, &[Target::Player(P1)]);
        let spell = g.stack()[0];
        g.cast(d, &[Target::Object(spell)]);
        assert_eq!(g.life(P1), 20, "countered");
        assert_eq!(g.count(Zone::Graveyard, P0), if exile { 1 } else { 2 });
        assert_eq!(
            g.engine
                .state
                .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
                .len(),
            usize::from(exile)
        );
    }
}

#[test]
fn cast_only_if_you_have_cast_another_spell_this_turn() {
    let mut t = Table::default();
    let follow = t.card(
        "{R}",
        "Instant",
        None,
        "Cast this spell only if you've cast another spell this turn.\n~ deals 3 damage to \
         any target.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let f = g.put(follow, P0, Zone::Hand);
    let s = g.put(shock, P0, Zone::Hand);
    let actions = g.main();
    assert!(!offers_cast(&actions, f), "nothing cast yet");
    g.cast(s, &[Target::Player(P1)]);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(offers_cast(&actions, f));
    g.cast(f, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 16);
}
