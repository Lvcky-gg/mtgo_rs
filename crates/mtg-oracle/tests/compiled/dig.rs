//! "Look at the top N cards of your library", take some, and put the rest away.
use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

fn stack_top(g: &mut Game, cards: &[mtg_core::CardId]) -> Vec<mtg_core::ObjectId> {
    let ids: Vec<_> = cards.iter().map(|c| g.put(*c, P0, Zone::Library)).collect();
    let lib = ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| !ids.contains(o));
    for (i, id) in ids.iter().enumerate() {
        order.insert(i, *id);
    }
    ids
}

#[test]
fn reveal_a_creature_and_bottom_the_rest() {
    let mut t = Table::default();
    let dig = t.card(
        "{G}",
        "Sorcery",
        None,
        "Look at the top three cards of your library. You may reveal a creature card from \
         among them and put it into your hand. Put the rest on the bottom of your library in \
         a random order.",
    );
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[rock, bear, rock]);
    g.act(
        Action::Cast { object: d },
        &[],
        &[Answer::Objects(vec![ids[1]])],
    );
    let hand: Vec<_> = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert!(hand.contains(&bear), "the creature card");
    let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    let bottom: Vec<_> = lib[lib.len() - 2..]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(bottom, vec![rock, rock], "the rest on the bottom");
}

#[test]
fn one_to_hand_and_the_rest_to_the_graveyard() {
    let mut t = Table::default();
    let dig = t.card(
        "{U}",
        "Sorcery",
        None,
        "Look at the top three cards of your library. Put one of them into your hand and the \
         rest into your graveyard.",
    );
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    stack_top(&mut g, &[rock, rock, rock]);
    g.cast(d, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        3,
        "two of them and the sorcery"
    );
}
