//! Keyword mechanics that change how a card is cast or what happens around it.
use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn affinity_for_artifacts_reduces_the_cost_in_hand() {
    for (artifacts, castable) in [(3, false), (4, true)] {
        let mut t = Table::default();
        let golem = t.card(
            "{4}{U}",
            "Artifact Creature — Wizard",
            Some((3, 3)),
            "Affinity for artifacts",
        );
        let trinket = t.card("{1}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let hand = g.put(golem, P0, Zone::Hand);
        for _ in 0..artifacts {
            g.put(trinket, P0, Zone::Battlefield);
        }
        let actions = g.main();
        assert_eq!(
            actions.contains(&Action::Cast { object: hand }),
            castable,
            "one land, {artifacts} artifacts"
        );
        if castable {
            g.cast(hand, &[]);
            assert!(g.find(golem).is_some());
        }
    }
}

#[test]
fn a_spell_costs_less_for_each_card_it_counts() {
    let mut t = Table::default();
    let horror = t.card(
        "{5}{B}",
        "Creature — Zombie",
        Some((5, 5)),
        "This spell costs {1} less to cast for each creature card in your graveyard.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(horror, P0, Zone::Hand);
    for _ in 0..4 {
        g.put(bear, P0, Zone::Graveyard);
    }
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: h }), "{{1}}{{B}}");
    g.cast(h, &[]);
    assert!(g.find(horror).is_some());
}

/// Play on to `who`'s next precombat main phase, answering every yes/no question `pay`.
fn next_main(g: &mut Game, who: mtg_core::PlayerId, pay: bool) {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let start = g.engine.state.turn;
    for _ in 0..10_000 {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && g.engine.state.turn > start
                    && g.engine.state.active_player == who
                    && g.engine.state.step == mtg_core::Step::PrecombatMain
                    && g.stack().is_empty()
                {
                    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
                    return;
                }
                let a = match c.kind {
                    ChoiceKind::Confirm => Answer::Bool(pay),
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
        }
    }
    panic!("never reached the next main phase");
}

fn echo_beast(t: &mut Table) -> mtg_core::CardId {
    t.card("{1}{R}", "Creature — Goblin", Some((3, 3)), "Echo {1}{R}")
}

#[test]
fn echo_is_paid_once_on_the_next_upkeep() {
    let mut t = Table::default();
    let beast = echo_beast(&mut t);
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(beast, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    next_main(&mut g, P0, true);
    assert!(g.find(beast).is_some(), "paid, kept");
    let lands_tapped = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| g.engine.state.objects[id].tapped)
        .count();
    assert_eq!(lands_tapped, 2, "the echo cost was paid");
    // The turn after, nothing is owed: declining would sacrifice it if it were asked.
    next_main(&mut g, P0, false);
    assert!(g.find(beast).is_some(), "no echo a second time");
}

#[test]
fn unpaid_echo_sacrifices_the_permanent() {
    let mut t = Table::default();
    let beast = echo_beast(&mut t);
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(beast, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    next_main(&mut g, P0, false);
    assert!(g.find(beast).is_none());
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn buyback_returns_the_spell_to_hand_when_paid() {
    for paid in [true, false] {
        let mut t = Table::default();
        let shock = t.card(
            "{R}",
            "Instant",
            None,
            "Buyback {3}\n~ deals 1 damage to any target.",
        );
        let mut g = Game::new(t);
        g.lands(4);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: s },
            &[mtg_core::Target::Player(P1)],
            &[mtg_engine::choice::Answer::Bool(paid)],
        );
        assert_eq!(g.life(P1), 19);
        let in_hand = g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == shock);
        assert_eq!(in_hand, paid, "paid: back in hand; not paid: graveyard");
        assert_eq!(g.count(Zone::Graveyard, P0), usize::from(!paid));
    }
}

#[test]
fn buyback_can_be_paid_with_a_sacrifice_or_discard() {
    use mtg_engine::choice::Answer;
    for discard in [false, true] {
        for paid in [false, true] {
            let mut t = Table::default();
            let spell = t.card(
                "{R}",
                "Instant",
                None,
                if discard {
                    "Buyback—Discard two cards.\n~ deals 1 damage to any target."
                } else {
                    "Buyback—Sacrifice a land.\n~ deals 1 damage to any target."
                },
            );
            let land = t.mountain();
            let mut g = Game::new(t);
            g.lands(1);
            let fodder: Vec<_> = (0..if discard { 2 } else { 1 })
                .map(|_| {
                    g.put(
                        land,
                        P0,
                        if discard {
                            Zone::Hand
                        } else {
                            Zone::Battlefield
                        },
                    )
                })
                .collect();
            let s = g.put(spell, P0, Zone::Hand);
            g.main();
            let mut answers = vec![Answer::Bool(paid)];
            if paid {
                answers.push(Answer::Objects(fodder.clone()));
            }
            g.act(
                Action::Cast { object: s },
                &[mtg_core::Target::Player(P1)],
                &answers,
            );
            assert_eq!(g.life(P1), 19);
            assert_eq!(
                g.count(Zone::Graveyard, P0),
                if paid { fodder.len() } else { 1 }
            );
            let in_hand = g
                .engine
                .state
                .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == spell);
            assert_eq!(in_hand, paid);
            for id in fodder {
                assert_eq!(g.engine.state.objects.contains_key(&id), !paid);
            }
        }
    }
}

#[test]
fn a_card_returns_itself_from_the_graveyard_to_hand() {
    let mut t = Table::default();
    let rat = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 1)),
        "{2}{B}: Return this card from your graveyard to your hand.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let r = g.put(rat, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(offers(&actions, r), "offered from the graveyard");
    g.act(activate(r, 0), &[], &[]);
    assert_eq!(g.count(Zone::Graveyard, P0), 0);
    let back = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == rat);
    assert!(back);
}

fn unearthed() -> (Game, mtg_core::CardId, mtg_core::ObjectId) {
    let mut t = Table::default();
    let ghoul = t.card("{2}{B}", "Creature — Zombie", Some((3, 3)), "Unearth {B}");
    let mut g = Game::new(t);
    g.lands(1);
    let gy = g.put(ghoul, P0, Zone::Graveyard);
    g.main();
    g.act(activate(gy, 0), &[], &[]);
    let on = g.find(ghoul).expect("unearthed");
    (g, ghoul, on)
}

#[test]
fn unearth_returns_a_hasty_creature_that_is_exiled_at_end_of_turn() {
    let (mut g, ghoul, on) = unearthed();
    assert!(g.has(on, mtg_core::Keyword::Haste));
    g.combat(&[on], &[], &[], &[]);
    assert_eq!(g.life(P1), 17, "it attacked the turn it returned");
    g.until(P1, mtg_core::Step::Upkeep);
    assert!(g.find(ghoul).is_none());
    assert_eq!(g.count(Zone::Graveyard, P0), 0);
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .any(|o| o.card == ghoul && o.zone.zone == Zone::Exile);
    assert!(exiled);
}

#[test]
fn an_unearthed_creature_that_would_die_is_exiled_instead() {
    let mut t = Table::default();
    let ghoul = t.card("{2}{B}", "Creature — Zombie", Some((3, 3)), "Unearth {B}");
    let murder = t.card("{B}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(2);
    let gy = g.put(ghoul, P0, Zone::Graveyard);
    let m = g.put(murder, P0, Zone::Hand);
    g.main();
    g.act(activate(gy, 0), &[], &[]);
    let on = g.find(ghoul).unwrap();
    g.cast(m, &[mtg_core::Target::Object(on)]);
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the murder spell");
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == ghoul && o.zone.zone == Zone::Exile)
    );
}

#[test]
fn delve_exiles_graveyard_cards_to_pay_generic_mana() {
    let mut t = Table::default();
    let scour = t.card(
        "{5}{B}",
        "Sorcery",
        None,
        "Delve\nTarget player loses 2 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(scour, P0, Zone::Hand);
    for _ in 0..5 {
        g.put(bear, P0, Zone::Graveyard);
    }
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: s }),
        "one land for {{B}}, five cards for {{5}}"
    );
    g.cast(s, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the spell itself");
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.zone.zone == Zone::Exile && o.card == bear)
        .count();
    assert_eq!(exiled, 5);
}

#[test]
fn a_bounce_land_returns_a_land_its_controller_chooses() {
    let mut t = Table::default();
    let karoo = t.card(
        "",
        "Land",
        None,
        "This land enters tapped.\nWhen this land enters, return a land you control to its owner's hand.\n{T}: Add {W}{U}.",
    );
    let plains = t.card("", "Basic Land — Plains", None, "");
    let mut g = Game::new(t);
    let p = g.put(plains, P0, Zone::Battlefield);
    let k = g.put(karoo, P0, Zone::Hand);
    g.main();
    g.act(
        Action::PlayLand { object: k },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![p])],
    );
    let on = g.find(karoo).expect("the karoo stayed");
    assert!(g.engine.state.objects[&on].tapped);
    assert!(g.find(plains).is_none(), "the plains went back to hand");
    let back = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == plains);
    assert!(back);
}

#[test]
fn soulshift_returns_a_small_spirit() {
    let mut t = Table::default();
    let shade = t.card("{3}{B}", "Creature — Spirit", Some((2, 2)), "Soulshift 3");
    let small_card = t.card("{2}{B}", "Creature — Spirit", Some((1, 1)), "");
    let big = t.card("{4}{B}", "Creature — Spirit", Some((4, 4)), "");
    let murder = t.card("{B}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(shade, P0, Zone::Battlefield);
    let small = g.put(small_card, P0, Zone::Graveyard);
    g.put(big, P0, Zone::Graveyard);
    let m = g.put(murder, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: m },
        &[mtg_core::Target::Object(s), mtg_core::Target::Object(small)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    let hand: Vec<_> = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert!(
        hand.contains(&small_card),
        "the mana value 3 Spirit came back"
    );
}

#[test]
fn an_illusion_is_sacrificed_when_targeted() {
    let mut t = Table::default();
    let illusion = t.card(
        "{U}",
        "Creature — Wizard",
        Some((2, 1)),
        "When this creature becomes the target of a spell or ability, sacrifice it.",
    );
    let growth = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let i = g.put(illusion, P0, Zone::Battlefield);
    let s = g.put(growth, P0, Zone::Hand);
    g.main();
    g.cast(s, &[mtg_core::Target::Object(i)]);
    assert!(
        g.find(illusion).is_none(),
        "sacrificed before the pump resolved"
    );
}

fn explorer(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "When this creature enters, it explores.",
    )
}

#[test]
fn exploring_onto_a_nonland_gives_a_counter_and_may_mill_it() {
    let mut t = Table::default();
    let e = explorer(&mut t);
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(e, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    g.act(
        Action::Cast { object: c },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    let on = g.find(e).unwrap();
    assert_eq!(g.pt(on), (2, 2));
    assert_eq!(g.count(Zone::Library, P0), library - 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        1,
        "the revealed bear was milled"
    );
}

#[test]
fn exploring_onto_a_land_puts_it_in_hand() {
    let mut t = Table::default();
    let e = explorer(&mut t);
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(e, P0, Zone::Hand);
    g.main();
    // Put a land on top of the library.
    let land = g.put(forest, P0, Zone::Library);
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|id| *id != land);
    order.insert(0, land);
    g.cast(c, &[]);
    let on = g.find(e).unwrap();
    assert_eq!(g.pt(on), (1, 1), "no counter");
    let in_hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == forest);
    assert!(in_hand);
}

#[test]
fn a_control_aura_steals_a_creature_that_cannot_attack_until_next_turn() {
    let mut t = Table::default();
    let mind = t.card(
        "{3}{U}{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nYou control enchanted creature.",
    );
    let bear = t.bear();
    let disenchant = t.card("{W}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    g.lands(6);
    let b = g.put(bear, P1, Zone::Battlefield);
    let aura = g.put(mind, P0, Zone::Hand);
    let d = g.put(disenchant, P0, Zone::Hand);
    g.main();
    g.cast(aura, &[mtg_core::Target::Object(b)]);
    let controller = |g: &Game| mtg_engine::layers::controller(&g.engine.state, b);
    assert_eq!(controller(&g), Some(P0));
    assert!(
        mtg_engine::layers::summoning_sick(&g.engine.state, b),
        "stolen this turn: it can't attack yet"
    );
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(controller(&g), Some(P0));
    assert!(!mtg_engine::layers::summoning_sick(&g.engine.state, b));
    let on = g.find(mind).unwrap();
    g.cast(d, &[mtg_core::Target::Object(on)]);
    assert_eq!(
        controller(&g),
        Some(P1),
        "control returns with the Aura gone"
    );
}

fn shock(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "",
        "Land — Mountain Forest",
        None,
        "As this land enters, you may pay 2 life. If you don't, it enters tapped.",
    )
}

#[test]
fn a_shock_land_played_by_hand_asks_to_pay_life() {
    for pay in [true, false] {
        let mut t = Table::default();
        let s = shock(&mut t);
        let mut g = Game::new(t);
        let land = g.put(s, P0, Zone::Hand);
        g.main();
        g.act(
            Action::PlayLand { object: land },
            &[],
            &[mtg_engine::choice::Answer::Bool(pay)],
        );
        let on = g.find(s).unwrap();
        assert_eq!(g.engine.state.objects[&on].tapped, !pay);
        assert_eq!(g.life(P0), if pay { 18 } else { 20 });
    }
}

#[test]
fn a_shock_land_put_onto_the_battlefield_by_an_effect_asks_too() {
    let mut t = Table::default();
    let s = shock(&mut t);
    let fetch = t.card(
        "{G}",
        "Sorcery",
        None,
        "Search your library for a land card, put it onto the battlefield, then shuffle.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let f = g.put(fetch, P0, Zone::Hand);
    let land = g.put(s, P0, Zone::Library);
    g.main();
    g.act(
        Action::Cast { object: f },
        &[],
        &[
            mtg_engine::choice::Answer::Objects(vec![land]),
            mtg_engine::choice::Answer::Bool(true),
        ],
    );
    let on = g.find(s).expect("fetched");
    assert!(!g.engine.state.objects[&on].tapped);
    assert_eq!(g.life(P0), 18);
}

#[test]
fn modular_moves_its_counters_when_it_dies() {
    let mut t = Table::default();
    let worker = t.card("{2}", "Artifact Creature", Some((0, 0)), "Modular 2");
    let golem = t.card("{3}", "Artifact Creature", Some((2, 2)), "");
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    g.lands(3);
    let w = g.put(worker, P0, Zone::Hand);
    let gol = g.put(golem, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let on = g.find(worker).unwrap();
    assert_eq!(g.pt(on), (2, 2), "entered with two counters");
    g.act(
        Action::Cast { object: s },
        &[mtg_core::Target::Object(on), mtg_core::Target::Object(gol)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert!(g.find(worker).is_none());
    assert_eq!(g.pt(gol), (4, 4), "its two counters moved");
}

#[test]
fn fading_counts_down_then_sacrifices() {
    let mut t = Table::default();
    let spirit = t.card("{1}{G}", "Creature — Spirit", Some((3, 3)), "Fading 1");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(spirit, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(g.find(spirit).is_some(), "removed its one fade counter");
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(g.find(spirit).is_none(), "none left to remove: sacrificed");
}

#[test]
fn cumulative_upkeep_costs_more_each_turn() {
    let mut t = Table::default();
    let wall = t.card("", "Creature — Wall", Some((0, 5)), "Cumulative upkeep {1}");
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(wall, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    // First upkeep: one age counter, {1} paid with the one land.
    next_main(&mut g, P0, true);
    assert!(g.find(wall).is_some(), "paid {{1}}");
    // Second upkeep: two age counters, {2} — one land can't pay it.
    next_main(&mut g, P0, true);
    assert!(g.find(wall).is_none(), "couldn't pay {{2}}: sacrificed");
}

#[test]
fn evolve_grows_only_for_a_bigger_creature() {
    let mut t = Table::default();
    let evolver = t.card("{G}", "Creature — Elf", Some((1, 1)), "Evolve");
    let small = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let e = g.put(evolver, P0, Zone::Battlefield);
    let s = g.put(small, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.pt(e), (1, 1), "a 1/1 is not bigger");
    g.cast(b, &[]);
    assert_eq!(g.pt(e), (2, 2), "a 2/2 is");
}

#[test]
fn bloodthirst_counts_only_after_an_opponent_was_hurt() {
    for hurt in [false, true] {
        let mut t = Table::default();
        let brute = t.card("{2}{R}", "Creature — Goblin", Some((2, 2)), "Bloodthirst 2");
        let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
        let mut g = Game::new(t);
        g.lands(4);
        let b = g.put(brute, P0, Zone::Hand);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        if hurt {
            g.cast(s, &[mtg_core::Target::Player(P1)]);
        }
        g.cast(b, &[]);
        let on = g.find(brute).unwrap();
        assert_eq!(g.pt(on), if hurt { (4, 4) } else { (2, 2) });
    }
}

#[test]
fn fabricate_chooses_counters_or_servos_as_it_resolves() {
    for counters in [true, false] {
        let mut t = Table::default();
        let smith = t.card("{2}", "Artifact Creature", Some((1, 1)), "Fabricate 2");
        let mut g = Game::new(t);
        g.lands(2);
        let s = g.put(smith, P0, Zone::Hand);
        g.main();
        let before = g.engine.state.battlefield().len();
        g.act(
            Action::Cast { object: s },
            &[],
            &[mtg_engine::choice::Answer::Modes(vec![if counters {
                0
            } else {
                1
            }])],
        );
        let on = g.find(smith).unwrap();
        if counters {
            assert_eq!(g.pt(on), (3, 3));
            assert_eq!(g.engine.state.battlefield().len(), before + 1);
        } else {
            assert_eq!(g.pt(on), (1, 1));
            assert_eq!(
                g.engine.state.battlefield().len(),
                before + 3,
                "it and two Servos"
            );
        }
    }
}

#[test]
fn living_weapon_equips_a_fresh_germ() {
    let mut t = Table::default();
    let blade = t.card(
        "{3}",
        "Artifact — Equipment",
        None,
        "Living weapon\nEquipped creature gets +2/+2.\nEquip {2}",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(blade, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    let eq = g.find(blade).unwrap();
    let germ = g.engine.state.objects[&eq]
        .attached_to
        .expect("attached to the Germ");
    assert_eq!(g.pt(germ), (2, 2), "a 0/0 Germ with +2/+2");
}

#[test]
fn rebound_casts_the_spell_again_next_upkeep() {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut t = Table::default();
    let drain = t.card(
        "{2}{B}",
        "Sorcery",
        None,
        "Rebound\nTarget player loses 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let d = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(d, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        0,
        "exiled, not in the graveyard"
    );
    // On to P0's next main phase, casting the rebound copy at P1 when asked.
    let start = g.engine.state.turn;
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && g.engine.state.turn > start
                    && g.engine.state.active_player == P0
                    && g.engine.state.step == mtg_core::Step::PrecombatMain
                    && g.stack().is_empty()
                {
                    break;
                }
                let a = match c.kind {
                    ChoiceKind::Confirm => Answer::Bool(true),
                    ChoiceKind::ChooseTargets { .. } => {
                        Answer::Targets(vec![vec![mtg_core::Target::Player(P1)]])
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
        }
    }
    assert_eq!(g.life(P1), 16, "cast again from exile");
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        1,
        "and then it went to the graveyard"
    );
}

#[test]
fn madness_casts_a_discarded_card_for_its_madness_cost() {
    for cast in [true, false] {
        let mut t = Table::default();
        let fiend = t.card("{3}{R}", "Creature — Goblin", Some((3, 3)), "Madness {R}");
        let loot = t.card("{U}", "Sorcery", None, "Draw a card, then discard a card.");
        let mut g = Game::new(t);
        g.lands(2);
        let f = g.put(fiend, P0, Zone::Hand);
        let l = g.put(loot, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: l },
            &[],
            &[
                mtg_engine::choice::Answer::Objects(vec![f]),
                mtg_engine::choice::Answer::Bool(cast),
            ],
        );
        assert_eq!(
            g.find(fiend).is_some(),
            cast,
            "cast for {{R}} or not at all"
        );
        let in_graveyard = g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Graveyard, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == fiend);
        assert_eq!(in_graveyard, !cast, "declined: to the graveyard");
    }
}

#[test]
fn a_morph_is_a_hidden_vanilla_two_two_until_turned_face_up() {
    for mega in [false, true] {
        let mut t = Table::default();
        let text = if mega {
            "Flying\nMegamorph {2}{U}"
        } else {
            "Flying\nMorph {2}{U}"
        };
        let drake = t.card("{4}{U}", "Creature — Wizard", Some((4, 4)), text);
        let mut g = Game::new(t);
        g.lands(6);
        let d = g.put(drake, P0, Zone::Hand);
        let actions = g.main();
        assert!(actions.contains(&Action::CastFaceDown { object: d }));
        g.act(Action::CastFaceDown { object: d }, &[], &[]);
        let on = *g
            .engine
            .state
            .battlefield()
            .iter()
            .find(|id| g.engine.state.objects[id].face_down)
            .expect("face down on the battlefield");
        assert_eq!(g.pt(on), (2, 2));
        assert!(
            !g.has(on, mtg_core::Keyword::Flying),
            "no abilities face down"
        );
        let foe_view = mtg_engine::view::project(&g.engine.state, P1);
        assert_eq!(foe_view.visible[&on].card, None, "hidden from the opponent");
        let lands_before = g
            .engine
            .state
            .battlefield()
            .iter()
            .filter(|id| g.engine.state.objects[id].tapped)
            .count();
        assert_eq!(lands_before, 3, "cast for {{3}}");

        let actions = g.main();
        let up = actions
            .iter()
            .find(|a| matches!(a, Action::SpecialAction { source, .. } if *source == on))
            .cloned()
            .expect("can turn it face up");
        g.act(up, &[], &[]);
        assert!(!g.engine.state.objects[&on].face_down);
        assert_eq!(g.pt(on), if mega { (5, 5) } else { (4, 4) });
        assert!(g.has(on, mtg_core::Keyword::Flying));
    }
}

#[test]
fn a_face_down_creature_has_no_dies_trigger() {
    let mut t = Table::default();
    let seer = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "When this creature dies, draw a card.\nMorph {U}",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(4);
    let s = g.put(seer, P0, Zone::Hand);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.act(Action::CastFaceDown { object: s }, &[], &[]);
    let on = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].face_down)
        .unwrap();
    let hand = g.count(Zone::Hand, P0);
    g.cast(b, &[mtg_core::Target::Object(on)]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand - 1,
        "cast the bolt; nothing drawn"
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
}

fn ring(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{2}{W}",
        "Enchantment",
        None,
        "When this enchantment enters, exile target nonland permanent an opponent controls until this enchantment leaves the battlefield.",
    )
}

#[test]
fn an_exiled_permanent_returns_when_the_ring_leaves() {
    let mut t = Table::default();
    let r = ring(&mut t);
    let bear = t.bear();
    let dis = t.card("{W}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P1, Zone::Battlefield);
    let ring = g.put(r, P0, Zone::Hand);
    let d = g.put(dis, P0, Zone::Hand);
    g.main();
    g.cast(ring, &[mtg_core::Target::Object(b)]);
    assert!(g.find(bear).is_none(), "exiled");
    let on = g.find(r).unwrap();
    g.cast(d, &[mtg_core::Target::Object(on)]);
    let back = g.find(bear).expect("returned");
    assert_eq!(
        g.engine.state.objects[&back].controller, P1,
        "under its owner's control"
    );
}

#[test]
fn nothing_is_exiled_if_the_ring_left_first() {
    let mut t = Table::default();
    let r = ring(&mut t);
    let bear = t.bear();
    let dis = t.card("{W}", "Instant", None, "Destroy target enchantment.");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P1, Zone::Battlefield);
    let ring = g.put(r, P0, Zone::Hand);
    let d = g.put(dis, P0, Zone::Hand);
    g.main();
    // The ring resolves; hold priority with its trigger on the stack and destroy it.
    g.act_holding(Action::Cast { object: ring }, &[]);
    g.act_holding(Action::Pass, &[mtg_core::Target::Object(b)]);
    let on = g.find(r).expect("the ring is on the battlefield");
    assert_eq!(g.stack().len(), 1, "its trigger is waiting on the stack");
    g.act(
        Action::Cast { object: d },
        &[mtg_core::Target::Object(on)],
        &[],
    );
    assert!(g.find(bear).is_some(), "the trigger did nothing");
}

#[test]
fn two_targets_are_two_distinct_slots() {
    let mut t = Table::default();
    let raise = t.card(
        "{2}{B}",
        "Sorcery",
        None,
        "Return up to two target creature cards from your graveyard to your hand.",
    );
    let bear = t.bear();
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let a = g.put(bear, P0, Zone::Graveyard);
    let b = g.put(elf, P0, Zone::Graveyard);
    let s = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(
        s,
        &[mtg_core::Target::Object(a), mtg_core::Target::Object(b)],
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the spell is left");
    let hand: Vec<_> = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert!(hand.contains(&bear) && hand.contains(&elf));
}

#[test]
fn instant_and_sorcery_spells_are_both_discounted() {
    let mut t = Table::default();
    let adept = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Instant and sorcery spells you cast cost {1} less to cast.",
    );
    let think = t.card("{1}{U}", "Instant", None, "Draw a card.");
    let ponder = t.card("{1}{U}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(adept, P0, Zone::Battlefield);
    let i = g.put(think, P0, Zone::Hand);
    let s = g.put(ponder, P0, Zone::Hand);
    g.main();
    g.cast(i, &[]);
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Graveyard, P0), 2, "each cost {{U}}");
}

#[test]
fn an_upkeep_cost_or_the_creature_goes() {
    let mut t = Table::default();
    let beast = t.card(
        "{G}",
        "Creature — Beast",
        Some((4, 4)),
        "At the beginning of your upkeep, sacrifice this creature unless you pay {G}.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(beast, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    next_main(&mut g, P0, true);
    assert!(g.find(beast).is_some(), "paid");
    next_main(&mut g, P0, false);
    assert!(g.find(beast).is_none(), "not paid: sacrificed");
}

#[test]
fn a_permanent_goes_to_the_top_or_bottom_of_its_owners_library() {
    for top in [true, false] {
        let mut t = Table::default();
        let text = if top {
            "Put target creature on top of its owner's library."
        } else {
            "Put target creature on the bottom of its owner's library."
        };
        let spell = t.card("{U}", "Sorcery", None, text);
        let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let e = g.put(elf, P1, Zone::Battlefield);
        let s = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(s, &[mtg_core::Target::Object(e)]);
        let lib = g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Library, P1));
        let at = if top { lib[0] } else { *lib.last().unwrap() };
        assert_eq!(g.engine.state.objects[&at].card, elf);
    }
}

#[test]
fn damage_to_each_creature_without_flying() {
    let mut t = Table::default();
    let quake = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to each creature without flying.",
    );
    let bird = t.card("{W}", "Creature — Bird", Some((1, 1)), "Flying");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    g.put(bird, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let q = g.put(quake, P0, Zone::Hand);
    g.main();
    g.cast(q, &[]);
    assert!(g.find(bird).is_some());
    assert!(g.find(bear).is_none());
}

#[test]
fn power_and_toughness_equal_to_a_count() {
    let mut t = Table::default();
    let crowd = t.card(
        "{2}{G}",
        "Creature — Elf",
        None,
        "~'s power and toughness are each equal to the number of creatures you control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(crowd, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    assert_eq!(g.pt(c), (3, 3), "itself and two bears; not the opponent's");
}

#[test]
fn no_maximum_hand_size_skips_the_cleanup_discard() {
    for unlimited in [false, true] {
        let mut t = Table::default();
        let sage = t.card(
            "{2}{U}",
            "Creature — Wizard",
            Some((1, 1)),
            if unlimited {
                "You have no maximum hand size."
            } else {
                ""
            },
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.put(sage, P0, Zone::Battlefield);
        for _ in 0..9 {
            g.put(bear, P0, Zone::Hand);
        }
        g.main();
        g.until(P1, mtg_core::Step::PrecombatMain);
        // Nine, plus the card drawn for the turn; seven without the ability.
        assert_eq!(g.count(Zone::Hand, P0), if unlimited { 10 } else { 7 });
    }
}

/// Play on to `who`'s next main phase, saying yes to every question and aiming every
/// target at `at`.
fn next_main_aiming(g: &mut Game, who: mtg_core::PlayerId, at: mtg_core::Target) {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let start = g.engine.state.turn;
    loop {
        match g.engine.advance(&g.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game over"),
            Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && g.engine.state.turn > start
                    && g.engine.state.active_player == who
                    && g.engine.state.step == mtg_core::Step::PrecombatMain
                    && g.stack().is_empty()
                {
                    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
                    return;
                }
                let a = match c.kind {
                    ChoiceKind::Confirm => Answer::Bool(true),
                    ChoiceKind::ChooseTargets { .. } => Answer::Targets(vec![vec![at]]),
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
        }
    }
}

#[test]
fn suspend_counts_down_then_casts_for_free() {
    let mut t = Table::default();
    let rift = t.card(
        "{2}{R}",
        "Sorcery",
        None,
        "Suspend 2—{R}\n~ deals 3 damage to any target.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(rift, P0, Zone::Hand);
    let actions = g.main();
    let suspend = actions
        .iter()
        .find(|a| matches!(a, Action::SpecialAction { source, .. } if *source == r))
        .cloned()
        .expect("suspend is offered");
    g.act(suspend, &[], &[]);
    let exiled = |g: &Game| {
        g.engine
            .state
            .objects
            .values()
            .find(|o| o.card == rift && o.zone.zone == Zone::Exile)
            .map(|o| o.counters.values().sum::<i32>())
    };
    assert_eq!(exiled(&g), Some(2));
    next_main_aiming(&mut g, P0, mtg_core::Target::Player(P1));
    assert_eq!(exiled(&g), Some(1));
    assert_eq!(g.life(P1), 20);
    next_main_aiming(&mut g, P0, mtg_core::Target::Player(P1));
    assert_eq!(g.life(P1), 17, "the last counter came off and it was cast");
    assert_eq!(exiled(&g), None);
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_suspended_creature_arrives_with_haste() {
    let mut t = Table::default();
    let sliver = t.card("{4}{R}", "Creature — Goblin", Some((3, 3)), "Suspend 1—{R}");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(sliver, P0, Zone::Hand);
    let actions = g.main();
    let suspend = actions
        .iter()
        .find(|a| matches!(a, Action::SpecialAction { source, .. } if *source == s))
        .cloned()
        .unwrap();
    g.act(suspend, &[], &[]);
    next_main_aiming(&mut g, P0, mtg_core::Target::Player(P1));
    let on = g.find(sliver).expect("cast on the upkeep");
    assert!(g.has(on, mtg_core::Keyword::Haste));
}

fn alternative(actions: &[Action], object: mtg_core::ObjectId) -> Action {
    actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object: o, .. } if *o == object))
        .cloned()
        .expect("an alternative cost is offered")
}

#[test]
fn dash_attacks_with_haste_and_returns_to_hand() {
    let mut t = Table::default();
    let raider = t.card("{3}{R}", "Creature — Goblin", Some((3, 2)), "Dash {1}{R}");
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(raider, P0, Zone::Hand);
    let actions = g.main();
    g.act(alternative(&actions, r), &[], &[]);
    let on = g.find(raider).expect("dashed in");
    assert!(g.has(on, mtg_core::Keyword::Haste));
    g.combat(&[on], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert!(g.find(raider).is_none());
    let back = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == raider);
    assert!(back, "returned to its owner's hand");
}

#[test]
fn blitz_attacks_with_haste_then_is_sacrificed_and_draws() {
    let mut t = Table::default();
    let raider = t.card("{3}{R}", "Creature — Goblin", Some((3, 2)), "Blitz {1}{R}");
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(raider, P0, Zone::Hand);
    let actions = g.main();
    g.act(alternative(&actions, r), &[], &[]);
    let on = g.find(raider).expect("blitzed in");
    assert!(g.has(on, mtg_core::Keyword::Haste));
    g.combat(&[on], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    let hand = g.count(Zone::Hand, P0);
    g.until(P1, mtg_core::Step::Upkeep);
    assert!(g.find(raider).is_none(), "sacrificed at the end step");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 1,
        "and its death drew a card"
    );
}

#[test]
fn surge_needs_another_spell_first_and_knows_it_was_paid() {
    let mut t = Table::default();
    let wave = t.card(
        "{4}{U}",
        "Sorcery",
        None,
        "Surge {U}\nYou gain 1 life. If this spell's surge cost was paid, you gain 3 life.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(wave, P0, Zone::Hand);
    let s = g.put(shock, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::CastAlternative { object, .. } if *object == w)),
        "no other spell yet"
    );
    g.cast(s, &[mtg_core::Target::Player(P1)]);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    g.act(alternative(&actions, w), &[], &[]);
    assert_eq!(g.life(P0), 24, "1 and the surge bonus 3");
}

#[test]
fn spectacle_once_an_opponent_has_lost_life() {
    let mut t = Table::default();
    let brute = t.card("{3}{R}", "Creature — Goblin", Some((4, 3)), "Spectacle {R}");
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(brute, P0, Zone::Hand);
    let s = g.put(shock, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::CastAlternative { object, .. } if *object == b))
    );
    g.cast(s, &[mtg_core::Target::Player(P1)]);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    g.act(alternative(&actions, b), &[], &[]);
    assert!(g.find(brute).is_some());
}

#[test]
fn awaken_also_animates_a_land_after_the_spells_own_targets() {
    let mut t = Table::default();
    let bolt = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to any target.\nAwaken 2—{2}{R}",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(bolt, P0, Zone::Hand);
    let land = g.engine.state.battlefield()[0];
    let actions = g.main();
    g.act(
        alternative(&actions, s),
        &[mtg_core::Target::Player(P1), mtg_core::Target::Object(land)],
        &[],
    );
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.pt(land), (2, 2), "a 0/0 with two counters");
    assert!(g.has(land, mtg_core::Keyword::Haste));
    let c = mtg_engine::layers::compute(&g.engine.state, &g.table, land).unwrap();
    assert!(c.has_type(mtg_core::CardType::Creature) && c.has_type(mtg_core::CardType::Land));
}

#[test]
fn cast_normally_an_awaken_spell_animates_nothing() {
    let mut t = Table::default();
    let bolt = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to any target.\nAwaken 2—{2}{R}",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(bolt, P0, Zone::Hand);
    let land = g.engine.state.battlefield()[0];
    g.main();
    g.cast(s, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    let c = mtg_engine::layers::compute(&g.engine.state, &g.table, land).unwrap();
    assert!(!c.has_type(mtg_core::CardType::Creature));
}

#[test]
fn evoke_gets_the_enters_trigger_then_sacrifices() {
    let mut t = Table::default();
    let elemental = t.card(
        "{3}{U}{U}",
        "Creature — Elemental",
        Some((3, 3)),
        "When this creature enters, draw two cards.\nEvoke {1}{U}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let e = g.put(elemental, P0, Zone::Hand);
    let actions = g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(alternative(&actions, e), &[], &[]);
    assert!(g.find(elemental).is_none(), "sacrificed");
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2, "drew two");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn split_second_leaves_nothing_to_respond_with() {
    use mtg_engine::{Progress, choice::ChoiceKind};
    let mut t = Table::default();
    let sudden = t.card(
        "{2}{R}",
        "Instant",
        None,
        "Split second\n~ deals 2 damage to any target.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(4);
    let s = g.put(sudden, P0, Zone::Hand);
    let k = g.put(shock, P0, Zone::Hand);
    let before = g.main();
    assert!(before.contains(&Action::Cast { object: k }));
    g.act_holding(Action::Cast { object: s }, &[mtg_core::Target::Player(P1)]);
    let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!("a priority prompt")
    };
    let ChoiceKind::Priority { legal } = c.kind else {
        panic!("priority")
    };
    assert!(
        !legal.actions.contains(&Action::Cast { object: k }),
        "can't cast with a split second spell on the stack"
    );
}

#[test]
fn flanking_shrinks_a_blocker_without_flanking() {
    for blocker_flanks in [false, true] {
        let mut t = Table::default();
        let knight = t.card("{1}{W}", "Creature — Soldier", Some((2, 2)), "Flanking");
        let guard = t.card(
            "{1}{W}",
            "Creature — Soldier",
            Some((2, 2)),
            if blocker_flanks { "Flanking" } else { "" },
        );
        let mut g = Game::new(t);
        let k = g.put(knight, P0, Zone::Battlefield);
        let b = g.put(guard, P1, Zone::Battlefield);
        g.main();
        g.combat(&[k], &[(b, k)], &[], &[]);
        if blocker_flanks {
            assert!(
                g.find(knight).is_none() && g.find(guard).is_none(),
                "a fair trade"
            );
        } else {
            assert!(g.find(knight).is_some(), "the 1/1 blocker dealt only 1");
            assert!(g.find(guard).is_none());
        }
    }
}

#[test]
fn unleash_trades_blocking_for_a_counter() {
    for counter in [true, false] {
        let mut t = Table::default();
        let brute = t.card("{1}{R}", "Creature — Goblin", Some((2, 2)), "Unleash");
        let mut g = Game::new(t);
        g.lands(2);
        let b = g.put(brute, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: b },
            &[],
            &[mtg_engine::choice::Answer::Bool(counter)],
        );
        let on = g.find(brute).unwrap();
        assert_eq!(g.pt(on), if counter { (3, 3) } else { (2, 2) });
        let restricted = mtg_engine::layers::restricted(&g.engine.state, &g.table, on, |r| {
            matches!(r, mtg_ir::effect::Restriction::CantBlock)
        });
        assert_eq!(restricted, counter, "can't block only with the counter");
    }
}

#[test]
fn riot_chooses_a_counter_or_haste() {
    for counter in [true, false] {
        let mut t = Table::default();
        let rioter = t.card("{1}{R}", "Creature — Goblin", Some((2, 2)), "Riot");
        let mut g = Game::new(t);
        g.lands(2);
        let r = g.put(rioter, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: r },
            &[],
            &[mtg_engine::choice::Answer::Bool(counter)],
        );
        let on = g.find(rioter).unwrap();
        assert_eq!(g.pt(on), if counter { (3, 3) } else { (2, 2) });
        assert_eq!(g.has(on, mtg_core::Keyword::Haste), !counter);
    }
}

#[test]
fn renown_grows_once() {
    let mut t = Table::default();
    let hero = t.card("{1}{W}", "Creature — Soldier", Some((2, 2)), "Renown 1");
    let mut g = Game::new(t);
    let h = g.put(hero, P0, Zone::Battlefield);
    g.main();
    g.combat(&[h], &[], &[], &[]);
    assert_eq!(g.pt(h), (3, 3), "renowned");
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    g.combat(&[h], &[], &[], &[]);
    assert_eq!(g.pt(h), (3, 3), "already renowned: no second counter");
    assert_eq!(g.life(P1), 20 - 2 - 3);
}

#[test]
fn extort_drains_when_paid() {
    let mut t = Table::default();
    let priest = t.card("{1}{B}", "Creature — Cleric", Some((1, 1)), "Extort");
    let thought = t.card("{U}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(priest, P0, Zone::Battlefield);
    let s = g.put(thought, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn itself_or_another_ally_entering() {
    let mut t = Table::default();
    let ally = t.card(
        "{1}{W}",
        "Creature — Ally",
        Some((1, 1)),
        "Whenever this creature or another Ally you control enters, you may put a +1/+1 counter on this creature.",
    );
    let friend = t.card("{W}", "Creature — Ally", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let a = g.put(ally, P0, Zone::Hand);
    let f = g.put(friend, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    let yes = [mtg_engine::choice::Answer::Bool(true)];
    g.main();
    g.act(Action::Cast { object: a }, &[], &yes);
    let on = g.find(ally).unwrap();
    assert_eq!(g.pt(on), (2, 2), "itself entering");
    g.act(Action::Cast { object: f }, &[], &yes);
    assert_eq!(g.pt(on), (3, 3), "another Ally");
    g.act(Action::Cast { object: b }, &[], &yes);
    assert_eq!(g.pt(on), (3, 3), "not a bear");
}

#[test]
fn search_for_a_cheap_subtype_permanent() {
    let mut t = Table::default();
    let boss = t.card(
        "{2}{R}",
        "Creature — Mercenary",
        Some((2, 2)),
        "{1}, {T}: Search your library for a Mercenary permanent card with mana value 1 or less, put it onto the battlefield, then shuffle.",
    );
    let cheap = t.card("{R}", "Creature — Mercenary", Some((1, 1)), "");
    let pricey = t.card("{2}{R}", "Creature — Mercenary", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let bo = g.put(boss, P0, Zone::Battlefield);
    let c = g.put(cheap, P0, Zone::Library);
    g.put(pricey, P0, Zone::Library);
    g.main();
    g.act(
        activate(bo, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![c])],
    );
    assert!(g.find(cheap).is_some());
    assert!(g.find(pricey).is_none());
}

#[test]
fn kicked_it_enters_with_counters() {
    for kick in [true, false] {
        let mut t = Table::default();
        let elf = t.card(
            "{G}",
            "Creature — Elf",
            Some((1, 1)),
            "Kicker {1}\nIf this creature was kicked, it enters with two +1/+1 counters on it.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let e = g.put(elf, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: e },
            &[],
            &[mtg_engine::choice::Answer::Bool(kick)],
        );
        let on = g.find(elf).unwrap();
        assert_eq!(g.pt(on), if kick { (3, 3) } else { (1, 1) });
    }
}

#[test]
fn conniving_grows_only_for_a_nonland_discard() {
    for nonland in [true, false] {
        let mut t = Table::default();
        let rogue = t.card(
            "{1}{U}",
            "Creature — Wizard",
            Some((1, 1)),
            "When this creature enters, it connives.",
        );
        let forest = t.card("", "Basic Land — Forest", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let r = g.put(rogue, P0, Zone::Hand);
        let pitch = g.put(if nonland { bear } else { forest }, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: r },
            &[],
            &[mtg_engine::choice::Answer::Objects(vec![pitch])],
        );
        let on = g.find(rogue).unwrap();
        assert_eq!(g.pt(on), if nonland { (2, 2) } else { (1, 1) });
        assert_eq!(g.count(Zone::Graveyard, P0), 1, "discarded one");
    }
}

#[test]
fn printed_auras_with_combined_suppression_compile_completely() {
    let mut t = Table::default();
    for text in [
        "Enchant artifact or creature\nEnchanted permanent doesn't untap during its controller's untap step and its activated abilities can't be activated.",
        "Flash\nEnchant artifact or creature\nWhen this Aura enters, tap enchanted permanent.\nEnchanted permanent doesn't untap during its controller's untap step and its activated abilities can't be activated.",
        "Enchant creature or planeswalker\nWhen this Aura enters, tap enchanted permanent and investigate.\nEnchanted permanent doesn't untap during its controller's untap step and its activated abilities can't be activated.",
    ] {
        t.card("{2}{U}", "Enchantment — Aura", None, text);
    }
}

#[test]
fn aura_combines_untap_and_activated_ability_suppression() {
    use mtg_core::{Step, Target};
    for comma in [false, true] {
        let mut t = Table::default();
        let aura = t.card(
            "{U}{U}{U}",
            "Enchantment — Aura",
            None,
            if comma {
                "Enchant artifact or creature\nEnchanted permanent doesn't untap during its controller's untap step, and its activated abilities can't be activated."
            } else {
                "Enchant artifact or creature\nEnchanted permanent doesn't untap during its controller's untap step and its activated abilities can't be activated."
            },
        );
        let artifact = t.card(
            "{1}",
            "Artifact",
            None,
            "{T}: Add {C}.\n{0}: You gain 1 life.",
        );
        let destroy = t.card("{W}", "Instant", None, "Destroy target enchantment.");
        let mut g = Game::new(t);
        g.lands(4);
        let source = g.put(artifact, P0, Zone::Battlefield);
        let other = g.put(artifact, P0, Zone::Battlefield);
        let aura_spell = g.put(aura, P0, Zone::Hand);
        let destroy_spell = g.put(destroy, P0, Zone::Hand);
        let mana_offered = |g: &Game, id| {
            let mtg_engine::choice::ChoiceKind::Priority { legal } =
                &g.pending.as_ref().unwrap().kind
            else {
                panic!("priority choice")
            };
            legal
                .mana_abilities
                .iter()
                .any(|a| matches!(a, Action::ActivateManaAbility { source, .. } if *source == id))
        };
        let actions = g.main();
        assert!(offers(&actions, source));
        assert!(mana_offered(&g, source));
        g.cast(aura_spell, &[Target::Object(source)]);
        let actions = g.main();
        assert!(!offers(&actions, source));
        assert!(!mana_offered(&g, source));
        assert!(offers(&actions, other));
        assert!(mana_offered(&g, other));
        g.engine.state.objects.get_mut(&source).unwrap().tapped = true;
        g.engine.state.objects.get_mut(&other).unwrap().tapped = true;
        g.until(P1, Step::PrecombatMain);
        g.main();
        assert!(g.engine.state.objects[&source].tapped);
        assert!(!g.engine.state.objects[&other].tapped);
        let attached = g.find(aura).unwrap();
        g.cast(destroy_spell, &[Target::Object(attached)]);
        let actions = g.main();
        assert!(offers(&actions, source));
        g.until(P1, Step::PrecombatMain);
        g.main();
        assert!(!g.engine.state.objects[&source].tapped);
        assert!(mana_offered(&g, source));
    }
}

#[test]
fn a_frozen_creature_skips_one_untap_step() {
    let mut t = Table::default();
    let frost = t.card(
        "{U}",
        "Sorcery",
        None,
        "Tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let f = g.put(frost, P0, Zone::Hand);
    g.main();
    g.cast(f, &[mtg_core::Target::Object(b)]);
    assert!(g.engine.state.objects[&b].tapped);
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert!(
        g.engine.state.objects[&b].tapped,
        "skipped its next untap step"
    );
    g.until(P0, mtg_core::Step::PrecombatMain);
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert!(
        !g.engine.state.objects[&b].tapped,
        "and untaps the one after"
    );
}

#[test]
fn a_serpent_attacks_only_into_an_island() {
    for island in [false, true] {
        let mut t = Table::default();
        let serpent = t.card(
            "{5}{U}",
            "Creature — Wizard",
            Some((5, 5)),
            "This creature can't attack unless defending player controls an Island.",
        );
        let isl = t.card("", "Basic Land — Island", None, "");
        let mut g = Game::new(t);
        let s = g.put(serpent, P0, Zone::Battlefield);
        if island {
            g.put(isl, P1, Zone::Battlefield);
        }
        g.main();
        let eligible = mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0);
        assert_eq!(eligible.contains(&s), island);
    }
}

#[test]
fn only_one_creature_may_block_it() {
    let mut t = Table::default();
    let rogue = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "This creature can't be blocked by more than one creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let r = g.put(rogue, P0, Zone::Battlefield);
    let a = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let check = |blockers: Vec<mtg_core::ObjectId>| {
        let blocks = [(r, blockers)].into_iter().collect();
        mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P1, &blocks)
    };
    assert!(check(vec![a]).is_ok());
    assert!(check(vec![a, b]).is_err());
}

#[test]
fn ninjutsu_swaps_in_for_an_unblocked_attacker() {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut t = Table::default();
    let ninja = t.card(
        "{3}{U}",
        "Creature — Wizard",
        Some((3, 3)),
        "Ninjutsu {1}{U}",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let n = g.put(ninja, P0, Zone::Hand);
    g.main();
    let mut activated = false;
    loop {
        let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
            continue;
        };
        let step = g.engine.state.step;
        let a = match &c.kind {
            ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![b]),
            ChoiceKind::DeclareBlockers { .. } => Answer::Blocks(vec![]),
            ChoiceKind::ChooseObjects { .. } => Answer::Objects(vec![b]),
            ChoiceKind::Priority { legal }
                if c.who == P0
                    && step == mtg_core::Step::DeclareBlockers
                    && !activated
                    && g.stack().is_empty() =>
            {
                assert!(offers(&legal.actions, n), "ninjutsu is offered");
                activated = true;
                Answer::Action(activate(n, 0))
            }
            ChoiceKind::Priority { .. }
                if c.who == P0 && step == mtg_core::Step::PostcombatMain =>
            {
                break;
            }
            _ => c.default.clone().unwrap_or(Answer::Pass),
        };
        g.engine.answer(&g.table, c.id, a).unwrap();
    }
    let on = g.find(ninja).expect("the ninja is on the battlefield");
    assert!(g.engine.state.objects[&on].tapped);
    assert_eq!(g.life(P1), 17, "the ninja dealt the damage");
    let back = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == bear);
    assert!(back, "the bear went back to hand");
}

#[test]
fn threaten_borrows_a_creature_for_the_turn() {
    let mut t = Table::default();
    let treason = t.card(
        "{2}{R}",
        "Sorcery",
        None,
        "Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&b).unwrap().tapped = true;
    let s = g.put(treason, P0, Zone::Hand);
    g.main();
    g.cast(s, &[mtg_core::Target::Object(b)]);
    let ctrl = |g: &Game| mtg_engine::layers::controller(&g.engine.state, b);
    assert_eq!(ctrl(&g), Some(P0));
    assert!(!g.engine.state.objects[&b].tapped);
    g.combat(&[b], &[], &[], &[]);
    assert_eq!(g.life(P1), 18, "it attacked its owner");
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(ctrl(&g), Some(P1), "back at end of turn");
}

#[test]
fn switching_and_setting_power_and_toughness() {
    let mut t = Table::default();
    let twist = t.card(
        "{U}",
        "Instant",
        None,
        "Switch target creature's power and toughness until end of turn.",
    );
    let shrink = t.card(
        "{U}",
        "Instant",
        None,
        "Target creature has base power and toughness 1/1 until end of turn.",
    );
    let wall = t.card("{1}{W}", "Creature — Spirit", Some((0, 4)), "");
    let giant = t.card("{4}{G}", "Creature — Beast", Some((5, 5)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(wall, P0, Zone::Battlefield);
    let gi = g.put(giant, P1, Zone::Battlefield);
    let a = g.put(twist, P0, Zone::Hand);
    let b = g.put(shrink, P0, Zone::Hand);
    g.main();
    g.cast(b, &[mtg_core::Target::Object(gi)]);
    assert_eq!(g.pt(gi), (1, 1));
    g.cast(a, &[mtg_core::Target::Object(w)]);
    assert!(
        g.find(wall).is_none(),
        "0 toughness after the switch: it died"
    );
}

#[test]
fn a_draw_trigger_counts_each_card() {
    let mut t = Table::default();
    let sage = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever you draw a card, put a +1/+1 counter on this creature.",
    );
    let think = t.card("{U}", "Instant", None, "Draw two cards.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(sage, P0, Zone::Battlefield);
    let d = g.put(think, P0, Zone::Hand);
    g.main();
    let after_draw_step = g.pt(s);
    g.cast(d, &[]);
    assert_eq!(g.pt(s).0, after_draw_step.0 + 2);
}

#[test]
fn raid_counts_only_after_an_attack() {
    for attack in [false, true] {
        let mut t = Table::default();
        let raider = t.card(
            "{1}{R}",
            "Creature — Goblin",
            Some((2, 2)),
            "Raid — This creature enters with a +1/+1 counter on it if you attacked this turn.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let b = g.put(bear, P0, Zone::Battlefield);
        let r = g.put(raider, P0, Zone::Hand);
        g.main();
        let attackers: Vec<_> = if attack { vec![b] } else { Vec::new() };
        g.combat(&attackers, &[], &[], &[]);
        g.cast(r, &[]);
        let on = g.find(raider).unwrap();
        assert_eq!(g.pt(on), if attack { (3, 3) } else { (2, 2) });
    }
}

#[test]
fn a_state_trigger_sacrifices_without_an_island() {
    let mut t = Table::default();
    let serpent = t.card(
        "{U}",
        "Creature — Wizard",
        Some((4, 4)),
        "When you control no Islands, sacrifice this creature.",
    );
    let island = t.card("", "Basic Land — Island", None, "");
    let stone = t.card("{R}", "Instant", None, "Destroy target land.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(serpent, P0, Zone::Battlefield);
    let i = g.put(island, P0, Zone::Battlefield);
    let d = g.put(stone, P0, Zone::Hand);
    g.main();
    assert!(g.find(serpent).is_some(), "an Island: it stays");
    g.cast(d, &[mtg_core::Target::Object(i)]);
    assert!(g.find(serpent).is_none(), "no Islands: sacrificed");
    let _ = s;
}

#[test]
fn proliferate_adds_to_chosen_permanents() {
    let mut t = Table::default();
    let spread = t.card("{1}{G}", "Sorcery", None, "Proliferate.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    for id in [a, b] {
        g.engine
            .state
            .objects
            .get_mut(&id)
            .unwrap()
            .counters
            .insert(mtg_core::CounterKind::PlusOnePlusOne, 1);
    }
    let s = g.put(spread, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![a])],
    );
    assert_eq!(g.pt(a), (4, 4), "chosen: one more");
    assert_eq!(g.pt(b), (3, 3), "not chosen");
}

#[test]
fn you_choose_what_they_discard() {
    let mut t = Table::default();
    let seize = t.card(
        "{B}",
        "Sorcery",
        None,
        "Target opponent reveals their hand. You choose a nonland card from it. That player discards that card.",
    );
    let bear = t.bear();
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(seize, P0, Zone::Hand);
    g.main();
    // P1's hand: drawn bears plus these two.
    let target = g.put(bear, P1, Zone::Hand);
    g.put(forest, P1, Zone::Hand);
    g.act(
        Action::Cast { object: s },
        &[mtg_core::Target::Player(P1)],
        &[mtg_engine::choice::Answer::Objects(vec![target])],
    );
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    let gone = !g.engine.state.objects.contains_key(&target);
    assert!(gone, "the chosen bear was discarded");
}

#[test]
fn each_creature_takes_a_singular_verb() {
    let mut t = Table::default();
    let captain = t.card(
        "{2}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Each other creature you control gets +1/+1.\nEach creature you control with a +1/+1 counter on it has trample.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(captain, P0, Zone::Battlefield);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&b)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 1);
    assert_eq!(g.pt(c), (2, 2));
    assert_eq!(g.pt(a), (3, 3));
    assert!(!g.has(a, mtg_core::Keyword::Trample));
    assert!(g.has(b, mtg_core::Keyword::Trample));
}

#[test]
fn an_additional_land_each_turn() {
    let mut t = Table::default();
    let explorer = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 2)),
        "You may play an additional land on each of your turns.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.put(explorer, P0, Zone::Battlefield);
    let a = g.put(forest, P0, Zone::Hand);
    let b = g.put(forest, P0, Zone::Hand);
    let c = g.put(forest, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: a }, &[], &[]);
    let actions = g.main();
    assert!(
        actions.contains(&Action::PlayLand { object: b }),
        "a second land"
    );
    g.act(Action::PlayLand { object: b }, &[], &[]);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::PlayLand { object: c }),
        "not a third"
    );
}

#[test]
fn choose_one_or_both() {
    for picked in [vec![0u8], vec![0, 1]] {
        let mut t = Table::default();
        let charm = t.card(
            "{G}{U}",
            "Instant",
            None,
            "Choose one or both —\n• You gain 3 life.\n• Draw a card.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let c = g.put(charm, P0, Zone::Hand);
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.act(
            Action::Cast { object: c },
            &[],
            &[mtg_engine::choice::Answer::Modes(picked.clone())],
        );
        assert_eq!(g.life(P0), 23);
        let drew = picked.len() == 2;
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + usize::from(drew));
    }
}

#[test]
fn monstrosity_happens_once() {
    let mut t = Table::default();
    let hydra = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "{1}: Monstrosity 2.\nWhen this creature becomes monstrous, it deals 2 damage to each opponent.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(hydra, P0, Zone::Battlefield);
    g.main();
    g.act(activate(h, 0), &[], &[]);
    assert_eq!(g.pt(h), (4, 4));
    assert_eq!(g.life(P1), 18);
    g.act(activate(h, 0), &[], &[]);
    assert_eq!(g.pt(h), (4, 4), "already monstrous");
    assert_eq!(g.life(P1), 18);
}

#[test]
fn ward_by_paying_life() {
    for pay in [true, false] {
        let mut t = Table::default();
        let guard = t.card(
            "{1}{B}",
            "Creature — Zombie",
            Some((2, 2)),
            "Ward—Pay 3 life.",
        );
        let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
        let mut g = Game::new(t);
        g.lands(1);
        let w = g.put(guard, P1, Zone::Battlefield);
        let b = g.put(bolt, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: b },
            &[mtg_core::Target::Object(w)],
            &[mtg_engine::choice::Answer::Bool(pay)],
        );
        assert_eq!(g.find(guard).is_none(), pay, "paid: the bolt resolves");
        assert_eq!(g.life(P0), if pay { 17 } else { 20 });
    }
}

/// Cast `object`, answering a "choose" question with the option labelled `pick`, and play on
/// until P0 has priority with an empty stack.
fn cast_choosing(g: &mut Game, object: mtg_core::ObjectId, pick: &str) {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut cast = false;
    loop {
        let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
            continue;
        };
        let a = match &c.kind {
            ChoiceKind::Priority { .. } if c.who == P0 && !cast => {
                cast = true;
                Answer::Action(Action::Cast { object })
            }
            ChoiceKind::Priority { .. } if c.who == P0 && g.stack().is_empty() => break,
            ChoiceKind::ChooseModes { available, .. } => {
                let i = available
                    .iter()
                    .position(|l| l.as_ref() == pick)
                    .unwrap_or_else(|| panic!("{pick} not offered: {available:?}"));
                Answer::Modes(vec![i as u8])
            }
            _ => c.default.clone().unwrap_or(Answer::Pass),
        };
        g.engine.answer(&g.table, c.id, a).unwrap();
    }
}

#[test]
fn a_chosen_creature_type_lord() {
    let mut t = Table::default();
    let banner = t.card(
        "{3}",
        "Artifact",
        None,
        "As this artifact enters, choose a creature type.\nCreatures you control of the chosen type get +1/+1.",
    );
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let gob = g.put(goblin, P0, Zone::Battlefield);
    let e = g.put(elf, P0, Zone::Battlefield);
    let b = g.put(banner, P0, Zone::Hand);
    g.main();
    cast_choosing(&mut g, b, "Goblin");
    assert_eq!(g.pt(gob), (2, 2));
    assert_eq!(g.pt(e), (1, 1));
}

#[test]
fn mana_of_the_chosen_color() {
    let mut t = Table::default();
    let gem = t.card(
        "{2}",
        "Artifact",
        None,
        "As this artifact enters, choose a color.\n{T}: Add one mana of the chosen color.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let gm = g.put(gem, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    cast_choosing(&mut g, gm, "green");
    // Two lands spent on the gem; the gem and nothing else can't make {1}{G}.
    let on = g.find(gem).unwrap();
    assert_eq!(
        g.engine.state.objects[&on].chosen_color,
        Some(mtg_core::Color::Green)
    );
    let sources = mtg_engine::mana::mana_sources(&g.engine.state, &g.table, P0);
    let gem_makes: Vec<_> = sources
        .iter()
        .filter(|s| s.object == on)
        .flat_map(|s| s.outputs.iter().flat_map(|o| o.possible_colors()))
        .collect();
    assert_eq!(gem_makes, vec![mtg_core::Color::Green]);
    let _ = b;
}

#[test]
fn energy_is_gained_and_spent() {
    let mut t = Table::default();
    let thopter = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "When this creature enters, you get {E}{E}.\nPay {E}{E}: Draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let th = g.put(thopter, P0, Zone::Hand);
    g.main();
    g.cast(th, &[]);
    assert_eq!(g.engine.state.player(P0).energy, 2);
    let on = g.find(thopter).unwrap();
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(on, 1), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert_eq!(g.engine.state.player(P0).energy, 0);
    let actions = g.main();
    assert!(!offers(&actions, on), "no energy left to pay");
}

#[test]
fn choosing_not_to_untap() {
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut t = Table::default();
    let sleeper = t.card(
        "{2}",
        "Artifact Creature",
        Some((2, 2)),
        "You may choose not to untap this creature during your untap step.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let s = g.put(sleeper, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    for id in [s, b] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    let mut asked = false;
    loop {
        let Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
            continue;
        };
        if matches!(c.kind, ChoiceKind::Priority { .. }) && c.who == P0 {
            break;
        }
        let a = match &c.kind {
            ChoiceKind::ChooseObjects { from, .. } => {
                asked = true;
                assert_eq!(from, &vec![s], "only the optional one is asked about");
                Answer::Objects(vec![s])
            }
            _ => c.default.clone().unwrap_or(Answer::Pass),
        };
        g.engine.answer(&g.table, c.id, a).unwrap();
    }
    assert!(asked);
    assert!(g.engine.state.objects[&s].tapped, "kept tapped");
    assert!(!g.engine.state.objects[&b].tapped);
}

#[test]
fn delirium_counts_card_types_in_the_graveyard() {
    let mut t = Table::default();
    let horror = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "Delirium — This creature gets +2/+2 as long as there are four or more card types among cards in your graveyard.",
    );
    let bear = t.bear();
    let forest = t.card("", "Basic Land — Forest", None, "");
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let rite = t.card("{B}", "Sorcery", None, "Target player loses 1 life.");
    let relic = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    let h = g.put(horror, P0, Zone::Battlefield);
    for c in [bear, forest, bolt] {
        g.put(c, P0, Zone::Graveyard);
    }
    assert_eq!(g.pt(h), (2, 2), "three types");
    g.put(rite, P0, Zone::Graveyard);
    assert_eq!(g.pt(h), (4, 4), "four types");
    g.put(relic, P1, Zone::Graveyard);
    assert_eq!(g.pt(h), (4, 4), "an opponent's graveyard doesn't count");
}

#[test]
fn putting_the_top_cards_back_in_any_order() {
    let mut t = Table::default();
    let seer = t.card(
        "{U}",
        "Sorcery",
        None,
        "Look at the top three cards of your library, then put them back in any order.",
    );
    let a = t.card("{1}", "Artifact", None, "");
    let b = t.card("{2}", "Artifact", None, "");
    let c = t.card("{3}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(seer, P0, Zone::Hand);
    g.main();
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let ids: Vec<_> = [a, b, c]
        .iter()
        .map(|card| g.put(*card, P0, Zone::Library))
        .collect();
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|id| !ids.contains(id));
    for (i, id) in ids.iter().enumerate() {
        order.insert(i, *id);
    }
    use mtg_engine::choice::Answer;
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![ids[2]]), Answer::Objects(vec![ids[0]])],
    );
    let top: Vec<_> = g.engine.state.objects_in(lib)[..3]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(top, vec![c, a, b]);
}

#[test]
fn outlast_at_sorcery_speed() {
    let mut t = Table::default();
    let monk = t.card("{W}", "Creature — Soldier", Some((1, 1)), "Outlast {1}");
    let mut g = Game::new(t);
    g.lands(1);
    let m = g.put(monk, P0, Zone::Battlefield);
    g.main();
    g.act(activate(m, 0), &[], &[]);
    assert_eq!(g.pt(m), (2, 2));
    assert!(g.engine.state.objects[&m].tapped);
}

#[test]
fn graft_moves_a_counter_to_a_newcomer() {
    let mut t = Table::default();
    let vine = t.card("{2}{G}", "Creature — Plant", Some((0, 0)), "Graft 2");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let v = g.put(vine, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(v, &[]);
    let on = g.find(vine).unwrap();
    assert_eq!(g.pt(on), (2, 2));
    g.act(
        Action::Cast { object: b },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.pt(on), (1, 1));
    let bb = g.find(bear).unwrap();
    assert_eq!(g.pt(bb), (3, 3));
}

#[test]
fn no_more_than_twice_each_turn() {
    let mut t = Table::default();
    let shade = t.card(
        "{B}",
        "Creature — Zombie",
        Some((1, 1)),
        "{B}: This creature gets +1/+1 until end of turn. Activate no more than twice each turn.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(shade, P0, Zone::Battlefield);
    g.main();
    g.act(activate(s, 0), &[], &[]);
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (3, 3));
    let actions = g.main();
    assert!(!offers(&actions, s), "a third time is not allowed");
}

#[test]
fn a_creature_hits_itself_and_opponents_creatures_enter_tapped() {
    let mut t = Table::default();
    let hurt = t.card(
        "{R}",
        "Sorcery",
        None,
        "Target creature deals damage to itself equal to its power.",
    );
    let warden = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Creatures your opponents control enter tapped.",
    );
    let giant = t.card("{4}{G}", "Creature — Beast", Some((5, 4)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let gi = g.put(giant, P1, Zone::Battlefield);
    let h = g.put(hurt, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    // The opponent's warden: P0's creatures enter tapped.
    g.put(warden, P1, Zone::Battlefield);
    g.main();
    g.cast(h, &[mtg_core::Target::Object(gi)]);
    assert!(g.find(giant).is_none(), "5 damage to a 5/4");
    g.cast(b, &[]);
    let on = g.find(bear).unwrap();
    assert!(g.engine.state.objects[&on].tapped, "entered tapped");
}

#[test]
fn a_ritual_floats_mana() {
    let mut t = Table::default();
    let ritual = t.card("{B}", "Instant", None, "Add {B}{B}{B}.");
    let horror = t.card("{2}{B}", "Creature — Zombie", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(ritual, P0, Zone::Hand);
    let h = g.put(horror, P0, Zone::Hand);
    g.main();
    g.cast(r, &[]);
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: h }),
        "three black floating"
    );
    g.cast(h, &[]);
    assert!(g.find(horror).is_some());
}

#[test]
fn a_saga_reads_its_chapters_then_is_sacrificed() {
    let mut t = Table::default();
    let tale = t.card(
        "{2}{R}",
        "Enchantment — Saga",
        None,
        "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI — You gain 2 life.\nII — Draw a card.\nIII — This Saga deals 3 damage to each opponent.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(tale, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 22, "chapter I as it entered");
    assert!(g.find(tale).is_some());
    g.until(P1, mtg_core::Step::PrecombatMain);
    let hand = g.count(Zone::Hand, P0);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 2,
        "the draw step and chapter II"
    );
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(g.life(P1), 17, "chapter III");
    assert!(g.find(tale).is_none(), "sacrificed after its last chapter");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_werewolf_transforms_after_a_quiet_turn() {
    let mut t = Table::default();
    let front = t.card(
        "{1}{G}",
        "Creature — Wolf",
        Some((2, 2)),
        "At the beginning of each upkeep, if no spells were cast last turn, transform this creature.",
    );
    let back = t.card(
        "",
        "Creature — Wolf",
        Some((4, 4)),
        "At the beginning of each upkeep, if a player cast two or more spells last turn, transform this creature.",
    );
    t.pair(front, back, mtg_ir::Layout::Transforming);
    let mut g = Game::new(t);
    let w = g.put(front, P0, Zone::Battlefield);
    g.main();
    // P0 casts nothing this turn; at P1's upkeep it transforms.
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(g.engine.state.objects[&w].face, 1);
    assert_eq!(g.pt(w), (4, 4));
}

#[test]
fn a_saga_returns_transformed() {
    let mut t = Table::default();
    let saga = t.card(
        "{1}{W}",
        "Enchantment — Saga",
        None,
        "I — You gain 1 life.\nII — You gain 1 life.\nIII — Exile this Saga, then return it to the battlefield transformed under your control.",
    );
    let hero = t.card("", "Enchantment Creature — Soldier", Some((3, 3)), "");
    t.pair(saga, hero, mtg_ir::Layout::Transforming);
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(saga, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    for _ in 0..2 {
        g.until(P1, mtg_core::Step::PrecombatMain);
        g.until(P0, mtg_core::Step::PrecombatMain);
    }
    assert_eq!(g.life(P0), 22);
    let on = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].card == saga)
        .expect("back on the battlefield");
    assert_eq!(g.engine.state.objects[&on].face, 1);
    assert_eq!(g.pt(on), (3, 3));
}

#[test]
fn daybound_follows_day_and_night() {
    let mut t = Table::default();
    let day = t.card("{1}{G}", "Creature — Wolf", Some((2, 2)), "Daybound");
    let night = t.card("", "Creature — Wolf", Some((4, 4)), "Nightbound");
    t.pair(day, night, mtg_ir::Layout::Transforming);
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(day, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    assert_eq!(
        g.engine.state.day,
        Some(true),
        "the first daybound card makes it day"
    );
    let on = g.find(day).unwrap();
    // P0 cast one spell this turn: day stays as P1's turn begins.
    g.until(P1, mtg_core::Step::PrecombatMain);
    assert_eq!(g.engine.state.day, Some(true));
    assert_eq!(g.engine.state.objects[&on].face, 0);
    // P1 casts nothing: night as P0's turn begins, and the wolf turns.
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(g.engine.state.day, Some(false));
    assert_eq!(g.engine.state.objects[&on].face, 1);
    assert_eq!(g.pt(on), (4, 4));
}

#[test]
fn exiled_instead_of_going_to_the_graveyard() {
    let mut t = Table::default();
    let phantom = t.card(
        "{1}{W}",
        "Creature — Spirit",
        Some((2, 2)),
        "If this creature would be put into a graveyard from anywhere, exile it instead.",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let p = g.put(phantom, P0, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(b, &[mtg_core::Target::Object(p)]);
    assert!(g.find(phantom).is_none());
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the bolt");
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == phantom && o.zone.zone == Zone::Exile)
    );
}

#[test]
fn annihilator_and_ingest() {
    let mut t = Table::default();
    let eldrazi = t.card(
        "{5}",
        "Creature — Eldrazi",
        Some((5, 5)),
        "Annihilator 2\nIngest",
    );
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    let e = g.put(eldrazi, P0, Zone::Battlefield);
    for _ in 0..3 {
        g.put(rock, P1, Zone::Battlefield);
    }
    g.main();
    let library = g.count(Zone::Library, P1);
    g.combat(&[e], &[], &[], &[]);
    assert_eq!(g.count(Zone::Graveyard, P1), 2, "two permanents sacrificed");
    assert_eq!(g.life(P1), 15);
    assert_eq!(
        g.count(Zone::Library, P1),
        library - 1,
        "ingest exiled the top card"
    );
}

#[test]
fn afflict_drains_when_blocked() {
    let mut t = Table::default();
    let hound = t.card("{2}{B}", "Creature — Dog", Some((3, 1)), "Afflict 2");
    let bear = t.bear();
    let mut g = Game::new(t);
    let h = g.put(hound, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat(&[h], &[(b, h)], &[], &[]);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn dethrone_needs_the_player_with_the_most_life() {
    for (their_life, grows) in [(20, true), (10, false)] {
        let mut t = Table::default();
        let rogue = t.card("{1}{B}", "Creature — Rogue", Some((2, 2)), "Dethrone");
        let mut g = Game::new(t);
        let r = g.put(rogue, P0, Zone::Battlefield);
        g.engine.state.players.get_mut(&P1).unwrap().life = their_life;
        g.main();
        g.combat(&[r], &[], &[], &[]);
        assert_eq!(g.life(P1), their_life - if grows { 3 } else { 2 });
    }
}

#[test]
fn a_disguised_permanent_has_ward_two() {
    let mut t = Table::default();
    let spy = t.card(
        "{3}{U}",
        "Creature — Human Rogue",
        Some((4, 4)),
        "Disguise {2}{U}",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spy, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&s).unwrap().face_down = true;
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(b, &[mtg_core::Target::Object(s)]);
    assert!(
        g.engine.state.objects.contains_key(&s),
        "the bolt was countered: no {{2}} to pay for ward"
    );
    assert_eq!(g.engine.state.objects[&s].damage, 0);
}

#[test]
fn overload_changes_target_to_each() {
    let mut t = Table::default();
    let squall = t.card(
        "{U}",
        "Instant",
        None,
        "Tap target creature you don't control.\nOverload {3}{U}",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(squall, P0, Zone::Hand);
    let actions = g.main();
    let over = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == s))
        .cloned()
        .expect("overload offered");
    g.act(over, &[], &[]);
    assert!(g.engine.state.objects[&b1].tapped);
    assert!(g.engine.state.objects[&b2].tapped);
    assert!(
        !g.engine.state.objects[&mine].tapped,
        "only creatures you don't control"
    );
}

#[test]
fn mobilize_brings_attacking_tokens_for_the_turn() {
    let mut t = Table::default();
    let chief = t.card("{2}{R}", "Creature — Goblin", Some((2, 2)), "Mobilize 2");
    let mut g = Game::new(t);
    let c = g.put(chief, P0, Zone::Battlefield);
    g.main();
    g.combat(&[c], &[], &[], &[]);
    assert_eq!(g.life(P1), 20 - 2 - 1 - 1, "the chief and two warriors");
    let tokens = |g: &Game| {
        g.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| g.engine.state.objects[id].is_token)
            .count()
    };
    assert_eq!(tokens(&g), 2);
    g.until(P1, mtg_core::Step::Upkeep);
    assert_eq!(tokens(&g), 0, "sacrificed at the end step");
}

#[test]
fn smaller_creatures_cant_block_it() {
    let mut t = Table::default();
    let brute = t.card(
        "{3}{R}",
        "Creature — Ogre",
        Some((4, 4)),
        "Creatures with power less than this creature's power can't block it.",
    );
    let bear = t.bear();
    let giant = t.card("{4}{G}", "Creature — Giant", Some((5, 5)), "");
    let mut g = Game::new(t);
    let b = g.put(brute, P0, Zone::Battlefield);
    let small = g.put(bear, P1, Zone::Battlefield);
    let big = g.put(giant, P1, Zone::Battlefield);
    g.main();
    assert!(!mtg_engine::combat::can_block(
        &g.engine.state,
        &g.table,
        small,
        b
    ));
    assert!(mtg_engine::combat::can_block(
        &g.engine.state,
        &g.table,
        big,
        b
    ));
}

#[test]
fn entwine_chooses_every_mode() {
    let mut t = Table::default();
    let tidings = t.card(
        "{1}{W}",
        "Instant",
        None,
        "Choose one —\n• You gain 3 life.\n• Draw a card.\nEntwine {2}",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let s = g.put(tidings, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.life(P0), 23);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 1);
}

#[test]
fn sunburst_counts_the_colors_spent() {
    let mut t = Table::default();
    let golem = t.card(
        "{W}{U}",
        "Artifact Creature — Golem",
        Some((0, 0)),
        "Sunburst",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(golem, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    let on = g.find(golem).expect("survived with counters");
    assert_eq!(g.pt(on), (2, 2), "white and blue");
}

#[test]
fn if_red_was_spent_to_cast_it() {
    let mut t = Table::default();
    let imp = t.card(
        "{1}{R}",
        "Creature — Imp",
        Some((1, 1)),
        "When this creature enters, if {R} was spent to cast it, you gain 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(imp, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    assert_eq!(g.life(P0), 22);
}

#[test]
fn speed_rises_once_a_turn_to_max() {
    let mut t = Table::default();
    let racer = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "Start your engines!\nMax speed — This creature gets +1/+1.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(racer, P0, Zone::Battlefield);
    let s1 = g.put(shock, P0, Zone::Hand);
    let s2 = g.put(shock, P0, Zone::Hand);
    g.main();
    assert_eq!(g.engine.state.player(P0).speed, Some(1));
    g.engine.state.players.get_mut(&P0).unwrap().speed = Some(3);
    g.cast(s1, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.engine.state.player(P0).speed, Some(4));
    assert_eq!(g.pt(r), (3, 3), "max speed");
    g.cast(s2, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.engine.state.player(P0).speed, Some(4), "never past 4");
}

#[test]
fn ward_discard_a_card() {
    for pay in [true, false] {
        let mut t = Table::default();
        let wisp = t.card(
            "{1}{U}",
            "Creature — Spirit",
            Some((1, 1)),
            "Ward—Discard a card.",
        );
        let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let w = g.put(wisp, P1, Zone::Battlefield);
        let s = g.put(shock, P0, Zone::Hand);
        let b = g.put(bear, P0, Zone::Hand);
        g.main();
        let answer = mtg_engine::choice::Answer::Objects(if pay { vec![b] } else { vec![] });
        g.act(
            Action::Cast { object: s },
            &[mtg_core::Target::Object(w)],
            &[answer],
        );
        assert_eq!(
            g.engine.state.objects.contains_key(&w),
            !pay,
            "paid {pay}: the shock resolves only if a card was discarded"
        );
    }
}

#[test]
fn a_saddled_mount_gets_its_attack_bonus() {
    let mut t = Table::default();
    let steed = t.card(
        "{2}{W}",
        "Creature — Horse Mount",
        Some((3, 3)),
        "Whenever this creature attacks while saddled, it gets +2/+2 until end of turn.\nSaddle 1",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let s = g.put(steed, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.act(
        activate(s, 1),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![b])],
    );
    assert!(g.engine.state.objects[&b].tapped, "the bear saddled it");
    g.combat(&[s], &[], &[], &[]);
    assert_eq!(g.life(P1), 15, "5 damage while saddled");
}

#[test]
fn reinforce_from_the_hand() {
    let mut t = Table::default();
    let scout = t.card(
        "{2}{W}",
        "Creature — Kithkin",
        Some((2, 2)),
        "Reinforce 2—{1}{W}",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(scout, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(offers(&actions, s));
    g.act(activate(s, 0), &[mtg_core::Target::Object(b)], &[]);
    assert_eq!(g.pt(b), (4, 4));
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "discarded");
}

#[test]
fn transmute_finds_the_same_mana_value() {
    let mut t = Table::default();
    let tutor = t.card(
        "{1}{B}{B}",
        "Sorcery",
        None,
        "Draw a card.\nTransmute {1}{B}{B}",
    );
    let three = t.card("{2}{G}", "Creature — Beast", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(tutor, P0, Zone::Hand);
    let actions = g.main();
    assert!(offers(&actions, s));
    let target = g.put(three, P0, Zone::Library);
    g.act(
        activate(s, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![target])],
    );
    let found = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == three);
    assert!(found);
}

#[test]
fn a_land_that_skips_its_next_untap() {
    let mut t = Table::default();
    let karoo = t.card(
        "",
        "Land",
        None,
        "{T}: Add {R} or {G}. This land doesn't untap during your next untap step.",
    );
    let bear = t.card("{1}", "Creature — Bear", Some((2, 2)), "");
    let mut g = Game::new(t);
    let k = g.put(karoo, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    assert!(g.engine.state.objects[&k].tapped, "tapped to pay");
    g.until(P0, mtg_core::Step::End);
    g.until(P0, mtg_core::Step::Upkeep);
    assert!(g.engine.state.objects[&k].tapped, "skipped its next untap");
    g.until(P0, mtg_core::Step::End);
    g.until(P0, mtg_core::Step::Upkeep);
    assert!(!g.engine.state.objects[&k].tapped, "untaps the turn after");
}

#[test]
fn a_mana_ability_limited_to_once_each_turn_pays_once() {
    let mut t = Table::default();
    let prism = t.card(
        "{2}",
        "Artifact",
        None,
        "{T}: Add one mana of any color. Activate only once each turn.",
    );
    let key = t.card("{1}", "Artifact", None, "{0}: Untap target artifact.");
    let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    let p = g.put(prism, P0, Zone::Battlefield);
    let k = g.put(key, P0, Zone::Battlefield);
    let first = g.put(shock, P0, Zone::Hand);
    let second = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(first, &[mtg_core::Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    g.act(activate(k, 0), &[mtg_core::Target::Object(p)], &[]);
    assert!(!g.engine.state.objects[&p].tapped, "untapped again");
    let Some(mtg_engine::choice::ChoiceKind::Priority { legal }) =
        g.pending.as_ref().map(|c| c.kind.clone())
    else {
        panic!("no priority");
    };
    assert!(
        !legal.actions.contains(&Action::Cast { object: second }),
        "its one activation this turn is spent"
    );
    assert!(
        !legal
            .actions
            .iter()
            .any(|a| matches!(a, Action::ActivateManaAbility { source, .. } if *source == p)),
        "and it is not offered by hand either"
    );
}

/// The actions offered at P0's current priority prompt.
fn offered(g: &Game) -> Vec<Action> {
    match g.pending.as_ref().map(|c| &c.kind) {
        Some(mtg_engine::choice::ChoiceKind::Priority { legal }) => legal.actions.clone(),
        _ => panic!("no priority prompt"),
    }
}

#[test]
fn a_spell_grants_one_more_land_play_this_turn_only() {
    let mut t = Table::default();
    let explore = t.card(
        "{G}",
        "Sorcery",
        None,
        "You may play an additional land this turn.\nDraw a card.",
    );
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(explore, P0, Zone::Hand);
    let lands: Vec<_> = (0..3).map(|_| g.put(mountain, P0, Zone::Hand)).collect();
    g.main();
    g.act(Action::PlayLand { object: lands[0] }, &[], &[]);
    assert!(
        !offered(&g).contains(&Action::PlayLand { object: lands[1] }),
        "one land a turn"
    );
    g.cast(spell, &[]);
    assert!(
        offered(&g).contains(&Action::PlayLand { object: lands[1] }),
        "the spell, now in the graveyard, still grants its play"
    );
    g.act(Action::PlayLand { object: lands[1] }, &[], &[]);
    assert!(!offered(&g).contains(&Action::PlayLand { object: lands[2] }));
    g.until(P1, mtg_core::Step::PrecombatMain);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    let in_hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .into_iter()
        .filter(|o| g.engine.state.objects[o].card == mountain)
        .count();
    let plays = actions
        .iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .count();
    assert!(in_hand >= 1 && plays == in_hand, "a normal turn again");
}

#[test]
fn an_additional_card_each_draw_step() {
    let mut t = Table::default();
    let howling = t.card(
        "{2}{U}",
        "Enchantment",
        None,
        "At the beginning of your draw step, draw an additional card.",
    );
    let mountain = t.mountain();
    let mut g = Game::new(t);
    for _ in 0..10 {
        g.put(mountain, P0, Zone::Library);
    }
    g.put(howling, P0, Zone::Battlefield);
    g.main();
    let before = g.count(Zone::Hand, P0);
    g.until(P1, mtg_core::Step::PrecombatMain);
    g.until(P0, mtg_core::Step::PrecombatMain);
    assert_eq!(g.count(Zone::Hand, P0), before + 2, "the draw and one more");
}

#[test]
fn a_shield_counter_stops_destruction_once() {
    let mut t = Table::default();
    let knight = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "This creature enters with a shield counter on it.",
    );
    let murder = t.card("{B}", "Instant", None, "Destroy target creature.");
    let mut g = Game::new(t);
    g.lands(2);
    let hand = g.put(knight, P0, Zone::Hand);
    let m1 = g.put(murder, P0, Zone::Hand);
    let m2 = g.put(murder, P0, Zone::Hand);
    g.main();
    g.lands(2);
    g.cast(hand, &[]);
    let k = g.find(knight).expect("entered");
    let shields = |g: &Game| {
        g.engine.state.objects[&k]
            .counters
            .get(&mtg_core::CounterKind::Shield)
            .copied()
            .unwrap_or(0)
    };
    assert_eq!(shields(&g), 1);
    g.cast(m1, &[mtg_core::Target::Object(k)]);
    assert!(
        g.find(knight).is_some(),
        "the shield counter was spent instead"
    );
    assert_eq!(shields(&g), 0);
    g.cast(m2, &[mtg_core::Target::Object(k)]);
    assert!(g.find(knight).is_none());
}

#[test]
fn a_shield_counter_stops_all_damage_dealt_at_once() {
    let mut t = Table::default();
    let guard = t.card(
        "{3}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "This creature enters with a shield counter on it.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let attacker = g.put(guard, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&attacker)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Shield, 1);
    let b1 = g.put(bear, P1, Zone::Battlefield);
    let b2 = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat(&[attacker], &[(b1, attacker), (b2, attacker)], &[], &[]);
    let obj = &g.engine.state.objects[&attacker];
    assert_eq!(obj.damage, 0, "both blockers' damage was stopped");
    assert_eq!(
        obj.counters
            .get(&mtg_core::CounterKind::Shield)
            .copied()
            .unwrap_or(0),
        0,
        "by one counter"
    );
}

#[test]
fn umbra_armor_dies_in_the_creatures_place() {
    for by_damage in [false, true] {
        let mut t = Table::default();
        let umbra = t.card(
            "{1}{G}",
            "Enchantment — Aura",
            None,
            "Enchant creature\nEnchanted creature gets +1/+1.\nUmbra armor",
        );
        let murder = t.card("{B}", "Instant", None, "Destroy target creature.");
        let bolt = t.card("{R}", "Instant", None, "~ deals 5 damage to any target.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let b = g.put(bear, P0, Zone::Battlefield);
        let u = g.put(umbra, P0, Zone::Battlefield);
        g.engine.state.objects.get_mut(&u).unwrap().attached_to = Some(b);
        let spell = g.put(if by_damage { bolt } else { murder }, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[mtg_core::Target::Object(b)]);
        assert!(g.find(bear).is_some(), "the creature survived");
        assert!(g.find(umbra).is_none(), "the Aura was destroyed instead");
        assert_eq!(
            g.engine.state.objects[&b].damage, 0,
            "and its damage removed"
        );
    }
}

#[test]
fn casting_creatures_as_though_they_had_flash() {
    for permission in [false, true] {
        let mut t = Table::default();
        let ambush = t.card(
            "{1}{G}",
            "Enchantment",
            None,
            "You may cast creature spells as though they had flash.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        if permission {
            g.put(ambush, P0, Zone::Battlefield);
        }
        let b = g.put(bear, P0, Zone::Hand);
        let actions = g.until(P0, mtg_core::Step::BeginCombat);
        assert_eq!(actions.contains(&Action::Cast { object: b }), permission);
    }
}

#[test]
fn flash_for_itself_only_while_its_condition_holds() {
    for has_wizard in [false, true] {
        let mut t = Table::default();
        let watch = t.card(
            "{1}{W}",
            "Creature — Soldier",
            Some((2, 2)),
            "You may cast this spell as though it had flash if you control a Wizard.",
        );
        let wizard = t.card("{U}", "Creature — Wizard", Some((1, 1)), "");
        let mut g = Game::new(t);
        g.lands(2);
        if has_wizard {
            g.put(wizard, P0, Zone::Battlefield);
        }
        let w = g.put(watch, P0, Zone::Hand);
        let actions = g.until(P0, mtg_core::Step::BeginCombat);
        assert_eq!(actions.contains(&Action::Cast { object: w }), has_wizard);
    }
}

#[test]
fn cumulative_upkeep_paid_in_life() {
    for pay in [true, false] {
        let mut t = Table::default();
        let sage = t.card(
            "{B}",
            "Creature — Elf",
            Some((2, 2)),
            "Cumulative upkeep—Pay 1 life.",
        );
        let mut g = Game::new(t);
        g.main();
        g.put(sage, P0, Zone::Battlefield);
        next_main(&mut g, P0, pay);
        if pay {
            assert_eq!(g.life(P0), 19, "one age counter");
            next_main(&mut g, P0, pay);
            assert_eq!(g.life(P0), 17, "two age counters");
            assert!(g.find(sage).is_some());
        } else {
            assert_eq!(g.life(P0), 20);
            assert!(
                !g.engine
                    .state
                    .battlefield()
                    .iter()
                    .any(|id| g.engine.state.objects[id].card == sage),
                "sacrificed"
            );
        }
    }
}
