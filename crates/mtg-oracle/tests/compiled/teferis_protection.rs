//! Teferi's Protection: "Until your next turn, your life total can't change and you gain
//! protection from everything. All permanents you control phase out." (CR 702.26,
//! 702.16j, 119.10)

use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const PROTECTION: &str = "Until your next turn, your life total can't change and you gain \
                          protection from everything. All permanents you control phase out.";

#[test]
fn nothing_touches_you_until_your_next_turn() {
    let mut table = Table::default();
    let protection = table.card("{0}", "Instant", None, PROTECTION);
    let bolt = table.card(
        "{0}",
        "Instant",
        None,
        "This spell deals 3 damage to any target.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    let protection = game.put(protection, P0, Zone::Hand);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    let bolt = game.put(bolt, P1, Zone::Hand);
    game.main();
    game.cast(protection, &[]);
    assert!(game.engine.state.objects[&mine].phased_out);
    assert!(
        !game.engine.state.battlefield().contains(&mine),
        "treated as though it doesn't exist"
    );

    // P1's turn: P1 looks for targets for its burn spell, and attacks.
    let offered = std::cell::RefCell::new(None);
    let cast = std::cell::Cell::new(false);
    game.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. }
                if c.who == P1
                    && !cast.get()
                    && game.engine.state.active_player == P1
                    && game.engine.state.step == Step::PrecombatMain =>
            {
                cast.set(true);
                Some(Answer::Action(Action::Cast { object: bolt }))
            }
            ChoiceKind::ChooseTargets { slots, .. } if c.who == P1 => {
                *offered.borrow_mut() = Some(slots[0].clone());
                Some(Answer::Targets(vec![vec![Target::Player(P1)]]))
            }
            ChoiceKind::DeclareAttackers { .. } if c.who == P1 => {
                Some(Answer::Attackers(vec![(theirs, Target::Player(P0))]))
            }
            _ => None,
        },
        |game| game.engine.state.active_player == P1 && game.engine.state.step == Step::End,
    );
    let offered = offered
        .borrow()
        .clone()
        .expect("the burn spell asked for a target");
    assert!(
        !offered.contains(&Target::Player(P0)),
        "P0 can't be targeted"
    );
    assert!(
        !offered.contains(&Target::Object(mine)),
        "nor its phased-out Bear"
    );
    assert_eq!(
        game.life(P1),
        17,
        "the burn spell hit its own caster instead"
    );
    assert!(
        game.engine.log.iter().any(|e| matches!(
            e.event,
            mtg_core::Event::Attacked { attacker, .. } if attacker == theirs
        )),
        "P1 did attack"
    );
    assert_eq!(game.life(P0), 20, "the attack's damage was prevented");

    // P0's next turn: it phases back in, and the protection is gone.
    game.until(P0, Step::Upkeep);
    assert!(!game.engine.state.objects[&mine].phased_out);
    assert!(game.engine.state.battlefield().contains(&mine));
}
