//! "Exile target noncreature artifact or noncreature enchantment." (Haywire Mite), "Destroy
//! target artifact or tapped creature." (Radiant Strike): each alternative keeps its own
//! adjectives.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

const MITE: &str = "Exile target noncreature artifact or noncreature enchantment.";

/// Where a card is now (exile is shared, so `Game::find` doesn't look there).
fn zone_of(g: &Game, card: mtg_core::CardId) -> Zone {
    g.engine
        .state
        .objects
        .values()
        .find(|o| o.card == card)
        .map(|o| o.zone.zone)
        .unwrap()
}

#[test]
fn a_noncreature_enchantment_is_a_legal_target() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Instant", None, MITE);
    let aura = t.card("{1}", "Enchantment", None, "");
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    let target = g.put(aura, P1, Zone::Battlefield);
    g.main();
    g.cast(spell, &[Target::Object(target)]);
    assert_eq!(zone_of(&g, aura), Zone::Exile);
}

#[test]
fn an_artifact_creature_is_not() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Instant", None, MITE);
    let golem = t.card("{3}", "Artifact Creature — Golem", Some((3, 3)), "");
    let mut g = Game::new(t);
    let spell = g.put(spell, P0, Zone::Hand);
    g.put(golem, P1, Zone::Battlefield);
    // Its only possible target is illegal, so it can't be cast.
    assert!(!g.main().contains(&Action::Cast { object: spell }));
}

#[test]
fn only_a_tapped_creature_is_a_tapped_creature() {
    for tapped in [false, true] {
        let mut t = Table::default();
        let strike = t.card(
            "{0}",
            "Instant",
            None,
            "Destroy target artifact or tapped creature.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let spell = g.put(strike, P0, Zone::Hand);
        let target = g.put(bear, P1, Zone::Battlefield);
        g.engine.state.objects.get_mut(&target).unwrap().tapped = tapped;
        let castable = g.main().contains(&Action::Cast { object: spell });
        assert_eq!(castable, tapped);
        if tapped {
            g.cast(spell, &[Target::Object(target)]);
            assert_eq!(g.count(Zone::Graveyard, P1), 1);
        }
    }
}
