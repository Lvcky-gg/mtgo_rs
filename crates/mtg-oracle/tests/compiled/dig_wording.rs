//! Numeric library selection wording and hand remainders.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn one_of_those_cards_has_the_same_mandatory_quota_as_one_of_them() {
    for lead in [
        "Look at the top three cards of your library. ",
        "Look at the top three cards of your library, then ",
    ] {
        let mut t = Table::default();
        let spell = t.card(
            "{U}",
            "Sorcery",
            None,
            &format!(
                "{lead}Put one of those cards into your hand and the rest into your graveyard."
            ),
        );
        let first = t.bear();
        let chosen = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[first, chosen, first]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids,
            1,
            1,
            vec![ids[1]],
        );
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Hand, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == chosen)
        );
        assert_eq!(g.count(Zone::Graveyard, P0), 3);
    }
}

#[test]
fn optional_numeric_selection_can_leave_all_cards_for_the_remainder() {
    let mut t = Table::default();
    let spell = t.card("{U}", "Sorcery", None,
        "Look at the top three cards of your library. You may put one of those cards into your hand and the rest into your graveyard.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[bear, bear, bear]);
    resolve_selection(&mut g, Action::Cast { object: spell }, &ids, 0, 1, vec![]);
    assert_eq!(g.count(Zone::Graveyard, P0), 4);
}

#[test]
fn numeric_selection_is_clamped_to_the_cards_actually_in_the_library() {
    let mut t = Table::default();
    let spell = t.card("{U}", "Sorcery", None,
        "Look at the top four cards of your library. Put two of those cards into your hand and the rest into your graveyard.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.remove(&library);
    let ids = stack_top(&mut g, &[bear]);
    resolve_selection(&mut g, Action::Cast { object: spell }, &ids, 1, 1, vec![]);
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == bear)
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn genesis_ultimatum_moves_unselected_permanents_and_spells_into_hand_then_exiles_itself() {
    let mut t = Table::default();
    let spell_card = t.card("{G}{G}{U}{U}{U}{R}{R}", "Sorcery", None,
        "Look at the top five cards of your library. Put any number of permanent cards from among them onto the battlefield and the rest into your hand. Exile ~.");
    let chosen = t.bear();
    let unselected = t.card("{2}", "Artifact", None, "");
    let instant = t.card("{U}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(7);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[chosen, unselected, instant, instant, instant]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..2],
        0,
        2,
        vec![ids[0]],
    );
    assert!(g.find(chosen).is_some());
    assert!(g.find(unselected).is_none());
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
    assert_eq!(
        hand.iter()
            .filter(|id| g.engine.state.objects[id].card == instant)
            .count(),
        3
    );
    assert!(
        hand.iter()
            .any(|id| g.engine.state.objects[id].card == unselected)
    );
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .iter()
            .any(|id| g.engine.state.objects[id].card == spell_card)
    );
}
