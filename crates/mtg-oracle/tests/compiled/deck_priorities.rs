//! Regressions for the user's commander deck priorities.
use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

#[test]
fn soul_guide_lantern_exiles_opponents_graveyards_and_preserves_yours() {
    let mut t = Table::default();
    let lantern = t.card("{1}", "Artifact", None,
        "When this artifact enters, exile target card from a graveyard.\n{T}, Sacrifice this artifact: Exile each opponent's graveyard.\n{1}, {T}, Sacrifice this artifact: Draw a card.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let p2 = mtg_core::PlayerId(2);
    g.engine
        .state
        .players
        .insert(p2, mtg_engine::state::PlayerState::new(p2, 20));
    g.engine.state.turn_order.push(p2);
    let lantern = g.put(lantern, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Graveyard);
    g.put(bear, P1, Zone::Graveyard);
    g.put(bear, P1, Zone::Graveyard);
    g.put(bear, p2, Zone::Graveyard);
    g.main();
    g.act(activate(lantern, 1), &[], &[]);
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(
        g.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len(),
        3
    );
    assert_eq!(g.count(Zone::Graveyard, p2), 0);
}

#[test]
fn displacer_kitten_blinks_a_stolen_permanent_back_to_its_owner() {
    let mut t = Table::default();
    let kitten = t.card("{3}{U}", "Creature — Cat Beast", Some((2, 2)),
        "Avoidance — Whenever you cast a noncreature spell, exile up to one target nonland permanent you control, then return that card to the battlefield under its owner's control.");
    let rock = t.card("{2}", "Artifact", None, "");
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.put(kitten, P0, Zone::Battlefield);
    let stolen = g.put(rock, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&stolen).unwrap().controller = P0;
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(stolen)],
        &[],
    );
    assert!(!g.engine.state.objects.contains_key(&stolen));
    let returned = g.find(rock).unwrap();
    assert_eq!(g.engine.state.objects[&returned].controller, P1);
    assert_eq!(g.count(Zone::Exile, P1), 0);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn displacer_kitten_can_decline_its_optional_target() {
    let mut t = Table::default();
    let kitten = t.card("{3}{U}", "Creature", Some((2, 2)),
        "Avoidance — Whenever you cast a noncreature spell, exile up to one target nonland permanent you control, then return that card to the battlefield under its owner's control.");
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let kitten = g.put(kitten, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let prompt = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let mut asked = false;
    g.drive(
        |_, c| {
            if let ChoiceKind::ChooseTargets { optional, .. } = &c.kind {
                assert_eq!(optional, &[true]);
                asked = true;
                Some(Answer::Targets(vec![vec![]]))
            } else {
                None
            }
        },
        |g| g.stack().is_empty(),
    );
    assert!(asked);
    assert!(g.engine.state.objects.contains_key(&kitten));
    assert_eq!(g.life(P0), 21);
}

#[test]
fn sokenzan_gives_both_new_tokens_temporary_haste_only() {
    let mut t = Table::default();
    let land = t.card("", "Legendary Land", None,
        "{T}: Add {R}.\nChannel — {3}{R}, Discard this card: Create two 1/1 colorless Spirit creature tokens. They gain haste until end of turn. This ability costs {1} less to activate for each legendary creature you control.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let bear = g.put(bear, P0, Zone::Battlefield);
    let land = g.put(land, P0, Zone::Hand);
    g.main();
    g.act(activate(land, 1), &[], &[]);
    let tokens: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.is_token && o.zone.zone == Zone::Battlefield)
        .map(|o| o.id)
        .collect();
    assert_eq!(tokens.len(), 2);
    for &token in &tokens {
        assert!(g.has(token, Keyword::Haste));
        assert_eq!(g.pt(token), (1, 1));
    }
    assert!(!g.has(bear, Keyword::Haste));
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
    g.until(P1, Step::Upkeep);
    for token in tokens {
        assert!(!g.has(token, Keyword::Haste));
    }
}

#[test]
fn sigil_of_sleep_bounces_only_a_creature_controlled_by_the_damaged_player() {
    for owner in [P0, P1] {
        let mut t = Table::default();
        let sigil = t.card("{U}", "Enchantment — Aura", None,
        "Enchant creature\nWhenever enchanted creature deals damage to a player, return target creature that player controls to its owner's hand.");
        let bear = t.bear();
        let mut g = Game::new(t);
        let attacker = g.put(bear, P0, Zone::Battlefield);
        let victim = g.put(bear, owner, Zone::Battlefield);
        g.engine.state.objects.get_mut(&victim).unwrap().controller = P1;
        let aura = g.put(sigil, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(attacker);
        g.main();
        let hand_before = g.count(Zone::Hand, owner);
        g.combat(&[attacker], &[], &[Target::Object(victim)], &[]);
        assert!(g.engine.state.objects.contains_key(&attacker));
        assert!(!g.engine.state.objects.contains_key(&victim));
        assert_eq!(g.count(Zone::Hand, owner), hand_before + 1);
        assert_eq!(g.life(P1), 18);
    }
}
