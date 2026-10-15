use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::actions::Action;

const DRAIN: &str = "Counter target spell. At the beginning of your next main phase, add an amount of {C} equal to that spell's mana value.";

#[test]
fn mana_drain_remembers_the_amount_across_cleanup_and_waits_for_your_main_phase() {
    let mut t = Table::default();
    let drain = t.card("{U}{U}", "Instant", None, DRAIN);
    let spell = t.card("{3}", "Sorcery", None, "You gain 5 life.");
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(2);
    for _ in 0..3 {
        g.put(mountain, P1, Zone::Battlefield);
    }
    let drain = g.put(drain, P0, Zone::Hand);
    let spell = g.put(spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.act_holding(Action::Cast { object: spell }, &[]);
    let target = g.stack()[0];
    g.cast(drain, &[Target::Object(target)]);
    assert_eq!(g.life(P1), 20);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
    g.until(P1, Step::PostcombatMain);
    assert_eq!(
        g.engine.state.player(P0).mana.total(),
        0,
        "not an opponent's main phase"
    );
    g.until(P0, Step::PrecombatMain);
    assert_eq!(g.engine.state.player(P0).mana.amounts[5], 3);
    g.until(P0, Step::PostcombatMain);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0, "fires only once");
}

#[test]
fn mana_drain_during_your_first_main_phase_pays_at_your_second() {
    let mut t = Table::default();
    let drain = t.card("{U}{U}", "Instant", None, DRAIN);
    let spell = t.card("{3}", "Sorcery", None, "You gain 5 life.");
    let mut g = Game::new(t);
    g.lands(5);
    let drain = g.put(drain, P0, Zone::Hand);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: spell }, &[]);
    let target = g.stack()[0];
    g.cast(drain, &[Target::Object(target)]);
    g.until(P0, Step::PostcombatMain);
    assert_eq!(g.engine.state.player(P0).mana.amounts[5], 3);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn mana_drain_adds_mana_even_when_the_target_cannot_be_countered() {
    let mut t = Table::default();
    let drain = t.card("{U}{U}", "Instant", None, DRAIN);
    let spell = t.card(
        "{3}",
        "Sorcery",
        None,
        "This spell can't be countered.\nYou gain 5 life.",
    );
    let mut g = Game::new(t);
    g.lands(5);
    let drain = g.put(drain, P0, Zone::Hand);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: spell }, &[]);
    let target = g.stack()[0];
    g.cast(drain, &[Target::Object(target)]);
    assert_eq!(g.life(P0), 25);
    g.until(P0, Step::PostcombatMain);
    assert_eq!(g.engine.state.player(P0).mana.amounts[5], 3);
}

#[test]
fn mana_drain_remembers_x_after_countering_the_spell_and_crossing_cleanup() {
    use mtg_engine::choice::{Answer, ChoiceKind};
    let mut t = Table::default();
    let drain = t.card("{U}{U}", "Instant", None, DRAIN);
    let spell = t.card("{X}{1}", "Sorcery", None, "You gain X life.");
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(2);
    for _ in 0..5 {
        g.put(mountain, P1, Zone::Battlefield);
    }
    let drain = g.put(drain, P0, Zone::Hand);
    let spell = g.put(spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    let prompt = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    g.drive(
        |_, c| match c.kind {
            ChoiceKind::ChooseX { .. } => Some(Answer::Number(4)),
            _ => None,
        },
        |g| g.engine.state.priority == Some(P0) && g.stack().len() == 1,
    );
    let original = g.stack()[0];
    g.cast(drain, &[Target::Object(original)]);
    g.until(P0, Step::PrecombatMain);
    assert_eq!(g.engine.state.player(P0).mana.amounts[5], 5);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn mana_drain_does_not_schedule_mana_if_its_target_is_illegal_on_resolution() {
    let mut t = Table::default();
    let drain = t.card("{U}{U}", "Instant", None, DRAIN);
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let spell = t.card("{3}", "Sorcery", None, "You gain 5 life.");
    let mut g = Game::new(t);
    g.lands(5);
    let drain = g.put(drain, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: spell }, &[]);
    let original = g.stack()[0];
    g.act_holding(Action::Cast { object: drain }, &[Target::Object(original)]);
    g.cast(counter, &[Target::Object(original)]);
    g.until(P0, Step::PostcombatMain);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
}
