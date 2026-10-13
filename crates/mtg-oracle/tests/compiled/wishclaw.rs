use super::harness::*;
use mtg_core::{PlayerId, Target, Zone};
use mtg_engine::{choice::Answer, state::PlayerState};

const WISHCLAW: &str = "This artifact enters with three wish counters on it.\n{1}, {T}, Remove a wish counter from this artifact: Search your library for a card, put it into your hand, then shuffle. An opponent gains control of this artifact. Activate only during your turn.";

#[test]
fn tutor_finishes_before_the_chosen_opponent_receives_wishclaw() {
    for multiplayer in [false, true] {
        let mut t = Table::default();
        let wishclaw = t.card("{0}", "Artifact", None, WISHCLAW);
        let wanted = t.card("{4}", "Sorcery", None, "Draw a card.");
        let mut g = Game::new(t);
        if multiplayer {
            let p2 = PlayerId(2);
            g.engine.state.players.insert(p2, PlayerState::new(p2, 20));
            g.engine.state.turn_order.push(p2);
        }
        g.lands(1);
        let card = g.put(wishclaw, P0, Zone::Hand);
        g.main();
        g.cast(card, &[]);
        let source = g.find(wishclaw).unwrap();
        let wanted = g.put(wanted, P0, Zone::Library);
        let hand = g.count(Zone::Hand, P0);
        let mut answers = vec![Answer::Objects(vec![wanted])];
        if multiplayer {
            answers.push(Answer::Modes(vec![1]));
        }
        g.act(activate(source, 1), &[], &answers);
        assert_eq!(g.count(Zone::Hand, P0), hand + 1);
        let recipient = if multiplayer { PlayerId(2) } else { P1 };
        assert_eq!(
            mtg_engine::layers::controller(&g.engine.state, source),
            Some(recipient)
        );
        assert_eq!(g.engine.state.objects[&source].owner, P0);
        assert!(g.engine.state.objects[&source].tapped);
        assert_eq!(
            g.engine.state.objects[&source]
                .counters
                .values()
                .sum::<i32>(),
            2
        );
    }
}

#[test]
fn targeted_recipient_gets_control_without_becoming_the_effects_controller() {
    let mut t = Table::default();
    let gift = t.card(
        "{0}",
        "Sorcery",
        None,
        "Target opponent gains control of target creature until end of turn. You gain 3 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(gift, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Player(P1), Target::Object(creature)]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(g.life(P1), 20);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P1)
    );
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, creature),
        Some(P0)
    );
}
