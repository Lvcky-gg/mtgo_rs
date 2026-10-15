//! Birgi, God of Storytelling: "Whenever you cast a spell, add {R}. Until end of turn, you
//! don't lose this mana as steps and phases end. Creatures you control can boast twice
//! during each of your turns rather than once."

use super::harness::*;
use mtg_core::{Color, Step, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const BIRGI: &str = "Whenever you cast a spell, add {R}. Until end of turn, you don't lose \
                     this mana as steps and phases end.\nCreatures you control can boast \
                     twice during each of your turns rather than once.";

fn red(g: &Game) -> u16 {
    g.engine.state.player(P0).mana.amounts[Color::Red as usize]
}

#[test]
fn the_red_mana_lasts_until_end_of_turn() {
    let mut t = Table::default();
    let birgi = t.card("{2}{R}", "Legendary Creature — God", Some((3, 3)), BIRGI);
    let gift = t.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.put(birgi, P0, Zone::Battlefield);
    let gift = g.put(gift, P0, Zone::Hand);
    g.main();
    g.cast(gift, &[]);
    assert_eq!(red(&g), 1);
    // Through combat to the postcombat main phase: still there.
    g.combat(&[], &[], &[], &[]);
    assert_eq!(g.engine.state.step, Step::PostcombatMain);
    assert_eq!(red(&g), 1);
    // Gone once the turn ends.
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |g| g.engine.state.active_player == P1);
    assert_eq!(red(&g), 0);
    assert!(g.engine.state.persistent_mana.is_empty());
}

#[test]
fn creatures_boast_twice_on_your_turn() {
    let mut t = Table::default();
    let birgi = t.card("{2}{R}", "Legendary Creature — God", Some((3, 3)), BIRGI);
    let braggart = t.card(
        "{1}{R}",
        "Creature — Warrior",
        Some((2, 2)),
        "Boast — {0}: You gain 1 life. (Activate only if this creature attacked this turn \
         and only once each turn.)",
    );
    let mut g = Game::new(t);
    g.put(birgi, P0, Zone::Battlefield);
    let b = g.put(braggart, P0, Zone::Battlefield);
    g.main();
    g.combat(&[b], &[], &[], &[]);
    let now = |g: &Game| match &g.pending.as_ref().unwrap().kind {
        mtg_engine::choice::ChoiceKind::Priority { legal } => legal.actions.clone(),
        _ => Vec::new(),
    };
    for _ in 0..2 {
        let actions = now(&g);
        let boast = actions
            .iter()
            .find(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == b))
            .cloned()
            .expect("boast offered");
        g.act(boast, &[], &[]);
    }
    let actions = now(&g);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == b)),
        "not a third time"
    );
    assert_eq!(g.life(P0), 22);
}
