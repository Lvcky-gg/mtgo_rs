//! Dauthi Voidwalker: "If a card would be put into an opponent's graveyard from anywhere,
//! instead exile it with a void counter on it. {T}, Sacrifice this creature: Choose an
//! exiled card an opponent owns with a void counter on it. You may play it this turn
//! without paying its mana cost."

use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

const DAUTHI: &str = "Shadow\nIf a card would be put into an opponent's graveyard from \
                      anywhere, instead exile it with a void counter on it.\n{T}, Sacrifice \
                      this creature: Choose an exiled card an opponent owns with a void counter \
                      on it. You may play it this turn without paying its mana cost.";

fn exiled(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine.state.objects_in(ZoneRef::shared(Zone::Exile))
}

#[test]
fn opponents_cards_are_exiled_with_void_counters_and_yours_are_not() {
    let mut t = Table::default();
    let dauthi = t.card("{B}{B}", "Creature — Dauthi Rogue", Some((3, 2)), DAUTHI);
    let mill = t.card("{0}", "Sorcery", None, "Each player mills two cards.");
    let mut g = Game::new(t);
    g.put(dauthi, P0, Zone::Battlefield);
    let mill = g.put(mill, P0, Zone::Hand);
    g.main();
    g.cast(mill, &[]);
    let exiled = exiled(&g);
    assert_eq!(exiled.len(), 2, "the opponent's two");
    assert!(exiled.iter().all(|o| {
        let o = &g.engine.state.objects[o];
        o.owner == P1 && o.counters.values().sum::<i32>() == 1
    }));
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(g.count(Zone::Graveyard, P0), 3, "your two and the sorcery");
}

#[test]
fn sacrificed_it_plays_a_void_card_for_free_this_turn() {
    let mut t = Table::default();
    let dauthi = t.card("{B}{B}", "Creature — Dauthi Rogue", Some((3, 2)), DAUTHI);
    let big = t.card("{7}", "Sorcery", None, "You gain 5 life.");
    let discard = t.card("{0}", "Sorcery", None, "Each opponent discards a card.");
    let mut g = Game::new(t);
    let d = g.put(dauthi, P0, Zone::Battlefield);
    g.put(big, P1, Zone::Hand);
    let discard = g.put(discard, P0, Zone::Hand);
    g.main();
    g.cast(discard, &[]);
    let voided = exiled(&g);
    assert_eq!(voided.len(), 1);
    let actions = g.main();
    let sac = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == d))
        .cloned()
        .expect("offered");
    g.act(sac, &[], &[]);
    assert!(g.find(dauthi).is_none(), "sacrificed");
    let card = exiled(&g)
        .into_iter()
        .find(|o| g.engine.state.objects[o].owner == P1)
        .expect("still exiled");
    let actions = g.main();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == card)),
        "castable with no lands"
    );
    g.cast(card, &[]);
    assert_eq!(g.life(P0), 25);
    // The opponent's sorcery would go to its owner's graveyard: Dauthi is gone now.
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}
