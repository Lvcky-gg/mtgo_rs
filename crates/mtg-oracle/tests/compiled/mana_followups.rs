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

#[test]
fn removing_a_counter_pays_for_mana_until_none_are_left() {
    let mut table = Table::default();
    let land = table.card(
        "",
        "Land",
        None,
        "This land enters tapped with two charge counters on it.\nRemove a charge counter from \
         this land: Add one mana of any color.",
    );
    let mut game = Game::new(table);
    let id = game.put(land, P0, Zone::Hand);
    game.main();
    game.act(Action::PlayLand { object: id }, &[], &[]);
    let land = *game.engine.state.battlefield().last().unwrap();
    assert!(game.engine.state.objects[&land].tapped, "enters tapped");
    let counters = |g: &Game| g.engine.state.objects[&land].counters.values().sum::<i32>();
    let ability = mtg_engine::abilities::current(&game.engine.state, &game.table, land)
        .iter()
        .find(|a| {
            matches!(
                a.kind,
                mtg_ir::AbilityKind::Activated {
                    is_mana_ability: true,
                    ..
                }
            )
        })
        .map(|a| a.id)
        .unwrap();
    assert_eq!(counters(&game), 2);
    for left in [1, 0] {
        assert!(
            mtg_engine::mana::manual_source(&game.engine.state, &game.table, P0, land, ability)
                .is_some()
        );
        game.act(
            Action::ActivateManaAbility {
                source: land,
                ability,
                color: Some(Color::Red),
            },
            &[],
            &[],
        );
        assert_eq!(counters(&game), left);
    }
    assert_eq!(
        game.engine.state.players[&P0].mana.amounts[Color::Red as usize],
        2
    );
    assert!(
        mtg_engine::mana::manual_source(&game.engine.state, &game.table, P0, land, ability)
            .is_none(),
        "no counters left to remove"
    );
}

fn mana_ability_of(game: &Game, object: mtg_core::ObjectId) -> AbilityId {
    mtg_engine::abilities::current(&game.engine.state, &game.table, object)
        .iter()
        .find(|a| {
            matches!(
                a.kind,
                mtg_ir::AbilityKind::Activated {
                    is_mana_ability: true,
                    ..
                }
            )
        })
        .map(|a| a.id)
        .unwrap()
}

#[test]
fn depletion_land_is_sacrificed_when_its_last_counter_is_removed() {
    let mut table = Table::default();
    let land = table.card(
        "",
        "Land",
        None,
        "This land enters tapped with two depletion counters on it.\n{T}, Remove a depletion \
         counter from this land: Add {U}{U}. If there are no depletion counters on this land, \
         sacrifice it.",
    );
    let mut game = Game::new(table);
    let id = game.put(land, P0, Zone::Hand);
    game.main();
    game.act(Action::PlayLand { object: id }, &[], &[]);
    let land = *game.engine.state.battlefield().last().unwrap();
    let ability = mana_ability_of(&game, land);
    let blue = |g: &Game| g.engine.state.players[&P0].mana.amounts[Color::Blue as usize];
    for left in [1, 0] {
        // On to P0's next turn, which untaps it.
        let c = game.pending.take().unwrap();
        game.engine
            .answer(&game.table, c.id, mtg_engine::choice::Answer::Pass)
            .unwrap();
        game.main();
        game.act(
            Action::ActivateManaAbility {
                source: land,
                ability,
                color: None,
            },
            &[],
            &[],
        );
        assert_eq!(blue(&game), 2);
        if left > 0 {
            let counters = game.engine.state.objects[&land]
                .counters
                .values()
                .sum::<i32>();
            assert_eq!(counters, left, "one counter removed, the land stays");
        }
    }
    assert!(!game.engine.state.objects.contains_key(&land));
    assert_eq!(
        game.count(Zone::Graveyard, P0),
        1,
        "sacrificed with its last counter"
    );
}

#[test]
fn land_taps_only_the_turn_it_entered_or_alongside_a_basic() {
    let mut table = Table::default();
    let dual = table.card(
        "",
        "Land",
        None,
        "{T}: Add {R} or {G}. Activate only if this land entered this turn or if you control a \
         basic land.",
    );
    let mountain = table.mountain();
    let mut game = Game::new(table);
    let id = game.put(dual, P0, Zone::Hand);
    game.main();
    game.act(Action::PlayLand { object: id }, &[], &[]);
    let land = *game.engine.state.battlefield().last().unwrap();
    let ability = mana_ability_of(&game, land);
    let usable = |g: &Game| {
        mtg_engine::mana::manual_source(&g.engine.state, &g.table, P0, land, ability).is_some()
    };
    assert!(usable(&game), "entered this turn");
    // The next turn — the opponent's, so the land is still summoning sick — it has not
    // entered this turn.
    game.engine.state.turn += 1;
    assert!(
        !usable(&game),
        "neither entered this turn nor with a basic land"
    );
    game.put(mountain, P0, Zone::Battlefield);
    assert!(usable(&game), "a basic land is enough");
}
