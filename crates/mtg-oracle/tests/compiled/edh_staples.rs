//! EDHREC top-100 staples: Faeburrow Elder's pump, Damn, Rakdos Charm.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn faeburrow_elder_gets_bigger_for_each_color() {
    let mut table = Table::default();
    let elder = table.card(
        "{1}{G}{W}",
        "Creature — Treefolk Druid",
        Some((0, 0)),
        "This creature gets +1/+1 for each color among permanents you control.",
    );
    let blue = table.card("{U}", "Enchantment", None, "");
    let mut game = Game::new(table);
    let elder = game.put(elder, P0, Zone::Battlefield);
    game.put(blue, P0, Zone::Battlefield);
    assert_eq!(game.pt(elder), (3, 3), "green, white and blue");
}

#[test]
fn overloaded_damn_destroys_every_creature() {
    let mut table = Table::default();
    let damn = table.card(
        "{B}{B}",
        "Sorcery",
        None,
        "Destroy target creature. A creature destroyed this way can't be regenerated.\nOverload {2}{W}{W}",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(4);
    let damn = game.put(damn, P0, Zone::Hand);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    let actions = game.main();
    let overload = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == damn))
        .unwrap()
        .clone();
    game.act(overload, &[], &[]);
    assert!(!game.engine.state.objects.contains_key(&mine));
    assert!(!game.engine.state.objects.contains_key(&theirs));
}

#[test]
fn rakdos_charm_each_creature_hits_its_controller() {
    let mut table = Table::default();
    let charm = table.card(
        "{B}{R}",
        "Instant",
        None,
        "Choose one —\n• Exile target player's graveyard.\n• Destroy target artifact.\n• Each creature deals 1 damage to its controller.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let charm = game.put(charm, P0, Zone::Hand);
    game.put(bear, P0, Zone::Battlefield);
    game.put(bear, P1, Zone::Battlefield);
    game.put(bear, P1, Zone::Battlefield);
    // Castable with no artifact to target: its third mode needs none.
    assert!(game.main().contains(&Action::Cast { object: charm }));
    game.act(
        Action::Cast { object: charm },
        &[],
        &[Answer::Modes(vec![2])],
    );
    assert_eq!((game.life(P0), game.life(P1)), (19, 18));
}
