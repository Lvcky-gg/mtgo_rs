use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn brainstorm_draws_first_and_places_two_chosen_cards_on_top_in_chosen_order() {
    let mut t = Table::default();
    let brainstorm = t.card(
        "{0}",
        "Instant",
        None,
        "Draw three cards, then put two cards from your hand on top of your library in any order.",
    );
    let first_card = t.bear();
    let second_card = t.card("{R}", "Creature", Some((1, 1)), "");
    let draw = t.card("{0}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    let first = g.put(first_card, P0, Zone::Hand);
    let second = g.put(second_card, P0, Zone::Hand);
    let spell = g.put(brainstorm, P0, Zone::Hand);
    let draw = g.put(draw, P0, Zone::Hand);
    g.engine.state.step = mtg_core::Step::PrecombatMain;
    g.main();
    let before = g.count(Zone::Library, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Objects(vec![first, second]),
            Answer::Objects(vec![second]),
        ],
    );
    assert_eq!(g.count(Zone::Library, P0), before - 1);
    let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    assert_eq!(g.engine.state.objects[&library[0]].card, second_card);
    assert_eq!(g.engine.state.objects[&library[1]].card, first_card);
    g.cast(draw, &[]);
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == second_card)
    );
}

#[test]
fn bottom_placement_retains_order_and_draws_the_number_placed_plus_one() {
    for take in 0..=3 {
        let mut t = Table::default();
        let spell_card = t.card("{0}", "Instant", None,
            "Put any number of cards from your hand on the bottom of your library, then draw that many cards plus one.");
        let card_ids: Vec<_> = (1..=3)
            .map(|n| t.card("{0}", "Creature", Some((n, n)), ""))
            .collect();
        let mut g = Game::new(t);
        let hand: Vec<_> = card_ids.iter().map(|c| g.put(*c, P0, Zone::Hand)).collect();
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.engine.state.step = mtg_core::Step::PrecombatMain;
        g.main();
        let before = g.count(Zone::Library, P0);
        let chosen = hand[..take].to_vec();
        let mut answers = vec![Answer::Objects(chosen.clone())];
        for id in chosen.iter().rev().take(take.saturating_sub(1)) {
            answers.push(Answer::Objects(vec![*id]));
        }
        g.act(Action::Cast { object: spell }, &[], &answers);
        assert_eq!(g.count(Zone::Hand, P0), 4);
        assert_eq!(g.count(Zone::Library, P0), before - 1);
        let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        let tail: Vec<_> = library[library.len() - take..]
            .iter()
            .map(|id| g.engine.state.objects[id].card)
            .collect();
        assert_eq!(
            tail,
            card_ids[..take].iter().rev().copied().collect::<Vec<_>>()
        );
    }
}

#[test]
fn optional_bottom_then_draw_requires_a_card_and_can_be_declined() {
    for (has_card, pay) in [(false, true), (true, false), (true, true)] {
        let mut t = Table::default();
        let spell_card = t.card("{0}", "Instant", None,
            "You may put a card from your hand on the bottom of your library. If you do, draw a card.");
        let keep_card = t.card("{R}", "Creature", Some((1, 1)), "");
        let mut g = Game::new(t);
        let keep = has_card.then(|| g.put(keep_card, P0, Zone::Hand));
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.engine.state.step = mtg_core::Step::PrecombatMain;
        g.main();
        let before = g.count(Zone::Library, P0);
        let mut answers = vec![Answer::Bool(pay)];
        if let Some(keep) = keep {
            answers.push(Answer::Objects(vec![keep]));
        }
        g.act(Action::Cast { object: spell }, &[], &answers);
        assert_eq!(g.count(Zone::Hand, P0), usize::from(has_card));
        assert_eq!(g.count(Zone::Library, P0), before);
        let in_hand = g
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == keep_card);
        assert_eq!(in_hand, has_card && !pay);
    }
}

#[test]
fn mandatory_placement_does_as_much_as_possible_and_does_not_take_opponents_cards() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Put two cards from your hand on top of your library in any order.",
    );
    let keep_card = t.card("{R}", "Creature", Some((1, 1)), "");
    let mut g = Game::new(t);
    let keep = g.put(keep_card, P0, Zone::Hand);
    let opponent = g.put(keep_card, P1, Zone::Hand);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.engine.state.step = mtg_core::Step::PrecombatMain;
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![opponent, keep])],
    );
    assert_eq!(g.count(Zone::Hand, P0), 0);
    assert_eq!(
        g.engine.state.objects[&opponent].zone,
        ZoneRef::of(Zone::Hand, P1)
    );
    let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    assert_eq!(g.engine.state.objects[&library[0]].card, keep_card);
}
