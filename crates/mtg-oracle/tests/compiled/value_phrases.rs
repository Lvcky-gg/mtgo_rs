//! Values read as an effect resolves: devotion, counters on the source, the mana value of
//! the spell that triggered, the number of opponents.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn devotion_counts_colored_and_hybrid_symbols_including_its_own() {
    let mut table = Table::default();
    let disciple = table.card(
        "{2}{G}{G}",
        "Creature — Bear",
        Some((2, 2)),
        "When this creature enters, you gain life equal to your devotion to green.",
    );
    let hybrid = table.card("{G/W}", "Enchantment", None, "");
    let blue = table.card("{U}{U}", "Enchantment", None, "");
    let mut game = Game::new(table);
    game.lands(4);
    game.put(hybrid, P0, Zone::Battlefield);
    game.put(blue, P0, Zone::Battlefield);
    let disciple = game.put(disciple, P0, Zone::Hand);
    game.main();
    game.act(Action::Cast { object: disciple }, &[], &[]);
    assert_eq!(game.life(P0), 23, "GG of its own and the hybrid's G/W");
}

#[test]
fn counters_on_a_source_sacrificed_to_pay_are_read_as_it_last_existed() {
    let mut table = Table::default();
    let urn = table.card(
        "{1}",
        "Artifact",
        None,
        "{T}, Sacrifice this artifact: You gain life equal to the number of charge counters on \
         this artifact.",
    );
    let mut game = Game::new(table);
    let urn = game.put(urn, P0, Zone::Battlefield);
    let kind = mtg_engine::abilities::current(&game.engine.state, &game.table, urn)
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::Activated {
                effect:
                    mtg_ir::Effect::GainLife {
                        amount: mtg_ir::Value::Counters(_, kind),
                        ..
                    },
                ..
            } => Some(*kind),
            _ => None,
        })
        .expect("gains life equal to its charge counters");
    game.engine
        .state
        .objects
        .get_mut(&urn)
        .unwrap()
        .counters
        .insert(kind, 3);
    game.main();
    game.act(activate(urn, 0), &[], &[]);
    assert_eq!(game.life(P0), 23);
}

#[test]
fn the_triggering_spells_mana_value() {
    let mut table = Table::default();
    let ooze = table.card(
        "{2}{G}{G}",
        "Creature — Bear",
        Some((1, 1)),
        "Whenever you cast a spell, this creature gets +X/+X until end of turn, where X is that \
         spell's mana value.",
    );
    let spell = table.card("{2}{G}", "Sorcery", None, "You gain 1 life.");
    let mut game = Game::new(table);
    game.lands(3);
    let ooze = game.put(ooze, P0, Zone::Battlefield);
    let spell = game.put(spell, P0, Zone::Hand);
    game.main();
    game.act(Action::Cast { object: spell }, &[], &[]);
    assert_eq!(game.pt(ooze), (4, 4));
}

#[test]
fn the_number_of_opponents() {
    let mut table = Table::default();
    let sphinx = table.card(
        "{5}{U}",
        "Creature — Bear",
        Some((5, 5)),
        "When this creature enters, draw cards equal to the number of opponents you have.",
    );
    let mut game = Game::new(table);
    game.lands(6);
    let sphinx = game.put(sphinx, P0, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(Action::Cast { object: sphinx }, &[], &[]);
    assert_eq!(
        game.count(Zone::Hand, P0),
        hand,
        "cast one, drew one for one opponent"
    );
}

#[test]
fn life_gained_this_turn_counts_every_gain_so_far() {
    let mut table = Table::default();
    let heal = table.card("{W}", "Instant", None, "You gain 3 life.");
    let draught = table.card(
        "{W}",
        "Instant",
        None,
        "You gain 2 life. Target creature gets +X/+X until end of turn, where X is the amount \
         of life you gained this turn.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(2);
    let heal = game.put(heal, P0, Zone::Hand);
    let draught = game.put(draught, P0, Zone::Hand);
    let bear = game.put(bear, P0, Zone::Battlefield);
    game.main();
    game.act(Action::Cast { object: heal }, &[], &[]);
    game.act(
        Action::Cast { object: draught },
        &[mtg_core::Target::Object(bear)],
        &[],
    );
    assert_eq!(game.pt(bear), (7, 7), "3 + 2 gained this turn");
}

#[test]
fn creature_cards_in_all_graveyards() {
    let mut table = Table::default();
    let goyf = table.card(
        "{2}{B}{B}",
        "Creature — Bear",
        None,
        "~'s power and toughness are each equal to the number of creature cards in all \
         graveyards.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    let goyf = game.put(goyf, P0, Zone::Battlefield);
    game.put(bear, P0, Zone::Graveyard);
    game.put(bear, P1, Zone::Graveyard);
    game.put(bear, P1, Zone::Graveyard);
    assert_eq!(game.pt(goyf), (3, 3));
}

#[test]
fn twice_its_power_and_half_the_starting_life() {
    let mut table = Table::default();
    let swing = table.card(
        "{1}{G}",
        "Instant",
        None,
        "Target creature you control deals damage equal to twice its power to target creature \
         you don't control.",
    );
    let half = table.card(
        "{W}",
        "Sorcery",
        None,
        "You gain life equal to half your starting life total, rounded up.",
    );
    let bear = table.bear();
    let wall = table.card("{1}{W}", "Creature — Wall", Some((0, 5)), "");
    let mut game = Game::new(table);
    game.lands(3);
    let swing = game.put(swing, P0, Zone::Hand);
    let half = game.put(half, P0, Zone::Hand);
    let bear = game.put(bear, P0, Zone::Battlefield);
    let wall = game.put(wall, P1, Zone::Battlefield);
    game.main();
    game.act(
        Action::Cast { object: swing },
        &[
            mtg_core::Target::Object(bear),
            mtg_core::Target::Object(wall),
        ],
        &[],
    );
    assert_eq!(game.engine.state.objects[&wall].damage, 4);
    game.act(Action::Cast { object: half }, &[], &[]);
    assert_eq!(game.life(P0), 30, "half of 20");
}

#[test]
fn a_negative_difference_deals_no_damage() {
    let mut table = Table::default();
    let maiden = table.card(
        "{3}",
        "Artifact",
        None,
        "At the beginning of each opponent's upkeep, this artifact deals X damage to that \
         player, where X is the number of cards in their hand minus 4.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.put(maiden, P0, Zone::Battlefield);
    for _ in 0..6 {
        game.put(bear, P1, Zone::Hand);
    }
    game.main();
    // P1's upkeep: six cards in hand, minus four.
    game.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(game.life(P1), 18);
    // Down to two cards: 2 - 4 is negative, which deals nothing (and heals nothing).
    while game.count(Zone::Hand, P1) > 2 {
        let card = game
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P1))[0];
        game.engine.state.objects.remove(&card);
    }
    game.until(P0, mtg_core::Step::PrecombatMain);
    let before = game.life(P1);
    game.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(game.life(P1), before);
}

#[test]
fn spell_mastery_and_life_gained_conditions() {
    let mut table = Table::default();
    let runner = table.card(
        "{R}",
        "Creature — Bear",
        Some((1, 2)),
        "As long as there are two or more instant and/or sorcery cards in your graveyard, this \
         creature gets +1/+0.",
    );
    let aerie = table.card(
        "{1}{W}",
        "Enchantment",
        None,
        "At the beginning of your end step, if you gained 3 or more life this turn, you gain \
         10 life.",
    );
    let bolt = table.card("{R}", "Instant", None, "You gain 1 life.");
    let heal = table.card("{W}", "Instant", None, "You gain 3 life.");
    let mut game = Game::new(table);
    game.lands(1);
    let runner = game.put(runner, P0, Zone::Battlefield);
    game.put(aerie, P0, Zone::Battlefield);
    game.put(bolt, P0, Zone::Graveyard);
    assert_eq!(game.pt(runner), (1, 2), "one instant: no");
    game.put(bolt, P0, Zone::Graveyard);
    assert_eq!(game.pt(runner), (2, 2), "two: yes");
    let heal = game.put(heal, P0, Zone::Hand);
    game.main();
    game.act(Action::Cast { object: heal }, &[], &[]);
    game.until(P1, mtg_core::Step::Upkeep);
    assert_eq!(game.life(P0), 33, "gained 3, then 10 at the end step");
}

#[test]
fn coven_counts_different_powers() {
    let mut table = Table::default();
    let witch = table.card(
        "{1}{G}",
        "Creature — Bear",
        Some((1, 1)),
        "As long as you control three or more creatures with different powers, this creature \
         gets +2/+2.",
    );
    let two = table.bear();
    let three = table.card("{2}{G}", "Creature — Bear", Some((3, 3)), "");
    let mut game = Game::new(table);
    let witch = game.put(witch, P0, Zone::Battlefield);
    game.put(two, P0, Zone::Battlefield);
    game.put(two, P0, Zone::Battlefield);
    assert_eq!(game.pt(witch), (1, 1), "powers 1 and 2 only");
    game.put(three, P0, Zone::Battlefield);
    assert_eq!(game.pt(witch), (3, 3), "1, 2 and 3");
}

#[test]
fn cast_from_a_graveyard_is_known_as_the_spell_resolves() {
    let mut table = Table::default();
    let think = table.card(
        "{U}",
        "Instant",
        None,
        "Draw a card. If this spell was cast from a graveyard, draw two cards instead.\n\
         Flashback {1}{U}",
    );
    let mut game = Game::new(table);
    game.lands(3);
    let think = game.put(think, P0, Zone::Hand);
    game.main();
    let hand = game.count(Zone::Hand, P0);
    game.act(Action::Cast { object: think }, &[], &[]);
    assert_eq!(game.count(Zone::Hand, P0), hand, "cast from hand: one card");
    let in_graveyard = game
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Graveyard, P0))[0];
    game.act(
        Action::Cast {
            object: in_graveyard,
        },
        &[],
        &[],
    );
    assert_eq!(game.count(Zone::Hand, P0), hand + 2, "flashback: two");
}

#[test]
fn tarmogoyf_counts_card_types_in_all_graveyards() {
    let mut table = Table::default();
    let goyf = table.card(
        "{1}{G}",
        "Creature — Bear",
        None,
        "~'s power is equal to the number of card types among cards in all graveyards and its \
         toughness is equal to that number plus 1.",
    );
    let bear = table.bear();
    let bolt = table.card("{R}", "Instant", None, "You gain 1 life.");
    let mountain = table.mountain();
    let mut game = Game::new(table);
    let goyf = game.put(goyf, P0, Zone::Battlefield);
    game.put(bear, P0, Zone::Graveyard);
    game.put(bolt, P1, Zone::Graveyard);
    game.put(mountain, P1, Zone::Graveyard);
    assert_eq!(game.pt(goyf), (3, 4));
}

#[test]
fn a_creature_that_cant_block_the_source_can_block_others() {
    let mut table = Table::default();
    let goblin = table.card(
        "{R}",
        "Creature — Bear",
        Some((2, 2)),
        "{1}: Target creature can't block this creature this turn.",
    );
    let bear = table.bear();
    let mut game = Game::new(table);
    game.lands(1);
    let goblin = game.put(goblin, P0, Zone::Battlefield);
    let other = game.put(bear, P0, Zone::Battlefield);
    let blocker = game.put(bear, P1, Zone::Battlefield);
    game.main();
    game.act(
        activate(goblin, 0),
        &[mtg_core::Target::Object(blocker)],
        &[],
    );
    assert!(!mtg_engine::combat::can_block(
        &game.engine.state,
        &game.table,
        blocker,
        goblin
    ));
    assert!(mtg_engine::combat::can_block(
        &game.engine.state,
        &game.table,
        blocker,
        other
    ));
}
