//! Mana abilities whose cost is a choice — "tap an untapped creature you control",
//! "remove X storage counters" — are announced to ask for it, then resolve at once
//! (CR 605.3b): nothing goes on the stack.

use super::harness::*;
use mtg_core::{AbilityId, Color, CounterKind, ManaPool, ObjectId, Zone};
use mtg_engine::{actions::Action, choice::Answer};

fn mana_ability(game: &Game, object: ObjectId) -> (AbilityId, Option<CounterKind>) {
    mtg_engine::abilities::current(&game.engine.state, &game.table, object)
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::Activated {
                cost,
                is_mana_ability: true,
                ..
            } => Some((
                a.id,
                cost.additional.iter().find_map(|p| match p {
                    mtg_ir::AdditionalCost::RemoveCounters { kind, .. } => Some(*kind),
                    _ => None,
                }),
            )),
            _ => None,
        })
        .expect("a mana ability")
}

fn pool(game: &Game, slot: usize) -> u16 {
    game.engine.state.players[&P0].mana.amounts[slot]
}

#[test]
fn tapping_a_chosen_creature_pays_for_mana_even_while_it_is_summoning_sick() {
    let mut table = Table::default();
    let drum = table.card(
        "{1}",
        "Artifact",
        None,
        "{T}, Tap an untapped creature you control: Add one mana of any color.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    let drum = game.put(drum, P0, Zone::Battlefield);
    let bear = game.put(bear, P0, Zone::Battlefield);
    game.engine
        .state
        .objects
        .get_mut(&bear)
        .unwrap()
        .summoning_sick = true;
    let (ability, _) = mana_ability(&game, drum);
    game.main();
    game.act(
        Action::ActivateManaAbility {
            source: drum,
            ability,
            color: Some(Color::Green),
        },
        &[],
        &[Answer::Objects(vec![bear])],
    );
    assert_eq!(pool(&game, Color::Green as usize), 1);
    assert!(game.engine.state.objects[&drum].tapped);
    assert!(game.engine.state.objects[&bear].tapped, "the creature paid");
    assert!(game.stack().is_empty(), "a mana ability uses no stack");
    // With no untapped creature left, it can't be activated again.
    game.engine.state.objects.get_mut(&drum).unwrap().tapped = false;
    assert!(
        mtg_engine::mana::manual_source(&game.engine.state, &game.table, P0, drum, ability)
            .is_none()
    );
}

#[test]
fn storage_land_makes_one_mana_per_counter_removed() {
    let mut table = Table::default();
    let land = table.card(
        "",
        "Land",
        None,
        "{T}, Remove any number of storage counters from this land: Add {C} for each storage \
         counter removed this way.",
    );
    let mut game = Game::new(table);
    let land = game.put(land, P0, Zone::Battlefield);
    let (ability, kind) = mana_ability(&game, land);
    let kind = kind.unwrap();
    game.engine
        .state
        .objects
        .get_mut(&land)
        .unwrap()
        .counters
        .insert(kind, 3);
    game.main();
    game.act(
        Action::ActivateManaAbility {
            source: land,
            ability,
            color: None,
        },
        &[],
        &[Answer::Number(2)],
    );
    assert_eq!(
        pool(&game, ManaPool::COLORLESS_SLOT),
        2,
        "two counters, two mana"
    );
    assert_eq!(game.engine.state.objects[&land].counters[&kind], 1);
    assert!(game.engine.state.objects[&land].tapped);
    assert!(game.stack().is_empty());
}

#[test]
fn battery_adds_one_more_mana_than_counters_removed() {
    let mut table = Table::default();
    let battery = table.card(
        "{3}",
        "Artifact",
        None,
        "{T}, Remove any number of charge counters from this artifact: Add {C}, then add an \
         additional {C} for each charge counter removed this way.",
    );
    let mut game = Game::new(table);
    let battery = game.put(battery, P0, Zone::Battlefield);
    let (ability, kind) = mana_ability(&game, battery);
    let kind = kind.unwrap();
    game.engine
        .state
        .objects
        .get_mut(&battery)
        .unwrap()
        .counters
        .insert(kind, 2);
    game.main();
    game.act(
        Action::ActivateManaAbility {
            source: battery,
            ability,
            color: None,
        },
        &[],
        &[Answer::Number(2)],
    );
    assert_eq!(pool(&game, ManaPool::COLORLESS_SLOT), 3);
    assert_eq!(
        game.engine.state.objects[&battery]
            .counters
            .get(&kind)
            .copied()
            .unwrap_or(0),
        0
    );
}

/// The same cost on an ability that uses the stack: X is announced, no more than the
/// counters there are or the mana can pay for.
#[test]
fn removing_x_counters_pays_for_an_x_damage_ability() {
    let mut table = Table::default();
    let sentry = table.card(
        "{5}",
        "Artifact Creature — Bear",
        Some((0, 0)),
        "{X}, {T}, Remove X +1/+1 counters from this creature: It deals X damage to any target.",
    );
    let mut game = Game::new(table);
    game.lands(2);
    let sentry = game.put(sentry, P0, Zone::Battlefield);
    game.engine
        .state
        .objects
        .get_mut(&sentry)
        .unwrap()
        .counters
        .insert(CounterKind::PlusOnePlusOne, 3);
    let actions = game.main();
    assert!(offers(&actions, sentry));
    let ability = actions
        .iter()
        .find_map(|a| match a {
            Action::ActivateAbility { source, ability } if *source == sentry => Some(*ability),
            _ => None,
        })
        .unwrap();
    // Three counters, but two lands: X is at most 2.
    let prompt = game.pending.take().unwrap().id;
    let action = Action::ActivateAbility {
        source: sentry,
        ability,
    };
    game.engine
        .answer(&game.table, prompt, Answer::Action(action))
        .unwrap();
    loop {
        let mtg_engine::Progress::NeedsChoice(c) = game.engine.advance(&game.table) else {
            continue;
        };
        match c.kind {
            mtg_engine::choice::ChoiceKind::ChooseX { min, max } => {
                assert_eq!((min, max), (0, 2));
                game.engine
                    .answer(&game.table, c.id, Answer::Number(2))
                    .unwrap();
            }
            mtg_engine::choice::ChoiceKind::ChooseTargets { .. } => {
                let target = vec![vec![mtg_core::Target::Player(P1)]];
                game.engine
                    .answer(&game.table, c.id, Answer::Targets(target))
                    .unwrap();
            }
            mtg_engine::choice::ChoiceKind::Priority { .. } if game.stack().is_empty() => break,
            _ => game.engine.answer(&game.table, c.id, Answer::Pass).unwrap(),
        }
    }
    assert_eq!(game.life(P1), 18);
    assert_eq!(
        game.engine.state.objects[&sentry].counters[&CounterKind::PlusOnePlusOne],
        1
    );
}

/// A storage land kept tapped through the untap step gains a counter at upkeep; "it" in
/// "if this land is tapped, put a storage counter on it" is the land.
#[test]
fn storage_land_kept_tapped_gains_a_counter_at_upkeep() {
    let mut table = Table::default();
    let vault = table.card(
        "",
        "Land",
        None,
        "You may choose not to untap this land during your untap step.\nAt the beginning of \
         your upkeep, if this land is tapped, put a storage counter on it.\n{T}, Remove any \
         number of storage counters from this land: Add {B} for each storage counter removed \
         this way.",
    );
    let mut game = Game::new(table);
    let vault = game.put(vault, P0, Zone::Battlefield);
    game.engine.state.objects.get_mut(&vault).unwrap().tapped = true;
    let (_, kind) = mana_ability(&game, vault);
    let kind = kind.unwrap();
    // Keep it tapped when asked during the untap step.
    loop {
        match game.engine.advance(&game.table) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::NeedsChoice(c) => {
                if let mtg_engine::choice::ChoiceKind::ChooseObjects { from, .. } = &c.kind {
                    assert!(from.contains(&vault));
                    game.engine
                        .answer(&game.table, c.id, Answer::Objects(vec![vault]))
                        .unwrap();
                    break;
                }
                panic!("unexpected question {:?}", c.kind);
            }
            mtg_engine::Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    game.main();
    assert!(game.engine.state.objects[&vault].tapped);
    assert_eq!(
        game.engine.state.objects[&vault].counters.get(&kind),
        Some(&1)
    );
}

#[test]
fn tapped_artifact_hurts_its_controller_at_the_draw_step() {
    let mut table = Table::default();
    let vault = table.card(
        "{1}",
        "Artifact",
        None,
        "This artifact doesn't untap during your untap step.\nAt the beginning of your draw \
         step, if this artifact is tapped, it deals 1 damage to you.",
    );
    let mut game = Game::new(table);
    let vault = game.put(vault, P0, Zone::Battlefield);
    game.engine.state.objects.get_mut(&vault).unwrap().tapped = true;
    game.main();
    assert!(game.engine.state.objects[&vault].tapped);
    assert_eq!(
        game.life(P0),
        19,
        "\"it\" is the artifact, which deals the damage"
    );
}
