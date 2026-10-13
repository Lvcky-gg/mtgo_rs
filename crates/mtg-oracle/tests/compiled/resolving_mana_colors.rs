use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::choice::Answer;

const DEATHRITE: &str = "{T}: Exile target land card from a graveyard. Add one mana of any color.\n{B}, {T}: Exile target instant or sorcery card from a graveyard. Each opponent loses 2 life.\n{G}, {T}: Exile target creature card from a graveyard. You gain 2 life.";
const RITUAL: &str = "Destroy each nonland permanent with mana value 2 or less. Add {B} or {G} for each permanent destroyed this way.";

#[test]
fn deathrite_exiles_a_land_then_chooses_mana_on_resolution() {
    let mut t = Table::default();
    let shaman = t.card("{B/G}", "Creature — Elf", Some((1, 2)), DEATHRITE);
    let land = t.mountain();
    let mut g = Game::new(t);
    let source = g.put(shaman, P0, Zone::Battlefield);
    let target = g.put(land, P1, Zone::Graveyard);
    g.main();
    g.act(
        activate(source, 0),
        &[Target::Object(target)],
        &[Answer::Modes(vec![4])],
    );
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == land && o.owner == P1 && o.zone.zone == Zone::Exile)
    );
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Green as usize],
        1
    );
    assert!(g.engine.state.objects[&source].tapped);
}

#[test]
fn culling_ritual_counts_destroyed_objects_and_chooses_each_mana_separately() {
    let mut t = Table::default();
    let ritual = t.card("{0}", "Sorcery", None, RITUAL);
    let cheap = t.bear();
    let expensive = t.card("{3}", "Artifact", None, "");
    let indestructible = t.card("{1}", "Creature", Some((2, 2)), "Indestructible");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(cheap, P0, Zone::Battlefield);
    g.put(cheap, P1, Zone::Battlefield);
    let expensive = g.put(expensive, P1, Zone::Battlefield);
    let survivor = g.put(indestructible, P1, Zone::Battlefield);
    let spell = g.put(ritual, P0, Zone::Hand);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: spell },
        &[],
        &[Answer::Modes(vec![0]), Answer::Modes(vec![1])],
    );
    assert!(g.engine.state.objects.contains_key(&expensive));
    assert!(g.engine.state.objects.contains_key(&survivor));
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Black as usize],
        1
    );
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Green as usize],
        1
    );
}

#[test]
fn no_destroyed_permanents_means_no_mana_or_color_prompt() {
    let mut t = Table::default();
    let ritual = t.card("{0}", "Sorcery", None, RITUAL);
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(ritual, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert_eq!(g.engine.state.player(P0).mana.amounts, [0; 6]);
}

#[test]
fn loyalty_mana_chooses_a_color_when_the_ability_resolves() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "{3}",
        "Planeswalker",
        3,
        "+1: Add two mana of any one color.",
    );
    let mut g = Game::new(t);
    let source = g.put(walker, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&source)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, 3);
    g.main();
    g.act(activate(source, 0), &[], &[Answer::Modes(vec![3])]);
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Red as usize],
        2
    );
    assert_eq!(g.engine.state.player(P0).mana.total(), 2);
}

#[test]
fn illegal_deathrite_target_prevents_exile_and_mana() {
    let mut t = Table::default();
    let shaman = t.card("{B/G}", "Creature — Elf", Some((1, 2)), DEATHRITE);
    let land = t.mountain();
    let exile = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target card from a graveyard.",
    );
    let mut g = Game::new(t);
    let source = g.put(shaman, P0, Zone::Battlefield);
    let target = g.put(land, P1, Zone::Graveyard);
    let response = g.put(exile, P0, Zone::Hand);
    g.main();
    g.act_holding(activate(source, 0), &[Target::Object(target)]);
    g.cast(response, &[Target::Object(target)]);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
}

#[test]
fn scaled_mana_ability_remains_visible_to_the_payment_planner() {
    let mut t = Table::default();
    let rock = t.card("{0}", "Artifact", None, "This artifact enters with three charge counters on it.\n{T}: Add one mana of any color for each charge counter on this artifact.");
    let spell = t.card("{R}{R}{R}", "Sorcery", None, "You gain 3 life.");
    let mut g = Game::new(t);
    let source = g.put(rock, P0, Zone::Hand);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(source, &[]);
    g.cast(spell, &[]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
    assert!(g.engine.state.objects[&g.find(rock).unwrap()].tapped);
}
