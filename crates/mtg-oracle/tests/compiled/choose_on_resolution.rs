//! "Choose a creature type. …" / "Choose a color. …" asked as a spell resolves; "the
//! chosen type" and "that type" mean the answer, fixed as it resolves.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

/// The index of a creature type among the options the engine offers (sorted names).
fn type_option(game: &Game, name: &str) -> u8 {
    let mut names: Vec<&str> = (0..64u16)
        .filter_map(|i| {
            mtg_ir::PrintedCards::subtype_name(&game.table, mtg_core::Subtype(i))
        })
        .filter(|n| mtg_core::is_creature_type(n))
        .collect();
    names.sort_unstable();
    names.dedup();
    names.iter().position(|n| *n == name).unwrap() as u8
}

#[test]
fn creatures_of_the_chosen_type_keep_the_pump_after_the_spell_is_gone() {
    let mut table = Table::default();
    let spell = table.card(
        "{1}{W}",
        "Instant",
        None,
        "Choose a creature type. Creatures you control of the chosen type get +1/+0 and gain \
         indestructible until end of turn.",
    );
    let elf = table.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let spell = game.put(spell, P0, Zone::Hand);
    let elf = game.put(elf, P0, Zone::Battlefield);
    let bear = game.put(bear, P0, Zone::Battlefield);
    game.main();
    let elf_type = type_option(&game, "Elf");
    game.act(
        mtg_engine::actions::Action::Cast { object: spell },
        &[],
        &[Answer::Modes(vec![elf_type])],
    );
    assert_eq!(game.count(Zone::Graveyard, P0), 1, "the spell has resolved");
    assert_eq!(game.pt(elf), (2, 1), "an Elf: pumped");
    assert_eq!(game.pt(bear), (2, 2), "a Bear: not");
}

#[test]
fn a_count_of_the_chosen_type_is_fixed_as_it_resolves() {
    let mut table = Table::default();
    let spell = table.card(
        "{1}{B}",
        "Instant",
        None,
        "Choose a creature type. Target creature gets -1/-1 until end of turn for each \
         permanent of the chosen type you control.",
    );
    let elf = table.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let spell = game.put(spell, P0, Zone::Hand);
    game.put(elf, P0, Zone::Battlefield);
    game.put(elf, P0, Zone::Battlefield);
    let target = game.put(bear, P1, Zone::Battlefield);
    game.main();
    let elf_type = type_option(&game, "Elf");
    game.act(
        mtg_engine::actions::Action::Cast { object: spell },
        &[Target::Object(target)],
        &[Answer::Modes(vec![elf_type])],
    );
    assert!(
        !game.engine.state.objects.contains_key(&target),
        "two Elves: -2/-2 kills the 2/2"
    );
}
