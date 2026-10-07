//! Trigger conditions beyond the plain ones: two events at once, the permanent this is
//! attached to, "you attack", and turning face up.
use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn enters_or_attacks_triggers_on_both() {
    let mut t = Table::default();
    let knight = t.card(
        "{1}{W}",
        "Creature — Knight",
        Some((2, 2)),
        "Whenever this creature enters or attacks, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let k = g.put(knight, P0, Zone::Hand);
    g.main();
    g.cast(k, &[]);
    assert_eq!(g.life(P0), 21, "entered");
    let k = g.find(knight).unwrap();
    // Summoning sick this turn: attack next turn.
    g.until(P0, mtg_core::Step::End);
    g.until(P0, mtg_core::Step::PrecombatMain);
    g.combat(&[k], &[], &[], &[]);
    assert_eq!(g.life(P1), 18, "the knight attacked");
    assert_eq!(g.life(P0), 22, "attacked");
}

#[test]
fn an_aura_sees_its_creature_die() {
    let mut t = Table::default();
    let aura = t.card(
        "{B}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nWhen enchanted creature dies, draw a card.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let a = g.put(aura, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(a, &[Target::Object(b)]);
    let hand = g.count(Zone::Hand, P0);
    g.cast(bolt, &[Target::Object(b)]);
    for e in g.engine.log.iter().rev().take(14) {
        eprintln!("{:?}", e.event);
    }
    assert_eq!(g.count(Zone::Hand, P0), hand, "bolt left, a card came");
    assert_eq!(g.count(Zone::Graveyard, P0), 3, "bear, aura, bolt");
}

#[test]
fn whenever_you_attack_is_once_per_combat() {
    let mut t = Table::default();
    let banner = t.card(
        "{2}",
        "Artifact",
        None,
        "Whenever you attack, you gain 1 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(banner, P0, Zone::Battlefield);
    let b1 = g.put(bear, P0, Zone::Battlefield);
    let b2 = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat(&[b1, b2], &[], &[], &[]);
    assert_eq!(g.life(P0), 21);
    assert_eq!(g.life(P1), 16);
}

#[test]
fn turning_a_morph_face_up_triggers() {
    let mut t = Table::default();
    let monk = t.card(
        "{2}{W}",
        "Creature — Monk",
        Some((2, 2)),
        "When this creature is turned face up, you gain 3 life.\nMorph {W}",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let m = g.put(monk, P0, Zone::Hand);
    g.main();
    g.act(Action::CastFaceDown { object: m }, &[], &[]);
    let on = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].face_down)
        .unwrap();
    let actions = g.main();
    let up = actions
        .iter()
        .find(|a| matches!(a, Action::SpecialAction { source, .. } if *source == on))
        .cloned()
        .unwrap();
    g.act(up, &[], &[]);
    assert_eq!(g.life(P0), 23);
}

#[test]
fn the_second_spell_each_turn_triggers_once() {
    let mut t = Table::default();
    let prodigy = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever you cast your second spell each turn, you gain 2 life.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(3);
    g.put(prodigy, P0, Zone::Battlefield);
    let s1 = g.put(salve, P0, Zone::Hand);
    let s2 = g.put(salve, P0, Zone::Hand);
    let s3 = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s1, &[]);
    assert_eq!(g.life(P0), 21);
    g.cast(s2, &[]);
    assert_eq!(g.life(P0), 24, "second spell: 1 + 2");
    g.cast(s3, &[]);
    assert_eq!(g.life(P0), 25, "third: just 1");
}

#[test]
fn discarding_a_card_and_nontoken_deaths_trigger() {
    let mut t = Table::default();
    let ghoul = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "Whenever you discard a card, you gain 2 life.\n\
         Whenever another nontoken creature you control dies, you gain 1 life.",
    );
    let rummage = t.card("{R}", "Sorcery", None, "Discard a card, then draw a card.");
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(ghoul, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let r = g.put(rummage, P0, Zone::Hand);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(bolt, &[Target::Object(b)]);
    assert_eq!(g.life(P0), 21, "a nontoken creature died");
    g.cast(r, &[]);
    assert_eq!(g.life(P0), 23, "discarded");
}

#[test]
fn a_cast_trigger_resolves_before_its_spell() {
    let mut t = Table::default();
    let titan = t.card(
        "{2}{G}",
        "Creature — Eldrazi",
        Some((3, 3)),
        "When you cast this spell, you gain 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let c = g.put(titan, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: c }, &[]);
    assert_eq!(g.stack().len(), 2, "the spell and its trigger");
    g.act(Action::Pass, &[], &[]);
    assert_eq!(g.life(P0), 22);
    assert!(g.find(titan).is_some());
}

#[test]
fn the_second_card_drawn_each_turn_triggers() {
    let mut t = Table::default();
    let sage = t.card(
        "{1}{U}",
        "Creature — Human Wizard",
        Some((1, 1)),
        "Whenever you draw your second card each turn, put a +1/+1 counter on this creature.",
    );
    let divination = t.card("{U}", "Sorcery", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(sage, P0, Zone::Battlefield);
    let d = g.put(divination, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(s), (1, 1), "the draw step was the first");
    g.cast(d, &[]);
    assert_eq!(g.pt(s), (2, 2), "the second card, not the third");
}

#[test]
fn heroic_triggers_on_a_spell_targeting_it() {
    let mut t = Table::default();
    let hero = t.card(
        "{W}",
        "Creature — Human Soldier",
        Some((1, 1)),
        "Heroic — Whenever you cast a spell that targets this creature, put a +1/+1 counter on \
         this creature.",
    );
    let pump = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +1/+1 until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(hero, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let p1 = g.put(pump, P0, Zone::Hand);
    let p2 = g.put(pump, P0, Zone::Hand);
    g.main();
    g.cast(p1, &[Target::Object(b)]);
    assert_eq!(g.pt(h), (1, 1), "not targeted");
    g.cast(p2, &[Target::Object(h)]);
    assert_eq!(g.pt(h), (3, 3), "a counter, and the pump");
}

#[test]
fn magecraft_triggers_on_casting_an_instant() {
    let mut t = Table::default();
    let mage = t.card(
        "{1}{R}",
        "Creature — Human Wizard",
        Some((1, 1)),
        "Magecraft — Whenever you cast or copy an instant or sorcery spell, you gain 1 life.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(mage, P0, Zone::Battlefield);
    let s = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 22);
}

#[test]
fn if_you_cast_it_is_false_for_a_permanent_put_onto_the_battlefield() {
    let mut t = Table::default();
    let titan = t.card(
        "{2}{G}",
        "Creature — Giant",
        Some((3, 3)),
        "When this creature enters, if you cast it, you gain 3 life.",
    );
    let raise = t.card(
        "{B}",
        "Sorcery",
        None,
        "Return target creature card from your graveyard to the battlefield.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let c = g.put(titan, P0, Zone::Hand);
    let dead = g.put(titan, P0, Zone::Graveyard);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    assert_eq!(g.life(P0), 23, "cast");
    g.cast(r, &[Target::Object(dead)]);
    assert_eq!(g.life(P0), 23, "returned, not cast");
}

#[test]
fn sacrificing_another_creature_never_takes_this_one() {
    for with_bear in [true, false] {
        let mut t = Table::default();
        let fiend = t.card(
            "{2}{B}",
            "Creature — Demon",
            Some((3, 3)),
            "When this creature enters, you may sacrifice another creature. If you do, draw two \
             cards.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(3);
        if with_bear {
            g.put(bear, P0, Zone::Battlefield);
        }
        let f = g.put(fiend, P0, Zone::Hand);
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.act(
            Action::Cast { object: f },
            &[],
            &[mtg_engine::choice::Answer::Bool(true)],
        );
        let f = g.find(fiend).expect("still here");
        assert_eq!(g.engine.state.objects[&f].zone.zone, Zone::Battlefield);
        if with_bear {
            assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2);
            assert_eq!(g.count(Zone::Graveyard, P0), 1, "the bear");
        } else {
            assert_eq!(g.count(Zone::Hand, P0), hand - 1, "nothing to sacrifice");
        }
    }
}

#[test]
fn sacrifice_it_unless_you_discard() {
    for (discard, card_in_hand) in [(true, true), (false, true), (true, false)] {
        let mut t = Table::default();
        let hound = t.card(
            "{1}{R}",
            "Creature — Dog",
            Some((3, 3)),
            "When this creature enters, sacrifice it unless you discard a card.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let h = g.put(hound, P0, Zone::Hand);
        g.main();
        // Empty the hand of everything but the hound (and maybe a bear).
        for id in g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        {
            if id != h {
                g.engine.state.objects.remove(&id);
                for order in g.engine.state.zone_order.values_mut() {
                    order.retain(|o| *o != id);
                }
            }
        }
        if card_in_hand {
            g.put(bear, P0, Zone::Hand);
        }
        g.act(
            Action::Cast { object: h },
            &[],
            &[mtg_engine::choice::Answer::Bool(discard)],
        );
        let kept = discard && card_in_hand;
        assert_eq!(
            g.find(hound).is_some(),
            kept,
            "discard {discard}, card {card_in_hand}"
        );
    }
}

#[test]
fn sacrifice_it_unless_it_escaped() {
    let mut t = Table::default();
    let titan = t.card(
        "{1}{G}{U}",
        "Creature — Elder Giant",
        Some((6, 6)),
        "When this creature enters, sacrifice it unless it escaped.\n\
         Escape—{G}{G}{U}{U}, Exile two other cards from your graveyard.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let u = g.put(titan, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Graveyard);
    g.main();
    g.cast(u, &[]);
    let u = g.find(titan).unwrap();
    assert_eq!(
        g.engine.state.objects[&u].zone.zone,
        Zone::Battlefield,
        "escaped"
    );
}

#[test]
fn morbid_sees_a_creature_that_died_this_turn() {
    let mut t = Table::default();
    let ghoul = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "Morbid — When this creature enters, if a creature died this turn, you gain 3 life.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(5);
    let g1 = g.put(ghoul, P0, Zone::Hand);
    let g2 = g.put(ghoul, P0, Zone::Hand);
    let b = g.put(bear, P1, Zone::Battlefield);
    let bolt = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(g1, &[]);
    assert_eq!(g.life(P0), 20, "nothing died yet");
    g.cast(bolt, &[Target::Object(b)]);
    g.cast(g2, &[]);
    assert_eq!(g.life(P0), 23, "morbid");
}

#[test]
fn a_spell_costs_less_if_you_control_a_wizard() {
    let mut t = Table::default();
    let bolt = t.card(
        "{2}{R}",
        "Instant",
        None,
        "This spell costs {2} less to cast if you control a Wizard.\n\
         ~ deals 3 damage to any target.",
    );
    let wizard = t.card("{U}", "Creature — Human Wizard", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bolt, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::Cast { object: b }),
        "costs {{2}}{{R}}"
    );
    g.put(wizard, P0, Zone::Battlefield);
    // Re-ask for actions with the wizard in play.
    g.act(Action::Pass, &[], &[]);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    let b = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == bolt)
        .unwrap();
    assert!(actions.contains(&Action::Cast { object: b }), "costs {{R}}");
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 17);
}

#[test]
fn a_stunned_creature_skips_one_untap() {
    let mut t = Table::default();
    let frost = t.card(
        "{1}{U}",
        "Instant",
        None,
        "Tap target creature. Put a stun counter on it.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P1, Zone::Battlefield);
    let f = g.put(frost, P0, Zone::Hand);
    g.main();
    g.cast(f, &[Target::Object(b)]);
    assert!(g.engine.state.objects[&b].tapped);
    g.until(P1, mtg_core::Step::Upkeep);
    assert!(
        g.engine.state.objects[&b].tapped,
        "the stun counter came off instead"
    );
    assert_eq!(
        g.engine.state.objects[&b]
            .counters
            .get(&mtg_core::CounterKind::Stun)
            .copied()
            .unwrap_or(0),
        0
    );
    g.until(P0, mtg_core::Step::Upkeep);
    g.until(P1, mtg_core::Step::Upkeep);
    assert!(
        !g.engine.state.objects[&b].tapped,
        "untapped the turn after"
    );
}

#[test]
fn a_goaded_creature_must_attack() {
    let mut t = Table::default();
    let taunt = t.card("{R}", "Instant", None, "Goad target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let tt = g.put(taunt, P0, Zone::Hand);
    g.main();
    assert!(mtg_engine::combat::must_attack(&g.engine.state, &g.table, P1).is_empty());
    g.cast(tt, &[Target::Object(b)]);
    assert_eq!(
        mtg_engine::combat::must_attack(&g.engine.state, &g.table, P1),
        vec![b]
    );
}

#[test]
fn the_monarch_draws_and_can_be_stolen() {
    let mut t = Table::default();
    let herald = t.card(
        "{2}{W}",
        "Creature — Human",
        Some((2, 2)),
        "When this creature enters, you become the monarch.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let h = g.put(herald, P0, Zone::Hand);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(h, &[]);
    assert_eq!(g.engine.state.monarch, Some(P0));
    let hand = g.count(Zone::Hand, P0);
    g.until(P0, mtg_core::Step::End);
    g.act(Action::Pass, &[], &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 1,
        "the monarch drew at the end step"
    );
    let _ = b;
}

#[test]
fn combat_damage_to_the_monarch_steals_it() {
    let mut t = Table::default();
    let crown = t.card("{1}", "Sorcery", None, "Target player becomes the monarch.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let c = g.put(crown, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.cast(c, &[Target::Player(P1)]);
    assert_eq!(g.engine.state.monarch, Some(P1));
    g.combat(&[b], &[], &[], &[]);
    assert_eq!(
        g.engine.state.monarch,
        Some(P0),
        "the bear's controller took it"
    );
}

#[test]
fn a_trigger_limited_to_once_each_turn() {
    let mut t = Table::default();
    let cleric = t.card(
        "{1}{W}",
        "Creature — Cleric",
        Some((1, 1)),
        "Whenever you gain life, put a +1/+1 counter on this creature. This ability triggers \
         only once each turn.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(cleric, P0, Zone::Battlefield);
    let s1 = g.put(salve, P0, Zone::Hand);
    let s2 = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s1, &[]);
    g.cast(s2, &[]);
    assert_eq!(g.pt(c), (2, 2), "only the first life gain counted");
}

#[test]
fn a_reflexive_trigger_targets_after_the_payment() {
    let mut t = Table::default();
    let gunner = t.card(
        "{2}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "When this creature enters, you may pay {1}. When you do, this creature deals 3 \
         damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P1, Zone::Battlefield);
    let gu = g.put(gunner, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: gu },
        &[Target::Object(b)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert!(!g.engine.state.objects.contains_key(&b), "the bear took 3");
}

#[test]
fn that_player_is_the_opponent_who_drew() {
    let mut t = Table::default();
    let curse = t.card(
        "{2}{B}",
        "Enchantment",
        None,
        "Whenever an opponent draws a card, that player loses 1 life.",
    );
    let mut g = Game::new(t);
    g.put(curse, P0, Zone::Battlefield);
    g.main();
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!((g.life(P0), g.life(P1)), (20, 19), "P1 drew for the turn");
}

#[test]
fn enters_with_a_counter_if_an_opponent_lost_life() {
    for drained in [false, true] {
        let mut t = Table::default();
        let vamp = t.card(
            "{1}{B}",
            "Creature — Zombie",
            Some((1, 1)),
            "This creature enters with a +1/+1 counter on it if an opponent lost life this turn.",
        );
        let drain = t.card("{B}", "Sorcery", None, "Each opponent loses 1 life.");
        let mut g = Game::new(t);
        g.lands(3);
        let v = g.put(vamp, P0, Zone::Hand);
        let d = g.put(drain, P0, Zone::Hand);
        g.main();
        if drained {
            g.cast(d, &[]);
        }
        g.cast(v, &[]);
        let id = g.find(vamp).unwrap();
        assert_eq!(g.pt(id), if drained { (2, 2) } else { (1, 1) });
    }
}

#[test]
fn cycling_triggers_its_own_card_and_watchers() {
    let mut t = Table::default();
    let drake = t.card(
        "{3}{U}",
        "Creature — Spirit",
        Some((2, 2)),
        "Cycling {1}\nWhen you cycle this card, you gain 3 life.",
    );
    let monk = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever you cycle a card, each opponent loses 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(drake, P0, Zone::Hand);
    g.put(monk, P0, Zone::Battlefield);
    g.main();
    g.act(activate(d, 0), &[], &[]);
    assert_eq!((g.life(P0), g.life(P1)), (23, 19), "both triggers");
}

#[test]
fn discarding_is_not_cycling() {
    let mut t = Table::default();
    let monk = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever you cycle a card, each opponent loses 1 life.",
    );
    let looter = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "{T}, Discard a card: Draw a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(monk, P0, Zone::Battlefield);
    let l = g.put(looter, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&l).unwrap().summoning_sick = false;
    g.put(bear, P0, Zone::Hand);
    g.main();
    g.act(activate(l, 0), &[], &[]);
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "discarded");
    assert_eq!(g.life(P1), 20, "but nothing was cycled");
}

#[test]
fn sacrifices_by_a_token_ability_and_by_a_cost_both_trigger() {
    let mut t = Table::default();
    let chef = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "When this creature enters, create a Food token.\nWhenever you sacrifice a Food, each \
         opponent loses 2 life.",
    );
    let altar = t.card(
        "{1}",
        "Artifact",
        None,
        "Sacrifice a creature: You gain 1 life.\nWhenever you sacrifice another permanent, \
         you gain 1 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let c = g.put(chef, P0, Zone::Hand);
    let a = g.put(altar, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.cast(c, &[]);
    let food = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| {
            mtg_ir::PrintedCards::face(&g.table, g.engine.state.objects[id].card, 0)
                .is_some_and(|f| &*f.name == "Food")
        })
        .expect("a Food");
    // Food: {2}, {T}, Sacrifice this: gain 3 life — plus the chef's 2 and the altar's 1.
    g.act(activate(food, 0), &[], &[]);
    assert_eq!((g.life(P0), g.life(P1)), (24, 18));
    // The altar's own cost: a sacrificed creature, seen by its other ability.
    g.act(
        activate(a, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![b])],
    );
    assert_eq!(g.life(P0), 26, "1 from the ability, 1 from the trigger");
}

#[test]
fn a_creature_dying_is_not_a_sacrifice() {
    let mut t = Table::default();
    let altar = t.card(
        "{1}",
        "Artifact",
        None,
        "Whenever you sacrifice a creature, you gain 1 life.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(altar, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert!(g.find(bear).is_none());
    assert_eq!(g.life(P0), 20);
}

#[test]
fn the_upkeep_of_enchanted_creatures_controller() {
    let mut t = Table::default();
    let curse = t.card(
        "{1}{B}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nAt the beginning of the upkeep of enchanted creature's controller, \
         that player loses 1 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let b = g.put(bear, P1, Zone::Battlefield);
    let c = g.put(curse, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&c).unwrap().attached_to = Some(b);
    g.main();
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(
        (g.life(P0), g.life(P1)),
        (20, 19),
        "only on its controller's upkeep"
    );
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn each_players_end_step() {
    let mut t = Table::default();
    let tithe = t.card(
        "{1}{W}",
        "Enchantment",
        None,
        "At the beginning of each player's end step, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.put(tithe, P0, Zone::Battlefield);
    g.main();
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(g.life(P0), 22, "my end step and theirs");
}

#[test]
fn attacks_alone_only_when_alone() {
    for alone in [true, false] {
        let mut t = Table::default();
        let banner = t.card(
            "{1}{W}",
            "Enchantment",
            None,
            "Whenever a creature you control attacks alone, it gets +2/+2 until end of turn.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.put(banner, P0, Zone::Battlefield);
        let a = g.put(bear, P0, Zone::Battlefield);
        let b = g.put(bear, P0, Zone::Battlefield);
        g.main();
        let attackers = if alone { vec![a] } else { vec![a, b] };
        g.combat(&attackers, &[], &[], &[]);
        assert_eq!(g.life(P1), 16, "four damage either way");
        assert_eq!(g.pt(a), if alone { (4, 4) } else { (2, 2) });
    }
}

#[test]
fn life_triggers_with_turn_qualifiers() {
    let mut t = Table::default();
    let first = t.card(
        "{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever you gain life for the first time each turn, put a +1/+1 counter on this \
         creature.",
    );
    let mine = t.card(
        "{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever you gain life during your turn, put a +1/+1 counter on this creature.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(3);
    let f = g.put(first, P0, Zone::Battlefield);
    let m = g.put(mine, P0, Zone::Battlefield);
    let s1 = g.put(salve, P0, Zone::Hand);
    let s2 = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s1, &[]);
    g.cast(s2, &[]);
    assert_eq!(g.pt(f), (2, 2), "only the first gain this turn");
    assert_eq!(g.pt(m), (3, 3), "both gains were on my turn");
}

#[test]
fn losing_life_and_being_dealt_damage() {
    let mut t = Table::default();
    let ward = t.card(
        "{B}",
        "Enchantment",
        None,
        "Whenever you're dealt damage, you gain 1 life.\nWhenever you lose life, each \
         opponent loses 1 life.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(ward, P0, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Player(P0)]);
    assert_eq!((g.life(P0), g.life(P1)), (18, 19));
}

#[test]
fn leaving_and_graveyard_from_the_battlefield_triggers() {
    let mut t = Table::default();
    let watcher = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Whenever an artifact you control is put into a graveyard from the battlefield, you \
         gain 2 life.\nWhenever another creature you control leaves the battlefield, you gain \
         1 life.",
    );
    let trinket = t.card("{1}", "Artifact", None, "");
    let shatter = t.card("{R}", "Instant", None, "Destroy target artifact.");
    let bear = t.bear();
    let unsummon = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    g.put(watcher, P0, Zone::Battlefield);
    let tr = g.put(trinket, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(shatter, P0, Zone::Hand);
    let u = g.put(unsummon, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(tr)]);
    assert_eq!(g.life(P0), 22);
    g.cast(u, &[Target::Object(b)]);
    assert_eq!(g.life(P0), 23, "bounced, not died, still left");
}

#[test]
fn scry_and_surveil_trigger_separately() {
    let mut t = Table::default();
    let seer = t.card(
        "{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever you scry, put a +1/+1 counter on this creature.",
    );
    let peek = t.card("{U}", "Sorcery", None, "Scry 2.");
    let dig = t.card("{U}", "Sorcery", None, "Surveil 2.");
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(seer, P0, Zone::Battlefield);
    let p = g.put(peek, P0, Zone::Hand);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    g.cast(d, &[]);
    assert_eq!(g.pt(w), (1, 1), "surveil is not scry");
    g.cast(p, &[]);
    assert_eq!(g.pt(w), (2, 2));
}

#[test]
fn support_puts_counters_on_other_creatures() {
    let mut t = Table::default();
    let captain = t.card(
        "{2}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "When this creature enters, support 2.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(captain, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: c },
        &[Target::Object(a), Target::Object(b)],
        &[],
    );
    assert_eq!((g.pt(a), g.pt(b)), ((3, 3), (3, 3)));
    let me = g.find(captain).unwrap();
    assert_eq!(g.pt(me), (2, 2), "not itself");
}

#[test]
fn bolster_finds_the_least_toughness() {
    let mut t = Table::default();
    let herald = t.card(
        "{2}{W}",
        "Creature — Soldier",
        Some((3, 3)),
        "When this creature enters, bolster 2.",
    );
    let bear = t.bear();
    let wall = t.card("{1}{W}", "Creature — Wall", Some((0, 4)), "Defender");
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(bear, P0, Zone::Battlefield);
    let w = g.put(wall, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let h = g.put(herald, P0, Zone::Hand);
    g.main();
    g.cast(h, &[]);
    assert_eq!(
        g.pt(b),
        (4, 4),
        "the 2-toughness bear, not the wall or itself"
    );
    assert_eq!(g.pt(w), (0, 4));
    assert_eq!(g.pt(theirs), (2, 2), "only creatures you control");
}

#[test]
fn life_gain_boosts_add_then_double() {
    for (plus, twice, gained) in [(true, false, 3), (false, true, 4), (true, true, 6)] {
        let mut t = Table::default();
        let cleric = t.card(
            "{W}",
            "Enchantment",
            None,
            "If you would gain life, you gain that much life plus 1 instead.",
        );
        let angel = t.card(
            "{W}",
            "Enchantment",
            None,
            "If you would gain life, you gain twice that much life instead.",
        );
        let salve = t.card("{W}", "Instant", None, "You gain 2 life.");
        let mut g = Game::new(t);
        g.lands(1);
        if plus {
            g.put(cleric, P0, Zone::Battlefield);
        }
        if twice {
            g.put(angel, P0, Zone::Battlefield);
        }
        let s = g.put(salve, P0, Zone::Hand);
        g.main();
        g.cast(s, &[]);
        assert_eq!(g.life(P0), 20 + gained, "plus {plus}, twice {twice}");
    }
}

#[test]
fn that_card_is_the_one_the_death_moved() {
    let mut t = Table::default();
    let aura = t.card(
        "{2}{B}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nWhen enchanted creature dies, return that card to the battlefield \
         under your control.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let a = g.put(aura, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&a).unwrap().attached_to = Some(b);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    let back: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].card == bear)
        .collect();
    assert_eq!(back.len(), 1, "it came back");
    assert_eq!(
        g.engine.state.objects[&back[0]].controller,
        P0,
        "under the aura's controller"
    );
}

#[test]
fn training_needs_a_stronger_fellow_attacker() {
    for (partner_power, grows) in [(1, false), (3, true)] {
        let mut t = Table::default();
        let cadet = t.card("{W}", "Creature — Soldier", Some((1, 1)), "Training");
        let partner = t.card("{1}", "Creature — Soldier", Some((partner_power, 1)), "");
        let mut g = Game::new(t);
        let c = g.put(cadet, P0, Zone::Battlefield);
        let p = g.put(partner, P0, Zone::Battlefield);
        g.main();
        g.combat(&[c, p], &[], &[], &[]);
        assert_eq!(
            g.engine.state.objects[&c]
                .counters
                .get(&mtg_core::CounterKind::PlusOnePlusOne)
                .copied()
                .unwrap_or(0),
            i32::from(grows),
            "partner power {partner_power}"
        );
    }
}

#[test]
fn a_spell_that_targets_a_creature_triggers_by_type_and_target() {
    let mut t = Table::default();
    let prowess = t.card(
        "{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1 \
         counter on this creature.",
    );
    let growth = t.card("{G}", "Instant", None, "Target creature gets +1/+1 until end of turn.");
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(prowess, P0, Zone::Battlefield);
    let gr = g.put(growth, P0, Zone::Hand);
    let sh = g.put(shock, P0, Zone::Hand);
    g.main();
    let counters = |g: &Game| {
        g.engine.state.objects[&w]
            .counters
            .get(&mtg_core::CounterKind::PlusOnePlusOne)
            .copied()
            .unwrap_or(0)
    };
    g.cast(sh, &[Target::Player(P1)]);
    assert_eq!(counters(&g), 0, "a player is not a creature");
    g.cast(gr, &[Target::Object(w)]);
    assert_eq!(counters(&g), 1);
}

#[test]
fn counter_only_a_spell_that_targets_a_creature_you_control() {
    for at_creature in [true, false] {
        let mut t = Table::default();
        let ward = t.card(
            "{U}",
            "Instant",
            None,
            "Counter target spell that targets a creature you control.",
        );
        let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let b = g.put(bear, P0, Zone::Battlefield);
        let w = g.put(ward, P0, Zone::Hand);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        let target = if at_creature {
            Target::Object(b)
        } else {
            Target::Player(P1)
        };
        g.act_holding(Action::Cast { object: s }, &[target]);
        let spell = g.stack()[0];
        let legal = mtg_engine::targeting::legal_targets(
            &g.engine.state,
            &g.table,
            &mtg_engine::targeting::specs_of(&g.engine.state, &g.table, w)[0],
            w,
            P0,
            &[],
        );
        assert_eq!(legal.contains(&Target::Object(spell)), at_creature);
    }
}

#[test]
fn looking_at_a_hand_shows_it_to_the_looker_only() {
    let mut t = Table::default();
    let peek = t.card(
        "{U}",
        "Sorcery",
        None,
        "Look at target player's hand. Draw a card.",
    );
    let secret = t.card("{5}", "Sorcery", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(secret, P1, Zone::Hand);
    let s = g.put(peek, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    let seen = |who| {
        mtg_engine::view::project(&g.engine.state, who)
            .revealed_cards
            .iter()
            .any(|c| c.card == secret)
    };
    assert!(seen(P0), "the caster saw it");
    let mine = mtg_engine::view::project(&g.engine.state, P1);
    assert!(
        !mine.revealed_cards.iter().any(|c| c.owner == P0),
        "nothing of the caster's was shown"
    );
}

#[test]
fn a_dying_creature_passes_its_counters_on() {
    let mut t = Table::default();
    let donor = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "When this creature dies, put its counters on target creature you control.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 5 damage to target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(donor, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&d)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 2);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[Target::Object(d), Target::Object(b)],
        &[],
    );
    assert_eq!(g.pt(b), (4, 4));
}

#[test]
fn a_counter_comes_off_at_end_of_combat() {
    let mut t = Table::default();
    let knight = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 1)),
        "Whenever this creature attacks or blocks, remove a +1/+1 counter from it at end of \
         combat.",
    );
    let mut g = Game::new(t);
    let k = g.put(knight, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&k)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 2);
    g.main();
    g.combat(&[k], &[], &[], &[]);
    assert_eq!(
        g.engine.state.objects[&k]
            .counters
            .get(&mtg_core::CounterKind::PlusOnePlusOne)
            .copied(),
        Some(1)
    );
    assert_eq!(g.life(P1), 17, "dealt 3 before the counter came off");
}
