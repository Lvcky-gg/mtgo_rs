//! Library searches: to the top, by name, onto the battlefield.
use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn sacrificed_fetch_land_puts_the_selected_land_onto_the_battlefield() {
    let mut t = Table::default();
    let fetch = t.card("", "Land", None,
        "{T}, Pay 1 life, Sacrifice this land: Search your library for a Mountain or Forest card, put it onto the battlefield, then shuffle.");
    let mountain = t.card("", "Basic Land — Mountain", None, "");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    let source = g.put(fetch, P0, Zone::Battlefield);
    let pick = g.put(forest, P0, Zone::Library);
    g.put(mountain, P0, Zone::Library);
    g.main();
    g.act(activate(source, 0), &[], &[Answer::Objects(vec![pick])]);
    assert_eq!(g.life(P0), 19);
    assert!(g.find(fetch).is_none());
    let found = g
        .find(forest)
        .expect("selected forest reaches the battlefield");
    assert!(!g.engine.state.objects[&found].tapped);
    assert_eq!(g.engine.state.objects[&found].controller, P0);
}

#[test]
fn a_tutor_to_the_top_puts_the_card_on_top_after_shuffling() {
    let mut t = Table::default();
    let tutor = t.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Search your library for a card, reveal it, then shuffle and put that card on top.",
    );
    let marker = t.card("{5}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(tutor, P0, Zone::Hand);
    g.main();
    let m = g.put(marker, P0, Zone::Library);
    g.act(Action::Cast { object: s }, &[], &[Answer::Objects(vec![m])]);
    let lib = ZoneRef::of(Zone::Library, P0);
    let top = g.engine.state.objects_in(lib)[0];
    assert_eq!(g.engine.state.objects[&top].card, marker);
}

#[test]
fn searching_for_cards_named_like_this_one() {
    let mut t = Table::default();
    let rat = t.card(
        "{1}{B}",
        "Creature — Rat",
        Some((1, 1)),
        "When this creature enters, you may search your library for any number of cards named \
         ~, reveal them, put them into your hand, then shuffle.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(rat, P0, Zone::Hand);
    g.main();
    let r2 = g.put(rat, P0, Zone::Library);
    let r3 = g.put(rat, P0, Zone::Library);
    let b = g.put(bear, P0, Zone::Library);
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: r },
        &[],
        &[Answer::Bool(true), Answer::Objects(vec![r2, r3])],
    );
    let _ = (hand, b);
    let in_hand: Vec<_> = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(in_hand.iter().filter(|c| **c == rat).count(), 2);
    assert!(!in_hand.contains(&bear));
}

#[test]
fn search_for_a_card_and_put_it_into_the_graveyard() {
    let mut t = Table::default();
    let entomb = t.card(
        "{B}",
        "Instant",
        None,
        "Search your library for a creature card, reveal that card, put it into your \
         graveyard, then shuffle.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(entomb, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P0);
    let pick = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[5];
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![pick])],
    );
    assert_eq!(g.count(Zone::Library, P0), before - 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        2,
        "the found card and the instant"
    );
}
