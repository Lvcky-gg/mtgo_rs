//! Vivi Ornitier: "{0}: Add X mana in any combination of {U} and/or {R}, where X is Vivi
//! Ornitier's power. Activate only during your turn and only once each turn. Whenever you
//! cast a noncreature spell, put a +1/+1 counter on Vivi Ornitier and it deals 1 damage to
//! each opponent."

use super::harness::*;
use mtg_core::{CounterKind, Step, Zone};
use mtg_engine::{actions::Action, mana::mana_sources};

const VIVI: &str = "{0}: Add X mana in any combination of {U} and/or {R}, where X is ~'s \
                    power. Activate only during your turn and only once each turn.\nWhenever \
                    you cast a noncreature spell, put a +1/+1 counter on ~ and it deals 1 \
                    damage to each opponent.";

fn castable(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

fn vivi_with_power(power: i32) -> (Game, mtg_core::ObjectId, Spells) {
    let mut t = Table::default();
    let vivi = t.named_card(
        "Vivi Ornitier",
        "{1}{U}{R}",
        "Legendary Creature — Wizard",
        Some((0, 3)),
        VIVI,
    );
    let mixed = t.card("{U}{R}", "Sorcery", None, "You gain 3 life.");
    let blue = t.card("{U}", "Sorcery", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let v = g.put(vivi, P0, Zone::Battlefield);
    if power > 0 {
        g.engine
            .state
            .objects
            .get_mut(&v)
            .unwrap()
            .counters
            .insert(CounterKind::PlusOnePlusOne, power);
    }
    let mixed = g.put(mixed, P0, Zone::Hand);
    let blue = g.put(blue, P0, Zone::Hand);
    (g, v, Spells { mixed, blue })
}

struct Spells {
    mixed: mtg_core::ObjectId,
    blue: mtg_core::ObjectId,
}

#[test]
fn one_activation_pays_blue_and_red_together_once_a_turn() {
    let (mut g, v, spells) = vivi_with_power(2);
    let actions = g.main();
    assert!(
        castable(&actions, spells.mixed),
        "{{U}}{{R}} from Vivi alone"
    );
    g.cast(spells.mixed, &[]);
    assert_eq!(g.life(P0), 23);
    // The noncreature spell grew Vivi and pinged the opponent.
    assert_eq!(g.pt(v), (3, 6));
    assert_eq!(g.life(P1), 19);
    // Once each turn: nothing left for the second spell.
    let actions = g.main();
    assert!(!castable(&actions, spells.blue));
}

#[test]
fn makes_as_much_as_its_power_and_only_on_your_turn() {
    let (mut g, v, spells) = vivi_with_power(1);
    let actions = g.main();
    assert!(!castable(&actions, spells.mixed), "one mana is not two");
    assert!(castable(&actions, spells.blue));
    let source = mana_sources(&g.engine.state, &g.table, P0)
        .into_iter()
        .find(|s| s.object == v)
        .expect("a mana source on your turn");
    assert_eq!(source.outputs.len(), 1);
    // On the opponent's turn it makes nothing.
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
        .unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.active_player == P1 && g.engine.state.step == Step::PrecombatMain,
    );
    assert!(
        mana_sources(&g.engine.state, &g.table, P0)
            .iter()
            .all(|s| s.object != v)
    );
}

#[test]
fn by_hand_it_makes_one_color() {
    let (mut g, v, _) = vivi_with_power(3);
    g.main();
    let legal = match &g.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => legal.clone(),
        _ => unreachable!(),
    };
    let red = legal
        .mana_abilities
        .iter()
        .find(|a| {
            matches!(a, Action::ActivateManaAbility { source, color: Some(mtg_core::Color::Red), .. } if *source == v)
        })
        .cloned()
        .expect("red offered");
    g.act(red, &[], &[]);
    let pool = g.engine.state.player(P0).mana.amounts;
    assert_eq!(pool[mtg_core::Color::Red as usize], 3);
}
