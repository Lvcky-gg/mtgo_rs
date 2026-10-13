use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::choice::Answer;

const DEREVI: &str = "Flying\nWhen ~ enters and whenever a creature you control deals combat damage to a player, you may tap or untap target permanent.\n{1}{G}{W}{U}: Put ~ onto the battlefield from the command zone.";

#[test]
fn command_zone_activation_ignores_casting_tax_and_triggers_on_entry() {
    let mut t = Table::default();
    let derevi = t.card(
        "{G}{W}{U}",
        "Legendary Creature — Wizard",
        Some((2, 3)),
        DEREVI,
    );
    let mut g = Game::new(t);
    g.lands(4);
    g.engine.state.commander.commanders.insert(P0, derevi);
    g.engine.state.commander.casts.insert(P0, 5);
    let card = g.put(derevi, P0, Zone::Command);
    let land = g.engine.state.battlefield()[0];
    assert!(g.main().contains(&activate(card, 2)));
    g.act(
        activate(card, 2),
        &[Target::Object(land)],
        &[Answer::Bool(true), Answer::Bool(false)],
    );
    assert!(g.find(derevi).is_some());
    assert!(!g.engine.state.objects[&land].tapped);
    assert_eq!(g.engine.state.commander.casts[&P0], 5);
    assert_eq!(
        g.engine
            .state
            .spells_by_player
            .get(&P0)
            .copied()
            .unwrap_or(0),
        0
    );
}

#[test]
fn command_zone_activation_is_not_available_from_other_zones() {
    for zone in [Zone::Hand, Zone::Graveyard, Zone::Battlefield] {
        let mut t = Table::default();
        let derevi = t.card(
            "{G}{W}{U}",
            "Legendary Creature — Wizard",
            Some((2, 3)),
            DEREVI,
        );
        let mut g = Game::new(t);
        g.lands(4);
        let card = g.put(derevi, P0, zone);
        assert!(!g.main().contains(&activate(card, 2)));
    }
}

#[test]
fn other_creatures_combat_damage_uses_the_same_triggered_ability() {
    let mut t = Table::default();
    let derevi = t.card(
        "{G}{W}{U}",
        "Legendary Creature — Wizard",
        Some((2, 3)),
        DEREVI,
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(derevi, P0, Zone::Battlefield);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let land = g.engine.state.battlefield()[0];
    g.main();
    g.engine.state.objects.get_mut(&land).unwrap().tapped = true;
    g.combat(
        &[attacker],
        &[],
        &[Target::Object(land)],
        &[Answer::Bool(true), Answer::Bool(false)],
    );
    assert_eq!(g.life(P1), 18);
    assert!(!g.engine.state.objects[&land].tapped);
}

#[test]
fn the_combined_combat_trigger_does_not_fire_for_noncombat_damage() {
    let mut t = Table::default();
    let derevi = t.card(
        "{G}{W}{U}",
        "Legendary Creature — Wizard",
        Some((2, 3)),
        DEREVI,
    );
    let bear = t.bear();
    let damage = t.card(
        "{0}",
        "Sorcery",
        None,
        "Target creature deals 1 damage to target player.",
    );
    let mut g = Game::new(t);
    g.put(derevi, P0, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Battlefield);
    let damage = g.put(damage, P0, Zone::Hand);
    g.main();
    g.cast(damage, &[Target::Object(bear), Target::Player(P1)]);
    assert_eq!(g.life(P1), 19);
}

#[test]
fn the_combined_second_draw_trigger_counts_each_event_in_a_batch() {
    let mut t = Table::default();
    let source = t.card(
        "{1}",
        "Creature",
        Some((2, 2)),
        "When ~ enters and whenever you draw your second card each turn, you gain 1 life.",
    );
    let draw = t.card("{0}", "Sorcery", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.put(source, P0, Zone::Battlefield);
    let draw = g.put(draw, P0, Zone::Hand);
    g.main();
    assert_eq!(g.life(P0), 20);
    g.cast(draw, &[]);
    assert_eq!(g.life(P0), 21);
}
