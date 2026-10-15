use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const ABDEL: &str = "When Abdel Adrian enters, exile any number of other nonland permanents you control until Abdel Adrian leaves the battlefield. Create a 1/1 white Soldier creature token for each permanent exiled this way.\nChoose a Background";

fn soldiers(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            mtg_engine::layers::compute(&g.engine.state, &g.table, *id)
                .is_some_and(|c| c.name.as_ref() == "Soldier")
        })
        .collect()
}

#[test]
fn abdel_chooses_nonlands_without_targeting_makes_one_soldier_each_and_returns_them() {
    let mut t = Table::default();
    let abdel = t.named_card(
        "Abdel Adrian, Gorion's Ward",
        "{0}",
        "Legendary Creature",
        Some((4, 4)),
        ABDEL,
    );
    let creature = t.card("{1}", "Creature", Some((2, 2)), "Shroud");
    let artifact = t.card("{1}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let source = g.put(abdel, P0, Zone::Hand);
    let creature = g.put(creature, P0, Zone::Battlefield);
    let artifact = g.put(artifact, P0, Zone::Battlefield);
    let enemy = g.put(artifact_card(&g, artifact), P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: source },
        &[],
        &[Answer::Objects(vec![creature, artifact])],
    );
    assert!(!g.engine.state.objects.contains_key(&creature));
    assert!(!g.engine.state.objects.contains_key(&artifact));
    assert!(g.engine.state.objects.contains_key(&enemy));
    assert_eq!(soldiers(&g).len(), 2);
    assert_eq!(
        g.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| g.engine.state.objects[id].controller == P0)
            .count(),
        4,
        "Abdel, land, two Soldiers"
    );
    let source = g.find(abdel).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert_eq!(
        g.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| g.engine.state.objects[id].controller == P0)
            .count(),
        5,
        "land, returned permanents, Soldiers"
    );
    assert_eq!(soldiers(&g).len(), 2);
    assert!(g.engine.state.linked_exile.is_empty());
}

fn artifact_card(g: &Game, object: mtg_core::ObjectId) -> mtg_core::CardId {
    g.engine.state.objects[&object].card
}

#[test]
fn abdel_counts_an_exiled_token_but_that_token_cannot_return() {
    let mut t = Table::default();
    let abdel = t.named_card(
        "Abdel Adrian, Gorion's Ward",
        "{0}",
        "Legendary Creature",
        Some((4, 4)),
        ABDEL,
    );
    let maker = t.card(
        "{0}",
        "Instant",
        None,
        "Create a 1/1 white Soldier creature token.",
    );
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(abdel, P0, Zone::Hand);
    let maker = g.put(maker, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(maker, &[]);
    let old_token = soldiers(&g)[0];
    g.act(
        Action::Cast { object: source },
        &[],
        &[Answer::Objects(vec![old_token])],
    );
    assert!(!g.engine.state.objects.contains_key(&old_token));
    assert_eq!(
        soldiers(&g).len(),
        1,
        "an exiled token still earns a Soldier"
    );
    let source = g.find(abdel).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert_eq!(soldiers(&g).len(), 1, "the exiled token ceased to exist");
}

#[test]
fn abdel_can_choose_zero_permanents() {
    let mut t = Table::default();
    let abdel = t.named_card(
        "Abdel Adrian, Gorion's Ward",
        "{0}",
        "Legendary Creature",
        Some((4, 4)),
        ABDEL,
    );
    let artifact = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    let source = g.put(abdel, P0, Zone::Hand);
    let artifact = g.put(artifact, P0, Zone::Battlefield);
    g.main();
    g.act(
        Action::Cast { object: source },
        &[],
        &[Answer::Objects(vec![])],
    );
    assert!(g.engine.state.objects.contains_key(&artifact));
    assert!(soldiers(&g).is_empty());
}

#[test]
fn abdel_leaving_before_his_enter_trigger_resolves_exiles_nothing_and_makes_no_tokens() {
    let mut t = Table::default();
    let abdel = t.named_card(
        "Abdel Adrian, Gorion's Ward",
        "{0}",
        "Legendary Creature",
        Some((4, 4)),
        ABDEL,
    );
    let artifact = t.card("{1}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(abdel, P0, Zone::Hand);
    let artifact = g.put(artifact, P0, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: source }),
        )
        .unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.priority == Some(P0) && g.find(abdel).is_some() && !g.stack().is_empty(),
    );
    let source = g.find(abdel).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(g.engine.state.objects.contains_key(&artifact));
    assert!(soldiers(&g).is_empty());
    assert!(g.engine.state.linked_exile.is_empty());
}

#[test]
fn abdel_choice_excludes_himself_lands_and_opponents_permanents() {
    let mut t = Table::default();
    let abdel = t.named_card(
        "Abdel Adrian, Gorion's Ward",
        "{0}",
        "Legendary Creature",
        Some((4, 4)),
        ABDEL,
    );
    let artifact = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let source = g.put(abdel, P0, Zone::Hand);
    let own = g.put(artifact, P0, Zone::Battlefield);
    g.put(artifact, P1, Zone::Battlefield);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: source }),
        )
        .unwrap();
    let mut chose = false;
    g.drive(
        |_, c| {
            if let ChoiceKind::ChooseObjects {
                from: ref options, ..
            } = c.kind
            {
                assert_eq!(options, &vec![own]);
                chose = true;
                Some(Answer::Objects(vec![own]))
            } else {
                None
            }
        },
        |g| g.engine.state.priority == Some(P0) && g.stack().is_empty(),
    );
    assert!(chose);
    assert_eq!(soldiers(&g).len(), 1);
}
