//! Ashiok, Dream Render and Opposition Agent: what happens to an opponent's library search.

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const ASHIOK: &str = "Spells and abilities your opponents control can't cause their \
                      controller to search their library.\n−1: Target player mills four \
                      cards. Then exile each opponent's graveyard.";
const AGENT: &str = "Flash\nYou control your opponents while they're searching their \
                     libraries.\nWhile an opponent is searching their library, they exile \
                     each card they find. You may play those cards for as long as they remain \
                     exiled, and you may spend mana as though it were mana of any color to \
                     cast them.";
const TUTOR: &str = "Search your library for a card, put it into your hand, then shuffle.";

/// P0 casts a tutor with `hoser` on P1's side and a {G}{G} sorcery in P0's library; who
/// was asked to choose, and the game after.
fn tutor_against(hoser: &str) -> (Game, Vec<mtg_core::PlayerId>, mtg_core::CardId) {
    let mut t = Table::default();
    let hoser = t.card("{2}{B}", "Creature — Human Rogue", Some((3, 2)), hoser);
    let tutor = t.card("{0}", "Sorcery", None, TUTOR);
    let prize = t.card("{G}{G}", "Sorcery", None, "You gain 4 life.");
    let mut g = Game::new(t);
    g.put(hoser, P1, Zone::Battlefield);
    g.put(prize, P0, Zone::Library);
    let tutor = g.put(tutor, P0, Zone::Hand);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: tutor }),
        )
        .unwrap();
    let asked = std::cell::RefCell::new(Vec::new());
    g.drive(
        |g, c| match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } => {
                asked.borrow_mut().push(c.who);
                // Pick the prize.
                let pick = from
                    .iter()
                    .copied()
                    .find(|o| g.engine.state.objects[o].card == prize);
                Some(Answer::Objects(pick.into_iter().collect()))
            }
            _ => None,
        },
        |g| g.stack().is_empty() && g.engine.state.step == Step::PrecombatMain,
    );
    (g, asked.into_inner(), prize)
}

fn in_hand(g: &Game, p: mtg_core::PlayerId, card: mtg_core::CardId) -> bool {
    g.engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, p))
        .iter()
        .any(|o| g.engine.state.objects[o].card == card)
}

#[test]
fn ashiok_stops_an_opponents_own_search() {
    let (g, asked, prize) = tutor_against(ASHIOK.split('\n').next().unwrap());
    assert!(asked.is_empty(), "no search: {asked:?}");
    assert!(!in_hand(&g, P0, prize));
}

#[test]
fn without_ashiok_the_tutor_finds_it() {
    let (g, asked, prize) = tutor_against("");
    assert_eq!(asked, vec![P0]);
    assert!(in_hand(&g, P0, prize));
}

#[test]
fn opposition_agent_searches_for_them_and_takes_what_is_found() {
    let (mut g, asked, prize) = tutor_against(AGENT);
    assert_eq!(asked, vec![P1], "the Agent's controller chooses");
    assert!(!in_hand(&g, P0, prize));
    let exiled = g
        .engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .into_iter()
        .find(|o| g.engine.state.objects[o].card == prize)
        .expect("exiled");
    // P1 casts it on their turn with colorless mana: any type will do.
    let colorless = g.table.card("", "Land", None, "{T}: Add {C}.");
    g.put(colorless, P1, Zone::Battlefield);
    g.put(colorless, P1, Zone::Battlefield);
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    let cast = std::cell::Cell::new(false);
    g.drive(
        |g, c| match &c.kind {
            ChoiceKind::Priority { legal }
                if c.who == P1
                    && g.engine.state.active_player == P1
                    && g.engine.state.step == Step::PrecombatMain
                    && !cast.get() =>
            {
                assert!(
                    legal.actions.contains(&Action::Cast { object: exiled }),
                    "castable by the Agent's controller"
                );
                cast.set(true);
                Some(Answer::Action(Action::Cast { object: exiled }))
            }
            _ => None,
        },
        |g| cast.get() && g.stack().is_empty(),
    );
    assert_eq!(g.life(P1), 24);
}
