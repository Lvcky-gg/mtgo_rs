//! Deflecting Swat: "You may choose new targets for target spell or ability." (CR
//! 115.7d) The new target must be legal for the retargeted spell, and the spell keeps a
//! target the controller doesn't change.

use super::harness::*;
use mtg_core::{ObjectId, Step, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const SWAT: &str = "If you control a commander, you may cast this spell without paying its mana \
                    cost.\nYou may choose new targets for target spell or ability.";

struct Fight {
    game: Game,
    murder: ObjectId,
    swat: ObjectId,
    mine: ObjectId,
    theirs: ObjectId,
}

fn fight() -> Fight {
    fight_with("{9}", SWAT)
}

const MISDIRECTION: &str = "Change the target of target spell with a single target.";

fn fight_with(cost: &str, text: &str) -> Fight {
    let mut table = Table::default();
    let murder = table.card("{1}", "Instant", None, "Destroy target creature.");
    let swat = table.card(cost, "Instant", None, text);
    let general = table.card("{2}", "Legendary Creature — Human", Some((2, 2)), "");
    let bear = table.bear();
    let land = table.card("", "Land", None, "{T}: Add one mana of any color.");
    let mut game = Game::new(table);
    game.engine.state.commander.commanders.insert(P0, general);
    game.put(general, P0, Zone::Battlefield);
    let mine = game.put(bear, P0, Zone::Battlefield);
    let theirs = game.put(bear, P1, Zone::Battlefield);
    game.put(land, P1, Zone::Battlefield);
    let murder = game.put(murder, P1, Zone::Hand);
    let swat = game.put(swat, P0, Zone::Hand);
    Fight {
        game,
        murder,
        swat,
        mine,
        theirs,
    }
}

/// P1 casts the destroy spell at P0's Bear in P1's main phase; P0 answers with Swat,
/// choosing `redirect` for the destroy spell when asked.
fn play(f: &mut Fight, redirect: ObjectId) {
    let (murder, swat, mine) = (f.murder, f.swat, f.mine);
    let mut cast_murder = false;
    let cast_swat = std::cell::Cell::new(false);
    f.game.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { legal }
                if c.who == P1
                    && !cast_murder
                    && game.engine.state.active_player == P1
                    && game.engine.state.step == Step::PrecombatMain =>
            {
                cast_murder = true;
                assert!(legal.actions.contains(&Action::Cast { object: murder }));
                Some(Answer::Action(Action::Cast { object: murder }))
            }
            ChoiceKind::ChooseTargets { .. } if c.who == P1 => {
                Some(Answer::Targets(vec![vec![Target::Object(mine)]]))
            }
            ChoiceKind::Priority { legal } if c.who == P0 && cast_murder && !cast_swat.get() => {
                cast_swat.set(true);
                // Swat for free with a commander; anything else for its cost.
                let cast = legal
                    .actions
                    .iter()
                    .find(
                        |a| matches!(a, Action::CastAlternative { object, .. } if *object == swat),
                    )
                    .cloned()
                    .unwrap_or(Action::Cast { object: swat });
                assert!(legal.actions.contains(&cast), "{cast:?} not castable");
                Some(Answer::Action(cast))
            }
            // Swat's new targets for the destroy spell, as Swat resolves.
            ChoiceKind::ChooseTargets { slots, .. }
                if c.who == P0 && c.because.contains("new targets") =>
            {
                assert!(
                    slots[0].contains(&Target::Object(redirect)),
                    "{redirect:?} not offered: {slots:?}"
                );
                Some(Answer::Targets(vec![vec![Target::Object(redirect)]]))
            }
            // Swat's own target, as it is cast: the destroy spell on the stack.
            ChoiceKind::ChooseTargets { .. } if c.who == P0 => {
                Some(Answer::Targets(vec![vec![Target::Object(
                    murder_on_stack(game),
                )]]))
            }
            _ => None,
        },
        |game| cast_swat.get() && game.stack().is_empty(),
    );
}

/// The destroy spell's current identity on the stack.
fn murder_on_stack(game: &Game) -> ObjectId {
    *game.stack().last().expect("something on the stack")
}

#[test]
fn swat_redirects_removal_to_the_opponents_creature() {
    let mut f = fight();
    let (mine, theirs) = (f.mine, f.theirs);
    play(&mut f, theirs);
    let alive = |id| f.game.engine.state.objects.contains_key(&id);
    assert!(alive(mine), "P0's Bear survives");
    assert!(!alive(theirs), "P1's Bear was destroyed instead");
}

#[test]
fn keeping_the_target_changes_nothing() {
    let mut f = fight();
    let (mine, theirs) = (f.mine, f.theirs);
    play(&mut f, mine);
    let alive = |id| f.game.engine.state.objects.contains_key(&id);
    assert!(!alive(mine));
    assert!(alive(theirs));
}

#[test]
fn misdirection_must_change_the_target() {
    let mut f = fight_with("{0}", MISDIRECTION);
    let (mine, theirs) = (f.mine, f.theirs);
    play(&mut f, theirs);
    let alive = |id| f.game.engine.state.objects.contains_key(&id);
    assert!(alive(mine));
    assert!(!alive(theirs));
}

#[test]
#[should_panic(expected = "not offered")]
fn misdirection_cannot_keep_the_target() {
    let mut f = fight_with("{0}", MISDIRECTION);
    let mine = f.mine;
    play(&mut f, mine);
}
