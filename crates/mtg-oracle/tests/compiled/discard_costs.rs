use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn typed_discard_cost_uses_only_matching_cards_in_hand() {
    let mut table = Table::default();
    let source = table.card(
        "{1}",
        "Artifact",
        None,
        "Discard a creature card: You gain 3 life.",
    );
    let creature = table.bear();
    let land = table.mountain();
    let mut game = Game::new(table);
    let source = game.put(source, P0, Zone::Battlefield);
    let land = game.put(land, P0, Zone::Hand);
    let opponent_creature = game.put(creature, P1, Zone::Hand);
    let battlefield_creature = game.put(creature, P0, Zone::Battlefield);
    game.engine.state.step = Step::PrecombatMain;
    assert!(!mtg_engine::cost::additional_payable(
        &game.engine.state,
        &game.table,
        source,
        P0,
        &mtg_engine::cost::ability_cost(
            &game.engine.state,
            &game.table,
            source,
            mtg_core::AbilityId(0)
        )
        .unwrap(),
    ));
    let creature = game.put(creature, P0, Zone::Hand);
    let actions = game.main();
    assert!(actions.contains(&activate(source, 0)));
    game.act(activate(source, 0), &[], &[Answer::Objects(vec![creature])]);
    assert_eq!(game.life(P0), 23);
    assert_eq!(game.count(Zone::Graveyard, P0), 1);
    for id in [land, opponent_creature, battlefield_creature] {
        assert!(game.engine.state.objects.contains_key(&id));
    }
}

#[test]
fn spell_additional_cost_discards_two_matching_cards_before_resolution() {
    let mut table = Table::default();
    let spell = table.card(
        "{R}",
        "Sorcery",
        None,
        "As an additional cost to cast this spell, discard two land cards.\nYou gain 4 life.",
    );
    let land = table.mountain();
    let creature = table.bear();
    let mut game = Game::new(table);
    game.lands(1);
    let spell = game.put(spell, P0, Zone::Hand);
    let first = game.put(land, P0, Zone::Hand);
    game.put(creature, P0, Zone::Hand);
    game.engine.state.step = Step::PrecombatMain;
    let face =
        mtg_ir::PrintedCards::face(&game.table, game.engine.state.objects[&spell].card, 0).unwrap();
    let cost = mtg_engine::cost::additional_cast_cost(face).unwrap();
    assert!(!mtg_engine::cost::additional_payable(
        &game.engine.state,
        &game.table,
        spell,
        P0,
        &cost
    ));
    let second = game.put(land, P0, Zone::Hand);
    assert!(game.main().contains(&Action::Cast { object: spell }));
    game.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![first, second])],
    );
    assert_eq!(game.life(P0), 24);
    assert_eq!(game.count(Zone::Graveyard, P0), 3);
    assert_eq!(game.count(Zone::Hand, P0), 1);
}
