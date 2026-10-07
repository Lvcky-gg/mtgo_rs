//! "This spell costs {3} less to cast if it targets a tapped creature." — a cost that reads
//! the spell's own targets (CR 601.2f).
use super::harness::*;
use mtg_core::{ObjectId, Target, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use mtg_ir::PrintedCards;

const SEIZE: &str =
    "This spell costs {3} less to cast if it targets a tapped creature.\nDestroy target creature.";

fn tapped(g: &mut Game, id: ObjectId) {
    g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
}

fn untapped_lands(g: &Game) -> usize {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &g.engine.state.objects[id];
            o.controller == P0 && !o.tapped && g.table.face(o.card, 0).unwrap().abilities.len() == 1
        })
        .count()
}

/// Announce the cast and return the target slots the engine offers.
fn offered_targets(g: &mut Game, object: ObjectId) -> (Vec<Target>, u64) {
    let c = g.pending.take().expect("no priority prompt");
    g.engine
        .answer(&g.table, c.id, Answer::Action(Action::Cast { object }))
        .unwrap();
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::ChooseTargets { slots, .. } => return (slots[0].clone(), c.id),
                other => panic!("unexpected {other:?}"),
            },
            Progress::GameOver { .. } => panic!("game over"),
        }
    }
}

#[test]
fn discount_applies_when_the_target_qualifies() {
    let mut t = Table::default();
    let seize = t.card("{4}{W}", "Instant", None, SEIZE);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let tired = g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    tapped(&mut g, tired);
    let spell = g.put(seize, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: spell }),
        "two lands cast it at {{1}}{{W}} with a tapped target"
    );
    let (offered, _) = offered_targets(&mut g, spell);
    assert_eq!(
        offered,
        vec![Target::Object(tired)],
        "only the discount makes it affordable, so only the tapped creature is offered"
    );
}

#[test]
fn discounted_cast_destroys_the_tapped_creature() {
    let mut t = Table::default();
    let seize = t.card("{4}{W}", "Instant", None, SEIZE);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let tired = g.put(bear, P1, Zone::Battlefield);
    tapped(&mut g, tired);
    let spell = g.put(seize, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(tired)]);
    assert!(g.find(bear).is_none_or(|id| id != tired));
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert_eq!(untapped_lands(&g), 0);
}

#[test]
fn no_qualifying_target_means_no_discount() {
    let mut t = Table::default();
    let seize = t.card("{4}{W}", "Instant", None, SEIZE);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(seize, P0, Zone::Hand);
    let actions = g.main();
    assert!(!actions.contains(&Action::Cast { object: spell }));
}

#[test]
fn untapped_target_pays_full_cost() {
    let mut t = Table::default();
    let seize = t.card("{4}{W}", "Instant", None, SEIZE);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(6);
    let tired = g.put(bear, P1, Zone::Battlefield);
    let fresh = g.put(bear, P1, Zone::Battlefield);
    tapped(&mut g, tired);
    let spell = g.put(seize, P0, Zone::Hand);
    g.main();
    let (offered, choice) = offered_targets(&mut g, spell);
    assert!(
        offered.contains(&Target::Object(fresh)),
        "the full cost is affordable, so any creature can be targeted"
    );
    g.engine
        .answer(
            &g.table,
            choice,
            Answer::Targets(vec![vec![Target::Object(fresh)]]),
        )
        .unwrap();
    for _ in 0..100 {
        if let Progress::NeedsChoice(c) = g.engine.advance(&g.table) {
            if g.stack().is_empty() {
                break;
            }
            let a = c.default.clone().unwrap_or(Answer::Pass);
            g.engine.answer(&g.table, c.id, a).unwrap();
        }
    }
    assert!(!g.engine.state.objects.contains_key(&fresh), "destroyed");
    assert_eq!(untapped_lands(&g), 1, "{{4}}{{W}} paid in full");
}
