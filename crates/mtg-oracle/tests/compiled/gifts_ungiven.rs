//! Gifts Ungiven and Intuition: the searcher finds cards, an opponent splits them.

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const GIFTS: &str = "Search your library for up to four cards with different names and reveal \
                     them. Target opponent chooses two of those cards. Put the chosen cards \
                     into your graveyard and the rest into your hand. Then shuffle.";
const INTUITION: &str = "Search your library for three cards and reveal them. Target opponent \
                         chooses one. Put that card into your hand and the rest into your \
                         graveyard. Then shuffle.";

struct Asked {
    searches: Vec<Vec<mtg_core::ObjectId>>,
    opponent: Option<(u32, Vec<mtg_core::ObjectId>)>,
}

/// Cast `text` at P1, finding `want` in order (one per search prompt when asked one at a
/// time), with P1 choosing its first offered cards.
fn cast(text: &str, library: &[&str], want: &[usize]) -> (Game, Vec<mtg_core::ObjectId>, Asked) {
    let mut t = Table::default();
    let spell = t.card("{0}", "Instant", None, text);
    let ids: Vec<_> = library
        .iter()
        .map(|n| t.named_card(n, "{1}", "Sorcery", None, "Draw a card."))
        .collect();
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let found: Vec<_> = ids.iter().map(|c| g.put(*c, P0, Zone::Library)).collect();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    let asked = std::cell::RefCell::new(Asked {
        searches: Vec::new(),
        opponent: None,
    });
    let mut next = want.iter().map(|i| found[*i]);
    let multi = want.len();
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseTargets { .. } => {
                Some(Answer::Targets(vec![vec![Target::Player(P1)]]))
            }
            ChoiceKind::ChooseObjects { from, max, .. } if c.who == P0 => {
                asked.borrow_mut().searches.push(from.clone());
                let take = if *max as usize >= multi {
                    next.by_ref().collect()
                } else {
                    next.next().into_iter().collect()
                };
                Some(Answer::Objects(take))
            }
            ChoiceKind::ChooseObjects { from, min, .. } if c.who == P1 => {
                asked.borrow_mut().opponent = Some((*min, from.clone()));
                Some(Answer::Objects(
                    from.iter().take(*min as usize).copied().collect(),
                ))
            }
            _ => None,
        },
        |g| !asked.borrow().searches.is_empty() && g.stack().is_empty(),
    );
    (g, found, asked.into_inner())
}

fn names_in(g: &Game, z: ZoneRef) -> Vec<String> {
    let mut v: Vec<String> = g
        .engine
        .state
        .objects_in(z)
        .into_iter()
        .filter_map(|o| {
            mtg_ir::PrintedCards::face(&g.table, g.engine.state.objects[&o].card, 0)
                .map(|f| f.name.to_string())
        })
        .filter(|n| n.starts_with("Gift "))
        .collect();
    v.sort();
    v
}

#[test]
fn gifts_offers_only_new_names_and_the_opponent_bins_two() {
    let lib = ["Gift A", "Gift A", "Gift B", "Gift C", "Gift D"];
    let (g, found, asked) = cast(GIFTS, &lib, &[0, 2, 3, 4]);
    assert_eq!(asked.searches.len(), 4, "one card at a time");
    assert!(
        !asked.searches[1].contains(&found[1]),
        "a second Gift A is not offered once one was found"
    );
    let (min, offered) = asked.opponent.expect("the opponent chose");
    assert_eq!(min, 2);
    assert_eq!(offered.len(), 4);
    let yard = names_in(&g, ZoneRef::of(Zone::Graveyard, P0));
    let hand = names_in(&g, ZoneRef::of(Zone::Hand, P0));
    assert_eq!(yard.len(), 2, "{yard:?}");
    assert_eq!(hand.len(), 2, "{hand:?}");
    assert_eq!(names_in(&g, ZoneRef::of(Zone::Library, P0)), vec!["Gift A"]);
}

#[test]
fn gifts_finding_two_puts_both_in_the_graveyard() {
    let lib = ["Gift A", "Gift B", "Gift C"];
    let (g, _, asked) = cast(GIFTS, &lib, &[0, 1]);
    assert_eq!(asked.opponent.expect("chose").0, 2);
    assert_eq!(names_in(&g, ZoneRef::of(Zone::Graveyard, P0)).len(), 2);
    assert!(names_in(&g, ZoneRef::of(Zone::Hand, P0)).is_empty());
}

#[test]
fn intuition_gives_the_opponent_one_for_your_hand() {
    let lib = ["Gift A", "Gift A", "Gift A", "Gift B"];
    let (g, _, asked) = cast(INTUITION, &lib, &[0, 1, 2]);
    assert_eq!(asked.searches.len(), 1, "duplicates allowed, one search");
    assert_eq!(asked.opponent.expect("chose").0, 1);
    assert_eq!(names_in(&g, ZoneRef::of(Zone::Hand, P0)), vec!["Gift A"]);
    assert_eq!(names_in(&g, ZoneRef::of(Zone::Graveyard, P0)).len(), 2);
}
