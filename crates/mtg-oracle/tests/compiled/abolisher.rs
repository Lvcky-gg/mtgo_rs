//! Grand Abolisher: "During your turn, your opponents can't cast spells or activate
//! abilities of artifacts, creatures, or enchantments." — mana abilities included.

use super::harness::*;
use mtg_core::{ObjectId, Zone};
use mtg_engine::{actions::LegalActions, choice::ChoiceKind};

const ABOLISHER: &str = "During your turn, your opponents can't cast spells or activate \
                         abilities of artifacts, creatures, or enchantments.";

/// P1's first priority offer in P0's turn (after P0 passes in the main phase), then in
/// P1's own turn.
fn offers(text: &str) -> (LegalActions, LegalActions, ObjectId, ObjectId) {
    let mut table = Table::default();
    let abolisher = table.card("{W}", "Creature — Human Cleric", Some((2, 2)), text);
    let instant = table.card("{0}", "Instant", None, "You gain 1 life.");
    let rock = table.card("{0}", "Artifact", None, "{T}: Add {C}.");
    let mut game = Game::new(table);
    game.put(abolisher, P0, Zone::Battlefield);
    let instant = game.put(instant, P1, Zone::Hand);
    let rock = game.put(rock, P1, Zone::Battlefield);
    game.main();
    let mut in_p0s_turn = None;
    let mut in_p1s_turn = None;
    game.drive(
        |game, c| {
            if let ChoiceKind::Priority { legal } = &c.kind
                && c.who == P1
            {
                if game.engine.state.active_player == P0 {
                    in_p0s_turn.get_or_insert_with(|| legal.clone());
                } else {
                    in_p1s_turn.get_or_insert_with(|| legal.clone());
                }
            }
            None
        },
        |game| {
            game.engine.state.active_player == P1
                && game.engine.state.step == mtg_core::Step::PrecombatMain
        },
    );
    (
        in_p0s_turn.unwrap(),
        in_p1s_turn.unwrap_or_else(|| panic!("P1 never had priority in its own turn")),
        instant,
        rock,
    )
}

fn can_cast(legal: &LegalActions, spell: ObjectId) -> bool {
    legal
        .actions
        .contains(&mtg_engine::actions::Action::Cast { object: spell })
}

fn can_tap(legal: &LegalActions, rock: ObjectId) -> bool {
    legal.mana_abilities.iter().any(|a| {
        matches!(a, mtg_engine::actions::Action::ActivateManaAbility { source, .. } if *source == rock)
    })
}

#[test]
fn opponents_cant_cast_or_tap_artifacts_during_your_turn() {
    let (theirs, own, instant, rock) = offers(ABOLISHER);
    assert!(!can_cast(&theirs, instant));
    assert!(!can_tap(&theirs, rock));
    assert!(can_cast(&own, instant), "on their own turn they can");
    assert!(can_tap(&own, rock));
}

#[test]
fn without_it_they_can() {
    let (theirs, _, instant, rock) = offers("");
    assert!(can_cast(&theirs, instant));
    assert!(can_tap(&theirs, rock));
}
