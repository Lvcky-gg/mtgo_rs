//! "Whenever ~ attacks while …" (Ruby), "return a <card> from your graveyard to the
//! battlefield" (Tyvar).

use super::harness::*;
use mtg_core::Zone;

const RUBY: &str = "Whenever ~ attacks while you control a creature with power 4 or greater, ~ gets +2/+2 until end of turn.";

fn ruby_after_attack(big: bool) -> (i32, i32) {
    let mut t = Table::default();
    let ruby = t.card("{0}", "Creature — Human", Some((1, 1)), RUBY);
    let ogre = t.card("{0}", "Creature — Ogre", Some((4, 4)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    let r = g.put(ruby, P0, Zone::Battlefield);
    g.put(if big { ogre } else { bear }, P0, Zone::Battlefield);
    g.main();
    g.combat(&[r], &[], &[], &[]);
    g.pt(r)
}

#[test]
fn attacks_while_checks_as_it_triggers() {
    assert_eq!(ruby_after_attack(true), (3, 3));
    assert_eq!(ruby_after_attack(false), (1, 1));
}

#[test]
fn return_a_creature_card_from_your_graveyard_to_the_battlefield() {
    let mut t = Table::default();
    let small = t.bear();
    let big = t.card("{5}", "Creature — Ogre", Some((4, 4)), "");
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return a creature card with mana value 2 or less from your graveyard to the battlefield.",
    );
    let mut g = Game::new(t);
    let s = g.put(small, P0, Zone::Graveyard);
    let b = g.put(big, P0, Zone::Graveyard);
    let c = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    let on_bf = |card| {
        g.engine
            .state
            .battlefield()
            .iter()
            .any(|id| g.engine.state.objects[id].card == card)
    };
    assert!(on_bf(small), "the bear returned");
    assert!(!on_bf(big), "too expensive");
    let _ = (s, b);
}
