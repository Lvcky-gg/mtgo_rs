//! Rituals paid for with a creature: mana equal to the sacrificed creature's mana value.

use super::harness::*;
use mtg_core::{Color, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const SACRIFICE: &str = "As an additional cost to cast this spell, sacrifice a creature.\nAdd an amount of {B} equal to the sacrificed creature's mana value.";
const BURNT_OFFERING: &str = "As an additional cost to cast this spell, sacrifice a creature.\nAdd X mana in any combination of {B} and/or {R}, where X is the sacrificed creature's mana value.";

fn pool(g: &Game, c: Color) -> u16 {
    g.engine.state.player(P0).mana.amounts[c as usize]
}

#[test]
fn sacrifice_adds_black_equal_to_the_creatures_mana_value() {
    let mut t = Table::default();
    let ritual = t.card("{B}", "Instant", None, SACRIFICE);
    let ogre = t.card("{2}{B}", "Creature — Ogre", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let victim = g.put(ogre, P0, Zone::Battlefield);
    let spell = g.put(ritual, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![victim])],
    );
    assert!(!g.engine.state.objects.contains_key(&victim));
    assert_eq!(pool(&g, Color::Black), 3);
}

#[test]
fn burnt_offering_splits_its_mana_between_black_and_red() {
    for (black, red) in [(1, 3), (4, 0), (0, 4)] {
        let mut t = Table::default();
        let ritual = t.card("{B}", "Instant", None, BURNT_OFFERING);
        let giant = t.card("{3}{R}", "Creature — Giant", Some((4, 4)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let victim = g.put(giant, P0, Zone::Battlefield);
        let spell = g.put(ritual, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(vec![victim]), Answer::Number(black)],
        );
        assert_eq!(pool(&g, Color::Black), black as u16);
        assert_eq!(pool(&g, Color::Red), red);
    }
}

const NEOFORM: &str = "As an additional cost to cast this spell, sacrifice a creature.\nSearch your library for a creature card with mana value equal to 1 plus the sacrificed creature's mana value, put that card onto the battlefield with an additional +1/+1 counter on it, then shuffle.";
const EVOLUTION: &str = "As an additional cost to cast this spell, sacrifice a creature.\nSearch your library for a creature card with mana value X or less, where X is 2 plus the sacrificed creature's mana value. Put that card onto the battlefield, then shuffle. Exile ~.";

/// A library of creatures with mana value 1 to 6, and the cards they were made from.
fn creatures(t: &mut Table) -> Vec<mtg_core::CardId> {
    ["{G}", "{1}{G}", "{2}{G}", "{3}{G}", "{4}{G}", "{5}{G}"]
        .into_iter()
        .map(|cost| t.card(cost, "Creature — Beast", Some((2, 2)), ""))
        .collect()
}

/// The search offers exactly the cards the filter admits: the choice lists them.
fn offered(g: &mut Game, spell: mtg_core::ObjectId, victim: mtg_core::ObjectId) -> Vec<u32> {
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let mut sacrificed = false;
    loop {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::NeedsChoice(c) => match &c.kind {
                mtg_engine::ChoiceKind::ChooseObjects { from, .. } if !sacrificed => {
                    sacrificed = true;
                    assert!(from.contains(&victim));
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(vec![victim]))
                        .unwrap();
                }
                mtg_engine::ChoiceKind::ChooseObjects { from, .. } => {
                    let mut values: Vec<u32> = from
                        .iter()
                        .map(|o| {
                            let card = g.engine.state.objects[o].card;
                            mtg_ir::PrintedCards::face(&g.table, card, 0)
                                .unwrap()
                                .mana_cost
                                .mana_value()
                        })
                        .collect();
                    values.sort_unstable();
                    let pick = *from.last().unwrap();
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(vec![pick]))
                        .unwrap();
                    // Let the spell finish: the card moves, the counter goes on.
                    g.drive(|_, _| None, |g| g.stack().is_empty());
                    return values;
                }
                mtg_engine::ChoiceKind::Priority { .. } => {
                    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
                }
                _ => {
                    let a = c.default.clone().unwrap();
                    g.engine.answer(&g.table, c.id, a).unwrap();
                }
            },
            mtg_engine::Progress::GameOver { .. } => panic!("game over"),
        }
    }
}

#[test]
fn neoform_finds_exactly_one_more_mana_value_and_adds_a_counter() {
    let mut t = Table::default();
    let spell = t.card("{G}{U}", "Sorcery", None, NEOFORM);
    let ogre = t.card("{2}{G}", "Creature — Ogre", Some((3, 3)), "");
    let library = creatures(&mut t);
    let mut g = Game::new(t);
    g.lands(2);
    for card in &library {
        g.put(*card, P0, Zone::Library);
    }
    let victim = g.put(ogre, P0, Zone::Battlefield);
    let neoform = g.put(spell, P0, Zone::Hand);
    g.main();
    assert_eq!(offered(&mut g, neoform, victim), vec![4]);
    let found = g.find(library[3]).expect("on the battlefield");
    assert_eq!(
        g.engine.state.objects[&found]
            .counters
            .get(&mtg_core::CounterKind::PlusOnePlusOne),
        Some(&1)
    );
    assert_eq!(g.pt(found), (3, 3));
}

#[test]
fn eldritch_evolution_finds_up_to_two_more_and_exiles_itself() {
    let mut t = Table::default();
    let spell = t.card("{1}{G}{G}", "Sorcery", None, EVOLUTION);
    let ogre = t.card("{2}{G}", "Creature — Ogre", Some((3, 3)), "");
    let library = creatures(&mut t);
    let mut g = Game::new(t);
    g.lands(3);
    for card in &library {
        g.put(*card, P0, Zone::Library);
    }
    let victim = g.put(ogre, P0, Zone::Battlefield);
    let evolution = g.put(spell, P0, Zone::Hand);
    g.main();
    // The library's filler bears (mana value 2) are creatures too.
    let values = offered(&mut g, evolution, victim);
    assert_eq!(values.iter().max(), Some(&5));
    assert!(values.contains(&1));
    assert!(g.find(library[4]).is_some(), "the 5-drop entered");
    // The spell exiled itself rather than going to the graveyard.
    let zone_of_spell = |z: Zone| {
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == spell && o.zone.zone == z)
    };
    assert!(zone_of_spell(Zone::Exile));
    assert!(!zone_of_spell(Zone::Graveyard));
}

const METAMORPHOSIS: &str = "As an additional cost to cast this spell, sacrifice a creature.\nAdd X mana of any one color, where X is 1 plus the sacrificed creature's mana value. Spend this mana only to cast creature spells.";

#[test]
fn metamorphosis_makes_one_color_for_creature_spells_only() {
    let mut t = Table::default();
    let ritual = t.card("{G}", "Sorcery", None, METAMORPHOSIS);
    let ogre = t.card("{2}{G}", "Creature — Ogre", Some((3, 3)), "");
    let beast = t.card("{2}{G}{G}", "Creature — Beast", Some((4, 4)), "");
    let sorcery = t.card("{3}{G}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(1);
    let victim = g.put(ogre, P0, Zone::Battlefield);
    let spell = g.put(ritual, P0, Zone::Hand);
    let creature = g.put(beast, P0, Zone::Hand);
    let other = g.put(sorcery, P0, Zone::Hand);
    g.main();
    // Colors are offered white, blue, black, red, green: green is the fifth.
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![victim]), Answer::Modes(vec![4])],
    );
    assert_eq!(pool(&g, Color::Green), 4);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: creature }));
    assert!(!actions.contains(&Action::Cast { object: other }));
}
