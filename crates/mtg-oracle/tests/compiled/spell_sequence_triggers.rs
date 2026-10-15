use super::harness::*;
use mtg_core::{PlayerId, Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer, state::PlayerState};

const HIPPOCAMP: &str =
    "Whenever you cast your first spell during each opponent's turn, draw a card.";
const TRACKER: &str = "Whenever you cast a spell other than your first spell each turn, put a +1/+1 counter on this creature.";

fn our_priority(g: &mut Game) {
    let question = g.pending.take().unwrap();
    if question.who == P0 {
        g.pending = Some(question);
        return;
    }
    g.engine
        .answer(&g.table, question.id, Answer::Pass)
        .unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.priority == Some(P0) && g.stack().is_empty(),
    );
}

#[test]
fn hippocamp_triggers_once_on_each_opponents_turn_and_never_on_its_owners_turn() {
    let mut t = Table::default();
    let hippocamp = t.card("{0}", "Creature", Some((2, 2)), HIPPOCAMP);
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let filler = t.bear();
    let mut g = Game::new(t);
    let p2 = PlayerId(2);
    g.engine.state.players.insert(p2, PlayerState::new(p2, 20));
    g.engine.state.turn_order.push(p2);
    for _ in 0..5 {
        g.put(filler, p2, Zone::Library);
    }
    g.put(hippocamp, P0, Zone::Battlefield);
    let spells: Vec<_> = (0..6).map(|_| g.put(instant, P0, Zone::Hand)).collect();
    g.main();
    for (index, active, draw) in [
        (0, P0, false),
        (1, P1, true),
        (2, P1, false),
        (3, p2, true),
        (4, p2, false),
        (5, P0, false),
    ] {
        if g.engine.state.active_player != active {
            g.until(active, Step::PrecombatMain);
        }
        our_priority(&mut g);
        let hand = g.count(Zone::Hand, P0);
        g.cast(spells[index], &[]);
        assert_eq!(
            g.count(Zone::Hand, P0),
            hand - 1 + usize::from(draw),
            "cast {index}"
        );
    }
}

#[test]
fn hippocamp_ignores_other_players_casts() {
    let mut t = Table::default();
    let hippocamp = t.card("{0}", "Creature", Some((2, 2)), HIPPOCAMP);
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.put(hippocamp, P0, Zone::Battlefield);
    let ours = g.put(instant, P0, Zone::Hand);
    let theirs = g.put(instant, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    let hand = g.count(Zone::Hand, P0);
    g.act(Action::Cast { object: theirs }, &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    g.cast(ours, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn hippocamp_does_not_treat_the_first_spell_after_it_enters_as_the_turns_first_spell() {
    let mut t = Table::default();
    let hippocamp = t.card("{0}", "Creature", Some((2, 2)), HIPPOCAMP);
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let first = g.put(instant, P0, Zone::Hand);
    let second = g.put(instant, P0, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    our_priority(&mut g);
    g.cast(first, &[]);
    g.put(hippocamp, P0, Zone::Battlefield);
    let hand = g.count(Zone::Hand, P0);
    g.cast(second, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
}

#[test]
fn countering_the_first_spell_does_not_reset_hippocamps_count() {
    let mut t = Table::default();
    let hippocamp = t.card("{0}", "Creature", Some((2, 2)), HIPPOCAMP);
    let instant = t.card("{0}", "Instant", None, "You gain 5 life.");
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    g.put(hippocamp, P0, Zone::Battlefield);
    let first = g.put(instant, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    our_priority(&mut g);
    let hand = g.count(Zone::Hand, P0);
    g.act_holding(Action::Cast { object: first }, &[]);
    let first = g
        .stack()
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == instant)
        .unwrap();
    g.cast(counter, &[Target::Object(first)]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn copying_a_spell_does_not_count_as_an_additional_cast_for_tracker() {
    let mut t = Table::default();
    let tracker = t.card("{0}", "Creature", Some((2, 2)), TRACKER);
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let copy = t.card(
        "{0}",
        "Instant",
        None,
        "Copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    let mut g = Game::new(t);
    let source = g.put(tracker, P0, Zone::Battlefield);
    let first = g.put(instant, P0, Zone::Hand);
    let copy = g.put(copy, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: first }, &[]);
    let original = g
        .stack()
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == instant)
        .unwrap();
    g.cast(copy, &[Target::Object(original)]);
    assert_eq!(g.pt(source), (3, 3));
    assert_eq!(g.life(P0), 22);
    assert_eq!(g.engine.state.spells_by_player.get(&P0), Some(&2));
}

#[test]
fn tracker_triggers_on_every_spell_after_the_first_and_resets_each_turn() {
    let mut t = Table::default();
    let tracker = t.card("{0}", "Creature", Some((2, 2)), TRACKER);
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let source = g.put(tracker, P0, Zone::Battlefield);
    let spells: Vec<_> = (0..5).map(|_| g.put(instant, P0, Zone::Hand)).collect();
    g.main();
    for (index, size) in [(0, 2), (1, 3), (2, 4)] {
        g.cast(spells[index], &[]);
        assert_eq!(g.pt(source), (size, size));
    }
    g.until(P1, Step::PrecombatMain);
    our_priority(&mut g);
    g.cast(spells[3], &[]);
    assert_eq!(g.pt(source), (4, 4));
    g.cast(spells[4], &[]);
    assert_eq!(g.pt(source), (5, 5));
}
