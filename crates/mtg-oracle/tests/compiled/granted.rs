//! Abilities granted in quotes (layer 6): "enchanted land has '{T}: Add …'".
use super::harness::*;
use mtg_core::{AbilityId, Target, Zone};
use mtg_engine::actions::Action;

const GRANTED: AbilityId = AbilityId(AbilityId::GRANTED_BASE);

#[test]
fn an_enchanted_land_taps_for_the_granted_mana() {
    let mut t = Table::default();
    let growth = t.card(
        "{G}",
        "Enchantment — Aura",
        None,
        "Enchant land\nEnchanted land has \"{T}: Add {G}{G}.\"",
    );
    // A land with no mana ability of its own, so only the granted one can pay.
    let barren = t.card("", "Land", None, "");
    let giant = t.card("{1}{G}", "Creature — Bear", Some((2, 2)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let land = g.put(barren, P0, Zone::Battlefield);
    let aura = g.put(growth, P0, Zone::Hand);
    let bear = g.put(giant, P0, Zone::Hand);
    g.main();
    g.cast(aura, &[Target::Object(land)]);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: bear }),
        "{{1}}{{G}} is payable from the granted {{G}}{{G}}"
    );
    g.cast(bear, &[]);
    assert!(g.find(giant).is_some());
    assert!(g.engine.state.objects[&land].tapped);
}

#[test]
fn a_granted_ability_uses_the_creature_as_its_source_and_outlives_the_aura() {
    let mut t = Table::default();
    let rod = t.card(
        "{R}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature has \"{1}: This creature deals 1 damage to any target.\"",
    );
    let bear = t.bear();
    let disenchant = t.card("{W}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(bear, P0, Zone::Battlefield);
    let aura = g.put(rod, P0, Zone::Hand);
    let dis = g.put(disenchant, P0, Zone::Hand);
    g.main();
    g.cast(aura, &[Target::Object(b)]);
    let actions = g.main();
    assert!(actions.contains(&Action::ActivateAbility {
        source: b,
        ability: GRANTED,
    }));
    // Activate, then destroy the Aura in response: the ability is already on the stack.
    g.act_holding(
        Action::ActivateAbility {
            source: b,
            ability: GRANTED,
        },
        &[Target::Player(P1)],
    );
    let aura_now = g.find(rod).expect("aura on the battlefield");
    g.act(
        Action::Cast { object: dis },
        &[Target::Object(aura_now)],
        &[],
    );
    assert!(g.find(rod).is_none(), "the Aura is gone");
    assert_eq!(g.life(P1), 19, "the ability resolved anyway");
    let actions = g.main();
    assert!(
        !actions.contains(&Action::ActivateAbility {
            source: b,
            ability: GRANTED,
        }),
        "no Aura, no ability"
    );
}

#[test]
fn a_lord_grants_each_creature_its_own_pump() {
    let mut t = Table::default();
    let lord = t.card(
        "{2}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Creatures you control have \"{1}: This creature gets +1/+1 until end of turn.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let l = g.put(lord, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let foe = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.act(
        Action::ActivateAbility {
            source: b,
            ability: GRANTED,
        },
        &[],
        &[],
    );
    assert_eq!(g.pt(b), (3, 3), "the bear pumped itself");
    assert_eq!(g.pt(l), (1, 1), "not the lord");
    assert_eq!(
        g.pt(foe),
        (2, 2),
        "an opponent's creature has no such ability"
    );
}

#[test]
fn a_granted_tap_ability_waits_out_summoning_sickness() {
    let mut t = Table::default();
    let charm = t.card(
        "{W}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature has \"{T}: You gain 2 life.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let aura = g.put(charm, P0, Zone::Hand);
    let cub = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(cub, &[]);
    let fresh = g.find(bear).unwrap();
    g.cast(aura, &[Target::Object(fresh)]);
    let actions = g.main();
    assert!(!actions.contains(&Action::ActivateAbility {
        source: fresh,
        ability: GRANTED,
    }));
}

#[test]
fn an_aura_grants_an_attack_trigger() {
    let mut t = Table::default();
    let blessing = t.card(
        "{W}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature has \"Whenever this creature attacks, you gain 2 life.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let aura = g.put(blessing, P0, Zone::Hand);
    g.main();
    g.cast(aura, &[Target::Object(b)]);
    g.combat(&[b, other], &[], &[], &[]);
    assert_eq!(
        g.life(P0),
        22,
        "one trigger, from the enchanted attacker only"
    );
    assert_eq!(g.life(P1), 16);
}

#[test]
fn a_granted_enters_trigger_fires_for_each_creature_including_the_lord() {
    let mut t = Table::default();
    let lord = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Creatures you control have \"When this creature enters, draw a card.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let l = g.put(lord, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(l, &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "cast the lord (-1), it drew (+1)"
    );
    g.cast(b, &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "cast the bear (-1), it drew (+1)"
    );
}

#[test]
fn a_scion_token_sacrifices_itself_for_mana() {
    let mut t = Table::default();
    let spawner = t.card(
        "{2}",
        "Creature — Eldrazi",
        Some((1, 1)),
        "When this creature enters, create a 1/1 colorless Eldrazi Scion creature token. It has \"Sacrifice this token: Add {C}.\"",
    );
    let relic = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(spawner, P0, Zone::Hand);
    let r = g.put(relic, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let creatures = |g: &Game| g.engine.state.battlefield().len();
    let before = creatures(&g);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: r }),
        "the lands are tapped; the Scion pays {{1}}"
    );
    g.cast(r, &[]);
    assert_eq!(
        creatures(&g),
        before,
        "the Scion left, the artifact arrived"
    );
}

#[test]
fn an_aura_can_shrink_and_strip_a_creature() {
    let mut t = Table::default();
    let curse = t.card(
        "{1}{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature gets -1/-1 and loses all abilities.",
    );
    let drake = t.card("{2}{U}", "Creature — Drake", Some((3, 3)), "Flying");
    let mut g = Game::new(t);
    g.lands(2);
    let d = g.put(drake, P1, Zone::Battlefield);
    let c = g.put(curse, P0, Zone::Hand);
    g.main();
    assert!(g.has(d, mtg_core::Keyword::Flying));
    g.cast(c, &[mtg_core::Target::Object(d)]);
    assert_eq!(g.pt(d), (2, 2));
    let ch = g.engine.characteristics(&g.table, d).unwrap().clone();
    assert!(!ch.granted_keywords.contains(&mtg_core::Keyword::Flying));
    assert!(ch.abilities.is_empty(), "no abilities left");
}

#[test]
fn bonuses_for_being_equipped_and_attacking() {
    let mut t = Table::default();
    let squire = t.card(
        "{1}{W}",
        "Creature — Human Knight",
        Some((1, 1)),
        "As long as this creature is equipped, it gets +2/+2.\n\
         This creature gets +1/+0 as long as it's attacking.",
    );
    let sword = t.card(
        "{1}",
        "Artifact — Equipment",
        None,
        "Equipped creature gets +1/+1.\nEquip {1}",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(squire, P0, Zone::Battlefield);
    let e = g.put(sword, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(s), (1, 1));
    g.act(activate(e, 1), &[mtg_core::Target::Object(s)], &[]);
    assert_eq!(g.pt(s), (4, 4), "equipped: +1/+1 and +2/+2");
    g.combat(&[s], &[], &[], &[]);
    assert_eq!(g.life(P1), 15, "5 power while attacking");
}

#[test]
fn a_manland_becomes_a_creature_until_end_of_turn() {
    let mut t = Table::default();
    let lair = t.card(
        "",
        "Land",
        None,
        "{T}: Add {G}.\n{1}{G}: This land becomes a 3/3 green Bear creature with trample until \
         end of turn. It's still a land.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let l = g.put(lair, P0, Zone::Battlefield);
    g.main();
    let ch = g.engine.characteristics(&g.table, l).unwrap().clone();
    assert!(!ch.has_type(mtg_core::CardType::Creature));
    g.act(activate(l, 1), &[], &[]);
    let ch = g.engine.characteristics(&g.table, l).unwrap().clone();
    assert!(ch.has_type(mtg_core::CardType::Creature));
    assert!(ch.has_type(mtg_core::CardType::Land), "still a land");
    assert_eq!(g.pt(l), (3, 3));
    assert!(g.has(l, mtg_core::Keyword::Trample));
    // The automatic payment may have tapped the land for its own activation.
    g.engine.state.objects.get_mut(&l).unwrap().tapped = false;
    g.combat(&[l], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    g.until(P1, mtg_core::Step::Upkeep);
    let ch = g.engine.characteristics(&g.table, l).unwrap().clone();
    assert!(!ch.has_type(mtg_core::CardType::Creature), "a land again");
}

#[test]
fn an_upkeep_only_ability_is_offered_only_in_the_upkeep() {
    let mut t = Table::default();
    let idol = t.card(
        "{2}",
        "Artifact",
        None,
        "{1}: You gain 1 life. Activate only during your upkeep.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let i = g.put(idol, P0, Zone::Battlefield);
    let actions = g.until(P0, mtg_core::Step::Upkeep);
    assert!(offers(&actions, i), "upkeep");
    let actions = g.main();
    assert_eq!(g.engine.state.step, mtg_core::Step::PrecombatMain);
    assert!(!offers(&actions, i), "main phase");
}

#[test]
fn players_cant_gain_life_and_you_have_hexproof() {
    let mut t = Table::default();
    let witch = t.card(
        "{1}{B}",
        "Creature — Human Warlock",
        Some((2, 2)),
        "Players can't gain life.",
    );
    let ward = t.card("{1}{W}", "Enchantment", None, "You have hexproof.");
    let salve = t.card("{W}", "Instant", None, "You gain 3 life.");
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(witch, P0, Zone::Battlefield);
    g.put(ward, P1, Zone::Battlefield);
    let s = g.put(salve, P0, Zone::Hand);
    let sh = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 20, "no life gained");
    let legal = mtg_engine::targeting::legal_targets(
        &g.engine.state,
        &g.table,
        &mtg_ir::PrintedCards::face(&g.table, g.engine.state.objects[&sh].card, 0)
            .unwrap()
            .abilities[0]
            .targets[0],
        sh,
        P0,
        &[],
    );
    assert!(
        !legal.contains(&mtg_core::Target::Player(P1)),
        "P1 has hexproof"
    );
    assert!(legal.contains(&mtg_core::Target::Player(P0)));
}

#[test]
fn arrest_stops_attacks_blocks_and_abilities() {
    let mut t = Table::default();
    let arrest = t.card(
        "{2}{W}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature can't attack or block, and its activated \
         abilities can't be activated.",
    );
    let elf = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "{1}: This creature gets +1/+1 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let e = g.put(elf, P0, Zone::Battlefield);
    let a = g.put(arrest, P0, Zone::Hand);
    let actions = g.main();
    assert!(offers(&actions, e));
    g.cast(a, &[mtg_core::Target::Object(e)]);
    let actions = g.main();
    assert!(!offers(&actions, e), "its ability can't be activated");
    assert!(
        !mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0).contains(&e),
        "can't attack"
    );
}

#[test]
fn power_alone_can_be_defined() {
    let mut t = Table::default();
    let crusader = t.card(
        "{2}{G}",
        "Creature — Elf",
        Some((0, 3)),
        "~'s power is equal to the number of creatures you control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(crusader, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(c), (2, 3));
}

#[test]
fn an_artifact_creature_lord() {
    let mut t = Table::default();
    let lord = t.card(
        "{3}",
        "Artifact Creature — Golem",
        Some((2, 2)),
        "Other artifact creatures you control get +1/+1.",
    );
    let golem = t.card("{2}", "Artifact Creature — Golem", Some((1, 1)), "");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    let l = g.put(lord, P0, Zone::Battlefield);
    let gm = g.put(golem, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.put(rock, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(gm), (2, 2));
    assert_eq!(g.pt(b), (2, 2), "not an artifact");
    assert_eq!(g.pt(l), (2, 2), "other");
}

#[test]
fn a_curse_enchants_a_player() {
    let mut t = Table::default();
    let curse = t.card(
        "{B}",
        "Enchantment — Aura Curse",
        None,
        "Enchant player\nAt the beginning of enchanted player's upkeep, that player loses 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let c = g.put(curse, P0, Zone::Hand);
    g.main();
    g.cast(c, &[mtg_core::Target::Player(P1)]);
    let on = g.find(curse).expect("attached to a player, so it stays");
    assert_eq!(g.engine.state.objects[&on].attached_player, Some(P1));
    g.until(P1, mtg_core::Step::Draw);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.life(P0), 20, "not the curse's controller");
}

#[test]
fn skip_your_draw_step() {
    let mut t = Table::default();
    let sloth = t.card("{1}{U}", "Enchantment", None, "Skip your draw step.");
    let mut g = Game::new(t);
    g.put(sloth, P0, Zone::Battlefield);
    let hand = g.count(Zone::Hand, P0);
    g.main();
    assert_eq!(g.count(Zone::Hand, P0), hand, "no draw");
}

#[test]
fn granted_protection_in_response_counters_a_burn_spell() {
    let mut t = Table::default();
    let knight = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "{W}: This creature gains protection from red until end of turn.",
    );
    let shock = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let k = g.put(knight, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: s }, &[Target::Object(k)]);
    assert_eq!(g.stack().len(), 1);
    g.act(activate(k, 0), &[], &[]);
    assert!(g.find(knight).is_some(), "the shock lost its only target");
    assert_eq!(g.engine.state.objects[&k].damage, 0);
}

#[test]
fn protection_from_the_color_of_your_choice() {
    use mtg_engine::choice::Answer;
    for (color, blockable) in [(3, false), (0, true)] {
        let mut t = Table::default();
        let ward = t.card(
            "{W}",
            "Instant",
            None,
            "Target creature you control gains protection from the color of your choice \
             until end of turn.",
        );
        let bear = t.bear();
        let goblin = t.card("{R}", "Creature — Goblin", Some((2, 2)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let b = g.put(bear, P0, Zone::Battlefield);
        let gob = g.put(goblin, P1, Zone::Battlefield);
        let w = g.put(ward, P0, Zone::Hand);
        g.main();
        // 3 is red, 0 is white.
        g.act(
            Action::Cast { object: w },
            &[Target::Object(b)],
            &[Answer::Modes(vec![color])],
        );
        g.engine
            .state
            .combat
            .attackers
            .insert(b, Target::Player(P1));
        assert_eq!(
            mtg_engine::combat::can_block(&g.engine.state, &g.table, gob, b),
            blockable,
            "a red creature can't block a creature with protection from red"
        );
    }
}

#[test]
fn equipment_that_grants_protection() {
    let mut t = Table::default();
    let cloak = t.card(
        "{2}",
        "Artifact — Equipment",
        None,
        "Equipped creature gets +1/+1 and has protection from red.",
    );
    let bear = t.bear();
    let goblin = t.card("{R}", "Creature — Goblin", Some((2, 2)), "");
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(cloak, P0, Zone::Battlefield);
    let gob = g.put(goblin, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&c).unwrap().attached_to = Some(b);
    g.main();
    assert_eq!(g.pt(b), (3, 3));
    g.engine
        .state
        .combat
        .attackers
        .insert(b, Target::Player(P1));
    assert!(!mtg_engine::combat::can_block(
        &g.engine.state,
        &g.table,
        gob,
        b
    ));
}

#[test]
fn static_conditions_on_hand_size_creature_count_and_the_enchanted_creature() {
    let mut t = Table::default();
    let scholar = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "As long as you have four or more cards in hand, this creature gets +2/+2.",
    );
    let loner = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "As long as you control exactly one creature, this creature gets +1/+1.",
    );
    let warpaint = t.card(
        "{R}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nAs long as enchanted creature is red, enchanted creature gets +2/+0.",
    );
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let bear = t.bear();
    let mountain = t.mountain();
    let mut g = Game::new(t);
    let s = g.put(scholar, P0, Zone::Battlefield);
    // Two now, three after the turn's draw.
    for _ in 0..2 {
        g.put(mountain, P0, Zone::Hand);
    }
    let l = g.put(loner, P1, Zone::Battlefield);
    let gob = g.put(goblin, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let w1 = g.put(warpaint, P0, Zone::Battlefield);
    let w2 = g.put(warpaint, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&w1).unwrap().attached_to = Some(gob);
    g.engine.state.objects.get_mut(&w2).unwrap().attached_to = Some(b);
    g.main();
    assert_eq!(g.pt(s), (1, 1), "three cards in hand");
    assert_eq!(g.pt(l), (2, 2), "its controller has only it");
    assert_eq!(g.pt(gob), (3, 1), "red");
    assert_eq!(g.pt(b), (2, 2), "the bear is green");
    g.put(mountain, P0, Zone::Hand);
    assert_eq!(g.pt(s), (3, 3), "four cards in hand");
}
