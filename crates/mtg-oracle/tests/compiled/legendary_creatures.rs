//! Shared rules support for the full legendary-creature catalog.
use super::harness::*;
use mtg_core::{CardType, Keyword, Step, Supertype, Target, Zone};
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

#[test]
fn a_devotion_god_loses_and_regains_its_creature_type_and_subtype() {
    let mut t = Table::default();
    let god = t.named_card("Erebos, God of the Dead", "{3}{B}", "Legendary Enchantment Creature — God", Some((5, 7)), "Indestructible\nAs long as your devotion to black is less than five, Erebos isn't a creature.\nYour opponents can't gain life.\n{1}{B}, Pay 2 life: Draw a card.");
    let support_card = t.card("{B}{B}{B}{B}", "Enchantment", None, "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target nonland permanent to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let god = g.put(god, P0, Zone::Battlefield);
    let support = g.put(support_card, P0, Zone::Battlefield);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    let god_type = g.table.subtype_named("God").unwrap();
    let ch = g.engine.characteristics(&g.table, god).unwrap();
    assert!(ch.has_type(CardType::Creature) && ch.subtypes.contains(&god_type));
    g.cast(bounce, &[Target::Object(support)]);
    let ch = g.engine.characteristics(&g.table, god).unwrap();
    assert!(!ch.has_type(CardType::Creature));
    assert!(!ch.subtypes.contains(&god_type));
    assert!(ch.has_type(CardType::Enchantment));
    assert!(g.has(god, Keyword::Indestructible));
    let support = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.zone.zone == Zone::Hand && o.card == support_card)
        .unwrap()
        .id;
    g.cast(support, &[]);
    let ch = g.engine.characteristics(&g.table, god).unwrap();
    assert!(ch.has_type(CardType::Creature) && ch.subtypes.contains(&god_type));
}

#[test]
fn two_color_devotion_counts_hybrid_symbols_once_and_ignores_opponents_permanents() {
    let mut t = Table::default();
    let god = t.named_card("Karametra, God of Harvests", "{3}{G}{W}", "Legendary Enchantment Creature — God", Some((6, 7)), "Indestructible\nAs long as your devotion to green and white is less than seven, Karametra isn't a creature.");
    let support = t.card("{G/W}{G/W}{G/W}", "Enchantment", None, "");
    let extra = t.card("{G}{W}", "Enchantment", None, "");
    let mut g = Game::new(t);
    let god = g.put(god, P0, Zone::Battlefield);
    g.put(support, P0, Zone::Battlefield);
    g.put(extra, P1, Zone::Battlefield);
    g.main();
    assert!(
        !g.engine
            .characteristics(&g.table, god)
            .unwrap()
            .has_type(CardType::Creature)
    );
    // Two more symbols you control reach seven: the three hybrid symbols each count once.
    g.put(extra, P0, Zone::Battlefield);
    assert!(
        g.engine
            .characteristics(&g.table, god)
            .unwrap()
            .has_type(CardType::Creature)
    );
}

#[test]
fn labeled_activated_abilities_keep_their_costs_and_counter_the_chosen_ability() {
    let mut t = Table::default();
    let adric = t.named_card("Adric, Mathematical Genius", "{1}{U}", "Legendary Creature — Human Artificer", Some((1, 1)), "Ultimate Sacrifice — {1}{U}, Sacrifice Adric: Counter target activated or triggered ability.");
    let source = t.card("{0}", "Artifact", None, "{0}: You gain 3 life.");
    let mut g = Game::new(t);
    g.lands(2);
    let adric = g.put(adric, P0, Zone::Battlefield);
    let source = g.put(source, P0, Zone::Battlefield);
    g.main();
    g.act_holding(activate(source, 0), &[]);
    let ability = g.stack()[0];
    g.act(activate(adric, 0), &[Target::Object(ability)], &[]);
    assert_eq!(g.life(P0), 20);
    assert!(!g.engine.state.objects.contains_key(&adric));
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn combined_keyword_lines_keep_both_flying_and_the_prowess_trigger() {
    let mut t = Table::default();
    let body = t.card(
        "{0}",
        "Legendary Creature — Human Wizard",
        Some((2, 2)),
        "Flying, prowess",
    );
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let creature = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let body = g.put(body, P0, Zone::Battlefield);
    let instant = g.put(instant, P0, Zone::Hand);
    let creature = g.put(creature, P0, Zone::Hand);
    g.main();
    assert!(g.has(body, Keyword::Flying));
    g.cast(instant, &[]);
    assert_eq!(g.pt(body), (3, 3));
    g.cast(creature, &[]);
    assert_eq!(g.pt(body), (3, 3));
    g.until(P1, Step::Upkeep);
    assert_eq!(g.pt(body), (2, 2));
}

#[test]
fn hexproof_from_artifacts_and_enchantments_only_stops_matching_opponents_sources() {
    let mut t = Table::default();
    let protected = t.card(
        "{0}",
        "Legendary Creature — Elf Druid",
        Some((2, 3)),
        "Hexproof from artifacts and enchantments",
    );
    let artifact = t.card("{0}", "Artifact", None, "");
    let enchantment = t.card("{0}", "Enchantment", None, "");
    let creature = t.bear();
    let instant = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let protected = g.put(protected, P0, Zone::Battlefield);
    let sources = [
        (g.put(artifact, P1, Zone::Battlefield), P1, false),
        (g.put(enchantment, P1, Zone::Battlefield), P1, false),
        (g.put(creature, P1, Zone::Battlefield), P1, true),
        (g.put(instant, P1, Zone::Hand), P1, true),
        (g.put(artifact, P0, Zone::Battlefield), P0, true),
    ];
    g.main();
    let chars = mtg_engine::eval::ComputedChars(&g.table);
    let bindings = Default::default();
    for (source, controller, expected) in sources {
        let ctx = mtg_engine::eval::Ctx {
            state: &g.engine.state,
            cards: &g.table,
            chars: &chars,
            source,
            controller,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: &bindings,
        };
        assert_eq!(
            mtg_engine::eval::matches(&ctx, &mtg_ir::ObjectFilter::Targetable, protected).unwrap(),
            expected
        );
    }
}

#[test]
fn granted_melee_counts_attacked_opponents_and_does_not_trigger_for_nonattackers() {
    let mut t = Table::default();
    let adriana = t.named_card(
        "Adriana, Captain of the Guard",
        "{3}{R}{W}",
        "Legendary Creature — Human Knight",
        Some((4, 4)),
        "Melee\nOther creatures you control have melee.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let p2 = mtg_core::PlayerId(2);
    g.engine
        .state
        .players
        .insert(p2, mtg_engine::state::PlayerState::new(p2, 20));
    g.engine.state.turn_order.push(p2);
    let adriana = g.put(adriana, P0, Zone::Battlefield);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let idle = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat_at(
        &[
            (adriana, Target::Player(P1)),
            (attacker, Target::Player(p2)),
        ],
        &[],
    );
    assert_eq!(g.pt(adriana), (6, 6));
    assert_eq!(g.pt(attacker), (4, 4));
    assert_eq!(g.pt(idle), (2, 2));
}

#[test]
fn aeves_tokens_are_not_legendary_but_the_original_is() {
    let mut t = Table::default();
    let aeve = t.named_card(
        "Aeve, Progenitor Ooze",
        "{0}",
        "Legendary Creature — Ooze",
        Some((2, 2)),
        "Aeve isn't legendary if it's a token.",
    );
    let copy = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a token that's a copy of target creature.",
    );
    let mut g = Game::new(t);
    let original = g.put(aeve, P0, Zone::Battlefield);
    let copy = g.put(copy, P0, Zone::Hand);
    g.main();
    g.cast(copy, &[Target::Object(original)]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .unwrap();
    assert!(
        g.engine
            .characteristics(&g.table, original)
            .unwrap()
            .supertypes
            .contains(&Supertype::Legendary)
    );
    assert!(
        !g.engine
            .characteristics(&g.table, token)
            .unwrap()
            .supertypes
            .contains(&Supertype::Legendary)
    );
}

#[test]
fn teferi_grants_flash_in_all_nonbattlefield_zones_without_granting_play_permission() {
    let mut t = Table::default();
    let teferi = t.named_card(
        "Teferi, Mage of Zhalfir",
        "{2}{U}{U}{U}",
        "Legendary Creature — Human Wizard",
        Some((3, 4)),
        "Flash\nCreature cards you own that aren't on the battlefield have flash.",
    );
    let creature = t.card("{0}", "Creature — Bear", Some((2, 2)), "");
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.put(teferi, P0, Zone::Battlefield);
    let hand = g.put(creature, P0, Zone::Hand);
    for zone in [Zone::Library, Zone::Graveyard, Zone::Exile, Zone::Command] {
        let own = g.put(creature, P0, zone);
        let enemy = g.put(creature, P1, zone);
        assert!(g.has(own, Keyword::Flash), "{zone:?}");
        assert!(!g.has(enemy, Keyword::Flash), "{zone:?}");
    }
    let battlefield = g.put(creature, P0, Zone::Battlefield);
    assert!(!g.has(battlefield, Keyword::Flash));
    let spell = g.put(instant, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.act_holding(Action::Cast { object: spell }, &[]);
    g.cast(hand, &[]);
    assert_eq!(
        g.engine
            .state
            .battlefield()
            .iter()
            .filter(|id| g.engine.state.objects[id].card == creature
                && g.engine.state.objects[id].owner == P0)
            .count(),
        2
    );
}

const TEYSA: &str = "If a creature dying causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.\nCreature tokens you control have vigilance and lifelink.";

#[test]
fn death_trigger_doublers_stack_additively_and_only_double_their_controllers_triggers() {
    for count in 0..=2 {
        let mut t = Table::default();
        let teysa = t.named_card(
            "Teysa Karlov",
            "{2}{W}{B}",
            "Legendary Creature — Human Advisor",
            Some((2, 4)),
            TEYSA,
        );
        let drivnod = t.named_card("Drivnod, Carnage Dominus", "{3}{B}{B}", "Legendary Creature — Phyrexian Horror", Some((8, 3)), "If a creature dying causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time.");
        let victim = t.card(
            "{0}",
            "Creature — Human",
            Some((1, 1)),
            "When this creature dies, you gain 1 life.",
        );
        let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
        let mut g = Game::new(t);
        if count >= 1 {
            g.put(teysa, P0, Zone::Battlefield);
        }
        if count >= 2 {
            g.put(drivnod, P0, Zone::Battlefield);
        }
        let own = g.put(victim, P0, Zone::Battlefield);
        let enemy = g.put(victim, P1, Zone::Battlefield);
        let a = g.put(kill, P0, Zone::Hand);
        let b = g.put(kill, P0, Zone::Hand);
        g.main();
        g.cast(a, &[Target::Object(own)]);
        assert_eq!(g.life(P0), 21 + count);
        g.cast(b, &[Target::Object(enemy)]);
        assert_eq!(g.life(P1), 21);
    }
}

#[test]
fn teysa_doubles_death_triggers_when_she_dies_in_the_same_batch_in_either_object_order() {
    for teysa_first in [true, false] {
        let mut t = Table::default();
        let teysa = t.named_card(
            "Teysa Karlov",
            "{2}{W}{B}",
            "Legendary Creature — Human Advisor",
            Some((2, 4)),
            TEYSA,
        );
        let victim = t.card(
            "{0}",
            "Creature — Human",
            Some((1, 1)),
            "When this creature dies, you gain 1 life.",
        );
        let wrath = t.card("{0}", "Sorcery", None, "Destroy all creatures.");
        let mut g = Game::new(t);
        for card in if teysa_first {
            [teysa, victim]
        } else {
            [victim, teysa]
        } {
            g.put(card, P0, Zone::Battlefield);
        }
        let wrath = g.put(wrath, P0, Zone::Hand);
        g.main();
        g.cast(wrath, &[]);
        assert_eq!(g.life(P0), 22);
        assert_eq!(g.engine.state.battlefield().len(), 0);
    }
}

#[test]
fn a_doubler_that_died_earlier_does_not_double_later_death_triggers() {
    let mut t = Table::default();
    let teysa = t.named_card(
        "Teysa Karlov",
        "{2}{W}{B}",
        "Legendary Creature — Human Advisor",
        Some((2, 4)),
        TEYSA,
    );
    let victim = t.card(
        "{0}",
        "Creature — Human",
        Some((1, 1)),
        "When this creature dies, you gain 1 life.",
    );
    let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let teysa = g.put(teysa, P0, Zone::Battlefield);
    let victim = g.put(victim, P0, Zone::Battlefield);
    let a = g.put(kill, P0, Zone::Hand);
    let b = g.put(kill, P0, Zone::Hand);
    g.main();
    g.cast(a, &[Target::Object(teysa)]);
    g.cast(b, &[Target::Object(victim)]);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn preston_checks_last_known_cast_history_and_copies_a_creature_that_left_before_resolution() {
    let mut t = Table::default();
    let preston = t.named_card("Preston, the Vanisher", "{3}{W}", "Legendary Creature — Rabbit Wizard", Some((2, 5)), "Whenever another nontoken creature you control enters, if it wasn't cast, create a token that's a copy of that creature, except it's a 0/1 white Illusion.");
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    let original = g.put(bear, P0, Zone::Battlefield);
    let blink = g.put(blink, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: blink }, &[Target::Object(original)]);
    g.drive(
        |_, _| None,
        |g| {
            g.stack().iter().any(|id| {
                g.engine.state.objects[id]
                    .cast_context
                    .as_ref()
                    .is_some_and(|c| c.ability.is_some())
            })
        },
    );
    let returned = g.find(bear).unwrap();
    g.cast(bounce, &[Target::Object(returned)]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("Preston still copies the departed creature");
    assert_eq!(g.pt(token), (0, 1));
}

#[test]
fn abomination_of_llanowar_sums_only_your_elves_in_play_and_elf_cards_in_your_graveyard() {
    let mut t = Table::default();
    let abomination = t.named_card("Abomination of Llanowar", "{1}{B}{G}", "Legendary Creature — Elf Horror", Some((0, 0)), "Vigilance\nMenace\nAbomination of Llanowar's power and toughness are each equal to the number of Elves you control plus the number of Elf cards in your graveyard.");
    let elf = t.card("{0}", "Creature — Elf", Some((1, 1)), "");
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let abomination = g.put(abomination, P0, Zone::Battlefield);
    let own = g.put(elf, P0, Zone::Battlefield);
    g.put(elf, P0, Zone::Graveyard);
    g.put(elf, P1, Zone::Battlefield);
    g.put(elf, P1, Zone::Graveyard);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(abomination), (3, 3));
    g.cast(bounce, &[Target::Object(own)]);
    assert_eq!(g.pt(abomination), (2, 2));
    let returned = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == elf && o.zone.zone == Zone::Hand)
        .unwrap()
        .id;
    g.cast(returned, &[]);
    assert_eq!(g.pt(abomination), (3, 3));
}

#[test]
fn adamaro_uses_the_largest_opponents_hand_and_multani_counts_all_hands() {
    let mut t = Table::default();
    let adamaro = t.named_card("Adamaro, First to Desire", "{1}{R}{R}", "Legendary Creature — Spirit", Some((0, 0)), "Adamaro's power and toughness are each equal to the number of cards in the hand of the opponent with the most cards in hand.");
    let multani = t.named_card("Multani, Maro-Sorcerer", "{4}{G}{G}", "Legendary Creature — Elemental", Some((0, 0)), "Shroud\nMultani's power and toughness are each equal to the total number of cards in all players' hands.");
    let filler = t.bear();
    let mut g = Game::new(t);
    let p2 = mtg_core::PlayerId(2);
    g.engine
        .state
        .players
        .insert(p2, mtg_engine::state::PlayerState::new(p2, 20));
    g.engine.state.turn_order.push(p2);
    let adamaro = g.put(adamaro, P0, Zone::Battlefield);
    let multani = g.put(multani, P0, Zone::Battlefield);
    for (player, count) in [(P0, 7), (P1, 3), (p2, 5)] {
        for _ in 0..count {
            g.put(filler, player, Zone::Hand);
        }
    }
    g.main();
    assert_eq!(g.pt(adamaro), (5, 5));
    let all = (g.count(Zone::Hand, P0) + g.count(Zone::Hand, P1) + g.count(Zone::Hand, p2)) as i32;
    assert_eq!(g.pt(multani), (all, all));
}

#[test]
fn daxos_defines_only_his_toughness_and_preserves_power_modifiers() {
    let mut t = Table::default();
    let daxos = t.named_card("Daxos, Blessed by the Sun", "{W}{W}", "Legendary Enchantment Creature — Demigod", Some((2, 0)), "Daxos's toughness is equal to your devotion to white.\nWhenever another creature you control enters or dies, you gain 1 life.");
    let support = t.card("{W}{W}{W}", "Enchantment", None, "");
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let mut g = Game::new(t);
    let daxos = g.put(daxos, P0, Zone::Battlefield);
    g.put(support, P0, Zone::Battlefield);
    let pump = g.put(pump, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(daxos), (2, 5));
    g.cast(pump, &[Target::Object(daxos)]);
    assert_eq!(g.pt(daxos), (5, 8));
    g.until(P1, Step::Upkeep);
    assert_eq!(g.pt(daxos), (2, 5));
}

#[test]
fn a_death_doubler_on_the_stack_does_not_function_before_it_enters() {
    let mut t = Table::default();
    let teysa = t.named_card(
        "Teysa Karlov",
        "{0}",
        "Legendary Creature — Human Advisor",
        Some((2, 4)),
        TEYSA,
    );
    let victim = t.card(
        "{0}",
        "Creature — Human",
        Some((1, 1)),
        "When this creature dies, you gain 1 life.",
    );
    let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    let teysa = g.put(teysa, P0, Zone::Hand);
    let victim = g.put(victim, P0, Zone::Battlefield);
    let kill = g.put(kill, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: teysa }, &[]);
    g.cast(kill, &[Target::Object(victim)]);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn doubling_a_stolen_creatures_death_trigger_uses_its_battlefield_controller() {
    let mut t = Table::default();
    let teysa = t.named_card(
        "Teysa Karlov",
        "{0}",
        "Legendary Creature — Human Advisor",
        Some((2, 4)),
        TEYSA,
    );
    let victim = t.card(
        "{0}",
        "Creature — Human",
        Some((1, 1)),
        "When this creature dies, you gain 1 life.",
    );
    let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.put(teysa, P0, Zone::Battlefield);
    let victim = g.put(victim, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&victim).unwrap().controller = P0;
    let kill = g.put(kill, P0, Zone::Hand);
    g.main();
    g.cast(kill, &[Target::Object(victim)]);
    assert_eq!(g.life(P0), 22);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn daxoss_combined_trigger_observes_entries_and_simultaneous_deaths_but_not_itself() {
    let mut t = Table::default();
    let daxos = t.named_card("Daxos, Blessed by the Sun", "{W}{W}", "Legendary Enchantment Creature — Demigod", Some((2, 0)), "Daxos's toughness is equal to your devotion to white.\nWhenever another creature you control enters or dies, you gain 1 life.");
    let bear = t.card("{0}", "Creature — Bear", Some((2, 2)), "");
    let wrath = t.card("{0}", "Sorcery", None, "Destroy all creatures.");
    let mut g = Game::new(t);
    g.put(daxos, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Hand);
    let wrath = g.put(wrath, P0, Zone::Hand);
    g.main();
    g.cast(bear, &[]);
    assert_eq!(g.life(P0), 21);
    g.cast(wrath, &[]);
    assert_eq!(
        g.life(P0),
        23,
        "two other creatures you controlled died alongside Daxos"
    );
}
