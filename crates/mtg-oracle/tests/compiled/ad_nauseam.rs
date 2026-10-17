use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const NAUSEAM: &str = "Reveal the top card of your library and put that card into your hand. You lose life equal to its mana value. You may repeat this process any number of times.";

fn game() -> (Game, mtg_core::ObjectId) {
    let mut t = Table::default();
    let nauseam = t.card("{0}", "Instant", None, NAUSEAM);
    let cards: Vec<_> = ["{3}", "{0}", "{4}"]
        .into_iter()
        .map(|cost| t.card(cost, "Artifact", None, ""))
        .collect();
    let mut g = Game::new(t);
    let spell = g.put(nauseam, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.insert(library, Vec::new());
    for card in cards {
        g.put(card, P0, Zone::Library);
    }
    (g, spell)
}

#[test]
fn declining_repetition_still_performs_the_process_once() {
    let (mut g, spell) = game();
    let hand = g.count(Zone::Hand, P0);
    g.act(Action::Cast { object: spell }, &[], &[Answer::Bool(false)]);
    assert_eq!(g.life(P0), 17);
    assert_eq!(g.count(Zone::Library, P0), 2);
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn each_repetition_reads_the_next_cards_mana_value() {
    let (mut g, spell) = game();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(true), Answer::Bool(true), Answer::Bool(false)],
    );
    assert_eq!(g.life(P0), 13);
    assert_eq!(g.count(Zone::Library, P0), 0);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
}

#[test]
fn repeating_with_an_empty_library_does_not_reuse_the_last_cards_value() {
    let (mut g, spell) = game();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Bool(true),
            Answer::Bool(true),
            Answer::Bool(true),
            Answer::Bool(false),
        ],
    );
    assert_eq!(g.life(P0), 13);
    assert!(!g.engine.state.player(P0).attempted_draw_from_empty);
}

#[test]
fn the_repetition_choice_shows_the_new_card_and_updated_life() {
    let (mut g, spell) = game();
    let hand = g.count(Zone::Hand, P0);
    let choice = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            choice.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let seen = std::cell::Cell::new(false);
    g.drive(
        |g, c| {
            if c.because.contains("repeat this process") {
                seen.set(true);
                let view = g.engine.view_for(P0);
                assert_eq!(view.players[&P0].life, 17);
                assert_eq!(
                    view.visible
                        .values()
                        .filter(|o| o.zone.player == Some(P0) && o.zone.zone == Zone::Hand)
                        .count(),
                    hand
                );
                Some(Answer::Bool(false))
            } else {
                None
            }
        },
        |g| seen.get() && g.stack().is_empty(),
    );
    assert!(seen.get());
    assert_eq!(g.life(P0), 17);
}

/// Saying yes every time ends with the library: a repeat that reveals nothing isn't offered
/// again. An unattended player once accepted forever, and each acceptance replayed the rest.
#[test]
fn always_repeating_stops_when_the_library_is_empty() {
    let (mut g, spell) = game();
    let hand = g.count(Zone::Hand, P0);
    // Three cards: asked after each of them, and not again once nothing is left to reveal.
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(true), Answer::Bool(true), Answer::Bool(true)],
    );
    assert_eq!(g.life(P0), 13);
    assert_eq!(g.count(Zone::Library, P0), 0);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
}
