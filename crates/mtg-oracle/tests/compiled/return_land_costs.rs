use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

fn alternative(actions: &[Action], object: mtg_core::ObjectId) -> Option<Action> {
    actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object: o, .. } if *o == object))
        .cloned()
}

#[test]
fn returning_one_two_or_three_islands_pays_an_alternative_cost() {
    for (n, word) in [(1, "an Island"), (2, "two Islands"), (3, "three Islands")] {
        let mut t = Table::default();
        let spell_card = t.card("{3}{U}{U}", "Instant", None, &format!(
            "You may return {word} you control to {} hand rather than pay this spell's mana cost.\nDraw two cards.",
            if n == 1 { "its owner's" } else { "their owner's" }));
        let island = t.card("", "Basic Land — Island", None, "{T}: Add {U}.");
        let mut g = Game::new(t);
        let lands: Vec<_> = (0..n)
            .map(|_| g.put(island, P0, Zone::Battlefield))
            .collect();
        let spell = g.put(spell_card, P0, Zone::Hand);
        let action = alternative(&g.main(), spell).expect("return-land alternative available");
        let before = g.count(Zone::Hand, P0);
        g.act(action, &[], &[Answer::Objects(lands.clone())]);
        for land in lands {
            assert!(!g.engine.state.objects.contains_key(&land));
        }
        assert_eq!(g.count(Zone::Hand, P0), before - 1 + n + 2);
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
    }
}

#[test]
fn returned_lands_are_paid_before_resolution_and_not_refunded_when_countered() {
    let mut t = Table::default();
    let spell_card = t.card("{3}{U}{U}", "Instant", None,
        "You may return two Islands you control to their owner's hand rather than pay this spell's mana cost.\nDraw two cards.");
    let counter_card = t.card("{0}", "Instant", None, "Counter target spell.");
    let island = t.card("", "Basic Land — Island", None, "{T}: Add {U}.");
    let mut g = Game::new(t);
    let lands: Vec<_> = (0..2)
        .map(|_| g.put(island, P0, Zone::Battlefield))
        .collect();
    let spell = g.put(spell_card, P0, Zone::Hand);
    let counter = g.put(counter_card, P0, Zone::Hand);
    let action = alternative(&g.main(), spell).unwrap();
    let library = g.count(Zone::Library, P0);
    g.act_holding(action, &[]);
    for land in lands {
        assert!(!g.engine.state.objects.contains_key(&land));
    }
    assert_eq!(
        g.count(Zone::Library, P0),
        library,
        "spell has not drawn cards yet"
    );
    let stack_spell = *g
        .stack()
        .first()
        .expect("the unpaid-for-mana spell is on the stack");
    assert_eq!(g.engine.state.objects[&stack_spell].zone.zone, Zone::Stack);
    g.cast(counter, &[Target::Object(stack_spell)]);
    assert_eq!(g.count(Zone::Library, P0), library);
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
    assert_eq!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .filter(|id| g.engine.state.objects[id].card == island)
            .count(),
        2
    );
}

#[test]
fn a_returned_land_can_supply_mana_and_returns_to_its_owner() {
    let mut t = Table::default();
    let spell_card = t.card("{2}{U}", "Instant", None,
        "You may pay {1} and return a basic land you control to its owner's hand rather than pay this spell's mana cost.\nDraw a card.");
    let land_card = t.mountain();
    let mut g = Game::new(t);
    let land = g.put(land_card, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&land).unwrap().controller = P0;
    let spell = g.put(spell_card, P0, Zone::Hand);
    let action = alternative(&g.main(), spell).expect("the land can supply the mana payment");
    let before = g.count(Zone::Hand, P1);
    g.act(action, &[], &[Answer::Objects(vec![land])]);
    assert!(!g.engine.state.objects.contains_key(&land));
    assert_eq!(g.count(Zone::Hand, P1), before + 1);
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn alternative_cost_requires_enough_matching_lands_you_control() {
    for matching in 0..=2 {
        let mut t = Table::default();
        let spell_card = t.card("{3}{U}{U}", "Instant", None,
            "You may return two Islands you control to their owner's hand rather than pay this spell's mana cost.\nDraw two cards.");
        let island = t.card("", "Basic Land — Island", None, "{T}: Add {U}.");
        let mut g = Game::new(t);
        g.lands(5);
        for _ in 0..matching {
            g.put(island, P0, Zone::Battlefield);
        }
        for _ in 0..3 {
            g.put(island, P1, Zone::Battlefield);
        }
        let spell = g.put(spell_card, P0, Zone::Hand);
        let actions = g.main();
        assert_eq!(alternative(&actions, spell).is_some(), matching == 2);
        let before = g.engine.state.battlefield().len();
        g.cast(spell, &[]);
        assert_eq!(
            g.engine.state.battlefield().len(),
            before,
            "normal payment returns no lands"
        );
    }
}
