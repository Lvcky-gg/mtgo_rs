//! Fixed-name tutors select only the printed names, including punctuation.
use super::{dig::resolve_selection, harness::*};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn named_alternatives_find_either_name_or_nothing_and_enter_tapped() {
    for pick in [None, Some(0), Some(1)] {
        let mut t = Table::default();
        let witch = t.card("{2}", "Sorcery", None,
            "Search your library for a card named Festering Newt or Bubbling Cauldron, put it onto the battlefield tapped, then shuffle.");
        let newt = t.named_card("Festering Newt", "{B}", "Creature — Bear", Some((1, 1)), "");
        let cauldron = t.named_card("Bubbling Cauldron", "{2}", "Artifact", None, "");
        let near = t.named_card("Bubbling Cauldron Replica", "{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(witch, P0, Zone::Hand);
        g.main();
        let ids: Vec<_> = [newt, cauldron, near]
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
        for (i, card) in [newt, cauldron, near].into_iter().enumerate() {
            assert_eq!(g.find(card).is_some(), pick == Some(i));
            if let Some(id) = g.find(card) {
                assert!(g.engine.state.objects[&id].tapped);
            }
        }
    }
}

#[test]
fn comma_in_name_is_preserved_and_equipment_alternative_keeps_its_type() {
    let mut t = Table::default();
    let tutor = t.card("{W}", "Sorcery", None,
        "Search your library for a card named Halvar, God of Battle or an Equipment card, reveal it, put it into your hand, then shuffle.");
    let halvar = t.named_card(
        "Halvar, God of Battle",
        "{2}{W}{W}",
        "Legendary Creature — Bear",
        Some((4, 4)),
        "",
    );
    let sword = t.named_card("A Sword", "{2}", "Artifact — Equipment", None, "");
    let near = t.named_card("Halvar", "{2}", "Creature — Bear", Some((2, 2)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(tutor, P0, Zone::Hand);
    g.main();
    let ids: Vec<_> = [halvar, sword, near]
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
        .any(|id| g.engine.state.objects[id].card == halvar));
    assert_eq!(g.engine.state.revealed_cards.len(), 1);
}

#[test]
fn source_name_search_still_finds_only_other_copies() {
    let mut t = Table::default();
    let scout = t.named_card("Whisper Squad", "{B}", "Creature — Bear", Some((1, 1)),
        "{1}{B}: Search your library for a card named Whisper Squad, put it onto the battlefield tapped, then shuffle.");
    let near = t.named_card(
        "Whisper Squad Veteran",
        "{B}",
        "Creature — Bear",
        Some((1, 1)),
        "",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(scout, P0, Zone::Battlefield);
    let action = g
        .main()
        .into_iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: s, .. } if *s == source))
        .unwrap();
    let copy = g.put(scout, P0, Zone::Library);
    g.put(near, P0, Zone::Library);
    resolve_selection(&mut g, action, &[copy], 0, 1, vec![copy]);
    assert_eq!(
        g.engine
            .state
            .battlefield()
            .iter()
            .filter(|id| g.engine.state.objects[id].card == scout)
            .count(),
        2
    );
    assert!(!g.engine.state.objects[&source].tapped);
}

#[test]
fn named_search_does_not_accept_separate_quotas_or_partial_self_reference() {
    use mtg_oracle::compile::{compile, FaceText, SubtypeNames};
    for text in [
        "Search your library for a card named Alpine Watchdog and/or a card named Igneous Cur, reveal them, put them into your hand, then shuffle.",
        "Search your library for a card named ~ Replica, reveal it, put it into your hand, then shuffle.",
    ] {
        let face = FaceText { name: "Unsupported", card_types: &[mtg_core::CardType::Sorcery], subtypes: &[], oracle_text: Some(text), mana_cost: "{G}" };
        assert!(!compile(&face, &SubtypeNames(vec![])).understood());
    }
}
