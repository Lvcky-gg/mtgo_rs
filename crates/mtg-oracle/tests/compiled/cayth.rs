use super::harness::*;
use mtg_core::{CounterKind, Step, Zone};
use mtg_engine::choice::Answer;

const CAYTH: &str = "Fabricate 1\nOther nontoken creatures you control have fabricate 1.\n{2}, {T}: Choose one —\n• Populate.\n• Proliferate.";

#[test]
fn cayth_has_her_own_fabricate_and_grants_it_only_to_other_nontoken_creatures() {
    let mut t = Table::default();
    let cayth_card = t.card("{1}{U}{R}{W}", "Legendary Creature", Some((3, 3)), CAYTH);
    let bear_card = t.card("{0}", "Creature", Some((2, 2)), "");
    let token_spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a 1/1 colorless Servo artifact creature token.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let cayth = g.put(cayth_card, P0, Zone::Hand);
    let bear = g.put(bear_card, P0, Zone::Hand);
    let token_spell = g.put(token_spell, P0, Zone::Hand);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: cayth },
        &[],
        &[Answer::Modes(vec![1])],
    );
    let tokens: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.is_token)
        .map(|o| o.id)
        .collect();
    assert_eq!(tokens.len(), 1, "Cayth's own fabricate creates one Servo");
    let cayth = g.find(cayth_card).unwrap();
    assert_eq!(
        g.pt(cayth),
        (3, 3),
        "Cayth does not grant herself a second fabricate"
    );
    g.act(
        mtg_engine::actions::Action::Cast { object: bear },
        &[],
        &[Answer::Modes(vec![0])],
    );
    let bear = g.find(bear_card).unwrap();
    assert_eq!(
        g.engine.state.objects[&bear].counters[&CounterKind::PlusOnePlusOne],
        1
    );
    g.cast(token_spell, &[]);
    let tokens: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.is_token)
        .map(|o| o.id)
        .collect();
    assert_eq!(tokens.len(), 2);
    for token in tokens {
        assert_eq!(g.pt(token), (1, 1), "tokens do not receive fabricate");
    }
}

#[test]
fn cayth_can_choose_populate_or_proliferate_when_activated() {
    let mut t = Table::default();
    let cayth = t.card("{1}{U}{R}{W}", "Legendary Creature", Some((3, 3)), CAYTH);
    let bear = t.bear();
    let make_token = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a 1/1 colorless Servo artifact creature token.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let cayth = g.put(cayth, P0, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&bear)
        .unwrap()
        .counters
        .insert(CounterKind::PlusOnePlusOne, 1);
    let spell = g.put(make_token, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    g.act(
        activate(cayth, 2),
        &[],
        &[Answer::Modes(vec![1]), Answer::Objects(vec![bear])],
    );
    assert_eq!(g.pt(bear), (4, 4));
    assert!(g.engine.state.objects[&cayth].tapped);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    let token = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.is_token)
        .unwrap()
        .id;
    g.act(
        activate(cayth, 2),
        &[],
        &[Answer::Modes(vec![0]), Answer::Objects(vec![token])],
    );
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.is_token)
            .count(),
        2
    );
}

#[test]
fn modal_activated_abilities_choose_targets_only_for_the_selected_mode() {
    use mtg_core::Target;
    for destroy in [false, true] {
        let mut t = Table::default();
        let creature = t.card(
            "{0}",
            "Creature",
            Some((2, 2)),
            "{1}, {T}: Choose one —\n• Destroy target enchantment.\n• Draw a card.",
        );
        let enchantment = t.card("{0}", "Enchantment", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let source = g.put(creature, P0, Zone::Battlefield);
        let target = destroy.then(|| g.put(enchantment, P1, Zone::Battlefield));
        g.main();
        let hand = g.count(Zone::Hand, P0);
        let targets: Vec<_> = target.map(Target::Object).into_iter().collect();
        g.act(
            activate(source, 0),
            &targets,
            &[Answer::Modes(vec![if destroy { 0 } else { 1 }])],
        );
        assert_eq!(g.count(Zone::Hand, P0), hand + usize::from(!destroy));
        assert_eq!(g.count(Zone::Graveyard, P1), usize::from(destroy));
        assert!(g.engine.state.objects[&source].tapped);
    }
}
