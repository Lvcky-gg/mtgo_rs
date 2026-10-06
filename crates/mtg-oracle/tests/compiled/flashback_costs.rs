use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn flashback_life_is_paid_only_when_cast_from_the_graveyard() {
    for zone in [Zone::Hand, Zone::Graveyard] {
        let mut table = Table::default();
        let spell = table.card(
            "{U}",
            "Sorcery",
            None,
            "Target player draws two cards.\nFlashback—{1}{U}, Pay 3 life.",
        );
        let mut game = Game::new(table);
        game.lands(2);
        let spell = game.put(spell, P0, zone);
        game.engine.state.step = Step::PrecombatMain;
        assert!(game.main().contains(&Action::Cast { object: spell }));
        game.cast(spell, &[Target::Player(P0)]);
        assert_eq!(game.life(P0), if zone == Zone::Hand { 20 } else { 17 });
        assert_eq!(game.count(Zone::Hand, P0), 2);
        assert_eq!(
            game.count(Zone::Graveyard, P0),
            usize::from(zone == Zone::Hand)
        );
        assert_eq!(
            game.engine
                .state
                .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
                .len(),
            usize::from(zone == Zone::Graveyard)
        );
    }
}

#[test]
fn flashback_is_not_offered_when_its_life_cost_cannot_be_paid() {
    let mut table = Table::default();
    let spell = table.card(
        "{U}",
        "Sorcery",
        None,
        "Draw two cards.\nFlashback—{1}{U}, Pay 3 life.",
    );
    let mut game = Game::new(table);
    game.lands(2);
    let spell = game.put(spell, P0, Zone::Graveyard);
    game.engine.state.players.get_mut(&P0).unwrap().life = 2;
    game.engine.state.step = Step::PrecombatMain;
    assert!(!game.main().contains(&Action::Cast { object: spell }));
}

#[test]
fn flashback_sacrifices_three_creatures_and_exiles_the_spell() {
    let mut table = Table::default();
    let spell = table.card("{2}{B}{B}", "Sorcery", None,
        "Return target creature card from your graveyard to the battlefield.\nFlashback—Sacrifice three creatures.");
    let creature = table.bear();
    let mut game = Game::new(table);
    let spell = game.put(spell, P0, Zone::Graveyard);
    let target = game.put(creature, P0, Zone::Graveyard);
    let sacrifices: Vec<_> = (0..3)
        .map(|_| game.put(creature, P0, Zone::Battlefield))
        .collect();
    game.engine.state.step = Step::PrecombatMain;
    assert!(game.main().contains(&Action::Cast { object: spell }));
    game.act(
        Action::Cast { object: spell },
        &[Target::Object(target)],
        &[Answer::Objects(sacrifices.clone())],
    );
    assert_eq!(game.count(Zone::Graveyard, P0), 3);
    assert_eq!(game.engine.state.battlefield().len(), 1);
    assert_eq!(
        game.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
    assert!(
        sacrifices
            .iter()
            .all(|id| !game.engine.state.objects.contains_key(id))
    );
}

#[test]
fn flashback_life_and_phyrexian_mana_cannot_spend_the_same_life() {
    for life in [5, 8] {
        let mut table = Table::default();
        let spell = table.card(
            "{B}",
            "Sorcery",
            None,
            "Draw a card.\nFlashback—{B/P}{B/P}, Pay 3 life.",
        );
        let mut game = Game::new(table);
        let spell = game.put(spell, P0, Zone::Graveyard);
        game.engine.state.players.get_mut(&P0).unwrap().life = life;
        game.engine.state.step = Step::PrecombatMain;
        game.main();
        game.cast(spell, &[]);
        if life == 5 {
            assert_eq!(game.life(P0), 5);
            assert_eq!(game.count(Zone::Hand, P0), 0);
            assert!(game.engine.state.objects.contains_key(&spell));
        } else {
            assert_eq!(game.life(P0), 1);
            assert_eq!(game.count(Zone::Hand, P0), 1);
            assert!(!game.engine.state.objects.contains_key(&spell));
        }
    }
}

#[test]
fn a_sacrificed_mana_source_cannot_pay_for_two_costs() {
    let mut table = Table::default();
    let spell = table.card(
        "{1}",
        "Sorcery",
        None,
        "As an additional cost to cast this spell, sacrifice an artifact.\nDraw a card.",
    );
    let source = table.card("{1}", "Artifact", None, "Sacrifice this artifact: Add {C}.");
    let mut game = Game::new(table);
    let spell = game.put(spell, P0, Zone::Hand);
    let source = game.put(source, P0, Zone::Battlefield);
    game.engine.state.step = Step::PrecombatMain;
    game.main();
    game.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![source])],
    );
    assert!(game.engine.state.objects.contains_key(&spell));
    assert!(game.engine.state.objects.contains_key(&source));
    assert_eq!(game.count(Zone::Hand, P0), 1);
    assert_eq!(game.count(Zone::Graveyard, P0), 0);
}
