use super::harness::*;
use mtg_core::{Event, Target, Zone, ZoneRef};

#[test]
fn targeted_reveal_shares_the_life_loss_target_and_keeps_cards_in_hand() {
    for count in 0..=2 {
        let mut t = Table::default();
        let card = t.card(
            "{0}",
            "Instant",
            None,
            "Target player loses 1 life and reveals their hand.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let hand: Vec<_> = (0..count).map(|_| g.put(bear, P1, Zone::Hand)).collect();
        let own = g.put(bear, P0, Zone::Hand);
        let spell = g.put(card, P0, Zone::Hand);
        g.main();
        let start = g.engine.log.len();
        g.cast(spell, &[Target::Player(P1)]);
        let mut revealed: Vec<_> = g.engine.log[start..]
            .iter()
            .filter_map(|e| match e.event {
                Event::Revealed { object } => Some(object),
                _ => None,
            })
            .collect();
        revealed.sort();
        assert_eq!(revealed, hand);
        assert_eq!(g.engine.state.players[&P1].life, 19);
        for object in hand.into_iter().chain([own]) {
            assert_eq!(
                g.engine.state.objects[&object].zone,
                ZoneRef::of(Zone::Hand, if object == own { P0 } else { P1 })
            );
        }
    }
}

#[test]
fn reveal_all_players_and_self_only_select_the_specified_hands() {
    for text in [
        "Each player reveals their hand.",
        "You reveal your hand.",
        "Target opponent reveals their hand.",
    ] {
        let mut t = Table::default();
        let card = t.card("{0}", "Instant", None, text);
        let bear = t.bear();
        let mut g = Game::new(t);
        g.put(bear, P0, Zone::Hand);
        g.put(bear, P1, Zone::Hand);
        let spell = g.put(card, P0, Zone::Hand);
        g.main();
        let mut expected = Vec::new();
        for player in [P0, P1] {
            if text.starts_with("Each")
                || (text.starts_with("You") && player == P0)
                || (text.starts_with("Target") && player == P1)
            {
                expected.extend(
                    g.engine
                        .state
                        .objects_in(ZoneRef::of(Zone::Hand, player))
                        .into_iter()
                        .filter(|id| *id != spell),
                );
            }
        }
        let start = g.engine.log.len();
        g.cast(
            spell,
            if text.starts_with("Target") {
                &[Target::Player(P1)]
            } else {
                &[]
            },
        );
        let mut actual: Vec<_> = g.engine.log[start..]
            .iter()
            .filter_map(|e| match e.event {
                Event::Revealed { object } => Some(object),
                _ => None,
            })
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
}
