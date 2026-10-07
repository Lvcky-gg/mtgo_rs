use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn keen_senses_draws_on_entry() {
    let mut t = Table::default();
    let owlbear = t.card(
        "{3}{G}{G}",
        "Creature — Bear",
        Some((4, 4)),
        "Trample\nKeen Senses — When this creature enters, draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(5);
    let spell = g.put(owlbear, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn martyrdom_draws_and_gains_life_after_source_dies() {
    let mut t = Table::default();
    let sister = t.card(
        "{3}{W}{B}",
        "Creature — Soldier",
        Some((5, 1)),
        "Martyrdom — When this creature dies, you gain 2 life and draw two cards.",
    );
    let destroy = t.card("{B}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(1);
    let source = g.put(sister, P0, Zone::Battlefield);
    let spell = g.put(destroy, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[Target::Object(source)]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert_eq!(g.engine.state.players[&P0].life, 22);
}

#[test]
fn rapacious_hunger_triggers_for_another_creature_dying() {
    let mut t = Table::default();
    let haruspex = t.card(
        "{3}{G}",
        "Creature — Bear",
        Some((3, 3)),
        "Rapacious Hunger — Whenever another creature dies, put a +1/+1 counter on this creature.",
    );
    let bear = t.bear();
    let destroy = t.card("{B}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(1);
    let source = g.put(haruspex, P0, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(destroy, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(other)]);
    assert_eq!(g.pt(source), (4, 4));
}

#[test]
fn medicus_ministorum_returns_target_and_gains_its_mana_value() {
    let mut t = Table::default();
    let sister = t.card("{4}{W}{B}", "Creature — Soldier", Some((3, 2)),
        "Medicus Ministorum — When this creature enters, return target creature card from your graveyard to the battlefield. You gain life equal to its mana value.");
    let expensive = t.card("{3}{G}{G}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(6);
    let target = g.put(expensive, P0, Zone::Graveyard);
    let spell = g.put(sister, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(target)]);
    assert_eq!(g.engine.state.players[&P0].life, 25);
    assert_eq!(g.count(Zone::Graveyard, P0), 0);
    assert_eq!(
        g.engine.state.objects[&g.find(expensive).unwrap()]
            .zone
            .zone,
        Zone::Battlefield
    );
}
