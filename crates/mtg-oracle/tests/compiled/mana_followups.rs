use super::harness::*;
use mtg_core::{AbilityId, Color, Step, Zone};
use mtg_engine::actions::Action;

#[test]
fn paid_sacrificing_mana_ability_adds_chosen_color_and_draws_immediately() {
    let mut table = Table::default();
    let sphere = table.card(
        "{1}",
        "Artifact",
        None,
        "{1}, {T}, Sacrifice this artifact: Add one mana of any color. Draw a card.",
    );
    let mut game = Game::new(table);
    game.lands(1);
    let sphere = game.put(sphere, P0, Zone::Battlefield);
    game.engine.state.step = Step::PrecombatMain;
    let action = Action::ActivateManaAbility {
        source: sphere,
        ability: AbilityId(0),
        color: Some(Color::Blue),
    };
    game.main();
    assert!(
        mtg_engine::mana::manual_source(&game.engine.state, &game.table, P0, sphere, AbilityId(0))
            .is_some()
    );
    game.act(action, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), 1);
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
    assert_eq!(
        game.engine.state.players[&P0].mana.amounts[Color::Blue as usize],
        1
    );
    assert!(game.stack().is_empty());
}

#[test]
fn automatic_mana_payment_keeps_the_sources_draw_effect() {
    let mut table = Table::default();
    let source = table.card(
        "{2}",
        "Artifact",
        None,
        "{T}, Sacrifice this artifact: Add {G}{G}. Draw a card.",
    );
    let spell = table.card("{G}{G}", "Sorcery", None, "You gain 3 life.");
    let mut game = Game::new(table);
    let source = game.put(source, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    game.engine.state.step = Step::PrecombatMain;
    assert!(game.main().contains(&Action::Cast { object: spell }));
    game.cast(spell, &[]);
    assert_eq!(game.life(P0), 23);
    assert_eq!(game.count(Zone::Hand, P0), 1);
    assert_eq!(game.count(Zone::Graveyard, P0), 2);
    assert!(!game.engine.state.objects.contains_key(&source));
}
