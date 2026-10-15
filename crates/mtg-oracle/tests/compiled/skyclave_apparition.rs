use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const SKYCLAVE: &str = "When this creature enters, exile up to one target nonland, nontoken permanent you don't control with mana value 4 or less.\nWhen this creature leaves the battlefield, the exiled card's owner creates an X/X blue Illusion creature token, where X is the mana value of the exiled card.";

fn illusions(g: &Game, player: mtg_core::PlayerId) -> Vec<mtg_core::ObjectId> {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            g.engine.state.objects[id].controller == player
                && mtg_engine::layers::compute(&g.engine.state, &g.table, *id)
                    .is_some_and(|c| c.name.as_ref() == "Illusion")
        })
        .collect()
}

#[test]
fn skyclave_rewards_the_exiled_cards_owner_with_the_right_copiable_size() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim = t.card("{3}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let copy = t.card(
        "{0}",
        "Instant",
        None,
        "Create a token that's a copy of target creature.",
    );
    let clone_card = t.card(
        "{0}",
        "Creature",
        Some((0, 0)),
        "You may have this creature enter as a copy of any creature on the battlefield.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim, P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    let bounce_clone = g.put(g.engine.state.objects[&bounce].card, P0, Zone::Hand);
    let copy = g.put(copy, P0, Zone::Hand);
    let clone = g.put(clone_card, P0, Zone::Hand);
    g.main();
    g.cast(source, &[Target::Object(victim)]);
    assert!(!g.engine.state.objects.contains_key(&victim));
    let source = g.find(apparition).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    let token = illusions(&g, P1)[0];
    assert_eq!(g.pt(token), (3, 3));
    let c = mtg_engine::layers::compute(&g.engine.state, &g.table, token).unwrap();
    assert!(c.colors.contains(Color::Blue));
    g.cast(copy, &[Target::Object(token)]);
    let copied = illusions(&g, P0)[0];
    assert_eq!(
        g.pt(copied),
        (3, 3),
        "size is copiable, not an external buff"
    );
    g.act(
        Action::Cast { object: clone },
        &[],
        &[Answer::Objects(vec![token])],
    );
    let clone = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].original == Some((clone_card, 0)))
        .unwrap();
    assert_eq!(
        g.pt(clone),
        (3, 3),
        "a nontoken Clone also copies the fixed size"
    );
    g.cast(bounce_clone, &[Target::Object(clone)]);
    let hand = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == clone_card && o.zone.zone == Zone::Hand)
        .unwrap();
    assert_eq!(
        hand.token_size, None,
        "the copied size ends when Clone changes zones"
    );
}

#[test]
fn skyclave_leaving_before_its_enter_trigger_resolves_exiles_without_a_reward() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim = t.card("{2}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim, P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    // Resolve the creature, then stop with its enter trigger waiting on the stack.
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: source }),
        )
        .unwrap();
    g.drive(
        |_, c| match c.kind {
            ChoiceKind::ChooseTargets { .. } => {
                Some(Answer::Targets(vec![vec![Target::Object(victim)]]))
            }
            _ => None,
        },
        |g| {
            g.engine.state.priority == Some(P0)
                && g.find(apparition).is_some()
                && !g.stack().is_empty()
        },
    );
    let source = g.find(apparition).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(illusions(&g, P1).is_empty());
    assert!(!g.engine.state.objects.contains_key(&victim));
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.owner == P1 && o.zone.zone == Zone::Exile)
    );
}

#[test]
fn skyclave_recasting_does_not_reuse_an_earlier_incarnations_exile() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let big = t.card("{3}", "Artifact", None, "");
    let small = t.card("{1}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let big = g.put(big, P1, Zone::Battlefield);
    let small = g.put(small, P1, Zone::Battlefield);
    let first_bounce = g.put(bounce, P0, Zone::Hand);
    let second_bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(source, &[Target::Object(big)]);
    let source = g.find(apparition).unwrap();
    g.cast(first_bounce, &[Target::Object(source)]);
    let source = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == apparition && o.zone.zone == Zone::Hand)
        .unwrap()
        .id;
    g.cast(source, &[Target::Object(small)]);
    let source = g.find(apparition).unwrap();
    g.cast(second_bounce, &[Target::Object(source)]);
    let mut sizes: Vec<_> = illusions(&g, P1).into_iter().map(|id| g.pt(id)).collect();
    sizes.sort();
    assert_eq!(sizes, vec![(1, 1), (3, 3)]);
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.owner == P1 && o.zone.zone == Zone::Exile)
            .count(),
        2
    );
}

#[test]
fn skyclave_rewards_the_owner_even_when_someone_else_controlled_the_exiled_card() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim = t.card("{2}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&victim).unwrap().controller = P1;
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(source, &[Target::Object(victim)]);
    let source = g.find(apparition).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(illusions(&g, P1).is_empty());
    let token = illusions(&g, P0)[0];
    assert_eq!(g.pt(token), (2, 2));
}

#[test]
fn skyclave_declining_the_enter_target_creates_no_illusion() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim = t.card("{2}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim, P1, Zone::Battlefield);
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
        |_, c| match c.kind {
            ChoiceKind::ChooseTargets { .. } => Some(Answer::Targets(vec![vec![]])),
            _ => None,
        },
        |g| g.engine.state.priority == Some(P0) && g.stack().is_empty(),
    );
    let source = g.find(apparition).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(g.engine.state.objects.contains_key(&victim));
    assert!(illusions(&g, P1).is_empty());
}

#[test]
fn skyclave_does_not_reward_a_card_that_left_exile_even_if_it_was_exiled_again() {
    use mtg_core::{Cause, Event, ZoneRef};
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim_card = t.card("{2}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim_card, P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(source, &[Target::Object(victim)]);
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == victim_card)
        .unwrap()
        .id;
    // Set up a separate exile -> hand -> exile journey with real identity-changing events.
    let hand = g.engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        Cause::PlayerAction(P1),
        Event::ZoneChange {
            object: exiled,
            new_object: hand,
            from: ZoneRef::shared(Zone::Exile),
            to: ZoneRef::of(Zone::Hand, P1),
            index: None,
        },
        &mut g.engine.log,
    );
    let reexiled = g.engine.state.new_object_id();
    mtg_engine::apply::apply(
        &mut g.engine.state,
        Cause::PlayerAction(P1),
        Event::ZoneChange {
            object: hand,
            new_object: reexiled,
            from: ZoneRef::of(Zone::Hand, P1),
            to: ZoneRef::shared(Zone::Exile),
            index: None,
        },
        &mut g.engine.log,
    );
    let source = g.find(apparition).unwrap();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(illusions(&g, P1).is_empty());
    assert!(g.engine.state.objects.contains_key(&reexiled));
}

#[test]
fn skyclave_exiling_a_zero_mana_value_card_creates_a_zero_zero_that_dies() {
    let mut t = Table::default();
    let apparition = t.card("{0}", "Creature", Some((2, 2)), SKYCLAVE);
    let victim = t.card("{0}", "Artifact", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let source = g.put(apparition, P0, Zone::Hand);
    let victim = g.put(victim, P1, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.cast(source, &[Target::Object(victim)]);
    let source = g.find(apparition).unwrap();
    let before = g.engine.log.len();
    g.cast(bounce, &[Target::Object(source)]);
    assert!(g.engine.log[before..].iter().any(|e| matches!(
        e.event,
        mtg_core::Event::TokenSizeSet {
            power: 0,
            toughness: 0,
            ..
        }
    )));
    assert!(illusions(&g, P1).is_empty());
}
