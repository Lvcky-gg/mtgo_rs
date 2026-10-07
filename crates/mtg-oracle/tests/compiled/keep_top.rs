//! Keeping one looked-at card on top preserves the unseen library below it.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn sage_of_days_can_keep_any_one_card_or_mill_all_three() {
    for pick in [None, Some(0), Some(1), Some(2)] {
        let mut t = Table::default();
        let sage = t.card("{2}{U}", "Creature — Bear", Some((3, 2)),
            "When this creature enters, look at the top three cards of your library. You may put one of those cards back on top of your library. Put the rest into your graveyard.");
        let first = t.bear();
        let second = t.card("{2}", "Artifact", None, "");
        let third = t.card("{U}", "Instant", None, "Draw a card.");
        let mut g = Game::new(t);
        g.lands(3);
        let spell = g.put(sage, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[first, second, third, first]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..3],
            0,
            1,
            pick.map_or_else(Vec::new, |i| vec![ids[i]]),
        );
        let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        if let Some(i) = pick {
            assert_eq!(
                g.engine.state.objects[&lib[0]].card,
                [first, second, third][i]
            );
            assert_eq!(lib[1], ids[3]);
        } else {
            assert_eq!(lib[0], ids[3]);
        }
        assert_eq!(
            g.count(Zone::Graveyard, P0),
            3 - usize::from(pick.is_some())
        );
        assert!(g.engine.state.revealed_cards.is_empty());
    }
}

#[test]
fn up_to_one_on_top_and_the_rest_on_bottom_can_be_declined() {
    for take in [false, true] {
        let mut t = Table::default();
        let spell = t.card("{U}", "Sorcery", None,
            "Look at the top three cards of your library. Put up to one of them on top of your library and the rest on the bottom of your library in a random order.");
        let first = t.bear();
        let second = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[first, second, first, second]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..3],
            0,
            1,
            if take { vec![ids[1]] } else { vec![] },
        );
        let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        if take {
            assert_eq!(g.engine.state.objects[&lib[0]].card, second);
            assert_eq!(lib[1], ids[3]);
        } else {
            assert_eq!(lib[0], ids[3]);
        }
        let bottom = &lib[lib.len() - (3 - usize::from(take))..];
        assert_eq!(
            bottom
                .iter()
                .filter(|id| g.engine.state.objects[id].card == first)
                .count(),
            2
        );
    }
}

#[test]
fn multiple_kept_cards_with_unspecified_order_remain_rejected() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    let face = FaceText {
        name: "Unsupported",
        card_types: &[mtg_core::CardType::Sorcery],
        subtypes: &[],
        oracle_text: Some(
            "Look at the top four cards of your library. Put two of those cards on top of your library and the rest into your graveyard.",
        ),
        mana_cost: "{U}",
    };
    assert!(!compile(&face, &SubtypeNames(vec![])).understood());
}
