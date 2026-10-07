//! Repeated card nouns preserve individual search restrictions.
use super::{dig::resolve_selection, harness::*};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn basic_land_or_desert_allows_nonbasic_deserts_but_not_other_nonbasics() {
    for pick in [None, Some(0), Some(1)] {
        let mut t = Table::default();
        let tutor = t.card("{G}", "Sorcery", None,
            "Search your library for a basic land card or a Desert card, reveal it, put it into your hand, then shuffle.");
        let basic = t.card("", "Basic Land — Forest", None, "");
        let desert = t.card("", "Land — Desert", None, "");
        let other = t.card("", "Land — Forest", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let ids: Vec<_> = [basic, desert, other]
            .into_iter()
            .map(|card| g.put(card, P0, Zone::Library))
            .collect();
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..2],
            0,
            1,
            pick.map_or_else(Vec::new, |i| vec![ids[i]]),
        );
        let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
        for (i, card) in [basic, desert, other].into_iter().enumerate() {
            assert_eq!(
                hand.iter()
                    .any(|id| g.engine.state.objects[id].card == card),
                pick == Some(i)
            );
        }
        assert_eq!(
            g.engine.state.revealed_cards.len(),
            usize::from(pick.is_some())
        );
    }
}

#[test]
fn instant_or_card_with_flash_can_find_flash_creatures_only_in_second_branch() {
    let mut t = Table::default();
    let tutor = t.card("{3}{U}", "Instant", None,
        "Search your library for an instant card or a card with flash, reveal it, put it into your hand, then shuffle.");
    let instant = t.card("{U}", "Instant", None, "Draw a card.");
    let flash = t.card("{U}", "Creature — Bear", Some((1, 1)), "Flash");
    let bear = t.bear();
    let sorcery = t.card("{U}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(4);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let ids: Vec<_> = [instant, flash, bear, sorcery]
        .into_iter()
        .map(|card| g.put(card, P0, Zone::Library))
        .collect();
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..2],
        0,
        1,
        vec![ids[1]],
    );
    assert!(g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == flash));
}

#[test]
fn repeated_and_or_nouns_and_multi_card_or_nouns_remain_rejected() {
    use mtg_oracle::compile::{compile, FaceText, SubtypeNames};
    for text in [
        "Search your library for an instant card and/or a sorcery card, reveal them, put them into your hand, then shuffle.",
        "Search your library for two basic land cards or two Desert cards, reveal them, put them into your hand, then shuffle.",
    ] {
        let face = FaceText { name: "Separate quotas", card_types: &[mtg_core::CardType::Sorcery], subtypes: &[], oracle_text: Some(text), mana_cost: "{G}" };
        assert!(!compile(&face, &SubtypeNames(vec![])).understood());
    }
}

#[test]
fn second_alternative_mana_value_limit_does_not_restrict_the_first() {
    let mut t = Table::default();
    let tutor = t.card("{G}", "Sorcery", None,
        "Search your library for an artifact card or a creature card with mana value 1 or less, reveal it, put it into your hand, then shuffle.");
    let artifact = t.card("{6}", "Artifact", None, "");
    let small = t.card("{G}", "Creature — Bear", Some((1, 1)), "");
    let large = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let ids: Vec<_> = [artifact, small, large]
        .into_iter()
        .map(|card| g.put(card, P0, Zone::Library))
        .collect();
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..2],
        0,
        1,
        vec![ids[0]],
    );
    assert!(g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == artifact));
}
