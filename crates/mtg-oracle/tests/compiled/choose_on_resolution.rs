//! "Choose a creature type. …" / "Choose a color. …" asked as a spell resolves; "the
//! chosen type" and "that type" mean the answer, fixed as it resolves.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

/// The index of a creature type among the options the engine offers (sorted names).
fn type_option(game: &Game, name: &str) -> u8 {
    let mut names: Vec<&str> = (0..64u16)
        .filter_map(|i| mtg_ir::PrintedCards::subtype_name(&game.table, mtg_core::Subtype(i)))
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

#[test]
fn becoming_the_chosen_creature_type_replaces_the_others() {
    let mut table = Table::default();
    let shifter = table.card(
        "{1}{U}",
        "Creature — Bear",
        Some((2, 2)),
        "{1}: This creature becomes the creature type of your choice until end of turn.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let shifter = game.put(shifter, P0, Zone::Battlefield);
    game.main();
    let elf = type_option(&game, "Elf");
    game.act(activate(shifter, 0), &[], &[Answer::Modes(vec![elf])]);
    let ch = game
        .engine
        .characteristics(&game.table, shifter)
        .unwrap()
        .clone();
    let named = |n: &str| {
        mtg_ir::PrintedCards::subtype_named(&game.table, n)
            .is_some_and(|s| ch.subtypes.contains(&s))
    };
    assert!(named("Elf"), "an Elf now");
    assert!(!named("Bear"), "and no longer a Bear");
}

#[test]
fn becoming_the_chosen_color() {
    let mut table = Table::default();
    let shifter = table.card(
        "{1}{R}",
        "Creature — Bear",
        Some((2, 2)),
        "{1}: This creature becomes the color of your choice until end of turn.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let shifter = game.put(shifter, P0, Zone::Battlefield);
    game.main();
    // Options are white, blue, black, red, green.
    game.act(activate(shifter, 0), &[], &[Answer::Modes(vec![1])]);
    let ch = game
        .engine
        .characteristics(&game.table, shifter)
        .unwrap()
        .clone();
    assert_eq!(ch.colors, mtg_core::ColorSet::single(mtg_core::Color::Blue));
}

#[test]
fn an_excluded_type_is_not_offered() {
    use mtg_engine::{Progress, choice::ChoiceKind};
    let mut table = Table::default();
    let shifter = table.card(
        "{1}{U}",
        "Creature — Bear",
        Some((2, 2)),
        "{1}: Choose a creature type other than Wall. Target creature becomes that type until \
         end of turn.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let shifter = game.put(shifter, P0, Zone::Battlefield);
    game.main();
    let prompt = game.pending.take().unwrap().id;
    game.engine
        .answer(&game.table, prompt, Answer::Action(activate(shifter, 0)))
        .unwrap();
    let mut offered = Vec::new();
    loop {
        let Progress::NeedsChoice(c) = game.engine.advance(&game.table) else {
            continue;
        };
        match &c.kind {
            ChoiceKind::ChooseTargets { .. } => {
                let t = vec![vec![Target::Object(shifter)]];
                game.engine
                    .answer(&game.table, c.id, Answer::Targets(t))
                    .unwrap();
            }
            ChoiceKind::ChooseModes { available, .. } => {
                offered = available.iter().map(|l| l.to_string()).collect();
                let elf = offered.iter().position(|l| l == "Elf").unwrap() as u8;
                game.engine
                    .answer(&game.table, c.id, Answer::Modes(vec![elf]))
                    .unwrap();
            }
            ChoiceKind::Priority { .. } if game.stack().is_empty() => break,
            _ => game.engine.answer(&game.table, c.id, Answer::Pass).unwrap(),
        }
    }
    assert!(!offered.is_empty() && !offered.iter().any(|l| l == "Wall"));
    let ch = game
        .engine
        .characteristics(&game.table, shifter)
        .unwrap()
        .clone();
    let elf = mtg_ir::PrintedCards::subtype_named(&game.table, "Elf").unwrap();
    assert!(ch.subtypes.contains(&elf));
}
