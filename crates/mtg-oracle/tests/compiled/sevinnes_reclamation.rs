use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

const RECLAMATION: &str = "Return target permanent card with mana value 3 or less from your graveyard to the battlefield. If this spell was cast from a graveyard, you may copy this spell and may choose a new target for the copy.\nFlashback {4}{W}";

#[test]
fn flashback_returns_two_permanents_without_copying_the_copy() {
    let mut t = Table::default();
    let spell = t.card("{2}{W}", "Sorcery", None, RECLAMATION);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let a = g.put(bear, P0, Zone::Graveyard);
    let b = g.put(bear, P0, Zone::Graveyard);
    let spell = g.put(spell, P0, Zone::Graveyard);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(a), Target::Object(b)],
        &[Answer::Bool(true)],
    );
    assert_eq!(g.engine.state.battlefield().len(), 7);
    assert_eq!(g.count(Zone::Graveyard, P0), 0);
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.zone.zone == Zone::Exile)
            .count(),
        1
    );
    assert!(g.stack().is_empty());
}

#[test]
fn casting_from_hand_does_not_offer_a_copy() {
    let mut t = Table::default();
    let spell = t.card("{2}{W}", "Sorcery", None, RECLAMATION);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let a = g.put(bear, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Graveyard);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(a)]);
    assert_eq!(g.engine.state.battlefield().len(), 4);
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
}
