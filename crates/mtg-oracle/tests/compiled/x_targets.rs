//! "Untap X target lands" (Candelabra of Tawnos), "Destroy X target artifacts" (By Force):
//! X is announced before targets (CR 601.2b-c), and the slot takes exactly X objects.
//! "Destroy up to X target lands, where X is the number of verse counters on this
//! enchantment" (Rumbling Crescendo): the slot's size is the defined X.

use super::harness::*;
use mtg_core::{ObjectId, Target, Zone};
use mtg_engine::Progress;
use mtg_engine::actions::Action;
use mtg_engine::choice::{Answer, ChoiceKind};

/// Take `action` with X = `x`, choosing `picks` one slot at a time (none once they run
/// out), and let the stack resolve.
fn with_x(g: &mut Game, action: Action, x: u32, picks: &[ObjectId]) {
    let c = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, c.id, Answer::Action(action))
        .unwrap();
    let mut picks = picks.iter();
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::ChooseX { .. } => Answer::Number(x),
                    ChoiceKind::ChooseTargets { .. } => Answer::Targets(vec![
                        picks
                            .next()
                            .map(|&o| Target::Object(o))
                            .into_iter()
                            .collect(),
                    ]),
                    ChoiceKind::Priority { .. } if c.who == P0 && g.stack().is_empty() => {
                        g.pending = Some(c);
                        return;
                    }
                    ChoiceKind::Priority { .. } => Answer::Pass,
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine
                    .answer(&g.table, c.id, answer)
                    .unwrap_or_else(|e| panic!("{:?}: {e:?}", c.kind));
            }
        }
    }
    panic!("never resolved");
}

#[test]
fn candelabra_untaps_x_lands() {
    let mut t = Table::default();
    let candelabra = t.card("{1}", "Artifact", None, "{X}, {T}: Untap X target lands.");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    let candelabra = g.put(candelabra, P0, Zone::Battlefield);
    let lands: Vec<_> = (0..4)
        .map(|_| g.put(forest, P0, Zone::Battlefield))
        .collect();
    g.main();
    // Two of the Forests pay for X; untap them both.
    with_x(&mut g, activate(candelabra, 0), 2, &lands[..2]);
    let tapped = lands
        .iter()
        .filter(|l| g.engine.state.objects[l].tapped)
        .count();
    assert_eq!(tapped, 0, "the two used for X were untapped again");
    assert!(g.engine.state.objects[&candelabra].tapped);
}

#[test]
fn by_force_destroys_exactly_x() {
    let mut t = Table::default();
    let by_force = t.card("{X}{R}", "Sorcery", None, "Destroy X target artifacts.");
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(by_force, P0, Zone::Hand);
    let rocks: Vec<_> = (0..3).map(|_| g.put(rock, P1, Zone::Battlefield)).collect();
    g.main();
    with_x(&mut g, Action::Cast { object: spell }, 2, &rocks[..2]);
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
    assert!(g.engine.state.objects.contains_key(&rocks[2]));
}

#[test]
fn x_beyond_the_legal_targets_rewinds_the_cast() {
    let mut t = Table::default();
    let by_force = t.card("{X}{R}", "Sorcery", None, "Destroy X target artifacts.");
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(by_force, P0, Zone::Hand);
    let rock = g.put(rock, P1, Zone::Battlefield);
    g.main();
    with_x(&mut g, Action::Cast { object: spell }, 2, &[rock]);
    assert_eq!(g.engine.state.objects[&spell].zone.zone, Zone::Hand);
    assert!(g.engine.state.objects.contains_key(&rock));
}

#[test]
fn x_of_zero_takes_no_targets() {
    let mut t = Table::default();
    let by_force = t.card("{X}{R}", "Sorcery", None, "Destroy X target artifacts.");
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(by_force, P0, Zone::Hand);
    let rock = g.put(rock, P1, Zone::Battlefield);
    g.main();
    with_x(&mut g, Action::Cast { object: spell }, 0, &[]);
    assert!(g.engine.state.objects.contains_key(&rock));
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "it resolved");
}

#[test]
fn a_defined_x_sizes_the_slot() {
    let mut t = Table::default();
    let crescendo = t.card(
        "{3}{R}{R}",
        "Enchantment",
        None,
        "{R}, Sacrifice this enchantment: Destroy up to X target lands, where X is the number \
         of creatures you control.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let crescendo = g.put(crescendo, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    let lands: Vec<_> = (0..3)
        .map(|_| g.put(forest, P1, Zone::Battlefield))
        .collect();
    g.main();
    with_x(&mut g, activate(crescendo, 0), 0, &lands[..2]);
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
}
