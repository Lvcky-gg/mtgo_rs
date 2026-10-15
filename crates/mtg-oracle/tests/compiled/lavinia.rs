use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const LAVINIA: &str = "Each opponent can't cast noncreature spells with mana value greater than the number of lands that player controls.\nWhenever an opponent casts a spell, if no mana was spent to cast it, counter that spell.";

#[test]
fn lavinia_compares_opponents_noncreature_mana_value_to_their_own_lands() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let small = t.card("{2}", "Sorcery", None, "You gain 2 life.");
    let big = t.card("{3}", "Sorcery", None, "You gain 3 life.");
    let creature = t.card("{3}", "Creature", Some((3, 3)), "");
    let rock = t.card("{0}", "Artifact", None, "{T}: Add {C}{C}{C}{C}.");
    let land = t.mountain();
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(rock, P1, Zone::Battlefield);
    for _ in 0..2 {
        g.put(land, P1, Zone::Battlefield);
    }
    let small = g.put(small, P1, Zone::Hand);
    let big = g.put(big, P1, Zone::Hand);
    let creature = g.put(creature, P1, Zone::Hand);
    let own_big = g.put(g.engine.state.objects[&big].card, P0, Zone::Hand);
    g.put(rock, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: own_big }),
        "Lavinia does not restrict her controller"
    );
    let actions = g.until(P1, Step::PrecombatMain);
    assert!(actions.contains(&Action::Cast { object: small }));
    assert!(
        !actions.contains(&Action::Cast { object: big }),
        "mana rocks do not increase the land limit"
    );
    assert!(
        actions.contains(&Action::Cast { object: creature }),
        "creature spells are exempt"
    );
    g.cast(small, &[]);
    assert_eq!(g.life(P1), 22, "spending mana avoids the counter trigger");
}

#[test]
fn lavinia_counters_free_creatures_and_artifacts_but_not_paid_colorless_mana() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let free_creature = t.card("{0}", "Creature", Some((2, 2)), "");
    let free_artifact = t.card("{0}", "Artifact", None, "");
    let paid = t.card("{1}", "Sorcery", None, "You gain 3 life.");
    let land = t.card("", "Land", None, "{T}: Add {C}.");
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(land, P1, Zone::Battlefield);
    let creature = g.put(free_creature, P1, Zone::Hand);
    let artifact = g.put(free_artifact, P1, Zone::Hand);
    let paid = g.put(paid, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.cast(creature, &[]);
    g.until(P1, Step::PrecombatMain);
    g.cast(artifact, &[]);
    assert!(g.find(free_creature).is_none());
    assert!(g.find(free_artifact).is_none());
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
    g.until(P1, Step::PrecombatMain);
    g.cast(paid, &[]);
    assert_eq!(g.life(P1), 23);
}

#[test]
fn lavinia_caps_announced_x_by_mana_value_with_multiple_x_symbols() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let x_spell = t.card("{X}{X}{1}", "Sorcery", None, "You gain X life.");
    let rock = t.card(
        "{0}",
        "Artifact",
        None,
        "{T}: Add {C}{C}{C}{C}{C}{C}{C}{C}{C}{C}.",
    );
    let land = t.mountain();
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(rock, P1, Zone::Battlefield);
    for _ in 0..4 {
        g.put(land, P1, Zone::Battlefield);
    }
    let spell = g.put(x_spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let mut saw_x = false;
    g.drive(
        |_, c| {
            if let ChoiceKind::ChooseX { max, .. } = c.kind {
                assert_eq!(max, 1, "two X symbols plus one must fit within four lands");
                saw_x = true;
                Some(Answer::Number(1))
            } else {
                None
            }
        },
        |g| g.engine.state.priority == Some(P0) && g.stack().is_empty(),
    );
    assert!(saw_x);
    assert_eq!(g.life(P1), 21);
}

#[test]
fn lavinia_trigger_survives_her_removal_and_respects_uncounterable() {
    for uncounterable in [false, true] {
        let mut t = Table::default();
        let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
        let free = t.card(
            "{0}",
            "Sorcery",
            None,
            if uncounterable {
                "This spell can't be countered.\nYou gain 4 life."
            } else {
                "You gain 4 life."
            },
        );
        let bounce = t.card(
            "{0}",
            "Instant",
            None,
            "Return target creature to its owner's hand.",
        );
        let mut g = Game::new(t);
        let source = g.put(lavinia, P0, Zone::Battlefield);
        let free = g.put(free, P1, Zone::Hand);
        let bounce = g.put(bounce, P0, Zone::Hand);
        g.until(P1, Step::PrecombatMain);
        g.act_holding(Action::Cast { object: free }, &[]);
        assert_eq!(g.stack().len(), 2, "the free spell triggers Lavinia");
        g.cast(bounce, &[mtg_core::Target::Object(source)]);
        assert!(g.find(lavinia).is_none());
        assert_eq!(g.life(P1), if uncounterable { 24 } else { 20 });
    }
}

#[test]
fn lavinia_does_not_counter_a_zero_cost_spell_when_a_tax_was_paid() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let tax = t.card("{2}", "Artifact", None, "Spells cost {1} more to cast.");
    let free = t.card("{0}", "Sorcery", None, "You gain 4 life.");
    let land = t.mountain();
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(tax, P0, Zone::Battlefield);
    g.put(land, P1, Zone::Battlefield);
    let free = g.put(free, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.cast(free, &[]);
    assert_eq!(
        g.life(P1),
        24,
        "actual mana payment, not printed cost, matters"
    );
}

#[test]
fn lavinia_counters_a_phyrexian_spell_paid_entirely_with_life() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let spell = t.card("{U/P}", "Sorcery", None, "You gain 4 life.");
    let land = t.card("", "Land", None, "{T}: Add {C}.");
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(land, P1, Zone::Battlefield);
    let spell = g.put(spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.cast(spell, &[]);
    assert_eq!(g.life(P1), 18, "life payment is not mana payment");
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}

#[test]
fn lavinia_allows_face_down_creature_spells_using_their_face_down_characteristics() {
    let mut t = Table::default();
    let lavinia = t.card("{W}{U}", "Legendary Creature", Some((2, 2)), LAVINIA);
    let morph = t.card("{5}", "Artifact", None, "Morph {2}");
    let rock = t.card("{0}", "Artifact", None, "{T}: Add {C}{C}{C}{C}{C}.");
    let mut g = Game::new(t);
    g.put(lavinia, P0, Zone::Battlefield);
    g.put(rock, P1, Zone::Battlefield);
    let morph = g.put(morph, P1, Zone::Hand);
    let actions = g.until(P1, Step::PrecombatMain);
    assert!(!actions.contains(&Action::Cast { object: morph }));
    assert!(actions.contains(&Action::CastFaceDown { object: morph }));
    g.act(Action::CastFaceDown { object: morph }, &[], &[]);
    let creature = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].face_down)
        .unwrap();
    assert_eq!(g.pt(creature), (2, 2));
}
