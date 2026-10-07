use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn targeted_opponents_tapped_creatures_are_counted_at_resolution() {
    for count in 0..=3 {
        let mut t = Table::default();
        let spell_card = t.card(
            "{0}",
            "Sorcery",
            None,
            "Draw a card for each tapped creature target opponent controls.",
        );
        let bear = t.bear();
        let land = t.mountain();
        let mut g = Game::new(t);
        let chosen: Vec<_> = (0..count)
            .map(|_| g.put(bear, P1, Zone::Battlefield))
            .collect();
        let own = g.put(bear, P0, Zone::Battlefield);
        let untapped = g.put(bear, P1, Zone::Battlefield);
        let opponent_land = g.put(land, P1, Zone::Battlefield);
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        for id in chosen.iter().copied().chain([own, opponent_land]) {
            g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
        }
        assert!(!g.engine.state.objects[&untapped].tapped);
        let before = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: spell }, &[Target::Player(P1)], &[]);
        assert_eq!(g.count(Zone::Hand, P0), before - 1 + count);
    }
}

#[test]
fn counted_target_can_be_you_and_is_distinct_from_an_earlier_object_target() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +1/+1 until end of turn for each Mountain target player controls.",
    );
    let mountain = t.mountain();
    let bear = t.bear();
    let mut g = Game::new(t);
    for _ in 0..3 {
        g.put(mountain, P0, Zone::Battlefield);
    }
    g.put(mountain, P1, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(bear), Target::Player(P0)],
        &[],
    );
    assert_eq!(g.pt(bear), (5, 5));
}

#[test]
fn changes_in_response_affect_the_count_and_an_illegal_player_target_fizzles() {
    for hexproof in [false, true] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{0}",
            "Instant",
            None,
            "You gain 2 life for each Mountain target opponent controls.",
        );
        let response_card = t.card("{0}", "Instant", None, "Destroy target land.");
        let ward = t.card("{0}", "Enchantment", None, "You have hexproof.");
        let mountain = t.mountain();
        let mut g = Game::new(t);
        let land = g.put(mountain, P1, Zone::Battlefield);
        let spell = g.put(spell_card, P0, Zone::Hand);
        let response = g.put(response_card, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: spell }, &[Target::Player(P1)]);
        if hexproof {
            g.put(ward, P1, Zone::Battlefield);
            g.main();
        } else {
            g.cast(response, &[Target::Object(land)]);
        }
        assert_eq!(g.life(P0), 20);
    }
}
