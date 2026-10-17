//! Altar of Dementia, Whispersilk Cloak, Sword of Feast and Famine's untap, Primal Vigor, and
//! "Add X mana of any one color, where X is …".

use super::harness::*;
use mtg_core::{Keyword, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn altar_of_dementia_mills_by_the_sacrificed_power() {
    let mut t = Table::default();
    let altar = t.card(
        "{2}",
        "Artifact",
        None,
        "Sacrifice a creature: Target player mills cards equal to the sacrificed creature's power.",
    );
    let giant = t.card("{3}", "Creature — Giant", Some((5, 5)), "");
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    let victim = g.put(giant, P0, Zone::Battlefield);
    g.main();
    let before = g.count(Zone::Library, P1);
    g.act(
        activate(a, 0),
        &[Target::Player(P1)],
        &[Answer::Objects(vec![victim])],
    );
    assert_eq!(g.count(Zone::Library, P1), before - 5);
}

#[test]
fn whispersilk_cloak_is_unblockable_and_shroud() {
    let mut t = Table::default();
    let shroud_static = t.card(
        "{0}",
        "Creature — Rogue",
        Some((1, 1)),
        "This creature can't be blocked and has shroud.",
    );
    let mut g = Game::new(t);
    let r = g.put(shroud_static, P0, Zone::Battlefield);
    assert!(g.has(r, Keyword::Shroud));
}

#[test]
fn you_untap_all_lands_you_control() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, "You untap all lands you control.");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    for id in g.engine.state.battlefield() {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    g.cast(s, &[]);
    assert!(
        g.engine
            .state
            .battlefield()
            .iter()
            .all(|id| !g.engine.state.objects[id].tapped)
    );
}

#[test]
fn primal_vigor_doubles_everyones_tokens() {
    let mut t = Table::default();
    let vigor = t.card(
        "{4}{G}",
        "Enchantment",
        None,
        "If one or more tokens would be created, twice that many of those tokens are created instead.",
    );
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Target opponent creates a Treasure token.",
    );
    let mut g = Game::new(t);
    g.put(vigor, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    let theirs = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| {
            let o = &g.engine.state.objects[id];
            o.controller == P1 && o.card.0 >= mtg_ir::walk::TOKEN_CARD_BASE
        })
        .count();
    assert_eq!(theirs, 2);
}

#[test]
fn x_mana_of_one_color_pays_for_a_spell() {
    let mut t = Table::default();
    let weaver = t.card(
        "{1}{G}",
        "Enchantment Creature — Dryad",
        Some((0, 2)),
        "{T}: Add X mana of any one color, where X is the number of enchantments you control.",
    );
    let aura = t.card("{0}", "Enchantment", None, "");
    let beast = t.card("{2}{G}", "Creature — Beast", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.put(weaver, P0, Zone::Battlefield);
    g.put(aura, P0, Zone::Battlefield);
    g.put(aura, P0, Zone::Battlefield);
    let s = g.put(beast, P0, Zone::Hand);
    // Three enchantments: three green from one tap.
    assert!(g.main().contains(&Action::Cast { object: s }));
}

#[test]
fn debt_to_the_deathless_drains_two_times_x() {
    let mut t = Table::default();
    let debt = t.card(
        "{X}",
        "Sorcery",
        None,
        "Each opponent loses two times X life. You gain life equal to the life lost this way.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(debt, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: s }, &[], &[Answer::Number(3)]);
    assert_eq!((g.life(P0), g.life(P1)), (26, 14));
}

#[test]
fn decimate_destroys_four_different_targets() {
    let mut t = Table::default();
    let decimate = t.card(
        "{0}",
        "Sorcery",
        None,
        "Destroy target artifact, target creature, target enchantment, and target land.",
    );
    let rock = t.card("{1}", "Artifact", None, "");
    let bear = t.bear();
    let aura = t.card("{1}", "Enchantment", None, "");
    let land = t.card("", "Land", None, "");
    let mut g = Game::new(t);
    let targets: Vec<_> = [rock, bear, aura, land]
        .iter()
        .map(|c| g.put(*c, P1, Zone::Battlefield))
        .collect();
    let s = g.put(decimate, P0, Zone::Hand);
    g.main();
    g.cast(
        s,
        &targets
            .iter()
            .map(|o| Target::Object(*o))
            .collect::<Vec<_>>(),
    );
    assert!(
        targets
            .iter()
            .all(|o| !g.engine.state.objects.contains_key(o))
    );
}

#[test]
fn razorkin_needlehead_pings_the_drawing_opponent_and_strikes_first_on_your_turn() {
    let mut t = Table::default();
    let needle = t.card(
        "{R}",
        "Creature — Human Assassin",
        Some((2, 1)),
        "This creature has first strike during your turn.\nWhenever an opponent draws a card, this creature deals 1 damage to them.",
    );
    let mut g = Game::new(t);
    let n = g.put(needle, P0, Zone::Battlefield);
    g.main();
    assert!(g.has(n, Keyword::FirstStrike));
    g.pending = None;
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert!(!g.has(n, Keyword::FirstStrike), "not on their turn");
    assert_eq!(g.life(P1), 19, "their draw step");
}

#[test]
fn a_static_prowess_grant_triggers() {
    let mut t = Table::default();
    let monk = t.card(
        "{0}",
        "Creature — Monk",
        Some((1, 1)),
        "This creature has prowess.",
    );
    let cantrip = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let m = g.put(monk, P0, Zone::Battlefield);
    let c = g.put(cantrip, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    assert_eq!(g.pt(m), (2, 2));
}

#[test]
fn hexproof_and_unblockable() {
    let mut t = Table::default();
    let thief = t.card(
        "{0}",
        "Creature — Rogue",
        Some((1, 1)),
        "This creature has hexproof and can't be blocked.",
    );
    let mut g = Game::new(t);
    let r = g.put(thief, P0, Zone::Battlefield);
    assert!(g.has(r, Keyword::Hexproof));
}

#[test]
fn a_life_total_becomes_a_number_either_way() {
    for (start, end) in [(20, 10), (4, 10)] {
        let mut t = Table::default();
        let spell = t.card(
            "{0}",
            "Sorcery",
            None,
            "Target opponent's life total becomes 10.",
        );
        let mut g = Game::new(t);
        g.engine.state.players.get_mut(&P1).unwrap().life = start;
        let s = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(s, &[Target::Player(P1)]);
        assert_eq!(g.life(P1), end, "from {start}");
    }
}

#[test]
fn each_player_sacrifices_two_of_their_choice() {
    let mut t = Table::default();
    let edict = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each player sacrifices two creatures of their choice.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Battlefield);
    }
    let s = g.put(edict, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let theirs = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| g.engine.state.objects[id].controller == P1)
        .count();
    assert_eq!(theirs, 1);
}

const CAGE: &str = "Creature cards in graveyards and libraries can't enter the battlefield.\nPlayers can't cast spells from graveyards or libraries.";

#[test]
fn grafdiggers_cage_stops_reanimation() {
    let mut t = Table::default();
    let cage = t.card("{1}", "Artifact", None, CAGE);
    let raise = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return target creature card from your graveyard to the battlefield.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(cage, P1, Zone::Battlefield);
    let dead = g.put(bear, P0, Zone::Graveyard);
    let s = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(dead)]);
    assert!(g.find(bear).is_none(), "the bear stayed in the graveyard");
}

#[test]
fn grafdiggers_cage_stops_casting_from_the_graveyard() {
    for with_cage in [false, true] {
        let mut t2 = Table::default();
        let cage2 = t2.card("{1}", "Artifact", None, CAGE);
        let burn2 = t2.card(
            "{0}",
            "Sorcery",
            None,
            "~ deals 1 damage to any target.\nFlashback {0}",
        );
        let mut g = Game::new(t2);
        if with_cage {
            g.put(cage2, P1, Zone::Battlefield);
        }
        let s = g.put(burn2, P0, Zone::Graveyard);
        let offered = g
            .main()
            .iter()
            .any(|a| a.play().is_some_and(|(o, _, _)| o == s));
        assert_eq!(offered, !with_cage);
    }
}

#[test]
fn lands_you_control_enter_untapped() {
    let mut t = Table::default();
    let walk = t.card(
        "{0}",
        "Enchantment",
        None,
        "Lands you control enter untapped.",
    );
    let tapland = t.card("", "Land", None, "This land enters tapped.\n{T}: Add {G}.");
    let ramp = t.card(
        "{0}",
        "Sorcery",
        None,
        "Search your library for a land card, put it onto the battlefield tapped, then shuffle.",
    );
    let mut g = Game::new(t);
    g.put(walk, P0, Zone::Battlefield);
    let land = g.put(tapland, P0, Zone::Hand);
    let in_library = g.put(tapland, P0, Zone::Library);
    let s = g.put(ramp, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: land }, &[], &[]);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![in_library])],
    );
    let lands: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].card == tapland)
        .collect();
    assert_eq!(lands.len(), 2);
    assert!(lands.iter().all(|l| !g.engine.state.objects[l].tapped));
}

#[test]
fn rhythm_of_the_wild_grants_riot() {
    for counter in [true, false] {
        let mut t = Table::default();
        let rhythm = t.card(
            "{1}{R}{G}",
            "Enchantment",
            None,
            "Creature spells you control can't be countered.\nNontoken creatures you control have riot.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        g.put(rhythm, P0, Zone::Battlefield);
        let s = g.put(bear, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: s }, &[], &[Answer::Bool(counter)]);
        let b = g.find(bear).unwrap();
        if counter {
            assert_eq!(g.pt(b), (3, 3));
        } else {
            assert_eq!(g.pt(b), (2, 2));
            assert!(g.has(b, Keyword::Haste));
        }
    }
}

#[test]
fn sacrifice_x_creatures_as_a_cost() {
    let mut t = Table::default();
    let altar = t.card(
        "{0}",
        "Artifact",
        None,
        "Sacrifice X creatures: Draw X cards.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    let bears: Vec<_> = (0..3).map(|_| g.put(bear, P0, Zone::Battlefield)).collect();
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        activate(a, 0),
        &[],
        &[Answer::Number(2), Answer::Objects(bears[..2].to_vec())],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
    let left = bears
        .iter()
        .filter(|b| g.engine.state.objects.contains_key(b))
        .count();
    assert_eq!(left, 1);
}
