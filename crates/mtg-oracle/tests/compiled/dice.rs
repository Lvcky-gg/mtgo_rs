//! Rolling a die (CR 706): a results table picks the row holding the number rolled, and
//! "the result" is that number.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, state::Rng};

fn rolled(game: &Game) -> u32 {
    game.engine
        .log
        .iter()
        .find_map(|e| match e.event {
            mtg_core::Event::DieRolled { result, .. } => Some(result),
            _ => None,
        })
        .expect("a roll")
}

/// Cast a sorcery with this text under each of several seeds, returning (roll, life).
fn cast_rolls(text: &str, mana: usize) -> Vec<(u32, i32)> {
    (0u8..24)
        .map(|seed| {
            let mut table = Table::default();
            let spell = table.card("{1}", "Sorcery", None, text);
            let mut game = Game::new(table);
            game.lands(mana);
            let id = game.put(spell, P0, Zone::Hand);
            game.engine.state.rng = Rng::from_seed(&[seed; 32]);
            game.main();
            game.act(Action::Cast { object: id }, &[], &[]);
            (rolled(&game), game.life(P0))
        })
        .collect()
}

#[test]
fn the_row_holding_the_roll_happens() {
    let results = cast_rolls(
        "Roll a d20.\n1—9 | You gain 1 life.\n10—19 | You gain 2 life.\n20 | You gain 5 life.",
        1,
    );
    for (roll, life) in &results {
        let gained = match roll {
            1..=9 => 1,
            10..=19 => 2,
            _ => 5,
        };
        assert_eq!(*life, 20 + gained, "rolled {roll}");
    }
    assert!(results.iter().any(|(r, _)| *r <= 9) && results.iter().any(|(r, _)| *r >= 10));
    assert!(results.iter().all(|(r, _)| (1..=20).contains(r)));
}

#[test]
fn the_result_is_the_number_rolled() {
    for (roll, life) in cast_rolls("Roll a d6. You gain life equal to the result.", 1) {
        assert!((1..=6).contains(&roll));
        assert_eq!(life, 20 + roll as i32);
    }
}

#[test]
fn x_in_a_row_is_still_the_spells_x() {
    let mut table = Table::default();
    let spell = table.card(
        "{X}",
        "Sorcery",
        None,
        "Roll a d20.\n1—20 | You gain X life.",
    );
    let mut game = Game::new(table);
    game.lands(3);
    let id = game.put(spell, P0, Zone::Hand);
    game.main();
    game.act(
        Action::Cast { object: id },
        &[],
        &[mtg_engine::choice::Answer::Number(3)],
    );
    let _ = rolled(&game);
    assert_eq!(game.life(P0), 23, "X is 3, whatever was rolled");
}
