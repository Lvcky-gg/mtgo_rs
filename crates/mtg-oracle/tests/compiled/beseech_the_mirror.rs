//! Beseech the Mirror: "Bargain. Search your library for a card, exile it face down, then
//! shuffle. If this spell was bargained, you may cast the exiled card without paying its
//! mana cost if that spell's mana value is 4 or less. Put the exiled card into your hand if
//! it wasn't cast this way."

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const BESEECH: &str = "Bargain (You may sacrifice an artifact, enchantment, or token as you \
                       cast this spell.)\nSearch your library for a card, exile it face down, \
                       then shuffle. If this spell was bargained, you may cast the exiled card \
                       without paying its mana cost if that spell's mana value is 4 or less. \
                       Put the exiled card into your hand if it wasn't cast this way.";

/// Cast Beseech (bargaining when `bargain`) for a sorcery of mana value `mv`; life after,
/// and whether the found card ended in hand.
fn beseech(bargain: bool, mv: &str) -> (i32, bool) {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, BESEECH);
    let prize = t.card(mv, "Sorcery", None, "You gain 4 life.");
    let trinket = t.card("{0}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.put(trinket, P0, Zone::Battlefield);
    g.put(prize, P0, Zone::Library);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let done = std::cell::Cell::new(false);
    g.drive(
        |g, c| match &c.kind {
            ChoiceKind::Confirm if c.because.contains("kicker") => Some(Answer::Bool(bargain)),
            ChoiceKind::Confirm => Some(Answer::Bool(true)),
            ChoiceKind::ChooseObjects { from, .. } => {
                let pick = from
                    .iter()
                    .copied()
                    .find(|o| g.engine.state.objects[o].card == prize)
                    .or_else(|| from.first().copied());
                done.set(true);
                Some(Answer::Objects(pick.into_iter().collect()))
            }
            _ => None,
        },
        |g| done.get() && g.stack().is_empty() && g.engine.state.step == Step::PrecombatMain,
    );
    let in_hand = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|o| g.engine.state.objects[o].card == prize);
    (g.life(P0), in_hand)
}

#[test]
fn bargained_it_casts_a_small_card_for_free() {
    assert_eq!(beseech(true, "{3}"), (24, false));
}

#[test]
fn bargained_but_too_big_it_goes_to_hand() {
    assert_eq!(beseech(true, "{5}"), (20, true));
}

#[test]
fn not_bargained_it_goes_to_hand() {
    assert_eq!(beseech(false, "{1}"), (20, true));
}
