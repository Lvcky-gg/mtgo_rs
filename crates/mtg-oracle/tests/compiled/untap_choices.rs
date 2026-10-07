use super::harness::*;
use mtg_core::{Event, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn non_targeted_untap_selects_zero_one_or_two_lands_from_either_player() {
    for take in 0..=2 {
        let mut t = Table::default();
        let spell = t.card("{0}", "Instant", None, "Untap up to two lands.");
        let land = t.mountain();
        let bear = t.bear();
        let mut g = Game::new(t);
        let own = g.put(land, P0, Zone::Battlefield);
        let opponent = g.put(land, P1, Zone::Battlefield);
        let other = g.put(land, P0, Zone::Battlefield);
        let creature = g.put(bear, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        for id in [own, opponent, other, creature] {
            g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
        }
        let start = g.engine.log.len();
        let chosen = [own, opponent][..take].to_vec();
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(chosen.clone())],
        );
        for id in [own, opponent, other, creature] {
            assert_eq!(g.engine.state.objects[&id].tapped, !chosen.contains(&id));
        }
        assert_eq!(
            g.engine.log[start..]
                .iter()
                .filter(|e| matches!(e.event, Event::TapChanged { tapped: false, .. }))
                .count(),
            take
        );
    }
}

#[test]
fn restricted_untap_excludes_opponents_and_nonlands_and_clamps_to_count() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Instant", None, "Untap up to two lands you control.");
    let land = t.mountain();
    let bear = t.bear();
    let mut g = Game::new(t);
    let own: Vec<_> = (0..3).map(|_| g.put(land, P0, Zone::Battlefield)).collect();
    let opponent = g.put(land, P1, Zone::Battlefield);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    for id in own.iter().copied().chain([opponent, creature]) {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![
            opponent, creature, own[0], own[0], own[1], own[2],
        ])],
    );
    assert!(!g.engine.state.objects[&own[0]].tapped);
    assert!(!g.engine.state.objects[&own[1]].tapped);
    assert!(g.engine.state.objects[&own[2]].tapped);
    assert!(g.engine.state.objects[&opponent].tapped);
    assert!(g.engine.state.objects[&creature].tapped);
}

#[test]
fn printed_free_spell_and_enter_trigger_text_compile() {
    let mut t = Table::default();
    for text in [
        "Draw two cards, then discard two cards. Untap up to three lands.",
        "Counter target spell. Untap up to four lands.",
        "Return target creature to its owner's hand. Untap up to two lands.",
    ] {
        t.card("{1}{U}", "Instant", None, text);
    }
    for text in [
        "Flying\nWhen this creature enters, untap up to two lands.",
        "Flying\nWhen this creature enters, untap up to five lands.",
        "When this creature enters, untap up to seven lands.",
    ] {
        t.card("{1}{U}", "Creature", Some((1, 1)), text);
    }
    t.card("{0}", "Instant", None, "Untap up to two target lands.");
}

#[test]
fn enter_trigger_untaps_the_lands_chosen_after_the_creature_resolves() {
    let mut t = Table::default();
    let creature_card = t.card(
        "{0}",
        "Creature",
        Some((1, 1)),
        "Flying\nWhen this creature enters, untap up to two lands.",
    );
    let land = t.mountain();
    let mut g = Game::new(t);
    let land = g.put(land, P0, Zone::Battlefield);
    let creature = g.put(creature_card, P0, Zone::Hand);
    g.main();
    g.engine.state.objects.get_mut(&land).unwrap().tapped = true;
    g.act(
        Action::Cast { object: creature },
        &[],
        &[Answer::Objects(vec![land])],
    );
    assert!(!g.engine.state.objects[&land].tapped);
    assert_eq!(g.engine.state.objects[&land].zone.zone, Zone::Battlefield);
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| { o.zone.zone == Zone::Battlefield && o.card == creature_card })
    );
}
