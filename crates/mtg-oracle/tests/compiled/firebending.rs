use super::harness::*;
use mtg_core::{Color, Step, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

fn red(g: &Game) -> u16 {
    g.engine.state.player(P0).mana.amounts[Color::Red as usize]
}

fn declare(g: &mut Game, attacker: mtg_core::ObjectId, stop_on_trigger: bool) {
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::DeclareAttackers { .. } => Some(Answer::Objects(vec![attacker])),
            _ => None,
        },
        |g| {
            g.engine.state.step == Step::DeclareAttackers
                && if stop_on_trigger {
                    !g.stack().is_empty()
                } else {
                    g.stack().is_empty() && red(g) > 0
                }
        },
    );
}

#[test]
fn firebending_mana_lasts_through_every_combat_step_and_expires_at_end_of_combat() {
    let mut t = Table::default();
    let zuko = t.named_card(
        "Zuko, Exiled Prince",
        "{3}{R}",
        "Legendary Creature — Human Noble",
        Some((4, 3)),
        "Firebending 3\n{3}: Exile the top card of your library. You may play that card this turn.",
    );
    let mut g = Game::new(t);
    let zuko = g.put(zuko, P0, Zone::Battlefield);
    g.main();
    declare(&mut g, zuko, false);
    assert_eq!(red(&g), 3);
    for step in [Step::DeclareBlockers, Step::CombatDamage, Step::EndCombat] {
        g.until(P0, step);
        assert_eq!(red(&g), 3, "{step:?}");
    }
    g.until(P0, Step::PostcombatMain);
    assert_eq!(red(&g), 0);
    assert!(
        g.engine
            .state
            .combat_mana
            .values()
            .all(|k| k.iter().all(|n| *n == 0))
    );
}

#[test]
fn variable_firebending_uses_power_on_resolution_and_last_known_power_if_the_source_leaves() {
    for bounce in [false, true] {
        let mut t = Table::default();
        let student = t.card(
            "{0}",
            "Creature — Human Wizard",
            Some((2, 3)),
            "Firebending X, where X is this creature's power.",
        );
        let response = t.card(
            "{0}",
            "Instant",
            None,
            if bounce {
                "Return target creature to its owner's hand."
            } else {
                "Target creature gets +3/+3 until end of turn."
            },
        );
        let mut g = Game::new(t);
        let student = g.put(student, P0, Zone::Battlefield);
        let spell = g.put(response, P0, Zone::Hand);
        g.main();
        declare(&mut g, student, true);
        g.cast(spell, &[Target::Object(student)]);
        assert_eq!(red(&g), if bounce { 2 } else { 5 });
    }
}

#[test]
fn spent_combat_mana_does_not_make_later_ordinary_mana_persistent() {
    let mut t = Table::default();
    let attacker = t.card("{0}", "Creature — Human", Some((2, 3)), "Firebending 2");
    let rock = t.card("{0}", "Artifact", None, "{T}: Add {R}.");
    let spell = t.card("{R}{R}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let attacker = g.put(attacker, P0, Zone::Battlefield);
    let rock = g.put(rock, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    declare(&mut g, attacker, false);
    g.cast(spell, &[]);
    assert_eq!(red(&g), 0);
    g.act(
        Action::ActivateManaAbility {
            source: rock,
            ability: mtg_core::AbilityId(0),
            color: None,
        },
        &[],
        &[],
    );
    assert_eq!(red(&g), 1);
    g.until(P0, Step::DeclareBlockers);
    assert_eq!(red(&g), 0);
}

#[test]
fn birgis_turn_mana_survives_after_combat_mana_expires() {
    let mut t = Table::default();
    let attacker = t.card("{0}", "Creature — Human", Some((2, 3)), "Firebending 2");
    let birgi = t.named_card("Birgi, God of Storytelling", "{2}{R}", "Legendary Creature — God", Some((3, 3)), "Whenever you cast a spell, add {R}. Until end of turn, you don't lose this mana as steps and phases end.");
    let spell = t.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let attacker = g.put(attacker, P0, Zone::Battlefield);
    g.put(birgi, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert_eq!(red(&g), 1);
    g.combat(&[attacker], &[], &[], &[]);
    assert_eq!(red(&g), 1);
    g.until(P1, Step::Upkeep);
    assert_eq!(red(&g), 0);
}

#[test]
fn paying_with_same_color_mana_preserves_only_the_retained_units_actually_left() {
    for restricted in [false, true] {
        let mut t = Table::default();
        let attacker = t.card("{0}", "Creature — Human", Some((2, 3)), "Firebending 2");
        let rock = t.card(
            "{0}",
            "Artifact",
            None,
            if restricted {
                "{T}: Add {R}. Spend this mana only to cast artifact spells."
            } else {
                "{T}: Add {R}."
            },
        );
        let spell = t.card("{R}", "Instant", None, "You gain 1 life.");
        let mut g = Game::new(t);
        let attacker = g.put(attacker, P0, Zone::Battlefield);
        let rock = g.put(rock, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        declare(&mut g, attacker, false);
        g.act(
            Action::ActivateManaAbility {
                source: rock,
                ability: mtg_core::AbilityId(0),
                color: None,
            },
            &[],
            &[],
        );
        assert_eq!(red(&g), 3);
        g.cast(spell, &[]);
        assert_eq!(red(&g), 2);
        g.until(P0, Step::DeclareBlockers);
        assert_eq!(red(&g), if restricted { 1 } else { 2 });
    }
}

#[test]
fn firebending_in_keyword_lists_and_temporary_grants_triggers_on_attack() {
    for granted in [false, true] {
        let mut t = Table::default();
        let body = t.card(
            "{0}",
            "Creature — Human",
            Some((2, 3)),
            if granted {
                "Haste"
            } else {
                "Trample, firebending 2, haste"
            },
        );
        let grant = t.card(
            "{0}",
            "Instant",
            None,
            "Target creature gains firebending 2 until end of turn.",
        );
        let mut g = Game::new(t);
        let body = g.put(body, P0, Zone::Battlefield);
        let grant = g.put(grant, P0, Zone::Hand);
        g.main();
        if granted {
            g.cast(grant, &[Target::Object(body)]);
        }
        declare(&mut g, body, false);
        assert_eq!(red(&g), 2);
    }
}

#[test]
fn created_tokens_can_have_firebending_alongside_other_keywords() {
    let mut t = Table::default();
    let maker = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a 4/4 red Dragon creature token with flying and firebending 4.",
    );
    let mut g = Game::new(t);
    let maker = g.put(maker, P0, Zone::Hand);
    g.main();
    g.cast(maker, &[]);
    let token = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.is_token)
        .unwrap()
        .id;
    g.until(P1, Step::Upkeep);
    g.until(P0, Step::PrecombatMain);
    declare(&mut g, token, false);
    assert_eq!(red(&g), 4);
}
