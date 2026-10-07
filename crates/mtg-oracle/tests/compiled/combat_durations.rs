use super::harness::*;
use mtg_core::{CardType, Step, Target, Zone};
use mtg_engine::layers;

#[test]
fn combat_boost_and_keywords_expire_before_the_second_main_phase() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +3/+1 and gains flying until end of combat.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.until(P0, Step::BeginCombat);
    g.cast(spell, &[Target::Object(attacker)]);
    assert_eq!(g.pt(attacker), (5, 3));
    assert!(g.has(attacker, mtg_core::Keyword::Flying));
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P1), 15);
    assert_eq!(g.pt(attacker), (2, 2));
    assert!(
        !layers::compute(&g.engine.state, &g.table, attacker)
            .unwrap()
            .granted_keywords
            .contains(&mtg_core::Keyword::Flying)
    );
}

#[test]
fn animation_ends_with_combat_and_restores_the_artifact() {
    let mut t = Table::default();
    let artifact_card = t.card("{2}", "Artifact", None,
        "{0}: This artifact becomes a 3/6 Golem artifact creature until end of combat. Activate only during combat.");
    let mut g = Game::new(t);
    let artifact = g.put(artifact_card, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(
        !offers(&actions, artifact),
        "activation is restricted to combat"
    );
    g.until(P0, Step::BeginCombat);
    g.act(activate(artifact, 0), &[], &[]);
    assert_eq!(g.pt(artifact), (3, 6));
    g.combat(&[artifact], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    let chars = layers::compute(&g.engine.state, &g.table, artifact).unwrap();
    assert!(chars.has_type(CardType::Artifact));
    assert!(!chars.has_type(CardType::Creature));
}

#[test]
fn combat_control_change_and_haste_expire_together() {
    let mut t = Table::default();
    let spell_card = t.card("{0}", "Instant", None,
        "Gain control of target creature until end of combat. Untap it. It gains haste until end of combat.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let attacker = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.until(P0, Step::BeginCombat);
    g.cast(spell, &[Target::Object(attacker)]);
    assert_eq!(layers::controller(&g.engine.state, attacker), Some(P0));
    assert!(g.has(attacker, mtg_core::Keyword::Haste));
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(layers::controller(&g.engine.state, attacker), Some(P1));
    assert!(
        !layers::compute(&g.engine.state, &g.table, attacker)
            .unwrap()
            .granted_keywords
            .contains(&mtg_core::Keyword::Haste)
    );
}

#[test]
fn an_effect_created_during_end_of_combat_expires_before_main() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +2/+2 until end of combat.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let creature = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.until(P0, Step::EndCombat);
    g.cast(spell, &[Target::Object(creature)]);
    assert_eq!(g.pt(creature), (4, 4));
    g.until(P0, Step::PostcombatMain);
    assert_eq!(g.pt(creature), (2, 2));
}
