//! Compiled cards, played.
//!
//! The compiler's contract is two-sided: a line it accepts must be understood completely,
//! *and* the engine must then do what the line says. Unit tests in `compile::tests` hold
//! the first half; these hold the second, one scenario per shape the compiler emits.

mod adventure;
mod abilities_extended;
mod backup;
mod bestow;
mod changeling;
mod class;
mod combat;
mod copies;
mod damage;
mod dig;
mod granted;
mod graveyard;
mod harness;
mod impulse;
mod keywords;
mod leveler;
mod multiface;
mod prevention;
mod search;
mod split;
mod tokens;
mod triggers;

use harness::*;
use mtg_core::{Keyword, Step, Target, Zone};
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

// ---- spells ---------------------------------------------------------------------

#[test]
fn burn_hits_a_player() {
    let mut t = Table::default();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 17);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        1,
        "the instant went to the graveyard"
    );
}

#[test]
fn burn_kills_a_creature() {
    let mut t = Table::default();
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(theirs)]);
    assert!(g.find(bear).is_none(), "the bear died");
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}

#[test]
fn a_player_only_target_cannot_pick_a_creature_and_opponent_means_opponent() {
    let mut t = Table::default();
    let drain = t.card(
        "{B}",
        "Sorcery",
        None,
        "Target opponent loses 2 life and you gain 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!((g.life(P0), g.life(P1)), (22, 18));
}

#[test]
fn pump_lasts_until_end_of_turn() {
    let mut t = Table::default();
    let giant = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +3/+3 and gains trample until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(giant, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(mine)]);
    assert_eq!(g.pt(mine), (5, 5));
    assert!(g.has(mine, Keyword::Trample));
    g.until(P0, Step::End);
    // The next turn's main phase: it has worn off.
    g.until(mtg_core::PlayerId(1), Step::PrecombatMain);
    assert_eq!(g.pt(mine), (2, 2));
    assert!(!g.has(mine, Keyword::Trample));
}

#[test]
fn a_counterspell_counters_only_what_it_says() {
    let mut t = Table::default();
    let negate = t.card("{U}", "Instant", None, "Counter target noncreature spell.");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(negate, P0, Zone::Hand);
    let actions = g.main();
    // Nothing on the stack to counter, so it cannot even be cast.
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == spell)),
        "a spell with no legal target is not castable"
    );
}

#[test]
fn bounce_and_reanimate_to_hand() {
    let mut t = Table::default();
    let bounce = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let raise = t.card(
        "{B}",
        "Sorcery",
        None,
        "Return target creature card from your graveyard to your hand.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let dead = g.put(bear, P0, Zone::Graveyard);
    let b = g.put(bounce, P0, Zone::Hand);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    let hand_before = g.count(Zone::Hand, P1);
    g.cast(b, &[Target::Object(theirs)]);
    assert_eq!(g.count(Zone::Hand, P1), hand_before + 1);
    let mine_before = g.count(Zone::Hand, P0);
    g.cast(r, &[Target::Object(dead)]);
    // One card left the hand (the sorcery) and one arrived (the bear).
    assert_eq!(g.count(Zone::Hand, P0), mine_before);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        2,
        "both spells, and not the bear"
    );
}

// ---- triggered abilities ------------------------------------------------------------

#[test]
fn an_etb_draw_draws_when_it_enters() {
    let mut t = Table::default();
    let seer = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "When ~ enters, draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let card = g.put(seer, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(card, &[]);
    assert!(g.find(seer).is_some());
    // Cast one, drew one.
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn an_etb_trigger_that_targets() {
    let mut t = Table::default();
    let archer = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((1, 1)),
        "When ~ enters, it deals 1 damage to any target.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let card = g.put(archer, P0, Zone::Hand);
    g.main();
    g.cast(card, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 19);
}

#[test]
fn a_dies_trigger_fires_from_the_graveyard() {
    let mut t = Table::default();
    let martyr = t.card(
        "{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "When ~ dies, you gain 3 life.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 2 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let m = g.put(martyr, P0, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Object(m)]);
    assert_eq!(g.life(P0), 23);
}

#[test]
fn an_upkeep_trigger() {
    let mut t = Table::default();
    let bell = t.card(
        "{2}",
        "Artifact",
        None,
        "At the beginning of your upkeep, each opponent loses 1 life.",
    );
    let mut g = Game::new(t);
    g.put(bell, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.life(P1), 19);
}

// ---- static abilities -------------------------------------------------------------------

#[test]
fn an_anthem_pumps_only_your_creatures_while_it_is_there() {
    let mut t = Table::default();
    let anthem = t.card(
        "{2}{W}",
        "Enchantment",
        None,
        "Creatures you control get +1/+1.",
    );
    let shatter = t.card("{R}", "Instant", None, "Destroy target enchantment.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let a = g.put(anthem, P0, Zone::Battlefield);
    let s = g.put(shatter, P0, Zone::Hand);
    g.main();
    assert_eq!((g.pt(mine), g.pt(theirs)), ((3, 3), (2, 2)));
    g.cast(s, &[Target::Object(a)]);
    assert_eq!(g.pt(mine), (2, 2), "gone with its source");
}

#[test]
fn granted_keywords_come_from_the_layer_system() {
    let mut t = Table::default();
    let banner = t.card(
        "{3}",
        "Artifact",
        None,
        "Creatures you control have flying.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    g.put(banner, P0, Zone::Battlefield);
    g.main();
    assert!(g.has(mine, Keyword::Flying));
    assert!(!g.has(theirs, Keyword::Flying));
}

#[test]
fn cant_block_is_enforced() {
    let mut t = Table::default();
    let brute = t.card("{R}", "Creature — Goblin", Some((2, 2)), "~ can't block.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let b = g.put(brute, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let attacker = g.put(bear, P1, Zone::Battlefield);
    g.engine
        .state
        .combat
        .attackers
        .insert(attacker, Target::Player(P0));
    let blockers: Vec<_> = mtg_engine::combat::eligible_blockers(&g.engine.state, &g.table, P0)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(blockers.contains(&other), "an ordinary creature can block");
    assert!(
        !blockers.contains(&b),
        "a creature that can't block is not offered"
    );
}

// ---- auras and equipment -------------------------------------------------------------------

#[test]
fn an_aura_attaches_to_its_target_and_falls_off_with_it() {
    let mut t = Table::default();
    let wings = t.card(
        "{1}{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature gets +1/+1 and has flying.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 5 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let w = g.put(wings, P0, Zone::Hand);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(w, &[Target::Object(mine)]);
    let aura = g.find(wings).expect("the aura is on the battlefield");
    assert_eq!(g.engine.state.objects[&aura].attached_to, Some(mine));
    assert_eq!(g.pt(mine), (3, 3));
    assert!(g.has(mine, Keyword::Flying));

    g.cast(b, &[Target::Object(mine)]);
    assert!(g.find(bear).is_none());
    assert!(
        g.find(wings).is_none(),
        "the aura went to the graveyard (CR 704.5m)"
    );
}

#[test]
fn an_aura_whose_target_is_gone_never_enters() {
    let mut t = Table::default();
    let wings = t.card(
        "{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature gets +1/+1.\nWhen ~ enters, you gain 5 life.",
    );
    let bounce = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let w = g.put(wings, P0, Zone::Hand);
    let b = g.put(bounce, P0, Zone::Hand);
    g.main();
    // Cast the aura, then bounce its target in response.
    g.act_holding(Action::Cast { object: w }, &[Target::Object(mine)]);
    assert_eq!(g.stack().len(), 1);
    g.cast(b, &[Target::Object(mine)]);
    assert!(g.find(wings).is_none(), "it never entered the battlefield");
    assert_eq!(
        g.life(P0),
        20,
        "so its enters trigger never fired (CR 608.2b)"
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 2, "aura and instant");
}

#[test]
fn equip_pays_attaches_and_pumps() {
    let mut t = Table::default();
    let blade = t.card(
        "{1}",
        "Artifact — Equipment",
        None,
        "Equipped creature gets +2/+0.\nEquip {2}",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let e = g.put(blade, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(
        offers(&actions, e),
        "equip is offered at sorcery speed with {{2}} open"
    );
    g.act(activate(e, 1), &[Target::Object(mine)], &[]);
    assert_eq!(g.engine.state.objects[&e].attached_to, Some(mine));
    assert_eq!(g.pt(mine), (4, 2));
    let tapped = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| g.engine.state.objects[id].tapped)
        .count();
    assert_eq!(tapped, 2, "both lands paid for it");
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(!offers(&actions, e), "no mana left to equip again");
}

// ---- activated abilities and their costs ----------------------------------------------------

#[test]
fn a_tap_ability_taps_and_cannot_be_reused() {
    let mut t = Table::default();
    let pinger = t.card(
        "{1}{R}",
        "Creature — Wizard",
        Some((1, 1)),
        "{T}: ~ deals 1 damage to any target.",
    );
    let mut g = Game::new(t);
    let p = g.put(pinger, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(offers(&actions, p));
    g.act(activate(p, 0), &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 19);
    assert!(g.engine.state.objects[&p].tapped, "the cost was paid");
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(!offers(&actions, p), "tapped, so not again");
}

#[test]
fn a_summoning_sick_creature_cannot_use_tap_abilities() {
    let mut t = Table::default();
    let pinger = t.card(
        "{R}",
        "Creature — Wizard",
        Some((1, 1)),
        "{T}: ~ deals 1 damage to any target.",
    );
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "{T}: Add {G}.");
    let mut g = Game::new(t);
    g.lands(2);
    let p = g.put(pinger, P0, Zone::Hand);
    let e = g.put(elf, P0, Zone::Hand);
    g.main();
    g.cast(p, &[]);
    g.cast(e, &[]);
    let actions = g.until(P0, Step::PrecombatMain);
    let pinger_now = g.find(pinger).unwrap();
    let elf_now = g.find(elf).unwrap();
    assert!(!offers(&actions, pinger_now), "CR 302.6");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ActivateManaAbility { source, .. } if *source == elf_now)),
        "a mana creature is summoning sick too"
    );
}

#[test]
fn a_mana_cost_on_an_ability_is_paid() {
    let mut t = Table::default();
    let shade = t.card(
        "{B}",
        "Creature — Wizard",
        Some((1, 1)),
        "{1}: ~ gets +1/+1 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(shade, P0, Zone::Battlefield);
    g.main();
    g.act(activate(s, 0), &[], &[]);
    g.until(P0, Step::PrecombatMain);
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (3, 3), "the permanent is pumped, not the ability");
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(!offers(&actions, s), "both lands are spent");
}

#[test]
fn a_sacrifice_cost_is_paid_and_the_effect_still_happens() {
    let mut t = Table::default();
    let bomb = t.card(
        "{1}",
        "Artifact",
        None,
        "{T}, Sacrifice ~: ~ deals 2 damage to any target.",
    );
    let mut g = Game::new(t);
    let b = g.put(bomb, P0, Zone::Battlefield);
    g.main();
    g.act(activate(b, 0), &[Target::Player(P1)], &[]);
    assert!(g.find(bomb).is_none(), "sacrificed");
    assert_eq!(
        g.life(P1),
        18,
        "and the ability resolved without its source"
    );
}

#[test]
fn a_life_cost_needs_the_life() {
    let mut t = Table::default();
    let altar = t.card("{1}", "Artifact", None, "Pay 3 life: Draw a card.");
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    g.main();
    g.act(activate(a, 0), &[], &[]);
    assert_eq!(g.life(P0), 17);

    let mut t = Table::default();
    let altar = t.card("{1}", "Artifact", None, "Pay 3 life: Draw a card.");
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    g.engine.state.players.get_mut(&P0).unwrap().life = 2;
    let actions = g.main();
    assert!(!offers(&actions, a), "CR 119.4: can't pay 3 life with 2");
}

// ---- lands and permanents entering -------------------------------------------------------------

#[test]
fn a_land_that_enters_tapped() {
    let mut t = Table::default();
    let dual = t.card(
        "",
        "Land",
        None,
        "This land enters tapped.\n{T}: Add {W} or {U}.",
    );
    let mut g = Game::new(t);
    let l = g.put(dual, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: l }, &[], &[]);
    let now = g.find(dual).unwrap();
    assert!(g.engine.state.objects[&now].tapped);
}

#[test]
fn a_creature_that_enters_with_counters() {
    let mut t = Table::default();
    let hydra = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((0, 0)),
        "~ enters with two +1/+1 counters on it.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(hydra, P0, Zone::Hand);
    g.main();
    g.cast(h, &[]);
    let now = g.find(hydra).expect("survived as a 2/2");
    assert_eq!(g.pt(now), (2, 2));
}

#[test]
fn a_planeswalker_enters_with_loyalty_and_takes_damage_as_loyalty_loss() {
    let mut t = Table::default();
    let walker = t.loyalty("{2}", "Legendary Planeswalker — Test", 3, "");
    let bolt = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    g.lands(3);
    let w = g.put(walker, P0, Zone::Hand);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let now = g.find(walker).expect("did not die to CR 704.5i");
    g.cast(b, &[Target::Object(now)]);
    let loyalty = g.engine.state.objects[&now]
        .counters
        .get(&mtg_core::CounterKind::Loyalty)
        .copied();
    assert_eq!(loyalty, Some(1));
    assert_eq!(
        g.engine.state.objects[&now].damage, 0,
        "not marked as damage"
    );
}

// ---- combat, and the triggers it causes ------------------------------------------------------

#[test]
fn a_combat_damage_trigger_fires_on_combat_damage_only() {
    let mut t = Table::default();
    let ninja = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever ~ deals combat damage to a player, draw a card.",
    );
    let mut g = Game::new(t);
    let n = g.put(ninja, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[n], &[], &[], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn noncombat_damage_is_not_combat_damage() {
    let mut t = Table::default();
    let pinger = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "Whenever ~ deals combat damage to a player, draw a card.\n{T}: ~ deals 1 damage to target player.",
    );
    let mut g = Game::new(t);
    let p = g.put(pinger, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(p, 1), &[Target::Player(P1)], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "CR 510.2: not combat damage, no draw"
    );
}

#[test]
fn becomes_blocked_triggers_once_however_many_block() {
    let mut t = Table::default();
    let warrior = t.card(
        "{2}{G}",
        "Creature — Elf",
        Some((2, 2)),
        "Whenever ~ becomes blocked, it gets +1/+1 until end of turn.",
    );
    let wall = t.card("{0}", "Creature — Soldier", Some((0, 10)), "");
    let mut g = Game::new(t);
    let w = g.put(warrior, P0, Zone::Battlefield);
    let b1 = g.put(wall, P1, Zone::Battlefield);
    let b2 = g.put(wall, P1, Zone::Battlefield);
    g.main();
    g.combat(&[w], &[(b1, w), (b2, w)], &[], &[]);
    assert_eq!(g.pt(w), (3, 3), "one trigger, not two");
}

#[test]
fn an_attack_trigger_pumps_the_team() {
    let mut t = Table::default();
    let sergeant = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever ~ attacks, other Soldiers you control get +1/+0 until end of turn.",
    );
    let soldier = t.card("{W}", "Creature — Soldier", Some((1, 1)), "");
    let mut g = Game::new(t);
    let s = g.put(sergeant, P0, Zone::Battlefield);
    let o = g.put(soldier, P0, Zone::Battlefield);
    g.main();
    g.combat(&[s, o], &[], &[], &[]);
    assert_eq!(
        g.life(P1),
        17,
        "1 from the sergeant, 2 from the pumped soldier"
    );
    assert_eq!(g.pt(s), (1, 1), "'other' means other");
}

#[test]
fn a_cast_trigger_sees_only_matching_spells() {
    let mut t = Table::default();
    let adept = t.card(
        "{1}{R}",
        "Creature — Wizard",
        Some((1, 2)),
        "Whenever you cast a noncreature spell, ~ gets +1/+1 until end of turn.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to target player.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let a = g.put(adept, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    assert_eq!(g.pt(a), (1, 2), "a creature spell does not count");
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.pt(a), (2, 3));
}

#[test]
fn another_creature_entering_binds_that_creature() {
    let mut t = Table::default();
    let herald = t.card(
        "{2}{R}",
        "Creature — Goblin",
        Some((2, 2)),
        "Whenever another creature you control enters, it gains haste until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let h = g.put(herald, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    let now = g.find(bear).unwrap();
    assert!(g.has(now, Keyword::Haste));
    assert!(!g.has(h, Keyword::Haste));
}

// ---- library and hand effects ---------------------------------------------------------------

#[test]
fn scry_one_to_the_bottom() {
    let mut t = Table::default();
    let seer = t.card("{U}", "Sorcery", None, "Scry 1.");
    let marker = t.card("{5}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    // A recognisable card on top of the library, after the draw step.
    let s = g.put(seer, P0, Zone::Hand);
    g.main();
    let top = g.put(marker, P0, Zone::Library);
    // `place` appends; move it to the top by hand.
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| *o != top);
    order.insert(0, top);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![top])],
    );
    let order = g.engine.state.objects_in(lib);
    // A new object in its new position (CR 400.7), so compare the card.
    let bottom = order.last().map(|id| g.engine.state.objects[id].card);
    assert_eq!(bottom, Some(marker), "scried to the bottom");
    let top_now = order.first().map(|id| g.engine.state.objects[id].card);
    assert_ne!(top_now, Some(marker));
}

#[test]
fn mill_and_discard() {
    let mut t = Table::default();
    let mill = t.card("{U}", "Sorcery", None, "Target player mills three cards.");
    let rack = t.card("{B}", "Sorcery", None, "Each opponent discards a card.");
    let mut g = Game::new(t);
    g.lands(2);
    let m = g.put(mill, P0, Zone::Hand);
    let r = g.put(rack, P0, Zone::Hand);
    let bear = g.table.bear();
    g.put(bear, P1, Zone::Hand);
    g.main();
    g.cast(m, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 3);
    let hand = g.count(Zone::Hand, P1);
    g.cast(r, &[]);
    assert_eq!(g.count(Zone::Hand, P1), hand - 1);
    assert_eq!(g.count(Zone::Graveyard, P1), 4);
}

#[test]
fn an_edict_makes_the_opponent_sacrifice() {
    let mut t = Table::default();
    let edict = t.card(
        "{1}{B}",
        "Instant",
        None,
        "Target player sacrifices a creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let mine = g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let e = g.put(edict, P0, Zone::Hand);
    g.main();
    g.cast(e, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert!(
        g.engine.state.objects.contains_key(&mine),
        "only the target player's"
    );
}

#[test]
fn you_may_can_be_declined() {
    let mut t = Table::default();
    let well = t.card(
        "{2}",
        "Artifact",
        None,
        "When ~ enters, you may draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(well, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: w },
        &[],
        &[mtg_engine::choice::Answer::Bool(false)],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 1, "declined, so no draw");
}

// ---- tokens ---------------------------------------------------------------------------------

fn tokens(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect()
}

#[test]
fn creature_tokens_are_created_with_their_characteristics() {
    let mut t = Table::default();
    let raise = t.card(
        "{2}{W}",
        "Sorcery",
        None,
        "Create two 1/1 white Spirit creature tokens with flying.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let made = tokens(&g);
    assert_eq!(made.len(), 2);
    for id in made {
        assert_eq!(g.pt(id), (1, 1));
        assert!(g.has(id, Keyword::Flying));
        let ch = g.engine.characteristics(&g.table, id).unwrap().clone();
        assert!(
            ch.colors.contains(mtg_core::Color::White),
            "coloured without a mana cost"
        );
        assert_eq!(&*ch.name, "Spirit");
        assert_eq!(g.engine.state.objects[&id].controller, P0);
    }
}

#[test]
fn a_token_entering_triggers_enters_abilities_and_dies_like_a_creature() {
    let mut t = Table::default();
    let herald = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever another creature you control enters, you gain 1 life.",
    );
    let raise = t.card(
        "{W}",
        "Sorcery",
        None,
        "Create a 1/1 white Soldier creature token.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 1 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    g.put(herald, P0, Zone::Battlefield);
    let r = g.put(raise, P0, Zone::Hand);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(r, &[]);
    assert_eq!(g.life(P0), 21, "the token entered (CR 111.1)");
    let token = tokens(&g)[0];
    g.cast(b, &[Target::Object(token)]);
    assert!(tokens(&g).is_empty());
    assert!(
        !g.engine.state.objects.values().any(|o| o.is_token),
        "a token that left the battlefield ceased to exist (CR 704.5e)"
    );
}

#[test]
fn a_tapped_token_arrives_tapped() {
    let mut t = Table::default();
    let raise = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a tapped 2/2 black Zombie creature token.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[]);
    let token = tokens(&g)[0];
    assert!(g.engine.state.objects[&token].tapped);
}

#[test]
fn a_food_token_can_be_eaten() {
    let mut t = Table::default();
    let bake = t.card("{G}", "Sorcery", None, "Create a Food token.");
    let mut g = Game::new(t);
    g.lands(3);
    let b = g.put(bake, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    let food = tokens(&g)[0];
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(
        offers(&actions, food),
        "{{2}}, {{T}}, sacrifice: gain 3 life"
    );
    g.act(activate(food, 0), &[], &[]);
    assert_eq!(g.life(P0), 23);
    assert!(tokens(&g).is_empty(), "eaten");
}

#[test]
fn a_creature_token_is_summoning_sick() {
    let mut t = Table::default();
    let raise = t.card(
        "{R}",
        "Sorcery",
        None,
        "Create a 1/1 red Goblin creature token.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[]);
    let token = tokens(&g)[0];
    let eligible = mtg_engine::combat::eligible_attackers(&g.engine.state, &g.table, P0);
    assert!(!eligible.contains(&token));
}

// ---- evasion, attack requirements, keyword triggers ------------------------------------------

#[test]
fn evasion_keywords_limit_blockers() {
    let mut t = Table::default();
    let shade = t.card("{B}", "Creature — Wizard", Some((1, 1)), "Shadow");
    let dread = t.card("{B}", "Creature — Wizard", Some((1, 1)), "Fear");
    let walker = t.card("{B}", "Creature — Wizard", Some((1, 1)), "Swampwalk");
    let ghost = t.card(
        "{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "~ can't be blocked.",
    );
    let bear = t.bear();
    let swamp = t.card("", "Basic Land — Swamp", None, "");
    let mut g = Game::new(t);
    let s = g.put(shade, P0, Zone::Battlefield);
    let f = g.put(dread, P0, Zone::Battlefield);
    let w = g.put(walker, P0, Zone::Battlefield);
    let gh = g.put(ghost, P0, Zone::Battlefield);
    let blocker = g.put(bear, P1, Zone::Battlefield);
    let can = |g: &Game, a| mtg_engine::combat::can_block(&g.engine.state, &g.table, blocker, a);
    assert!(!can(&g, s), "shadow");
    assert!(!can(&g, f), "fear: a green creature can't block it");
    assert!(can(&g, w), "no swamp yet");
    assert!(!can(&g, gh), "can't be blocked");
    g.put(swamp, P1, Zone::Battlefield);
    assert!(!can(&g, w), "swampwalk once the defender has a Swamp");
}

#[test]
fn a_creature_that_can_block_only_flyers() {
    let mut t = Table::default();
    let archer = t.card(
        "{G}",
        "Creature — Elf",
        Some((1, 3)),
        "Reach\n~ can block only creatures with flying.",
    );
    let bird = t.card("{W}", "Creature — Soldier", Some((1, 1)), "Flying");
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(archer, P1, Zone::Battlefield);
    let b = g.put(bird, P0, Zone::Battlefield);
    let br = g.put(bear, P0, Zone::Battlefield);
    let can = |g: &Game, x| mtg_engine::combat::can_block(&g.engine.state, &g.table, a, x);
    assert!(can(&g, b));
    assert!(!can(&g, br));
}

#[test]
fn a_creature_that_must_attack_is_made_to() {
    let mut t = Table::default();
    let berserker = t.card(
        "{R}",
        "Creature — Goblin",
        Some((2, 1)),
        "~ attacks each combat if able.",
    );
    let mut g = Game::new(t);
    let b = g.put(berserker, P0, Zone::Battlefield);
    g.main();
    // Declaring no attackers is refused; the default answer includes it.
    let c = g.engine.advance(&g.table);
    let _ = c;
    let must = mtg_engine::combat::must_attack(&g.engine.state, &g.table, P0);
    assert_eq!(must, vec![b]);
}

#[test]
fn prowess_and_exalted() {
    let mut t = Table::default();
    let monk = t.card("{1}{R}", "Creature — Wizard", Some((1, 2)), "Prowess");
    let knight = t.card("{W}", "Creature — Soldier", Some((1, 1)), "Exalted");
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to target player.");
    let mut g = Game::new(t);
    g.lands(1);
    let m = g.put(monk, P0, Zone::Battlefield);
    let k = g.put(knight, P0, Zone::Battlefield);
    let _ = k;
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.pt(m), (2, 3), "prowess");
    g.combat(&[m], &[], &[], &[]);
    assert_eq!(
        g.life(P1),
        19 - 3,
        "attacked alone: 2 from prowess, +1 exalted"
    );
}

#[test]
fn landfall_is_an_ability_word() {
    let mut t = Table::default();
    let cat = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((2, 2)),
        "Landfall — Whenever a land you control enters, this creature gets +2/+2 until end of turn.",
    );
    let land = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    let c = g.put(cat, P0, Zone::Battlefield);
    let l = g.put(land, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: l }, &[], &[]);
    assert_eq!(g.pt(c), (4, 4));
}

#[test]
fn cycling_from_the_hand() {
    let mut t = Table::default();
    let beast = t.card("{5}{G}", "Creature — Elf", Some((5, 5)), "Cycling {2}");
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(beast, P0, Zone::Hand);
    let actions = g.main();
    assert!(offers(&actions, b), "cycling is offered from the hand");
    let hand = g.count(Zone::Hand, P0);
    g.act(activate(b, 0), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand, "discarded one, drew one");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_card_on_the_battlefield_cannot_cycle() {
    let mut t = Table::default();
    let beast = t.card("{G}", "Creature — Elf", Some((1, 1)), "Cycling {0}");
    let mut g = Game::new(t);
    let b = g.put(beast, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(!offers(&actions, b));
}

#[test]
fn devoid_is_colorless() {
    let mut t = Table::default();
    let spawn = t.card("{1}{B}", "Creature — Elf", Some((2, 1)), "Devoid");
    let mut g = Game::new(t);
    let s = g.put(spawn, P0, Zone::Battlefield);
    let ch = g.engine.characteristics(&g.table, s).unwrap();
    assert!(ch.colors.is_colorless());
}

#[test]
fn doesnt_untap() {
    let mut t = Table::default();
    let chains = t.card(
        "{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature doesn't untap during its controller's untap step.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    g.engine.state.objects.get_mut(&theirs).unwrap().tapped = true;
    let c = g.put(chains, P0, Zone::Hand);
    g.main();
    g.cast(c, &[Target::Object(theirs)]);
    g.until(P1, Step::PrecombatMain);
    assert!(
        g.engine.state.objects[&theirs].tapped,
        "still tapped on its controller's turn"
    );
}

#[test]
fn scry_two_orders_what_stays() {
    let mut t = Table::default();
    let seer = t.card("{U}", "Sorcery", None, "Scry 2.");
    let a = t.card("{1}", "Artifact", None, "");
    let b = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(seer, P0, Zone::Hand);
    g.main();
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let oa = g.put(a, P0, Zone::Library);
    let ob = g.put(b, P0, Zone::Library);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| *o != oa && *o != ob);
    order.insert(0, ob);
    order.insert(0, oa);
    // Keep both, and put `b` on top.
    g.act(
        Action::Cast { object: s },
        &[],
        &[
            mtg_engine::choice::Answer::Objects(vec![]),
            mtg_engine::choice::Answer::Objects(vec![ob]),
        ],
    );
    let top: Vec<_> = g.engine.state.objects_in(lib)[..2]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(top, vec![b, a]);
}

// ---- modes and X ------------------------------------------------------------------------------

#[test]
fn a_modal_spell_chooses_its_mode_and_only_that_modes_targets_on_cast() {
    let mut t = Table::default();
    let charm = t.card(
        "{R}",
        "Instant",
        None,
        "Choose one —\n• ~ deals 2 damage to target creature.\n• ~ deals 3 damage to target player.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let _ = theirs;
    let c = g.put(charm, P0, Zone::Hand);
    g.main();
    // Mode 2 only: one target is asked for, a player.
    g.act(
        Action::Cast { object: c },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Modes(vec![1])],
    );
    assert_eq!(g.life(P1), 17);
    assert!(g.find(bear).is_some(), "the unchosen mode did nothing");
}

#[test]
fn a_modal_enters_trigger() {
    let mut t = Table::default();
    let druid = t.card(
        "{2}{G}",
        "Creature — Elf",
        Some((2, 2)),
        "When ~ enters, choose one —\n• You gain 3 life.\n• Draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let d = g.put(druid, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: d },
        &[],
        &[mtg_engine::choice::Answer::Modes(vec![0])],
    );
    assert_eq!(g.life(P0), 23);
}

#[test]
fn x_is_announced_and_paid() {
    let mut t = Table::default();
    let blast = t.card("{X}{R}", "Sorcery", None, "~ deals X damage to any target.");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(blast, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: b },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Number(3)],
    );
    assert_eq!(g.life(P1), 17);
    let untapped = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| !g.engine.state.objects[id].tapped)
        .count();
    assert_eq!(untapped, 0, "X=3 plus {{R}} used all four lands");
}

#[test]
fn an_unchosen_up_to_target_does_not_break_the_spell() {
    let mut t = Table::default();
    let spell = t.card(
        "{U}",
        "Sorcery",
        None,
        "Tap up to one target creature. Draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    // No creature on the battlefield: the slot is left empty.
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand, "cast one, drew one");
}

// ---- conditions and values -------------------------------------------------------------------

#[test]
fn an_intervening_if_is_checked() {
    let mut t = Table::default();
    let knight = t.card(
        "{1}{B}",
        "Creature — Soldier",
        Some((2, 2)),
        "When ~ enters, if you control a Swamp, you gain 2 life.",
    );
    let swamp = t.card("", "Basic Land — Swamp", None, "");
    let mut g = Game::new(t);
    g.lands(4);
    let k1 = g.put(knight, P0, Zone::Hand);
    let k2 = g.put(knight, P0, Zone::Hand);
    let s = g.put(swamp, P0, Zone::Hand);
    g.main();
    g.cast(k1, &[]);
    assert_eq!(g.life(P0), 20, "no Swamp, no trigger");
    g.act(Action::PlayLand { object: s }, &[], &[]);
    g.cast(k2, &[]);
    assert_eq!(g.life(P0), 22);
}

#[test]
fn a_conditional_clause_and_for_each() {
    let mut t = Table::default();
    let spell = t.card(
        "{G}",
        "Sorcery",
        None,
        "Draw a card for each creature you control. If you control an artifact, you gain 3 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(s, &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand - 1 + 2,
        "two creatures, two cards"
    );
    assert_eq!(g.life(P0), 20, "no artifact");
}

#[test]
fn a_one_shot_for_each_pump_is_frozen_on_resolution() {
    let mut t = Table::default();
    let rally = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +1/+1 for each creature you control until end of turn.",
    );
    let bear = t.bear();
    let raise = t.card(
        "{W}",
        "Sorcery",
        None,
        "Create a 1/1 white Soldier creature token.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let r = g.put(rally, P0, Zone::Hand);
    let m = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[Target::Object(b)]);
    assert_eq!(g.pt(b), (3, 3), "one creature");
    g.cast(m, &[]);
    assert_eq!(g.pt(b), (3, 3), "CR 608.2h: counted once, as it resolved");
}

#[test]
fn as_long_as_and_during_your_turn() {
    let mut t = Table::default();
    let knight = t.card(
        "{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "During your turn, ~ has first strike.\n~ gets +2/+2 as long as you control an artifact.",
    );
    let rock = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    let k = g.put(knight, P0, Zone::Battlefield);
    g.main();
    assert!(g.has(k, Keyword::FirstStrike));
    assert_eq!(g.pt(k), (1, 1));
    g.put(rock, P0, Zone::Battlefield);
    assert_eq!(g.pt(k), (3, 3));
    g.until(mtg_core::PlayerId(1), Step::PrecombatMain);
    assert!(
        !g.has(k, Keyword::FirstStrike),
        "not on the opponent's turn"
    );
}

#[test]
fn damage_equal_to_power_and_its_controller() {
    let mut t = Table::default();
    let fling = t.card(
        "{R}",
        "Instant",
        None,
        "Destroy target creature. Its controller loses 2 life.",
    );
    let bite = t.card(
        "{G}",
        "Sorcery",
        None,
        "Target creature you control deals damage equal to its power to target creature you don't control.",
    );
    let bear = t.bear();
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let mine = g.put(big, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    let f = g.put(fling, P0, Zone::Hand);
    let b = g.put(bite, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Object(mine), Target::Object(theirs)]);
    assert!(
        !g.engine.state.objects.contains_key(&theirs),
        "4 damage killed it"
    );
    g.cast(f, &[Target::Object(other)]);
    assert_eq!(
        g.life(P1),
        18,
        "its controller, from last-known information"
    );
}

#[test]
fn that_player_in_a_combat_damage_trigger() {
    let mut t = Table::default();
    let rat = t.card(
        "{B}",
        "Creature — Soldier",
        Some((1, 1)),
        "Whenever ~ deals combat damage to a player, that player discards a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let r = g.put(rat, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P1);
    g.combat(&[r], &[], &[], &[]);
    assert_eq!(g.count(Zone::Hand, P1), hand - 1);
}

// ---- library search ---------------------------------------------------------------------------

#[test]
fn ramp_finds_a_basic_land_tapped_and_shuffles() {
    let mut t = Table::default();
    let growth = t.card(
        "{1}{G}",
        "Sorcery",
        None,
        "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(growth, P0, Zone::Hand);
    let f = g.put(forest, P0, Zone::Library);
    g.main();
    let shuffles = |g: &Game| {
        g.engine
            .log
            .iter()
            .filter(|e| matches!(e.event, mtg_core::Event::Shuffled { .. }))
            .count()
    };
    let before = shuffles(&g);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![f])],
    );
    let now = g.find(forest).expect("the forest is on the battlefield");
    assert!(g.engine.state.objects[&now].tapped);
    assert_eq!(shuffles(&g), before + 1);
}

#[test]
fn a_search_may_find_nothing() {
    let mut t = Table::default();
    let growth = t.card(
        "{G}",
        "Sorcery",
        None,
        "Search your library for a basic land card, reveal it, put it into your hand, then shuffle.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(growth, P0, Zone::Hand);
    g.put(forest, P0, Zone::Library);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![])],
    );
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand - 1,
        "chose to find nothing (CR 701.19b)"
    );
}

#[test]
fn landcycling_fetches_a_land() {
    let mut t = Table::default();
    let beast = t.card(
        "{6}{G}",
        "Creature — Elf",
        Some((6, 6)),
        "Forestcycling {1}",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(beast, P0, Zone::Hand);
    let f = g.put(forest, P0, Zone::Library);
    g.main();
    g.act(
        activate(b, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![f])],
    );
    let in_hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == forest);
    assert!(in_hand);
}

// ---- recursion, counters, token abilities, attaching --------------------------------------------

#[test]
fn reanimation_returns_to_the_battlefield() {
    let mut t = Table::default();
    let raise = t.card(
        "{2}{B}",
        "Sorcery",
        None,
        "Return target creature card from your graveyard to the battlefield.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let dead = g.put(bear, P0, Zone::Graveyard);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[Target::Object(dead)]);
    assert!(g.find(bear).is_some());
}

#[test]
fn charge_counters_count() {
    let mut t = Table::default();
    let battery = t.card(
        "{2}",
        "Artifact",
        None,
        "~ enters with two charge counters on it.\n{T}: You gain 1 life for each charge counter on ~.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(battery, P0, Zone::Hand);
    g.main();
    g.cast(b, &[]);
    let now = g.find(battery).unwrap();
    g.act(activate(now, 1), &[], &[]);
    assert_eq!(g.life(P0), 22);
}

#[test]
fn a_token_with_a_quoted_ability() {
    let mut t = Table::default();
    let raise = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a 1/1 black Zombie creature token with \"This token can't block.\"",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let r = g.put(raise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[]);
    let token = tokens(&g)[0];
    let bear = g.table.bear();
    let attacker = g.put(bear, P1, Zone::Battlefield);
    g.engine
        .state
        .combat
        .attackers
        .insert(attacker, Target::Player(P0));
    let blockers: Vec<_> = mtg_engine::combat::eligible_blockers(&g.engine.state, &g.table, P0)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(!blockers.contains(&token));
}

#[test]
fn an_equipment_that_makes_its_own_bearer() {
    let mut t = Table::default();
    let kit = t.card(
        "{2}",
        "Artifact — Equipment",
        None,
        "When this Equipment enters, create a 1/1 white Soldier creature token, then attach this Equipment to it.\nEquipped creature gets +1/+1.\nEquip {2}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let k = g.put(kit, P0, Zone::Hand);
    g.main();
    g.cast(k, &[]);
    let token = tokens(&g)[0];
    let e = g.find(kit).unwrap();
    assert_eq!(g.engine.state.objects[&e].attached_to, Some(token));
    assert_eq!(g.pt(token), (2, 2));
}

#[test]
fn a_treasure_pays_but_only_after_the_lands() {
    let mut t = Table::default();
    let loot = t.card("{R}", "Sorcery", None, "Create a Treasure token.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let l = g.put(loot, P0, Zone::Hand);
    let b1 = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(l, &[]);
    assert_eq!(tokens(&g).len(), 1);
    let _ = g.until(P0, Step::PrecombatMain);
    // Next turn: one land untaps; {1}{G} needs the land and the Treasure.
    g.until(P0, Step::End);
    g.until(P0, Step::PrecombatMain);
    g.cast(b1, &[]);
    assert!(g.find(bear).is_some(), "cast with land + treasure");
    assert!(tokens(&g).is_empty(), "the treasure was sacrificed");
}

#[test]
fn a_treasure_is_left_alone_when_lands_suffice() {
    let mut t = Table::default();
    let loot = t.card("{R}", "Sorcery", None, "Create a Treasure token.");
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to target player.");
    let mut g = Game::new(t);
    g.lands(2);
    let l = g.put(loot, P0, Zone::Hand);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(l, &[]);
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(
        tokens(&g).len(),
        1,
        "paid with the second land, not the treasure"
    );
}

// ---- destruction ---------------------------------------------------------------------------------

#[test]
fn indestructible_survives_lethal_damage() {
    let mut t = Table::default();
    let wall = t.card("{W}", "Creature — Soldier", Some((1, 1)), "Indestructible");
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(wall, P0, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Object(w)]);
    assert!(g.engine.state.objects.contains_key(&w), "CR 702.12b");
}

#[test]
fn regeneration_replaces_one_destruction() {
    let mut t = Table::default();
    let ghoul = t.card(
        "{B}",
        "Creature — Soldier",
        Some((2, 2)),
        "{B}: Regenerate ~.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let gh = g.put(ghoul, P0, Zone::Battlefield);
    let b1 = g.put(bolt, P0, Zone::Hand);
    let b2 = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.act(activate(gh, 0), &[], &[]);
    g.cast(b1, &[Target::Object(gh)]);
    assert!(g.engine.state.objects.contains_key(&gh), "regenerated");
    assert!(g.engine.state.objects[&gh].tapped, "and tapped");
    assert_eq!(
        g.engine.state.objects[&gh].damage, 0,
        "with its damage removed"
    );
    g.cast(b2, &[Target::Object(gh)]);
    assert!(
        !g.engine.state.objects.contains_key(&gh),
        "the shield was used up"
    );
}

#[test]
fn protection_from_red() {
    let mut t = Table::default();
    let knight = t.card(
        "{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Protection from red",
    );
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let k = g.put(knight, P0, Zone::Battlefield);
    let gob = g.put(goblin, P1, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    let legal = |g: &Game| {
        let spell = g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))[0];
        let spec = &g
            .table
            .face(g.engine.state.objects[&spell].card, 0)
            .unwrap()
            .abilities[0]
            .targets[0];
        mtg_engine::targeting::legal_targets(&g.engine.state, &g.table, spec, spell, P0, &[])
    };
    use mtg_ir::PrintedCards;
    let _ = b;
    assert!(
        !legal(&g).contains(&Target::Object(k)),
        "can't be targeted by red"
    );
    assert!(
        !mtg_engine::combat::can_block(&g.engine.state, &g.table, gob, k),
        "can't be blocked by red creatures"
    );
}

#[test]
fn infect_poisons_players_and_withers_creatures() {
    let mut t = Table::default();
    let mite = t.card("{G}", "Creature — Elf", Some((2, 2)), "Infect");
    let wall = t.card("{0}", "Creature — Soldier", Some((0, 4)), "");
    let mut g = Game::new(t);
    let m = g.put(mite, P0, Zone::Battlefield);
    let w = g.put(wall, P1, Zone::Battlefield);
    g.main();
    g.combat(&[m], &[], &[], &[]);
    assert_eq!(g.life(P1), 20, "no life lost");
    assert_eq!(g.engine.state.player(P1).poison, 2);
    g.until(P0, Step::End);
    g.main();
    g.combat(&[m], &[(w, m)], &[], &[]);
    assert_eq!(g.pt(w), (-2, 2), "-1/-1 counters, not damage");
    assert_eq!(g.engine.state.objects[&w].damage, 0);
}

#[test]
fn flashback_casts_from_the_graveyard_and_exiles() {
    let mut t = Table::default();
    let think = t.card("{U}", "Sorcery", None, "Draw a card.\nFlashback {2}{U}");
    let mut g = Game::new(t);
    g.lands(4);
    let th = g.put(think, P0, Zone::Hand);
    g.main();
    g.cast(th, &[]);
    let in_yard = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Graveyard, P0))[0];
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == in_yard)),
        "offered from the graveyard"
    );
    let hand = g.count(Zone::Hand, P0);
    g.cast(in_yard, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        0,
        "exiled, not back in the graveyard"
    );
    let untapped = g
        .engine
        .state
        .battlefield()
        .iter()
        .filter(|id| !g.engine.state.objects[id].tapped)
        .count();
    assert_eq!(untapped, 0, "{{U}} then {{2}}{{U}}: all four lands");
}

#[test]
fn kicker_is_announced_paid_and_checked() {
    let mut t = Table::default();
    let burst = t.card(
        "{R}",
        "Instant",
        None,
        "Kicker {2}\n~ deals 2 damage to target player. If this spell was kicked, you gain 3 life.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let a = g.put(burst, P0, Zone::Hand);
    let b = g.put(burst, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: a },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!((g.life(P0), g.life(P1)), (23, 18), "kicked");
    // One land left: the kicker can't be paid, so it isn't asked.
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!((g.life(P0), g.life(P1)), (23, 16), "not kicked");
}

// ---- costs that are a choice ------------------------------------------------------------------

#[test]
fn a_sacrifice_outlet_chooses_what_to_sacrifice() {
    let mut t = Table::default();
    let altar = t.card(
        "{1}",
        "Artifact",
        None,
        "Sacrifice a creature: You gain 2 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    let b1 = g.put(bear, P0, Zone::Battlefield);
    let b2 = g.put(bear, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(offers(&actions, a));
    g.act(
        activate(a, 0),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![b2])],
    );
    assert_eq!(g.life(P0), 22);
    assert!(g.engine.state.objects.contains_key(&b1));
    assert!(
        !g.engine.state.objects.contains_key(&b2),
        "the chosen one was sacrificed"
    );
}

#[test]
fn a_sacrifice_outlet_needs_something_to_sacrifice() {
    let mut t = Table::default();
    let altar = t.card(
        "{1}",
        "Artifact",
        None,
        "Sacrifice a creature: You gain 2 life.",
    );
    let mut g = Game::new(t);
    let a = g.put(altar, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(!offers(&actions, a), "no creature, no activation");
}

#[test]
fn an_additional_casting_cost() {
    let mut t = Table::default();
    let bargain = t.card(
        "{B}",
        "Instant",
        None,
        "As an additional cost to cast this spell, sacrifice a creature.\nDraw two cards.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(bargain, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == s)),
        "nothing to sacrifice"
    );
    let b = g.put(bear, P0, Zone::Battlefield);
    let _ = g.until(P0, Step::End);
    let _ = g.until(mtg_core::PlayerId(1), Step::End);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(s, &[]);
    assert!(!g.engine.state.objects.contains_key(&b));
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2);
}

// ---- "if you do" and "where X is" -----------------------------------------------------------

#[test]
fn you_may_pay_and_if_you_do() {
    let mut t = Table::default();
    let seer = t.card(
        "{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "When ~ enters, you may pay {1}. If you do, draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(seer, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand, "cast one, paid, drew one");
}

#[test]
fn you_may_discard_if_you_do_draw_only_with_a_hand() {
    let mut t = Table::default();
    let looter = t.card(
        "{U}",
        "Sorcery",
        None,
        "You may discard a card. If you do, draw two cards.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let l = g.put(looter, P0, Zone::Hand);
    g.main();
    // Empty the rest of the hand, so nothing can be discarded.
    for id in g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
    {
        if id != l {
            g.engine.state.objects.remove(&id);
            let hand = mtg_core::ZoneRef::of(Zone::Hand, P0);
            if let Some(o) = g.engine.state.zone_order.get_mut(&hand) {
                o.retain(|x| *x != id);
            }
        }
    }
    let _ = g.until(P0, Step::PrecombatMain);
    g.cast(l, &[]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        0,
        "nothing to discard, so nothing drawn"
    );
}

#[test]
fn where_x_is_a_count() {
    let mut t = Table::default();
    let rally = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    let r = g.put(rally, P0, Zone::Hand);
    g.main();
    g.cast(r, &[Target::Object(b)]);
    assert_eq!(g.pt(b), (4, 4));
}

#[test]
fn an_x_activated_ability() {
    let mut t = Table::default();
    let shade = t.card(
        "{B}",
        "Creature — Wizard",
        Some((1, 1)),
        "{X}: ~ gets +X/+0 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(shade, P0, Zone::Battlefield);
    g.main();
    g.act(
        activate(s, 0),
        &[],
        &[mtg_engine::choice::Answer::Number(3)],
    );
    assert_eq!(g.pt(s), (4, 1));
}

#[test]
fn last_known_power_after_destruction() {
    let mut t = Table::default();
    let chastise = t.card(
        "{3}{W}",
        "Instant",
        None,
        "Destroy target creature. You gain life equal to its power.",
    );
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(big, P1, Zone::Battlefield);
    let c = g.put(chastise, P0, Zone::Hand);
    g.main();
    g.cast(c, &[Target::Object(b)]);
    assert!(g.find(big).is_none());
    assert_eq!(g.life(P0), 24, "its power as it last existed (CR 608.2h)");
}

#[test]
fn a_permanent_remembers_x_and_kicker() {
    let mut t = Table::default();
    let hydra = t.card(
        "{X}{G}",
        "Creature — Elf",
        Some((0, 0)),
        "~ enters with X +1/+1 counters on it.",
    );
    let knight = t.card(
        "{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Kicker {1}\nWhen ~ enters, if it was kicked, you gain 4 life.",
    );
    let mut g = Game::new(t);
    g.lands(6);
    let h = g.put(hydra, P0, Zone::Hand);
    let k = g.put(knight, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: h },
        &[],
        &[mtg_engine::choice::Answer::Number(3)],
    );
    let now = g.find(hydra).expect("a 3/3, not a dead 0/0");
    assert_eq!(g.pt(now), (3, 3));
    g.act(
        Action::Cast { object: k },
        &[],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.life(P0), 24);
}

// ---- ward and taxing counters -------------------------------------------------------------------

#[test]
fn ward_counters_unless_paid() {
    let mut t = Table::default();
    let guard = t.card("{1}{U}", "Creature — Wizard", Some((2, 2)), "Ward {2}");
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let w = g.put(guard, P1, Zone::Battlefield);
    let b1 = g.put(bolt, P0, Zone::Hand);
    let b2 = g.put(bolt, P0, Zone::Hand);
    g.main();
    // Decline to pay: countered.
    g.act(
        Action::Cast { object: b1 },
        &[Target::Object(w)],
        &[mtg_engine::choice::Answer::Bool(false)],
    );
    assert!(
        g.engine.state.objects.contains_key(&w),
        "the bolt was countered"
    );
    // Two lands left after this bolt: pay {2}, and it resolves.
    g.act(
        Action::Cast { object: b2 },
        &[Target::Object(w)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    let _ = g.until(P0, Step::PrecombatMain);
    assert!(
        !g.engine.state.objects.contains_key(&w),
        "paid for, so it resolved"
    );
}

#[test]
fn ward_does_not_trigger_on_its_controllers_own_spells() {
    let mut t = Table::default();
    let guard = t.card("{1}{U}", "Creature — Wizard", Some((2, 2)), "Ward {2}");
    let giant = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(guard, P0, Zone::Battlefield);
    let s = g.put(giant, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(w)]);
    assert_eq!(g.pt(w), (5, 5));
}

#[test]
fn toxic_adds_poison_with_combat_damage() {
    let mut t = Table::default();
    let rat = t.card("{B}", "Creature — Soldier", Some((1, 1)), "Toxic 2");
    let mut g = Game::new(t);
    let r = g.put(rat, P0, Zone::Battlefield);
    g.main();
    g.combat(&[r], &[], &[], &[]);
    assert_eq!(g.life(P1), 19);
    assert_eq!(g.engine.state.player(P1).poison, 2);
}

// ---- flicker and delayed triggers --------------------------------------------------------------

#[test]
fn flicker_returns_the_card_and_it_enters_again() {
    let mut t = Table::default();
    let blink = t.card(
        "{W}",
        "Instant",
        None,
        "Exile target creature you control, then return that card to the battlefield under its owner's control.",
    );
    let seer = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "When ~ enters, you gain 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(seer, P0, Zone::Battlefield);
    let b = g.put(blink, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Object(s)]);
    assert!(g.find(seer).is_some(), "back on the battlefield");
    assert_ne!(g.find(seer), Some(s), "as a new object (CR 400.7)");
    assert_eq!(g.life(P0), 22, "and it entered again");
}

#[test]
fn a_delayed_draw_next_upkeep() {
    let mut t = Table::default();
    let later = t.card(
        "{U}",
        "Instant",
        None,
        "Draw a card at the beginning of the next turn's upkeep.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let l = g.put(later, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(l, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1, "nothing yet");
    g.until(mtg_core::PlayerId(1), Step::PrecombatMain);
    assert_eq!(g.count(Zone::Hand, P0), hand, "drew during the next upkeep");
    g.until(P0, Step::PrecombatMain);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 1,
        "only once: this is the normal draw"
    );
}

#[test]
fn exile_until_the_end_step() {
    let mut t = Table::default();
    let ghost = t.card(
        "{1}{W}",
        "Instant",
        None,
        "Exile target creature. Return it to the battlefield under its owner's control at the beginning of the next end step.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    g.put(bear, P1, Zone::Battlefield);
    let theirs = g.find(bear).unwrap();
    let s = g.put(ghost, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(theirs)]);
    assert!(g.find(bear).is_none(), "gone for now");
    g.until(mtg_core::PlayerId(1), Step::Upkeep);
    let back = g.find(bear).expect("returned at the end step");
    assert_eq!(g.engine.state.objects[&back].controller, P1);
}

// ---- fight, regeneration, cost changes, more nouns ----------------------------------------------

#[test]
fn fight_deals_damage_both_ways() {
    let mut t = Table::default();
    let brawl = t.card(
        "{G}",
        "Sorcery",
        None,
        "Target creature you control fights target creature you don't control.",
    );
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(big, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(brawl, P0, Zone::Hand);
    g.main();
    g.cast(b, &[Target::Object(mine), Target::Object(theirs)]);
    assert!(!g.engine.state.objects.contains_key(&theirs));
    assert_eq!(g.engine.state.objects[&mine].damage, 2);
}

#[test]
fn cant_be_regenerated() {
    let mut t = Table::default();
    let doom = t.card(
        "{1}{B}",
        "Instant",
        None,
        "Destroy target creature. It can't be regenerated.",
    );
    let ghoul = t.card(
        "{B}",
        "Creature — Soldier",
        Some((2, 2)),
        "{B}: Regenerate ~.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let gh = g.put(ghoul, P1, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&gh)
        .unwrap()
        .regeneration_shields = 1;
    let d = g.put(doom, P0, Zone::Hand);
    g.main();
    g.cast(d, &[Target::Object(gh)]);
    assert!(!g.engine.state.objects.contains_key(&gh));
}

#[test]
fn a_cost_reducer_makes_spells_castable() {
    let mut t = Table::default();
    let herald = t.card(
        "{1}",
        "Artifact",
        None,
        "Creature spells you cast cost {1} less to cast.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(herald, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == b)),
        "{{1}}{{G}} for {{G}}"
    );
    g.cast(b, &[]);
    assert!(g.find(bear).is_some());
}

#[test]
fn a_player_casts_trigger() {
    let mut t = Table::default();
    let shrine = t.card(
        "{W}",
        "Enchantment",
        None,
        "Whenever a player casts a red spell, you gain 1 life.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to target player.");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(shrine, P0, Zone::Battlefield);
    let s = g.put(shock, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.life(P0), 21);
}

#[test]
fn nouns_permanent_card_and_comma_adjectives() {
    let mut t = Table::default();
    let raise = t.card(
        "{G}",
        "Sorcery",
        None,
        "Return target permanent card from your graveyard to your hand.",
    );
    let doom = t.card(
        "{B}",
        "Instant",
        None,
        "Destroy target nonartifact, nonblack creature.",
    );
    let rock = t.card("{1}", "Artifact", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let r = g.put(rock, P0, Zone::Graveyard);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let a = g.put(raise, P0, Zone::Hand);
    let d = g.put(doom, P0, Zone::Hand);
    g.main();
    g.cast(a, &[Target::Object(r)]);
    g.cast(d, &[Target::Object(theirs)]);
    assert!(!g.engine.state.objects.contains_key(&theirs));
}

#[test]
fn crewing_a_vehicle() {
    let mut t = Table::default();
    let cart = t.card("{2}", "Artifact — Vehicle", Some((4, 4)), "Trample\nCrew 2");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    let v = g.put(cart, P0, Zone::Battlefield);
    let e1 = g.put(elf, P0, Zone::Battlefield);
    let e2 = g.put(elf, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(offers(&actions, v));
    g.act(
        activate(v, 1),
        &[],
        &[mtg_engine::choice::Answer::Objects(vec![e1, e2])],
    );
    assert!(g.engine.state.objects[&e1].tapped && g.engine.state.objects[&e2].tapped);
    let ch = g.engine.characteristics(&g.table, v).unwrap().clone();
    assert!(ch.has_type(mtg_core::CardType::Creature));
    g.combat(&[v], &[], &[], &[]);
    assert_eq!(g.life(P1), 16);
}

#[test]
fn crew_needs_enough_power() {
    let mut t = Table::default();
    let cart = t.card("{2}", "Artifact — Vehicle", Some((4, 4)), "Crew 3");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    let v = g.put(cart, P0, Zone::Battlefield);
    g.put(elf, P0, Zone::Battlefield);
    g.put(elf, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(!offers(&actions, v), "2 power can't crew 3");
}

// ---- lands that check, lands that hurt, cards that come back ------------------------------------

#[test]
fn a_checkland_enters_untapped_with_the_right_land() {
    let mut t = Table::default();
    let check = t.card(
        "",
        "Land",
        None,
        "This land enters tapped unless you control a Forest or a Plains.\n{T}: Add {G} or {W}.",
    );
    let forest = t.card("", "Basic Land — Forest", None, "");
    let mut g = Game::new(t);
    let c1 = g.put(check, P0, Zone::Hand);
    let c2 = g.put(check, P0, Zone::Hand);
    g.main();
    g.act(Action::PlayLand { object: c1 }, &[], &[]);
    let first = g.find(check).unwrap();
    assert!(
        g.engine.state.objects[&first].tapped,
        "no Forest or Plains yet"
    );
    g.put(forest, P0, Zone::Battlefield);
    g.until(P0, Step::End);
    g.main();
    g.act(Action::PlayLand { object: c2 }, &[], &[]);
    let second = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == check && *id != first)
        .unwrap();
    assert!(!g.engine.state.objects[&second].tapped);
}

#[test]
fn a_painland_hurts() {
    let mut t = Table::default();
    let pain = t.card(
        "",
        "Land",
        None,
        "{T}: Add {C}.\n{T}: Add {R} or {G}. This land deals 1 damage to you.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let p = g.put(pain, P0, Zone::Battlefield);
    let _ = p;
    let b = g.put(bear, P0, Zone::Hand);
    g.lands(1);
    g.main();
    g.cast(b, &[]);
    assert_eq!(g.life(P0), 19, "the green came from the painland");
}

#[test]
fn an_aura_that_returns_to_hand() {
    let mut t = Table::default();
    let aura = t.card(
        "{W}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature gets +1/+1.\nWhen this Aura is put into a graveyard from the battlefield, return it to its owner's hand.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 5 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let a = g.put(aura, P0, Zone::Hand);
    let k = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(a, &[Target::Object(b)]);
    let hand = g.count(Zone::Hand, P0);
    g.cast(k, &[Target::Object(b)]);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "bolt left the hand, the aura came back"
    );
    assert!(
        g.engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == aura)
    );
}

#[test]
fn undying_returns_once() {
    let mut t = Table::default();
    let ghoul = t.card("{1}{B}", "Creature — Soldier", Some((2, 2)), "Undying");
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 5 damage to target creature.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    g.put(ghoul, P0, Zone::Battlefield);
    let b1 = g.put(bolt, P0, Zone::Hand);
    let b2 = g.put(bolt, P0, Zone::Hand);
    g.main();
    let first = g.find(ghoul).unwrap();
    g.cast(b1, &[Target::Object(first)]);
    let back = g.find(ghoul).expect("returned");
    assert_eq!(g.pt(back), (3, 3), "with a +1/+1 counter");
    g.cast(b2, &[Target::Object(back)]);
    assert!(
        g.find(ghoul).is_none(),
        "it had a counter, so it stays dead"
    );
}

#[test]
fn convoke_taps_creatures_to_pay() {
    let mut t = Table::default();
    let call = t.card(
        "{3}{W}",
        "Sorcery",
        None,
        "Convoke\nCreate two 1/1 white Soldier creature tokens.",
    );
    let knight = t.card("{W}", "Creature — Soldier", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(knight, P0, Zone::Battlefield);
    let b1 = g.put(bear, P0, Zone::Battlefield);
    let b2 = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(call, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Cast { object } if *object == c)),
        "one land and three creatures pay {{3}}{{W}}"
    );
    g.cast(c, &[]);
    assert_eq!(tokens(&g).len(), 2);
    for id in [w, b1, b2] {
        assert!(g.engine.state.objects[&id].tapped, "tapped to convoke");
    }
}

#[test]
fn paid_mana_ability_spends_input_before_producing_output() {
    let mut t = Table::default();
    let filter = t.card("", "Land", None, "{1}, {T}: Add {G}{G}.");
    let spell = t.card("{G}{G}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    let source = g.put(filter, P0, Zone::Battlefield);
    let cast = g.put(spell, P0, Zone::Hand);
    g.lands(1);
    g.main();
    let ability = g.table.face(filter, 0).unwrap().abilities[0].id;
    assert!(
        mtg_engine::mana::manual_source(&g.engine.state, &g.table, P0, source, ability).is_some()
    );
    g.act(
        Action::ActivateManaAbility {
            source,
            ability,
            color: None,
        },
        &[],
        &[],
    );
    assert!(g.engine.state.objects[&source].tapped);
    assert_eq!(g.engine.state.player(P0).mana.total(), 2);
    g.cast(cast, &[]);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
}

#[test]
fn mana_source_cannot_fund_its_own_paid_activation() {
    let mut t = Table::default();
    let filter = t.card("", "Land", None, "{T}: Add {C}.\n{1}, {T}: Add {G}{G}.");
    let mut g = Game::new(t);
    let source = g.put(filter, P0, Zone::Battlefield);
    g.main();
    let ability = g.table.face(filter, 0).unwrap().abilities[1].id;
    assert!(
        mtg_engine::mana::manual_source(&g.engine.state, &g.table, P0, source, ability).is_none()
    );
}

#[test]
fn mana_ability_pays_life_and_requires_enough_life() {
    let mut t = Table::default();
    let land = t.card(
        "",
        "Land",
        None,
        "{T}, Pay 2 life: Add one mana of any color.",
    );
    let mut g = Game::new(t);
    let source = g.put(land, P0, Zone::Battlefield);
    g.main();
    let ability = g.table.face(land, 0).unwrap().abilities[0].id;
    g.act(
        Action::ActivateManaAbility {
            source,
            ability,
            color: Some(mtg_core::Color::Green),
        },
        &[],
        &[],
    );
    assert_eq!(g.engine.state.player(P0).life, 18);
    assert_eq!(g.engine.state.player(P0).mana.total(), 1);
    g.engine.state.objects.get_mut(&source).unwrap().tapped = false;
    g.engine.state.players.get_mut(&P0).unwrap().life = 1;
    assert!(
        mtg_engine::mana::manual_source(&g.engine.state, &g.table, P0, source, ability).is_none()
    );
}

#[test]
fn filter_mana_can_produce_different_fixed_colors() {
    let mut t = Table::default();
    let filter = t.card("", "Land", None, "{1}, {T}: Add {W}{U}.");
    let spell = t.card("{W}{U}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    let source = g.put(filter, P0, Zone::Battlefield);
    let cast = g.put(spell, P0, Zone::Hand);
    g.lands(1);
    g.main();
    let ability = g.table.face(filter, 0).unwrap().abilities[0].id;
    g.act(
        Action::ActivateManaAbility {
            source,
            ability,
            color: None,
        },
        &[],
        &[],
    );
    assert_eq!(g.engine.state.player(P0).mana.total(), 2);
    g.cast(cast, &[]);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
}

#[test]
fn loyalty_is_paid_before_resolution_and_shared_between_abilities() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "{1}",
        "Planeswalker",
        3,
        "+1: You gain 2 life.\n−2: Draw a card.\n0: You gain 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act_holding(activate(source, 0), &[]);
    assert_eq!(
        g.engine.state.objects[&source].counters[&mtg_core::CounterKind::Loyalty],
        4
    );
    assert_eq!(g.life(P0), 20, "effect is still on the stack");
    let mtg_engine::Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!("priority")
    };
    let mtg_engine::choice::ChoiceKind::Priority { legal } = &c.kind else {
        panic!("priority")
    };
    assert!(!offers(&legal.actions, source));
    assert!(
        g.engine
            .answer(
                &g.table,
                c.id,
                mtg_engine::choice::Answer::Action(activate(source, 1))
            )
            .is_err()
    );
    let actions = g.main();
    assert_eq!(g.life(P0), 22);
    assert!(!offers(&actions, source));
    g.until(P1, Step::PrecombatMain);
    let next = g.until(P0, Step::PrecombatMain);
    assert!(offers(&next, source), "limit resets on the next turn");
    g.act(activate(source, 1), &[], &[]);
    assert_eq!(
        g.engine.state.objects[&source].counters[&mtg_core::CounterKind::Loyalty],
        2
    );
}

#[test]
fn zero_loyalty_cost_still_uses_the_permanent_activation() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "{1}",
        "Planeswalker",
        3,
        "0: You gain 1 life.\n+1: Draw a card.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act(activate(source, 0), &[], &[]);
    assert_eq!(g.life(P0), 21);
    assert_eq!(
        g.engine.state.objects[&source].counters[&mtg_core::CounterKind::Loyalty],
        3
    );
    assert!(!offers(&g.main(), source));
}

#[test]
fn loyalty_minus_requires_counters_and_can_spend_the_last_one() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "{1}",
        "Planeswalker",
        3,
        "-4: You gain 4 life.\n-3: You gain 3 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    let actions = g.main();
    assert!(!actions.contains(&activate(source, 0)));
    assert!(actions.contains(&activate(source, 1)));
    g.act(activate(source, 1), &[], &[]);
    assert!(
        g.find(walker).is_none(),
        "zero loyalty dies before the effect resolves"
    );
    assert_eq!(g.life(P0), 23, "the ability survives its source");
}

#[test]
fn loyalty_abilities_require_own_main_phase_and_empty_stack() {
    let mut t = Table::default();
    let walker = t.loyalty("{1}", "Planeswalker", 3, "+1: You gain 1 life.");
    let spell = t.card("{1}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(walker, P0, Zone::Hand);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act_holding(Action::Cast { object: s }, &[]);
    let mtg_engine::Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!("priority")
    };
    let mtg_engine::choice::ChoiceKind::Priority { legal } = c.kind else {
        panic!("priority")
    };
    assert!(
        !offers(&legal.actions, source),
        "not in response to a spell"
    );
    g.main();
    assert!(
        !offers(&g.until(P0, Step::BeginCombat), source),
        "not during combat"
    );
    g.until(P1, Step::PrecombatMain);
    // The outstanding choice belongs to P1; answer it before checking P0's response.
    let mtg_engine::Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!("priority")
    };
    g.engine
        .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
        .unwrap();
    for _ in 0..100 {
        if let mtg_engine::Progress::NeedsChoice(c) = g.engine.advance(&g.table) {
            if c.who == P0 {
                let mtg_engine::choice::ChoiceKind::Priority { legal } = c.kind else {
                    panic!("priority")
                };
                assert!(
                    !offers(&legal.actions, source),
                    "not during an opponent's main phase"
                );
                return;
            }
            g.engine
                .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
                .unwrap();
        }
    }
    panic!("P0 never got priority");
}

fn put_walker(
    g: &mut Game,
    card: mtg_core::CardId,
    owner: mtg_core::PlayerId,
    loyalty: i32,
) -> mtg_core::ObjectId {
    let id = g.put(card, owner, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&id)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::Loyalty, loyalty);
    id
}

#[test]
fn combat_hits_a_planeswalker_instead_of_its_controller() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 3, "+1: You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 3);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat_at(&[(attacker, Target::Object(w))], &[]);
    assert_eq!(
        g.engine.state.objects[&w].counters[&mtg_core::CounterKind::Loyalty],
        1
    );
    assert_eq!(g.engine.state.objects[&w].damage, 0);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn planeswalkers_can_be_defended_by_their_controllers_creatures() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 3, "+1: You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 3);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let blocker = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat_at(&[(attacker, Target::Object(w))], &[(blocker, attacker)]);
    assert_eq!(
        g.engine.state.objects[&w].counters[&mtg_core::CounterKind::Loyalty],
        3
    );
    assert!(!g.engine.state.objects.contains_key(&attacker));
    assert!(!g.engine.state.objects.contains_key(&blocker));
}

#[test]
fn trample_and_lifelink_work_when_attacking_a_planeswalker() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 5, "0: You gain 1 life.");
    let beast = t.card("", "Creature", Some((5, 5)), "Trample, lifelink");
    let bear = t.bear();
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 5);
    let attacker = g.put(beast, P0, Zone::Battlefield);
    let blocker = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.combat_at(&[(attacker, Target::Object(w))], &[(blocker, attacker)]);
    assert_eq!(
        g.engine.state.objects[&w].counters[&mtg_core::CounterKind::Loyalty],
        2
    );
    assert_eq!(g.life(P0), 25);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn lethal_combat_damage_puts_a_planeswalker_in_the_graveyard() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 2, "0: You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 2);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.combat_at(&[(attacker, Target::Object(w))], &[]);
    assert!(!g.engine.state.objects.contains_key(&w));
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert_eq!(g.life(P1), 20);
}

fn attack_prompt(g: &mut Game) -> mtg_engine::Choice {
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::NeedsChoice(c) => {
                if matches!(
                    c.kind,
                    mtg_engine::choice::ChoiceKind::DeclareAttackers { .. }
                ) {
                    return c;
                }
                g.engine
                    .answer(&g.table, c.id, mtg_engine::choice::Answer::Pass)
                    .unwrap();
            }
            _ => panic!("game ended"),
        }
    }
    panic!("no attack prompt");
}

#[test]
fn attack_declarations_reject_duplicates_and_illegal_destinations() {
    use mtg_engine::choice::{Answer, ChoiceKind};
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 3, "0: You gain 1 life.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let own = put_walker(&mut g, walker, P0, 3);
    let opposing = put_walker(&mut g, walker, P1, 3);
    let attacker = g.put(bear, P0, Zone::Battlefield);
    let creature = g.put(bear, P1, Zone::Battlefield);
    g.main();
    let c = attack_prompt(&mut g);
    let ChoiceKind::DeclareAttackers { defenders, .. } = &c.kind else {
        panic!("attackers")
    };
    assert_eq!(defenders, &[Target::Player(P1), Target::Object(opposing)]);
    for answer in [
        Answer::Attackers(vec![(attacker, Target::Object(own))]),
        Answer::Attackers(vec![(attacker, Target::Object(creature))]),
        Answer::Attackers(vec![(attacker, Target::Player(P0))]),
        Answer::Attackers(vec![
            (attacker, Target::Player(P1)),
            (attacker, Target::Object(opposing)),
        ]),
        Answer::Objects(vec![attacker, attacker]),
        Answer::Objects(vec![mtg_core::ObjectId(9999)]),
    ] {
        assert!(g.engine.answer(&g.table, c.id, answer).is_err());
        assert!(g.engine.state.combat.attackers.is_empty());
        assert!(!g.engine.state.objects[&attacker].tapped);
    }
    let answer = Answer::Attackers(vec![(attacker, Target::Object(opposing))]);
    let wire = serde_json::to_vec(&answer).unwrap();
    let received: Answer = serde_json::from_slice(&wire).unwrap();
    g.engine.answer(&g.table, c.id, received).unwrap();
    let view = mtg_engine::view::project(&g.engine.state, P1);
    assert_eq!(
        view.visible[&attacker].attacking_target,
        Some(Target::Object(opposing))
    );
    let view: mtg_engine::PlayerView =
        serde_json::from_slice(&serde_json::to_vec(&view).unwrap()).unwrap();
    assert_eq!(
        view.visible[&attacker].attacking_target,
        Some(Target::Object(opposing))
    );
}

#[test]
fn a_departed_planeswalker_does_not_redirect_damage_or_grant_lifelink() {
    use mtg_engine::choice::{Answer, ChoiceKind};
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 3, "0: You gain 1 life.");
    let beast = t.card("", "Creature", Some((3, 3)), "Lifelink");
    let removal = t.card("", "Instant", None, "Destroy target planeswalker.");
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 3);
    let attacker = g.put(beast, P0, Zone::Battlefield);
    let spell = g.put(removal, P0, Zone::Hand);
    g.main();
    let c = attack_prompt(&mut g);
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Attackers(vec![(attacker, Target::Object(w))]),
        )
        .unwrap();
    let mut cast = false;
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::NeedsChoice(c) => {
                if g.engine.state.step == Step::PostcombatMain {
                    assert!(cast);
                    assert!(!g.engine.state.objects.contains_key(&w));
                    assert_eq!(g.life(P0), 20);
                    assert_eq!(g.life(P1), 20);
                    return;
                }
                let answer = match &c.kind {
                    ChoiceKind::Priority { .. } if !cast && c.who == P0 => {
                        cast = true;
                        Answer::Action(Action::Cast { object: spell })
                    }
                    ChoiceKind::ChooseTargets { .. } => {
                        Answer::Targets(vec![vec![Target::Object(w)]])
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            _ => panic!("game ended"),
        }
    }
    panic!("combat never ended");
}

#[test]
fn damage_to_a_planeswalker_fires_damage_triggers_once() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 4, "0: You gain 1 life.");
    let beast = t.card(
        "",
        "Creature",
        Some((2, 2)),
        "Whenever ~ deals combat damage, you gain 1 life.",
    );
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 4);
    let attacker = g.put(beast, P0, Zone::Battlefield);
    g.main();
    g.combat_at(&[(attacker, Target::Object(w))], &[]);
    assert_eq!(g.life(P0), 21);
    assert_eq!(
        g.engine.state.objects[&w].counters[&mtg_core::CounterKind::Loyalty],
        2
    );
}

#[test]
fn damage_to_a_creature_planeswalker_has_both_consequences_once() {
    let mut t = Table::default();
    let walker = t.creature_walker(5, (5, 5), "0: You gain 1 life.");
    let bolt = t.card("", "Instant", None, "~ deals 2 damage to any target.");
    let mut g = Game::new(t);
    let w = put_walker(&mut g, walker, P1, 5);
    let spell = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(w)]);
    assert_eq!(g.engine.state.objects[&w].damage, 2);
    assert_eq!(
        g.engine.state.objects[&w].counters[&mtg_core::CounterKind::Loyalty],
        3
    );
}

#[test]
fn loyalty_that_produces_mana_uses_the_stack() {
    let mut t = Table::default();
    let walker = t.loyalty("{1}", "Planeswalker", 3, "+1: Add {G}{G}.");
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act_holding(activate(source, 0), &[]);
    assert_eq!(g.engine.state.player(P0).mana.total(), 0);
    assert_eq!(g.stack().len(), 1);
    g.main();
    assert_eq!(g.engine.state.player(P0).mana.total(), 2);
}

#[test]
fn undo_restores_loyalty_and_the_permanent_activation_limit() {
    use mtg_engine::choice::Answer;
    let mut t = Table::default();
    let walker = t.loyalty("{1}", "Planeswalker", 3, "+1: You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act_holding(activate(source, 0), &[]);
    let mtg_engine::Progress::NeedsChoice(c) = g.engine.advance(&g.table) else {
        panic!("priority")
    };
    assert!(c.undo);
    g.engine.answer(&g.table, c.id, Answer::Undo).unwrap();
    let legal = g.main();
    assert!(legal.contains(&activate(source, 0)));
    assert_eq!(
        g.engine.state.objects[&source].counters[&mtg_core::CounterKind::Loyalty],
        3
    );
    assert!(g.stack().is_empty());
}

#[test]
fn a_targeted_loyalty_ability_pays_and_resolves_using_its_permanent_source() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "{1}",
        "Planeswalker",
        3,
        "-2: ~ deals 2 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let w = g.put(walker, P0, Zone::Hand);
    let victim = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.cast(w, &[]);
    let source = g.find(walker).unwrap();
    g.act(activate(source, 0), &[Target::Object(victim)], &[]);
    assert_eq!(
        g.engine.state.objects[&source].counters[&mtg_core::CounterKind::Loyalty],
        1
    );
    assert!(!g.engine.state.objects.contains_key(&victim));
}

fn drive_bot_through_combat(
    g: &mut Game,
    bot: &mtg_policy::bot::Bot,
    seat: mtg_core::PlayerId,
    scripted_attackers: Option<&[(mtg_core::ObjectId, Target)]>,
) {
    use mtg_engine::choice::{Answer, ChoiceKind};
    for _ in 0..10_000 {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::GameOver { .. } => return,
            mtg_engine::Progress::NeedsChoice(c) => {
                if matches!(c.kind, ChoiceKind::Priority { .. })
                    && g.engine.state.step == Step::PostcombatMain
                    && g.stack().is_empty()
                {
                    return;
                }
                let view = mtg_engine::view::project(&g.engine.state, c.who);
                let answer = if c.who == seat {
                    bot.decide(&c, &view)
                } else if matches!(c.kind, ChoiceKind::DeclareAttackers { .. }) {
                    match scripted_attackers {
                        Some(attackers) => Answer::Attackers(attackers.to_vec()),
                        None => mtg_policy::well_formed(&c, &view),
                    }
                } else {
                    mtg_policy::well_formed(&c, &view)
                };
                g.engine
                    .answer(&g.table, c.id, answer.clone())
                    .unwrap_or_else(|error| panic!("{error:?}: {:?} -> {answer:?}", c.kind));
            }
        }
    }
    panic!("bot never finished its turn");
}

#[test]
fn the_bot_activates_loyalty_once_then_attacks_an_enemy_planeswalker() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "",
        "Planeswalker",
        3,
        "+1: You gain 1 life.\n0: Draw a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let mine = put_walker(&mut g, walker, P0, 3);
    let enemy = put_walker(&mut g, walker, P1, 2);
    g.put(bear, P0, Zone::Battlefield);
    let bot = mtg_policy::bot::Bot::new(&g.table, [walker, bear]);
    drive_bot_through_combat(&mut g, &bot, P0, None);
    assert_eq!(
        g.engine.state.objects[&mine].counters[&mtg_core::CounterKind::Loyalty],
        4
    );
    assert_eq!(g.life(P0), 21);
    assert_eq!(g.life(P1), 20);
    assert!(!g.engine.state.objects.contains_key(&enemy));
    assert_eq!(
        g.engine
            .log
            .iter()
            .filter(|e| matches!(e.event,
        mtg_core::Event::AbilityActivated { source, loyalty: true, .. } if source == mine))
            .count(),
        1
    );
}

#[test]
fn the_bot_uses_a_minus_ability_and_targets_the_opponents_creature() {
    let mut t = Table::default();
    let walker = t.loyalty(
        "",
        "Planeswalker",
        3,
        "-2: ~ deals 2 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let mine = put_walker(&mut g, walker, P0, 3);
    let ally = g.put(bear, P0, Zone::Battlefield);
    let enemy = g.put(bear, P1, Zone::Battlefield);
    let bot = mtg_policy::bot::Bot::new(&g.table, [walker, bear]);
    drive_bot_through_combat(&mut g, &bot, P0, None);
    assert_eq!(
        g.engine.state.objects[&mine].counters[&mtg_core::CounterKind::Loyalty],
        1
    );
    assert!(!g.engine.state.objects.contains_key(&enemy));
    assert!(g.engine.state.objects.contains_key(&ally));
    assert_eq!(g.life(P1), 18);
}

#[test]
fn the_bot_blocks_an_attack_to_save_its_planeswalker() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 4, "0: You gain 1 life.");
    let large = t.card("", "Creature", Some((4, 4)), "");
    let small = t.card("", "Creature", Some((1, 1)), "");
    let mut g = Game::new(t);
    let defended = put_walker(&mut g, walker, P1, 4);
    let attacker = g.put(large, P0, Zone::Battlefield);
    let blocker = g.put(small, P1, Zone::Battlefield);
    let bot = mtg_policy::bot::Bot::new(&g.table, [walker, large, small]);
    drive_bot_through_combat(
        &mut g,
        &bot,
        P1,
        Some(&[(attacker, Target::Object(defended))]),
    );
    assert_eq!(
        g.engine.state.objects[&defended].counters[&mtg_core::CounterKind::Loyalty],
        4
    );
    assert!(!g.engine.state.objects.contains_key(&blocker));
    assert_eq!(g.life(P1), 20);
}

#[test]
fn the_bot_targets_itself_with_a_helpful_loyalty_ability() {
    let mut t = Table::default();
    let walker = t.loyalty("", "Planeswalker", 3, "+1: Target player draws a card.");
    let mut g = Game::new(t);
    let mine = put_walker(&mut g, walker, P0, 3);
    let bot = mtg_policy::bot::Bot::new(&g.table, [walker]);
    drive_bot_through_combat(&mut g, &bot, P0, None);
    assert_eq!(
        g.count(Zone::Library, P0),
        18,
        "turn draw plus its own ability"
    );
    assert_eq!(
        g.count(Zone::Library, P1),
        20,
        "opponent was not given a card"
    );
    assert_eq!(
        g.engine.state.objects[&mine].counters[&mtg_core::CounterKind::Loyalty],
        4
    );
}

// ---- "that much" ----------------------------------------------------------------

#[test]
fn that_much_is_the_damage_the_trigger_saw() {
    let mut t = Table::default();
    let vampire = t.card(
        "{2}{B}",
        "Creature — Vampire",
        Some((3, 3)),
        "Whenever ~ deals damage, you gain that much life.",
    );
    let mut g = Game::new(t);
    let v = g.put(vampire, P0, Zone::Battlefield);
    g.main();
    g.combat(&[v], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    assert_eq!(g.life(P0), 23, "3 damage, 3 life");
}

#[test]
fn that_many_counts_life_gained() {
    let mut t = Table::default();
    let cleric = t.card(
        "{1}{W}",
        "Creature — Cleric",
        Some((1, 1)),
        "Whenever you gain life, put that many +1/+1 counters on ~.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 4 life.");
    let mut g = Game::new(t);
    g.lands(1);
    let c = g.put(cleric, P0, Zone::Battlefield);
    let s = g.put(salve, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 24);
    assert_eq!(g.pt(c), (5, 5));
}

// ---- "one or more" (CR 603.2c) ----------------------------------------------------

#[test]
fn one_or_more_triggers_once_per_batch() {
    let mut t = Table::default();
    let captain = t.card(
        "{2}{U}",
        "Creature — Pirate",
        Some((1, 1)),
        "Whenever one or more creatures you control deal combat damage to a player, draw a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(captain, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[c, b], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand + 1,
        "one batch of combat damage"
    );
}

#[test]
fn a_creature_dealing_damage_triggers_for_each() {
    let mut t = Table::default();
    let captain = t.card(
        "{2}{U}",
        "Creature — Pirate",
        Some((1, 1)),
        "Whenever a creature you control deals combat damage to a player, draw a card.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(captain, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.combat(&[c, b], &[], &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
}

#[test]
fn one_or_more_creatures_dying_to_a_wrath_is_one_trigger() {
    let mut t = Table::default();
    let watcher = t.card(
        "{2}{B}",
        "Enchantment",
        None,
        "Whenever one or more creatures die, you gain 1 life.",
    );
    let wrath = t.card("{2}{W}{W}", "Sorcery", None, "Destroy all creatures.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    g.put(watcher, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let w = g.put(wrath, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
    assert_eq!(g.life(P0), 21, "one trigger for three deaths");
}

#[test]
fn one_player_two_verbs() {
    let mut t = Table::default();
    let sign = t.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Target player draws two cards and loses 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(sign, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P1);
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P1), hand + 2);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn an_extra_turn_comes_next() {
    let mut t = Table::default();
    let walk = t.card(
        "{1}{U}",
        "Sorcery",
        None,
        "Take an extra turn after this one.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(walk, P0, Zone::Hand);
    g.main();
    let turn = g.engine.state.turn;
    g.cast(w, &[]);
    assert_eq!(g.engine.state.extra_turns, vec![P0]);
    g.until(P0, Step::End);
    g.until(P0, Step::Upkeep);
    assert_eq!(g.engine.state.turn, turn + 1, "P0 again, straight away");
    g.until(P1, Step::Upkeep);
    assert_eq!(g.engine.state.turn, turn + 2, "then P1, as usual");
    g.until(P0, Step::Upkeep);
    assert_eq!(g.engine.state.turn, turn + 3);
}

#[test]
fn a_creature_that_would_die_is_exiled_instead() {
    let mut t = Table::default();
    let scorch = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature. If that creature would die this turn, exile it \
         instead.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(scorch, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 0, "not in the graveyard");
    assert_eq!(
        g.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
}

#[test]
fn edict_of_their_choice_and_cheating_from_hand() {
    let mut t = Table::default();
    let edict = t.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Each opponent sacrifices a creature of their choice.",
    );
    let sneak = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "You may put a creature card from your hand onto the battlefield.",
    );
    let wipe = t.card("{1}{R}", "Sorcery", None, "Discard your hand, then draw two cards.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(6);
    g.put(bear, P1, Zone::Battlefield);
    let e = g.put(edict, P0, Zone::Hand);
    let s = g.put(sneak, P0, Zone::Hand);
    let w = g.put(wipe, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    g.main();
    g.cast(e, &[]);
    assert_eq!(g.count(Zone::Graveyard, P1), 1, "the bear was sacrificed");
    g.act(
        Action::Cast { object: s },
        &[],
        &[
            mtg_engine::choice::Answer::Bool(true),
            mtg_engine::choice::Answer::Objects(vec![b]),
        ],
    );
    assert!(g.find(bear).is_some(), "put onto the battlefield");
    let hand = g.count(Zone::Hand, P0);
    g.cast(w, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 2, "discarded {} and drew two", hand - 1);
}
