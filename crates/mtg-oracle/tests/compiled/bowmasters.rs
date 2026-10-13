use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

const BOWMASTERS: &str = "Flash\nWhen ~ enters and whenever an opponent draws a card except the first one they draw in each of their draw steps, ~ deals 1 damage to any target. Then amass Orcs 1.";

fn army_counters(g: &Game) -> i32 {
    g.engine
        .state
        .objects
        .values()
        .filter(|o| o.zone.zone == Zone::Battlefield && o.is_token)
        .map(|o| o.counters.values().sum::<i32>())
        .sum()
}

#[test]
fn entering_deals_damage_and_amasses() {
    let mut t = Table::default();
    let master = t.card("{0}", "Creature — Orc", Some((1, 1)), BOWMASTERS);
    let mut g = Game::new(t);
    let card = g.put(master, P0, Zone::Hand);
    g.main();
    g.cast(card, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(army_counters(&g), 1);
}

#[test]
fn first_draw_step_draw_is_excluded_but_both_additional_draws_trigger() {
    let mut t = Table::default();
    let master = t.card("{0}", "Creature — Orc", Some((1, 1)), BOWMASTERS);
    let draw = t.card("{0}", "Instant", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.put(master, P0, Zone::Battlefield);
    let draw = g.put(draw, P1, Zone::Hand);
    g.main();
    g.until(P1, Step::Draw);
    assert_eq!(g.life(P1), 20);
    assert_eq!(army_counters(&g), 0);
    g.act(
        Action::Cast { object: draw },
        &[Target::Player(P1), Target::Player(P1)],
        &[],
    );
    assert_eq!(g.life(P1), 18);
    assert_eq!(army_counters(&g), 2);
}

#[test]
fn drawing_in_upkeep_does_not_consume_the_draw_step_exclusion() {
    let mut t = Table::default();
    let master = t.card("{0}", "Creature — Orc", Some((1, 1)), BOWMASTERS);
    let draw = t.card("{0}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    g.put(master, P0, Zone::Battlefield);
    let draw = g.put(draw, P1, Zone::Hand);
    g.main();
    g.until(P1, Step::Upkeep);
    g.act(Action::Cast { object: draw }, &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(army_counters(&g), 1);
    g.until(P1, Step::Draw);
    assert_eq!(g.life(P1), 19);
    assert_eq!(army_counters(&g), 1);
}

#[test]
fn amassing_orcs_adds_orc_to_an_existing_zombie_army() {
    let mut t = Table::default();
    let master = t.card("{0}", "Creature — Orc", Some((1, 1)), BOWMASTERS);
    let zombies = t.card("{0}", "Sorcery", None, "Amass Zombies 2.");
    let mut g = Game::new(t);
    let master = g.put(master, P0, Zone::Hand);
    let zombies = g.put(zombies, P0, Zone::Hand);
    g.main();
    g.cast(zombies, &[]);
    g.cast(master, &[Target::Player(P1)]);
    assert_eq!(army_counters(&g), 3);
    let armies: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.zone.zone == Zone::Battlefield && o.is_token)
        .map(|o| o.id)
        .collect();
    assert_eq!(armies.len(), 1);
    let chars = g.engine.characteristics(&g.table, armies[0]).unwrap();
    for name in ["Orc", "Zombie", "Army"] {
        assert!(
            chars
                .subtypes
                .contains(&g.table.subtype_named(name).unwrap())
        );
    }
}
