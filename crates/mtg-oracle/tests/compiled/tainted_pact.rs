use super::harness::*;
use mtg_core::{CardId, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const PACT: &str = "Exile the top card of your library. You may put that card into your hand unless it has the same name as another card exiled this way. Repeat this process until you put a card into your hand or you exile two cards with the same name, whichever comes first.";

fn setup(names: &[&str]) -> (Game, mtg_core::ObjectId, Vec<CardId>) {
    let mut t = Table::default();
    let pact = t.card("{0}", "Instant", None, PACT);
    let cards: Vec<_> = names
        .iter()
        .map(|name| t.named_card(name, "{1}", "Artifact", None, ""))
        .collect();
    let mut g = Game::new(t);
    let spell = g.put(pact, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.insert(library, Vec::new());
    for card in &cards {
        g.put(*card, P0, Zone::Library);
    }
    (g, spell, cards)
}

fn exiled(g: &Game) -> usize {
    g.engine
        .state
        .objects
        .values()
        .filter(|o| o.owner == P0 && o.zone.zone == Zone::Exile)
        .count()
}

#[test]
fn accepting_a_later_unique_card_stops_and_retains_the_earlier_exile() {
    let (mut g, spell, cards) = setup(&["First", "Wanted", "Last"]);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(false), Answer::Bool(true)],
    );
    assert_eq!(exiled(&g), 1);
    assert_eq!(g.count(Zone::Library, P0), 1);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == cards[1] && o.zone.zone == Zone::Hand)
    );
}

#[test]
fn a_second_card_with_the_same_name_stops_without_offering_it() {
    let (mut g, spell, _) = setup(&["Same name", "Different", "Same name", "Untouched"]);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Bool(false), Answer::Bool(false)],
    );
    assert_eq!(exiled(&g), 3);
    assert_eq!(g.count(Zone::Library, P0), 1);
    assert_eq!(g.count(Zone::Hand, P0), 1);
}

#[test]
fn unrelated_prior_exiles_do_not_count_as_this_process() {
    let (mut g, spell, cards) = setup(&["A name"]);
    g.put(cards[0], P0, Zone::Exile);
    g.act(Action::Cast { object: spell }, &[], &[Answer::Bool(true)]);
    assert_eq!(exiled(&g), 1);
    assert_eq!(g.count(Zone::Hand, P0), 2);
}

#[test]
fn declining_every_unique_card_can_empty_the_library_without_drawing() {
    let (mut g, spell, _) = setup(&["A", "B", "C"]);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Bool(false),
            Answer::Bool(false),
            Answer::Bool(false),
        ],
    );
    assert_eq!(exiled(&g), 3);
    assert_eq!(g.count(Zone::Library, P0), 0);
    assert!(!g.engine.state.player(P0).attempted_draw_from_empty);
}

#[test]
fn split_cards_share_both_names_but_double_faced_cards_only_share_the_front() {
    for split in [true, false] {
        let (mut g, spell, cards) = setup(&["Front name", "Back name", "Untouched"]);
        let back = g.table.named_card("Back name", "{1}", "Artifact", None, "");
        g.table.pair(
            cards[0],
            back,
            if split {
                mtg_ir::Layout::Split
            } else {
                mtg_ir::Layout::ModalDfc
            },
        );
        let answers = if split {
            vec![Answer::Bool(false)]
        } else {
            vec![Answer::Bool(false), Answer::Bool(true)]
        };
        g.act(Action::Cast { object: spell }, &[], &answers);
        assert_eq!(exiled(&g), if split { 2 } else { 1 });
        assert_eq!(g.count(Zone::Library, P0), 1);
        assert_eq!(g.count(Zone::Hand, P0), if split { 1 } else { 2 });
    }
}
