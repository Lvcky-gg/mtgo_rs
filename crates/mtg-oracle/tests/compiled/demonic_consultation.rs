//! Demonic Consultation: "Choose a card name. Exile the top six cards of your library, then
//! reveal cards from the top of your library until you reveal a card with the chosen name.
//! Put that card into your hand and exile all other cards revealed this way."

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::choice::{Answer, ChoiceKind};

const CONSULTATION: &str = "Choose a card name. Exile the top six cards of your library, then \
                            reveal cards from the top of your library until you reveal a card \
                            with the chosen name. Put that card into your hand and exile all \
                            other cards revealed this way.";

/// Cast it naming `name` (or, with `None`, a card not in the deck), with `wanted` placed
/// `depth` cards from the top of a library of 19 fillers (one was drawn).
fn consult(name: Option<&str>, depth: usize) -> (Game, mtg_core::ObjectId) {
    let mut t = Table::default();
    let spell = t.card("{B}", "Instant", None, CONSULTATION);
    let wanted = t.named_card("Wanted Card", "{U}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(1);
    let library = ZoneRef::of(Zone::Library, P0);
    let wanted = g.put(wanted, P0, Zone::Library);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let order = g.engine.state.zone_order.get_mut(&library).unwrap();
    order.retain(|o| *o != wanted);
    order.insert(depth, wanted);
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(mtg_engine::actions::Action::Cast { object: spell }),
        )
        .unwrap();
    let asked = std::cell::Cell::new(false);
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseModes { available, .. } if c.who == P0 => {
                asked.set(true);
                let i = match name {
                    Some(n) => available.iter().position(|a| &**a == n).expect("offered"),
                    None => available.len() - 1,
                };
                Some(Answer::Modes(vec![i as u8]))
            }
            _ => None,
        },
        |g| asked.get() && g.stack().is_empty(),
    );
    (g, wanted)
}

fn exiled(g: &Game) -> usize {
    g.engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .len()
}

#[test]
fn finds_the_named_card_below_the_top_six() {
    let (g, _) = consult(Some("Wanted Card"), 10);
    assert_eq!(exiled(&g), 10, "six, then four revealed");
    assert_eq!(g.count(Zone::Library, P0), 9);
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
    assert!(hand.iter().any(|o| {
        mtg_ir::PrintedCards::face(&g.table, g.engine.state.objects[o].card, 0)
            .is_some_and(|f| &*f.name == "Wanted Card")
    }));
}

#[test]
fn naming_a_card_not_in_the_deck_exiles_the_whole_library() {
    let (g, _) = consult(None, 3);
    assert_eq!(g.count(Zone::Library, P0), 0);
    assert_eq!(exiled(&g), 20);
}

#[test]
fn a_named_card_among_the_top_six_is_exiled_with_them() {
    let (g, _) = consult(Some("Wanted Card"), 2);
    assert_eq!(g.count(Zone::Library, P0), 0);
    assert_eq!(exiled(&g), 20);
}
