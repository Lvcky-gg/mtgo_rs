use super::harness::*;
use mtg_core::{PlayerId, Zone, ZoneRef};
use mtg_engine::state::PlayerState;

#[test]
fn windfall_uses_the_largest_discard_count_and_draws_for_every_player() {
    // Include an empty hand, a tie, and the caster holding the largest hand.
    for sizes in [[0, 5, 2], [4, 1, 4], [6, 2, 1], [0, 0, 0]] {
        let mut t = Table::default();
        let windfall = t.card("{0}", "Sorcery", None,
            "Each player discards their hand, then draws cards equal to the greatest number of cards a player discarded this way.");
        let filler = t.bear();
        let mut g = Game::new(t);
        let p2 = PlayerId(2);
        g.engine.state.players.insert(p2, PlayerState::new(p2, 20));
        g.engine.state.turn_order.push(p2);
        for _ in 0..20 {
            g.put(filler, p2, Zone::Library);
        }
        let spell = g.put(windfall, P0, Zone::Hand);
        g.main();
        // Set exact starting hands after the normal draw step.
        for (player, size) in [P0, P1, p2].into_iter().zip(sizes) {
            for id in g.engine.state.objects_in(ZoneRef::of(Zone::Hand, player)) {
                if id != spell {
                    g.engine.state.objects.remove(&id);
                }
            }
            g.engine
                .state
                .zone_order
                .entry(ZoneRef::of(Zone::Hand, player))
                .or_default()
                .retain(|id| *id == spell);
            for _ in 0..size {
                g.put(filler, player, Zone::Hand);
            }
        }
        let libraries: Vec<_> = [P0, P1, p2]
            .into_iter()
            .map(|p| g.count(Zone::Library, p))
            .collect();
        g.cast(spell, &[]);
        let most = sizes.into_iter().max().unwrap();
        for (i, player) in [P0, P1, p2].into_iter().enumerate() {
            assert_eq!(g.count(Zone::Hand, player), most);
            assert_eq!(
                g.count(Zone::Graveyard, player),
                sizes[i] + usize::from(player == P0)
            );
            assert_eq!(g.count(Zone::Library, player), libraries[i] - most);
        }
    }
}

#[test]
fn narset_and_windfall_limit_opponents_to_one_draw_and_count_earlier_draws() {
    use mtg_engine::actions::Action;
    for drew_already in [false, true] {
        let mut t = Table::default();
        let narset = t.loyalty("{1}{U}{U}", "Legendary Planeswalker", 5,
            "Each opponent can't draw more than one card each turn.\n−2: Look at the top four cards of your library. You may reveal a noncreature, nonland card from among them and put it into your hand. Put the rest on the bottom of your library in a random order.");
        let windfall = t.card("{0}", "Sorcery", None,
            "Each player discards their hand, then draws cards equal to the greatest number of cards a player discarded this way.");
        let filler = t.bear();
        let draw = t.card("{0}", "Instant", None, "Target player draws a card.");
        let mut g = Game::new(t);
        let spell = g.put(windfall, P0, Zone::Hand);
        for _ in 0..3 {
            g.put(filler, P1, Zone::Hand);
        }
        let earlier_draw = drew_already.then(|| g.put(draw, P0, Zone::Hand));
        g.main();
        if let Some(spell) = earlier_draw {
            g.cast(spell, &[mtg_core::Target::Player(P1)]);
        }
        // Place Narset after any earlier draw to ensure the limit counts the whole turn.
        let narset = g.put(narset, P0, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&narset)
            .unwrap()
            .counters
            .insert(mtg_core::CounterKind::Loyalty, 5);
        g.act(Action::Cast { object: spell }, &[], &[]);
        assert_eq!(g.count(Zone::Hand, P0), if drew_already { 4 } else { 3 });
        assert_eq!(g.count(Zone::Hand, P1), usize::from(!drew_already));
    }
}

#[test]
fn narset_limit_resets_each_turn_and_disappears_when_she_leaves() {
    let mut t = Table::default();
    let narset = t.loyalty(
        "{1}{U}{U}",
        "Legendary Planeswalker",
        5,
        "Each opponent can't draw more than one card each turn.",
    );
    let draw = t.card("{0}", "Instant", None, "Target player draws three cards.");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target permanent to its owner's hand.",
    );
    let mut g = Game::new(t);
    let narset = g.put(narset, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&narset)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 5);
    let spells: Vec<_> = (0..3).map(|_| g.put(draw, P0, Zone::Hand)).collect();
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Hand, P1);
    g.cast(spells[0], &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P1), before + 1);
    g.cast(spells[1], &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P1), before + 1);
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(
        g.count(Zone::Hand, P1),
        before + 2,
        "new turn allows the normal draw"
    );
    let prompt = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, prompt.id, mtg_engine::choice::Answer::Pass)
        .unwrap();
    g.drive(|_, _| None, |g| g.engine.state.priority == Some(P0));
    g.cast(bounce, &[mtg_core::Target::Object(narset)]);
    g.cast(spells[2], &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P1), before + 5);
}
