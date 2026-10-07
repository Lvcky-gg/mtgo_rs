//! Library selection retains the printed battlefield entry state.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn cartographers_survey_selects_up_to_two_lands_and_taps_them() {
    for take in 0..=2 {
        let mut t = Table::default();
        let survey = t.card("{3}{G}", "Sorcery", None,
            "Look at the top seven cards of your library. Put up to two land cards from among them onto the battlefield tapped. Put the rest on the bottom of your library in a random order.");
        let land = t.card("", "Land", None, "");
        let other = t.card("", "Land", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(4);
        let spell = g.put(survey, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[bear, land, bear, other, bear, bear, bear]);
        let candidates = vec![ids[1], ids[3]];
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &candidates,
            0,
            2,
            candidates[..take].to_vec(),
        );
        for (i, card) in [land, other].into_iter().enumerate() {
            if i < take {
                assert!(g.engine.state.objects[&g.find(card).unwrap()].tapped);
            } else {
                assert!(g.find(card).is_none());
            }
        }
        let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        let bottom = &lib[lib.len() - (7 - take)..];
        assert!(bottom
            .iter()
            .all(|id| [bear, land, other].contains(&g.engine.state.objects[id].card)));
    }
}

#[test]
fn revealed_selection_can_enter_tapped_and_bin_the_rest() {
    let mut t = Table::default();
    let spell = t.card("{G}", "Sorcery", None,
        "Reveal the top three cards of your library. You may put a land card from among them onto the battlefield tapped. Put the rest into your graveyard.");
    let land = t.card("", "Land", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[bear, land, bear]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &[ids[1]],
        0,
        1,
        vec![ids[1]],
    );
    assert!(g.engine.state.objects[&g.find(land).unwrap()].tapped);
    assert_eq!(g.count(Zone::Graveyard, P0), 3);
    assert_eq!(g.engine.state.revealed_cards.len(), 3);
}

#[test]
fn selection_without_tapped_preserves_untapped_entry() {
    let mut t = Table::default();
    let spell = t.card("{G}", "Sorcery", None,
        "Look at the top three cards of your library. You may put a land card from among them onto the battlefield. Put the rest into your graveyard.");
    let land = t.card("", "Land", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[bear, land, bear]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &[ids[1]],
        0,
        1,
        vec![ids[1]],
    );
    assert!(!g.engine.state.objects[&g.find(land).unwrap()].tapped);
}
