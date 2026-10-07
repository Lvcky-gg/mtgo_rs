use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn targeted_player_selects_and_orders_their_own_hand_cards() {
    for bottom in [false, true] {
        let mut t = Table::default();
        let spell_card = t.card("{0}", "Instant", None, if bottom {
            "Target opponent puts two cards from their hand on the bottom of their library in any order."
        } else {
            "Target player puts two cards from their hand on top of their library in any order."
        });
        let first_card = t.card("{R}", "Creature", Some((1, 1)), "");
        let second_card = t.card("{G}", "Creature", Some((3, 3)), "");
        let mut g = Game::new(t);
        let first = g.put(first_card, P1, Zone::Hand);
        let second = g.put(second_card, P1, Zone::Hand);
        let own = g.put(first_card, P0, Zone::Hand);
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let before = g.count(Zone::Library, P1);
        g.act(
            Action::Cast { object: spell },
            &[Target::Player(P1)],
            &[
                Answer::Objects(vec![own, first, second]),
                Answer::Objects(vec![second]),
            ],
        );
        assert_eq!(
            g.engine.state.objects[&own].zone,
            ZoneRef::of(Zone::Hand, P0)
        );
        assert_eq!(g.count(Zone::Hand, P1), 0);
        assert_eq!(g.count(Zone::Library, P1), before + 2);
        let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P1));
        let start = if bottom { library.len() - 2 } else { 0 };
        assert_eq!(g.engine.state.objects[&library[start]].card, second_card);
        assert_eq!(g.engine.state.objects[&library[start + 1]].card, first_card);
    }
}

#[test]
fn empty_hand_does_not_take_the_casters_cards() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Target opponent puts a card from their hand on top of their library.",
    );
    let mut g = Game::new(t);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P1);
    let own = g.count(Zone::Hand, P0);
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Library, P1), before);
    assert_eq!(g.count(Zone::Hand, P0), own - 1);
}
