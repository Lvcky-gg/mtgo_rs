//! Propaganda / Ghostly Prison: "Creatures can't attack you unless their controller pays
//! {2} for each creature they control that's attacking you." (CR 508.1h)

use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
};

const PROPAGANDA: &str = "Creatures can't attack you unless their controller pays {2} for each \
                          creature they control that's attacking you.";

/// P0 with `lands` lands attacks P1 (who has Propaganda) with a Bear. Whether the
/// declaration was accepted.
fn attack(lands: usize) -> (bool, Game) {
    let mut table = Table::default();
    let propaganda = table.card("{2}{U}", "Enchantment", None, PROPAGANDA);
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(lands);
    game.put(propaganda, P1, Zone::Battlefield);
    let bear = game.put(bear, P0, Zone::Battlefield);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine.answer(&game.table, c.id, Answer::Pass).unwrap();
    loop {
        match game.engine.advance(&game.table) {
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::DeclareAttackers { .. } => {
                    let ok = game
                        .engine
                        .answer(
                            &game.table,
                            c.id,
                            Answer::Attackers(vec![(bear, Target::Player(P1))]),
                        )
                        .is_ok();
                    return (ok, game);
                }
                _ => {
                    let a = c.default.clone().unwrap_or(Answer::Pass);
                    game.engine.answer(&game.table, c.id, a).unwrap();
                }
            },
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
        }
    }
}

#[test]
fn paying_two_lets_one_creature_attack() {
    let (ok, mut game) = attack(2);
    assert!(ok);
    let tapped = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &game.engine.state.objects[id];
            o.tapped && o.controller == P0
        })
        .count();
    assert_eq!(tapped, 3, "two lands paid, and the attacker");
    game.until(P0, Step::PostcombatMain);
    assert_eq!(game.life(P1), 18);
}

#[test]
fn unable_to_pay_the_attack_is_refused() {
    let (ok, _) = attack(1);
    assert!(!ok);
}
