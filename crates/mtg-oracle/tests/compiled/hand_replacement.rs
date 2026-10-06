use super::harness::*;
use mtg_core::{Step, Target, Zone};

#[test]
fn discard_and_redraw_remembers_the_hand_before_discarding() {
    for cards in [0, 3] {
        for extra in [false, true] {
            let mut table = Table::default();
            let text = if extra {
                "Discard all the cards in your hand, then draw that many cards plus one."
            } else {
                "Discard all the cards in your hand, then draw that many cards."
            };
            let spell = table.card("{U}", "Instant", None, text);
            let filler = table.bear();
            let mut game = Game::new(table);
            game.lands(1);
            let spell = game.put(spell, P0, Zone::Hand);
            let old_hand: Vec<_> = (0..cards)
                .map(|_| game.put(filler, P0, Zone::Hand))
                .collect();
            let opponent = game.put(filler, P1, Zone::Hand);
            game.engine.state.step = Step::PrecombatMain;
            game.main();
            game.cast(spell, &[]);
            assert_eq!(game.count(Zone::Hand, P0), cards + usize::from(extra));
            assert_eq!(game.count(Zone::Graveyard, P0), cards + 1);
            assert!(
                old_hand
                    .iter()
                    .all(|id| !game.engine.state.objects.contains_key(id))
            );
            assert!(game.engine.state.objects.contains_key(&opponent));
        }
    }
}

#[test]
fn hand_redraw_composes_with_damage_based_on_the_original_hand() {
    let mut table = Table::default();
    let spell = table.card("{2}{R}{R}", "Instant", None,
        "~ deals damage to any target equal to the number of cards in your hand. Discard all the cards in your hand, then draw that many cards.");
    let filler = table.bear();
    let mut game = Game::new(table);
    game.lands(4);
    let spell = game.put(spell, P0, Zone::Hand);
    for _ in 0..3 {
        game.put(filler, P0, Zone::Hand);
    }
    game.engine.state.step = Step::PrecombatMain;
    game.main();
    game.cast(spell, &[Target::Player(P1)]);
    assert_eq!(game.life(P1), 17);
    assert_eq!(game.count(Zone::Hand, P0), 3);
}
