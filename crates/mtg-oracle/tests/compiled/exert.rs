//! Exert (CR 701.43): chosen as attackers are declared; an exerted creature doesn't untap
//! during its controller's next untap step, and its "when you do" ability triggers.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

fn tapped(game: &Game, id: mtg_core::ObjectId) -> bool {
    game.engine.state.objects[&id].tapped
}

#[test]
fn exerted_attacker_is_pumped_and_skips_its_next_untap() {
    let mut table = Table::default();
    let cat = table.card(
        "{1}{W}",
        "Creature — Bear",
        Some((2, 2)),
        "You may exert this creature as it attacks. When you do, it gets +1/+1 and gains flying \
         until end of turn.",
    );
    let wall = table.card("{1}{G}", "Creature — Wall", Some((0, 4)), "");
    let mut game = Game::new(table);
    let cat = game.put(cat, P0, Zone::Battlefield);
    let wall = game.put(wall, P1, Zone::Battlefield);
    game.main();
    let _ = wall;
    // The wall can't block it once it flies, so no block is declared.
    game.combat(&[cat], &[], &[], &[Answer::Objects(vec![cat])]);
    assert_eq!(game.life(P1), 17, "3 damage from a 3/3");
    assert!(tapped(&game, cat));
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(tapped(&game, cat), "exerted: it didn't untap");
    assert_eq!(game.pt(cat), (2, 2), "the pump has worn off");
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(!tapped(&game, cat), "only the next untap step is skipped");
}

#[test]
fn declining_to_exert_untaps_as_usual_and_targets_only_when_exerted() {
    let mut table = Table::default();
    let striker = table.card(
        "{2}{R}",
        "Creature — Bear",
        Some((2, 2)),
        "You may exert this creature as it attacks. When you do, it deals 4 damage to target \
         creature an opponent controls.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    let striker = game.put(striker, P0, Zone::Battlefield);
    let bear = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.combat(&[striker], &[], &[], &[Answer::Objects(vec![])]);
    assert_eq!(game.life(P1), 18);
    assert!(game.engine.state.objects.contains_key(&bear), "not exerted: no damage");
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(!tapped(&game, striker), "not exerted: it untapped");
    game.combat(
        &[striker],
        &[],
        &[Target::Object(bear)],
        &[Answer::Objects(vec![striker])],
    );
    assert!(!game.engine.state.objects.contains_key(&bear), "exerted: 4 damage");
    assert_eq!(game.count(Zone::Graveyard, P1), 1);
}

#[test]
fn exert_as_a_cost_keeps_the_creature_tapped_through_the_next_untap() {
    let mut table = Table::default();
    let pinger = table.card(
        "{1}{R}",
        "Creature — Bear",
        Some((1, 1)),
        "{T}, Exert this creature: It deals 1 damage to target creature.",
    );
    let elf = table.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut game = Game::new(table);
    let pinger = game.put(pinger, P0, Zone::Battlefield);
    let elf = game.put(elf, P1, Zone::Battlefield);
    let actions = game.main();
    assert!(offers(&actions, pinger));
    game.act(activate(pinger, 0), &[Target::Object(elf)], &[]);
    assert!(!game.engine.state.objects.contains_key(&elf));
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(tapped(&game, pinger), "exerted: it didn't untap");
    game.until(P1, mtg_core::Step::PrecombatMain);
    game.until(P0, mtg_core::Step::PrecombatMain);
    assert!(!tapped(&game, pinger));
}
