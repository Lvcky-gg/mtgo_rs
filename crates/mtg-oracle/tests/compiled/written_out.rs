//! Keywords the compiler reads as the abilities their rules spell out: exploit, For
//! Mirrodin!, living metal.

use super::harness::*;
use mtg_core::{CardType, Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

fn butcher(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{4}{B}",
        "Creature — Zombie",
        Some((4, 3)),
        "Exploit (When this creature enters, you may sacrifice a creature.)\n\
         When this creature exploits a creature, target creature gets -3/-3 until end of turn.",
    )
}

#[test]
fn exploit_sacrifices_then_targets() {
    let mut t = Table::default();
    let butcher = butcher(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(butcher, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: b },
        &[Target::Object(theirs)],
        &[Answer::Bool(true), Answer::Objects(vec![mine])],
    );
    assert!(!g.engine.state.objects.contains_key(&mine), "sacrificed");
    assert!(!g.engine.state.objects.contains_key(&theirs), "took -3/-3");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}

#[test]
fn exploit_declined_does_nothing() {
    let mut t = Table::default();
    let butcher = butcher(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(butcher, P0, Zone::Hand);
    g.main();
    // No target is asked for: the "when you do" ability never triggers.
    g.act(Action::Cast { object: b }, &[], &[Answer::Bool(false)]);
    assert!(g.engine.state.objects.contains_key(&mine));
    assert!(g.engine.state.objects.contains_key(&theirs));
    assert_eq!(g.pt(theirs), (2, 2));
}

#[test]
fn exploit_may_sacrifice_itself() {
    let mut t = Table::default();
    let aven = t.card(
        "{3}{B}",
        "Creature — Zombie",
        Some((2, 3)),
        "Exploit\nWhen this creature exploits a creature, you draw two cards and you lose 2 \
         life.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let a = g.put(aven, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: a },
        &[],
        &[Answer::Bool(true), Answer::Objects(vec![a])],
    );
    assert_eq!(g.find(aven), None, "it sacrificed itself");
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2);
    assert_eq!(g.life(P0), 18);
}

#[test]
fn for_mirrodin_makes_and_equips_a_rebel() {
    let mut t = Table::default();
    let splitter = t.card(
        "{1}{R}",
        "Artifact — Equipment",
        None,
        "For Mirrodin! (When this Equipment enters, create a 2/2 red Rebel creature token, \
         then attach this to it.)\nEquipped creature gets +2/+0.\nEquip {2}{R}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(splitter, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let rebels: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect();
    assert_eq!(rebels.len(), 1, "one Rebel token");
    assert_eq!(g.pt(rebels[0]), (4, 2), "2/2 equipped with +2/+0");
}

#[test]
fn living_metal_is_a_creature_only_on_your_turn() {
    let mut t = Table::default();
    let walker = t.card(
        "{3}",
        "Artifact — Vehicle",
        Some((3, 3)),
        "Living metal (During your turn, this Vehicle is also a creature.)\nCrew 2",
    );
    let mut g = Game::new(t);
    let w = g.put(walker, P0, Zone::Battlefield);
    g.main();
    let types = |g: &mut Game| {
        g.engine
            .characteristics(&g.table, w)
            .unwrap()
            .card_types
            .clone()
    };
    assert!(types(&mut g).contains(&CardType::Creature));
    g.combat(&[w], &[], &[], &[]);
    assert_eq!(g.life(P1), 17, "it attacked as a 3/3");
    g.until(P1, Step::PrecombatMain);
    assert!(!types(&mut g).contains(&CardType::Creature));
}

fn shrink(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{1}{B}",
        "Instant",
        None,
        "Bargain (You may sacrifice an artifact, enchantment, or token as you cast this \
         spell.)\nTarget creature gets -3/-3 until end of turn. If this spell was bargained, \
         that creature gets -5/-5 until end of turn instead.",
    )
}

#[test]
fn bargain_sacrifices_a_token_for_the_bigger_effect() {
    let mut t = Table::default();
    let shrink = shrink(&mut t);
    let zombies = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a 2/2 black Zombie creature token.",
    );
    let ogre = t.card("{3}", "Creature — Bear", Some((5, 5)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let z = g.put(zombies, P0, Zone::Hand);
    let s = g.put(shrink, P0, Zone::Hand);
    g.main();
    g.cast(z, &[]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("a Zombie");
    g.act(
        Action::Cast { object: s },
        &[Target::Object(o)],
        &[Answer::Bool(true), Answer::Objects(vec![token])],
    );
    assert!(!g.engine.state.objects.contains_key(&token), "bargained");
    assert!(
        !g.engine.state.objects.contains_key(&o),
        "-5/-5 killed the 5/5"
    );
}

#[test]
fn bargain_declined_is_the_smaller_effect() {
    let mut t = Table::default();
    let shrink = shrink(&mut t);
    let ogre = t.card("{3}", "Creature — Bear", Some((5, 5)), "");
    let relic = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let r = g.put(relic, P0, Zone::Battlefield);
    let s = g.put(shrink, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[Target::Object(o)],
        &[Answer::Bool(false)],
    );
    assert!(
        g.engine.state.objects.contains_key(&r),
        "the artifact stays"
    );
    assert_eq!(g.pt(o), (2, 2));
}

#[test]
fn bargain_may_sacrifice_an_artifact() {
    let mut t = Table::default();
    let shrink = shrink(&mut t);
    let ogre = t.card("{3}", "Creature — Bear", Some((5, 5)), "");
    let relic = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let r = g.put(relic, P0, Zone::Battlefield);
    let s = g.put(shrink, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: s },
        &[Target::Object(o)],
        &[Answer::Bool(true), Answer::Objects(vec![r])],
    );
    assert!(!g.engine.state.objects.contains_key(&r));
    assert!(!g.engine.state.objects.contains_key(&o));
}

#[test]
fn destroy_all_tokens_spares_cards() {
    let mut t = Table::default();
    let wipe = t.card("{2}{W}", "Sorcery", None, "Destroy all tokens.");
    let zombies = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a 2/2 black Zombie creature token.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let b = g.put(bear, P0, Zone::Battlefield);
    let z = g.put(zombies, P0, Zone::Hand);
    let w = g.put(wipe, P0, Zone::Hand);
    g.main();
    g.cast(z, &[]);
    assert_eq!(g.engine.state.battlefield().len(), 4 + 1 + 1);
    g.cast(w, &[]);
    assert!(g.engine.state.objects.contains_key(&b));
    assert!(
        !g.engine
            .state
            .battlefield()
            .into_iter()
            .any(|id| g.engine.state.objects[&id].is_token)
    );
}

fn alternative(actions: &[Action], object: mtg_core::ObjectId) -> Option<Action> {
    actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object: o, .. } if *o == object))
        .cloned()
}

fn insight(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{3}{U}{U}",
        "Instant",
        None,
        "You may pay 1 life and exile a blue card from your hand rather than pay this \
         spell's mana cost.\nDraw two cards.",
    )
}

#[test]
fn pitch_spell_needs_another_blue_card() {
    let mut t = Table::default();
    let insight = insight(&mut t);
    let red = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let mut g = Game::new(t);
    let s = g.put(insight, P0, Zone::Hand);
    g.put(red, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        alternative(&actions, s).is_none(),
        "no other blue card to exile"
    );
}

#[test]
fn pitch_spell_pays_life_and_exiles_a_blue_card() {
    let mut t = Table::default();
    let insight = insight(&mut t);
    let blue = t.card("{U}", "Creature — Wizard", Some((1, 1)), "");
    let mut g = Game::new(t);
    let s = g.put(insight, P0, Zone::Hand);
    let b = g.put(blue, P0, Zone::Hand);
    let actions = g.main();
    let hand = g.count(Zone::Hand, P0);
    let alt = alternative(&actions, s).expect("offered with no lands");
    g.act(alt, &[], &[Answer::Objects(vec![b])]);
    assert_eq!(g.life(P0), 19);
    let exile = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::shared(Zone::Exile));
    assert_eq!(exile.len(), 1, "the blue card was exiled");
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        1,
        "only the spell, not the pitched card"
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 2 + 2);
}

#[test]
fn sacrifice_lands_rather_than_pay() {
    let mut t = Table::default();
    let fireblast = t.card(
        "{4}{R}{R}",
        "Instant",
        None,
        "You may sacrifice two Mountains rather than pay this spell's mana cost.\n~ deals 4 \
         damage to any target.",
    );
    let mountain = t.mountain();
    let mut g = Game::new(t);
    let m1 = g.put(mountain, P0, Zone::Battlefield);
    let s = g.put(fireblast, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        alternative(&actions, s).is_none(),
        "one Mountain is not enough"
    );
    let m2 = g.put(mountain, P0, Zone::Battlefield);
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    let alt = alternative(&actions, s).expect("two Mountains");
    g.act(alt, &[Target::Player(P1)], &[Answer::Objects(vec![m1, m2])]);
    assert_eq!(g.life(P1), 16);
    assert!(!g.engine.state.objects.contains_key(&m1));
    assert!(!g.engine.state.objects.contains_key(&m2));
}

#[test]
fn conditional_alternative_cost_taps_a_creature() {
    let mut t = Table::default();
    let spell = t.card(
        "{3}{W}",
        "Instant",
        None,
        "If you control a Plains, you may tap an untapped creature you control rather than \
         pay this spell's mana cost.\nYou gain 3 life.",
    );
    let plains = t.card("", "Basic Land — Plains", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    let actions = g.main();
    assert!(alternative(&actions, s).is_none(), "no Plains");
    g.put(plains, P0, Zone::Battlefield);
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    let alt = alternative(&actions, s).expect("with a Plains");
    g.act(alt, &[], &[Answer::Objects(vec![b])]);
    assert_eq!(g.life(P0), 23);
    assert!(g.engine.state.objects[&b].tapped);
}

#[test]
fn mana_ability_activates_only_if_you_control_a_swamp() {
    let mut t = Table::default();
    let isle = t.card(
        "",
        "Land",
        None,
        "{T}: Add {C}.\n{T}: Add {U} or {B}. Activate only if you control a Swamp.",
    );
    let swamp = t.card("", "Basic Land — Swamp", None, "");
    let rat = t.card("{B}", "Creature — Zombie", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.put(isle, P0, Zone::Battlefield);
    let r = g.put(rat, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::Cast { object: r }),
        "no Swamp: the land makes only {{C}}"
    );
    // A tapped Swamp still turns the ability on, and the land pays {B}.
    let s = g.put(swamp, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&s).unwrap().tapped = true;
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(actions.contains(&Action::Cast { object: r }));
    g.cast(r, &[]);
    assert!(g.find(rat).is_some());
}

#[test]
fn ability_activates_only_with_an_empty_hand() {
    let mut t = Table::default();
    let sage = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "{T}: Draw a card. Activate only if you have no cards in hand.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let s = g.put(sage, P0, Zone::Battlefield);
    let actions = g.main();
    assert!(
        !offers(&actions, s),
        "the card drawn for the turn is in hand"
    );
    // Cast it (the library's filler bear), emptying the hand.
    let drawn = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0));
    assert_eq!(drawn.len(), 1);
    g.cast(drawn[0], &[]);
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(offers(&actions, s));
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), 1);
}

#[test]
fn formidable_needs_total_power_eight() {
    let mut t = Table::default();
    let tusk = t.card(
        "{2}{G}",
        "Creature — Bear",
        Some((3, 3)),
        "Formidable — {G}: This creature gets +4/+4 until end of turn. Activate only if \
         creatures you control have total power 8 or greater.",
    );
    let big = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let huge = t.card("{5}", "Creature — Bear", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let k = g.put(tusk, P0, Zone::Battlefield);
    g.put(big, P0, Zone::Battlefield);
    // An opponent's creature does not count.
    g.put(big, P1, Zone::Battlefield);
    let actions = g.main();
    assert!(!offers(&actions, k), "3 + 4 is 7");
    g.put(huge, P0, Zone::Battlefield);
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(offers(&actions, k), "3 + 4 + 1 is 8");
    g.act(activate(k, 0), &[], &[]);
    assert_eq!(g.pt(k), (7, 7));
}

fn atoll(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "",
        "Land",
        None,
        "This land enters tapped.\nWhen this land enters, sacrifice it unless you return an \
         untapped Island you control to its owner's hand.\n{T}: Add {C}{U}.",
    )
}

#[test]
fn karoo_returns_an_untapped_island() {
    let mut t = Table::default();
    let atoll = atoll(&mut t);
    let island = t.card("", "Basic Land — Island", None, "");
    let mut g = Game::new(t);
    let i = g.put(island, P0, Zone::Battlefield);
    let a = g.put(atoll, P0, Zone::Hand);
    let actions = g.main();
    let play = actions
        .iter()
        .find(|x| matches!(x, Action::PlayLand { object } if *object == a))
        .cloned()
        .expect("a land drop");
    g.act(play, &[], &[Answer::Bool(true), Answer::Objects(vec![i])]);
    assert!(g.find(atoll).is_some(), "kept");
    assert_eq!(g.find(island), None, "the Island went back");
    assert_eq!(g.count(Zone::Hand, P0), 2, "the drawn card and the Island");
}

#[test]
fn karoo_without_an_island_is_sacrificed() {
    let mut t = Table::default();
    let atoll = atoll(&mut t);
    let mut g = Game::new(t);
    let a = g.put(atoll, P0, Zone::Hand);
    let actions = g.main();
    let play = actions
        .iter()
        .find(|x| matches!(x, Action::PlayLand { object } if *object == a))
        .cloned()
        .expect("a land drop");
    g.act(play, &[], &[]);
    assert_eq!(g.find(atoll), None);
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn may_return_another_creature_needs_one_to_return() {
    let mut t = Table::default();
    let flicker = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "When this creature enters, you may return another creature you control to its \
         owner's hand. If you do, you gain 3 life.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let f = g.put(flicker, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: f }, &[], &[Answer::Bool(true)]);
    assert!(g.find(flicker).is_some(), "it never returns itself");
    assert_eq!(g.life(P0), 20, "nothing was returned");
}

#[test]
fn upkeep_damage_unless_you_pay_mana() {
    for pay in [true, false] {
        let mut t = Table::default();
        let demon = t.card(
            "{2}{B}",
            "Creature — Zombie",
            Some((4, 4)),
            "At the beginning of your upkeep, this creature deals 2 damage to you unless you \
             pay {B}{B}.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        g.put(demon, P0, Zone::Battlefield);
        g.engine.state.step = Step::Untap;
        g.engine.state.turn = 1;
        g.engine.state.active_player = P1;
        // Into P0's next turn, answering the payment question.
        let mut answered = false;
        for _ in 0..10_000 {
            match g.engine.advance(&g.table) {
                mtg_engine::Progress::NeedsChoice(c) => {
                    let a = match &c.kind {
                        mtg_engine::choice::ChoiceKind::Confirm if !answered => {
                            answered = true;
                            Answer::Bool(pay)
                        }
                        _ => c.default.clone().unwrap_or(Answer::Pass),
                    };
                    if c.who == P0
                        && g.engine.state.active_player == P0
                        && g.engine.state.step == Step::PrecombatMain
                    {
                        break;
                    }
                    g.engine.answer(&g.table, c.id, a).unwrap();
                }
                mtg_engine::Progress::Continue => {}
                mtg_engine::Progress::GameOver { .. } => panic!("game over"),
            }
        }
        assert!(answered, "asked whether to pay");
        assert_eq!(g.life(P0), if pay { 20 } else { 18 });
    }
}

#[test]
fn lose_life_unless_you_control_another_pirate() {
    for other in [false, true] {
        let mut t = Table::default();
        let pirate = t.card(
            "{1}{B}",
            "Creature — Bear",
            Some((2, 2)),
            "When this creature enters, you lose 2 life unless you control another Bear.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        if other {
            g.put(bear, P0, Zone::Battlefield);
        }
        let p = g.put(pirate, P0, Zone::Hand);
        g.main();
        g.cast(p, &[]);
        assert_eq!(g.life(P0), if other { 20 } else { 18 });
    }
}

#[test]
fn draw_a_card_if_hand_is_empty_is_read_on_resolution() {
    let mut t = Table::default();
    let well = t.card(
        "{1}",
        "Artifact",
        None,
        "{1}: Draw a card if you have no cards in hand.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let w = g.put(well, P0, Zone::Battlefield);
    let actions = g.main();
    let hand = g.count(Zone::Hand, P0);
    assert!(hand > 0);
    assert!(
        offers(&actions, w),
        "activatable; the condition is checked later"
    );
    g.act(activate(w, 0), &[], &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand, "had a card: drew nothing");
}

#[test]
fn tap_unless_you_pay_life() {
    for pay in [true, false] {
        let mut t = Table::default();
        let golem = t.card(
            "{2}",
            "Artifact Creature — Bear",
            Some((3, 3)),
            "When this creature enters, tap it unless you pay 2 life.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let c = g.put(golem, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: c }, &[], &[Answer::Bool(pay)]);
        let id = g.find(golem).unwrap();
        assert_eq!(g.life(P0), if pay { 18 } else { 20 });
        assert_eq!(g.engine.state.objects[&id].tapped, !pay);
    }
}

#[test]
fn creature_tokens_you_control_get_a_bonus() {
    let mut t = Table::default();
    let anthem = t.card(
        "{2}{W}",
        "Enchantment",
        None,
        "Creature tokens you control get +1/+1.",
    );
    let zombies = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a 2/2 black Zombie creature token.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(anthem, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let z = g.put(zombies, P0, Zone::Hand);
    g.main();
    g.cast(z, &[]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .unwrap();
    assert_eq!(g.pt(token), (3, 3));
    assert_eq!(g.pt(b), (2, 2), "a card is not a token");
}

#[test]
fn each_player_sacrifices_a_creature_token() {
    let mut t = Table::default();
    let purge = t.card(
        "{B}",
        "Sorcery",
        None,
        "Each player sacrifices a creature token of their choice.",
    );
    let zombies = t.card(
        "{B}",
        "Sorcery",
        None,
        "Create a 2/2 black Zombie creature token.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let z = g.put(zombies, P0, Zone::Hand);
    let p = g.put(purge, P0, Zone::Hand);
    g.main();
    g.cast(z, &[]);
    g.cast(p, &[]);
    assert!(
        g.engine.state.objects.contains_key(&b),
        "the bear is not a token"
    );
    assert!(
        !g.engine
            .state
            .battlefield()
            .into_iter()
            .any(|id| g.engine.state.objects[&id].is_token)
    );
}

#[test]
fn mana_for_each_creature_you_control() {
    let mut t = Table::default();
    let cradle = t.card(
        "",
        "Land",
        None,
        "{T}: Add {G} for each creature you control.",
    );
    let giant = t.card("{G}{G}", "Creature — Bear", Some((3, 3)), "");
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(cradle, P0, Zone::Battlefield);
    let c = g.put(giant, P0, Zone::Hand);
    let e = g.put(elf, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::Cast { object: e }),
        "no creatures: no mana"
    );
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(
        actions.contains(&Action::Cast { object: c }),
        "two creatures: {{G}}{{G}}"
    );
    g.cast(c, &[]);
    assert!(g.find(giant).is_some());
}

#[test]
fn keyword_as_long_as_it_has_a_counter() {
    let mut t = Table::default();
    let drake = t.card(
        "{1}{U}",
        "Creature — Bear",
        Some((1, 1)),
        "This creature has flying as long as it has a +1/+1 counter on it.",
    );
    let pump = t.card(
        "{G}",
        "Instant",
        None,
        "Put a +1/+1 counter on target creature.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(drake, P0, Zone::Battlefield);
    let p = g.put(pump, P0, Zone::Hand);
    g.main();
    assert!(!g.has(d, mtg_core::Keyword::Flying));
    g.cast(p, &[Target::Object(d)]);
    assert!(g.has(d, mtg_core::Keyword::Flying));
    assert_eq!(g.pt(d), (2, 2));
}

#[test]
fn random_discard_takes_cards_without_asking() {
    let mut t = Table::default();
    let mind = t.card(
        "{1}{B}",
        "Sorcery",
        None,
        "Target player discards two cards at random.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Hand);
    }
    let m = g.put(mind, P0, Zone::Hand);
    g.main();
    // `act` answers any object choice from this list; a random discard asks none.
    g.act(Action::Cast { object: m }, &[Target::Player(P1)], &[]);
    assert_eq!(g.count(Zone::Hand, P1), 1);
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
}

#[test]
fn sacrifice_unless_you_discard_at_random() {
    for empty in [false, true] {
        let mut t = Table::default();
        let hound = t.card(
            "{R}",
            "Creature — Bear",
            Some((3, 3)),
            "When this creature enters, sacrifice it unless you discard a card at random.",
        );
        let mut g = Game::new(t);
        g.lands(3);
        let h = g.put(hound, P0, Zone::Hand);
        g.main();
        if empty {
            // Cast the card drawn this turn (the library's bear), emptying the hand.
            let drawn = g
                .engine
                .state
                .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0));
            let other = *drawn.iter().find(|id| **id != h).unwrap();
            g.cast(other, &[]);
            g.pending = None;
            g.until(P0, Step::PostcombatMain);
        }
        g.act(Action::Cast { object: h }, &[], &[Answer::Bool(true)]);
        assert_eq!(g.find(hound).is_some(), !empty);
        assert_eq!(g.count(Zone::Hand, P0), 0);
    }
}

#[test]
fn may_gain_life_and_may_pay_life() {
    for yes in [true, false] {
        let mut t = Table::default();
        let cleric = t.card(
            "{1}{W}",
            "Creature — Soldier",
            Some((2, 2)),
            "When this creature enters, you may gain 3 life.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let c = g.put(cleric, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: c }, &[], &[Answer::Bool(yes)]);
        assert_eq!(g.life(P0), if yes { 23 } else { 20 });
    }
    for yes in [true, false] {
        let mut t = Table::default();
        let vamp = t.card(
            "{1}{B}",
            "Creature — Soldier",
            Some((2, 2)),
            "When this creature enters, you may pay 2 life. If you do, draw a card.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let v = g.put(vamp, P0, Zone::Hand);
        g.main();
        let hand = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: v }, &[], &[Answer::Bool(yes)]);
        assert_eq!(g.life(P0), if yes { 18 } else { 20 });
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + usize::from(yes));
    }
}

#[test]
fn may_pay_life_when_you_do_targets_afterwards() {
    let mut t = Table::default();
    let rogue = t.card(
        "{1}{B}",
        "Creature — Soldier",
        Some((2, 2)),
        "When this creature enters, you may pay 2 life. When you do, target creature gets \
         -1/-1 until end of turn.",
    );
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let e = g.put(elf, P1, Zone::Battlefield);
    let r = g.put(rogue, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: r },
        &[Target::Object(e)],
        &[Answer::Bool(true)],
    );
    assert_eq!(g.life(P0), 18);
    assert!(!g.engine.state.objects.contains_key(&e));
}

#[test]
fn infusion_reads_whether_you_gained_life_this_turn() {
    let mut t = Table::default();
    let leech = t.card(
        "{1}{B}",
        "Creature — Bear",
        Some((1, 3)),
        "Infusion — This creature gets +2/+0 as long as you gained life this turn.",
    );
    let salve = t.card("{W}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(1);
    let l = g.put(leech, P0, Zone::Battlefield);
    let s = g.put(salve, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(l), (1, 3));
    g.cast(s, &[]);
    assert_eq!(g.pt(l), (3, 3));
    // A new turn forgets it.
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(l), (1, 3));
}

#[test]
fn you_may_have_it_fight() {
    for yes in [true, false] {
        let mut t = Table::default();
        let brawler = t.card(
            "{2}{G}",
            "Creature — Bear",
            Some((4, 4)),
            "When this creature enters, you may have it fight target creature you don't \
             control.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(3);
        let b = g.put(bear, P1, Zone::Battlefield);
        let br = g.put(brawler, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: br },
            &[Target::Object(b)],
            &[Answer::Bool(yes)],
        );
        assert_eq!(g.engine.state.objects.contains_key(&b), !yes);
        let me = g.find(brawler).unwrap();
        assert_eq!(g.engine.state.objects[&me].damage, if yes { 2 } else { 0 });
    }
}

#[test]
fn you_may_have_target_creature_get_minus() {
    let mut t = Table::default();
    let witch = t.card(
        "{1}{B}",
        "Creature — Bear",
        Some((1, 1)),
        "When this creature enters, you may have target creature get -1/-1 until end of turn.",
    );
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let e = g.put(elf, P1, Zone::Battlefield);
    let w = g.put(witch, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: w },
        &[Target::Object(e)],
        &[Answer::Bool(true)],
    );
    assert!(!g.engine.state.objects.contains_key(&e));
}

#[test]
fn enchanted_creature_deals_damage_to_a_player() {
    let mut t = Table::default();
    let curse = t.card(
        "{1}{B}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nWhenever enchanted creature deals damage to a player, that player \
         loses 2 life.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bear, P0, Zone::Battlefield);
    let c = g.put(curse, P0, Zone::Hand);
    g.main();
    g.cast(c, &[Target::Object(b)]);
    g.combat(&[b], &[], &[], &[]);
    assert_eq!(g.life(P1), 16, "2 combat damage and 2 more");
    assert_eq!(g.life(P0), 20);
}

#[test]
fn tap_or_untap_target_creature() {
    for tap in [true, false] {
        let mut t = Table::default();
        let wizard = t.card(
            "{1}{U}",
            "Creature — Wizard",
            Some((1, 1)),
            "{T}: You may tap or untap target creature.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let w = g.put(wizard, P0, Zone::Battlefield);
        let b = g.put(bear, P1, Zone::Battlefield);
        g.engine.state.objects.get_mut(&b).unwrap().tapped = !tap;
        g.main();
        g.act(
            activate(w, 0),
            &[Target::Object(b)],
            &[Answer::Bool(true), Answer::Bool(tap)],
        );
        assert_eq!(g.engine.state.objects[&b].tapped, tap);
    }
}

#[test]
fn creature_dealt_damage_by_this_dies() {
    let mut t = Table::default();
    let sengir = t.card(
        "{3}{B}{B}",
        "Creature — Bear",
        Some((4, 4)),
        "Whenever a creature dealt damage by this creature this turn dies, put a +1/+1 \
         counter on this creature.",
    );
    let murder = t.card("{B}", "Instant", None, "Destroy target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(sengir, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    let m = g.put(murder, P0, Zone::Hand);
    g.main();
    g.combat(&[s], &[(b, s)], &[], &[]);
    assert!(!g.engine.state.objects.contains_key(&b));
    assert_eq!(g.pt(s), (5, 5));
    // A creature it never damaged dies: no counter.
    g.cast(m, &[Target::Object(other)]);
    assert!(!g.engine.state.objects.contains_key(&other));
    assert_eq!(g.pt(s), (5, 5));
}

#[test]
fn aura_bonus_as_long_as_enchanted_creature_is_green() {
    for green in [true, false] {
        let mut t = Table::default();
        let aura = t.card(
            "{G}",
            "Enchantment — Aura",
            None,
            "Enchant creature\nAs long as enchanted creature is green, it gets +1/+1 and has \
             trample.",
        );
        let host = if green {
            t.card("{G}", "Creature — Elf", Some((1, 1)), "")
        } else {
            t.card("{R}", "Creature — Goblin", Some((1, 1)), "")
        };
        let mut g = Game::new(t);
        g.lands(1);
        let h = g.put(host, P0, Zone::Battlefield);
        let a = g.put(aura, P0, Zone::Hand);
        g.main();
        g.cast(a, &[Target::Object(h)]);
        assert_eq!(g.pt(h), if green { (2, 2) } else { (1, 1) });
        assert_eq!(g.has(h, mtg_core::Keyword::Trample), green);
    }
}

#[test]
fn bonus_as_long_as_you_drew_two_cards() {
    let mut t = Table::default();
    let sphinx = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((1, 1)),
        "This creature gets +2/+0 as long as you've drawn two or more cards this turn.",
    );
    let opt = t.card("{U}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(sphinx, P0, Zone::Battlefield);
    let o = g.put(opt, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(s), (1, 1), "one card drawn for the turn");
    g.cast(o, &[]);
    assert_eq!(g.pt(s), (3, 1));
}

#[test]
fn costs_less_by_the_greatest_power_you_control() {
    let mut t = Table::default();
    let titan = t.card(
        "{6}{G}",
        "Creature — Bear",
        Some((7, 7)),
        "This spell costs {X} less to cast, where X is the greatest power among creatures \
         you control.",
    );
    let ogre = t.card("{3}", "Creature — Bear", Some((4, 4)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    g.put(bear, P0, Zone::Battlefield);
    let c = g.put(titan, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::Cast { object: c }),
        "7 - 2 is 5 > 3"
    );
    g.put(ogre, P0, Zone::Battlefield);
    g.pending = None;
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(actions.contains(&Action::Cast { object: c }), "7 - 4 is 3");
    g.cast(c, &[]);
    assert!(g.find(titan).is_some());
}

#[test]
fn gain_life_equal_to_the_greatest_toughness() {
    let mut t = Table::default();
    let healer = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((1, 1)),
        "When this creature enters, you gain life equal to the greatest toughness among \
         other creatures you control.",
    );
    let wall = t.card("{2}", "Creature — Wall", Some((0, 5)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    g.put(wall, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    let h = g.put(healer, P0, Zone::Hand);
    g.main();
    g.cast(h, &[]);
    assert_eq!(g.life(P0), 25);
}

#[test]
fn destroy_target_creature_that_was_dealt_damage_this_turn() {
    let mut t = Table::default();
    let finisher = t.card(
        "{B}",
        "Instant",
        None,
        "Destroy target creature that was dealt damage this turn.",
    );
    let shock = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let ogre = t.card("{3}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(2);
    let o = g.put(ogre, P1, Zone::Battlefield);
    let f = g.put(finisher, P0, Zone::Hand);
    let s = g.put(shock, P0, Zone::Hand);
    let actions = g.main();
    assert!(
        !actions.contains(&Action::Cast { object: f }),
        "no creature was dealt damage: no legal target"
    );
    g.cast(s, &[Target::Object(o)]);
    let actions = g.until(P0, Step::PostcombatMain);
    assert!(actions.contains(&Action::Cast { object: f }));
    g.cast(f, &[Target::Object(o)]);
    assert!(!g.engine.state.objects.contains_key(&o));
}

#[test]
fn creatures_with_counters_on_them_have_trample() {
    let mut t = Table::default();
    let anthem = t.card(
        "{2}{G}",
        "Enchantment",
        None,
        "Creatures you control with +1/+1 counters on them have trample.",
    );
    let pump = t.card(
        "{G}",
        "Instant",
        None,
        "Put a +1/+1 counter on target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(anthem, P0, Zone::Battlefield);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let p = g.put(pump, P0, Zone::Hand);
    g.main();
    g.cast(p, &[Target::Object(a)]);
    assert!(g.has(a, mtg_core::Keyword::Trample));
    assert!(!g.has(b, mtg_core::Keyword::Trample));
}

#[test]
fn granted_ability_until_end_of_turn() {
    let mut t = Table::default();
    let gift = t.card(
        "{U}",
        "Instant",
        None,
        "Until end of turn, target creature gains \"{T}: Tap target creature.\"",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P0, Zone::Battlefield);
    let foe = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(gift, P0, Zone::Hand);
    let actions = g.main();
    assert!(!offers(&actions, b));
    g.cast(s, &[Target::Object(b)]);
    let actions = g.until(P0, Step::PostcombatMain);
    let tap = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source, .. } if *source == b))
        .cloned()
        .expect("the granted ability");
    g.act(tap, &[Target::Object(foe)], &[]);
    assert!(g.engine.state.objects[&foe].tapped);
    // Gone next turn.
    let actions = g.until(P1, Step::PrecombatMain);
    assert!(!offers(&actions, b));
}

#[test]
fn loses_all_abilities_and_reversed_pump() {
    let mut t = Table::default();
    let hush = t.card(
        "{U}",
        "Instant",
        None,
        "Target creature loses all abilities until end of turn.",
    );
    let rage = t.card(
        "{R}",
        "Instant",
        None,
        "Until end of turn, target creature gains trample and gets +2/+2.",
    );
    let flier = t.card("{1}{U}", "Creature — Bear", Some((1, 1)), "Flying");
    let mut g = Game::new(t);
    g.lands(2);
    let f = g.put(flier, P1, Zone::Battlefield);
    let h = g.put(hush, P0, Zone::Hand);
    let r = g.put(rage, P0, Zone::Hand);
    g.main();
    assert!(g.has(f, mtg_core::Keyword::Flying));
    g.cast(h, &[Target::Object(f)]);
    assert!(!g.has(f, mtg_core::Keyword::Flying));
    g.cast(r, &[Target::Object(f)]);
    assert_eq!(g.pt(f), (3, 3));
    assert!(g.has(f, mtg_core::Keyword::Trample));
}

#[test]
fn loses_flying_until_end_of_turn() {
    let mut t = Table::default();
    let net = t.card(
        "{G}",
        "Instant",
        None,
        "Target creature loses flying until end of turn.",
    );
    let bird = t.card("{U}", "Creature — Bear", Some((1, 1)), "Flying");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bird, P1, Zone::Battlefield);
    let n = g.put(net, P0, Zone::Hand);
    g.main();
    g.cast(n, &[Target::Object(b)]);
    assert!(!g.has(b, mtg_core::Keyword::Flying));
    g.until(P1, Step::PrecombatMain);
    assert!(g.has(b, mtg_core::Keyword::Flying), "back next turn");
}

#[test]
fn wall_attacks_once_it_loses_defender() {
    for activated in [false, true] {
        let mut t = Table::default();
        let wall = t.card(
            "{1}{R}",
            "Creature — Wall",
            Some((3, 3)),
            "Defender\n{R}: This creature loses defender until end of turn.",
        );
        let mut g = Game::new(t);
        g.lands(1);
        let w = g.put(wall, P0, Zone::Battlefield);
        g.main();
        if activated {
            g.act(activate(w, 1), &[], &[]);
        }
        g.combat(&[w], &[], &[], &[]);
        assert_eq!(g.life(P1), if activated { 17 } else { 20 });
    }
}

#[test]
fn creatures_with_defender_can_attack_as_though_they_didnt() {
    let mut t = Table::default();
    let drum = t.card(
        "{2}",
        "Artifact",
        None,
        "Creatures you control can attack as though they didn't have defender.",
    );
    let wall = t.card("{1}", "Creature — Wall", Some((2, 2)), "Defender");
    let mut g = Game::new(t);
    g.put(drum, P0, Zone::Battlefield);
    let w = g.put(wall, P0, Zone::Battlefield);
    g.main();
    g.combat(&[w], &[], &[], &[]);
    assert_eq!(g.life(P1), 18);
}

#[test]
fn reanimated_creature_gains_haste_and_is_sacrificed_later() {
    let mut t = Table::default();
    let rite = t.card(
        "{2}{B}",
        "Sorcery",
        None,
        "Return target creature card from your graveyard to the battlefield. It gains haste. \
         Sacrifice it at the beginning of the next end step.",
    );
    let ogre = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
    let mut g = Game::new(t);
    g.lands(3);
    let o = g.put(ogre, P0, Zone::Graveyard);
    let r = g.put(rite, P0, Zone::Hand);
    g.main();
    g.cast(r, &[Target::Object(o)]);
    let back = g.find(ogre).expect("returned");
    assert!(g.has(back, mtg_core::Keyword::Haste));
    g.combat(&[back], &[], &[], &[]);
    assert_eq!(g.life(P1), 16, "attacked the turn it returned");
    g.until(P1, Step::Upkeep);
    assert_eq!(g.find(ogre), None, "sacrificed at the end step");
}

#[test]
fn leading_until_end_of_turn_covers_the_whole_sentence() {
    let mut t = Table::default();
    let spell = t.card(
        "{B}",
        "Instant",
        None,
        "Until end of turn, creatures you control get +1/+1 and creatures your opponents \
         control get -1/-1.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!((g.pt(mine), g.pt(theirs)), ((3, 3), (1, 1)));
    g.until(P1, Step::PrecombatMain);
    assert_eq!((g.pt(mine), g.pt(theirs)), ((2, 2), (2, 2)));
}

#[test]
fn drain_gains_the_life_lost() {
    let mut t = Table::default();
    let drain = t.card(
        "{2}{B}",
        "Sorcery",
        None,
        "Each opponent loses 3 life. You gain life equal to the life lost this way.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let d = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(d, &[]);
    assert_eq!((g.life(P0), g.life(P1)), (23, 17));
}

#[test]
fn choose_target_then_act_on_it() {
    let mut t = Table::default();
    let spell = t.card(
        "{G}",
        "Sorcery",
        None,
        "Choose target creature you control. Put two +1/+1 counters on it.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(g.pt(b), (4, 4));
}

#[test]
fn reveal_hand_choose_a_noncreature_nonland_card() {
    let mut t = Table::default();
    let probe = t.card(
        "{B}",
        "Sorcery",
        None,
        "Target opponent reveals their hand. You choose a noncreature, nonland card from it. \
         That player discards that card.",
    );
    let bear = t.bear();
    let forest = t.card("", "Basic Land — Forest", None, "");
    let blast = t.card("{R}", "Sorcery", None, "~ deals 1 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(probe, P0, Zone::Hand);
    let b = g.put(bear, P1, Zone::Hand);
    let f = g.put(forest, P1, Zone::Hand);
    let x = g.put(blast, P1, Zone::Hand);
    g.main();
    // Only the sorcery may be chosen, so the default choice is it.
    g.act(Action::Cast { object: s }, &[Target::Player(P1)], &[]);
    assert!(
        !g.engine.state.objects.contains_key(&x),
        "the sorcery was discarded"
    );
    assert!(g.engine.state.objects.contains_key(&b));
    assert!(g.engine.state.objects.contains_key(&f));
}

#[test]
fn dig_up_to_two_creature_cards() {
    let mut t = Table::default();
    let dig = t.card(
        "{2}{G}",
        "Sorcery",
        None,
        "Look at the top four cards of your library. You may reveal up to two creature cards \
         from among them and put them into your hand. Put the rest on the bottom of your \
         library in a random order.",
    );
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    let ids: Vec<_> = [bear, rock, bear, rock]
        .iter()
        .map(|c| g.put(*c, P0, Zone::Library))
        .collect();
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| !ids.contains(o));
    for (i, id) in ids.iter().enumerate() {
        order.insert(i, *id);
    }
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: d },
        &[],
        &[Answer::Objects(vec![ids[0], ids[2]])],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2, "both creature cards");
    let lib = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Library, P0));
    let bottom: Vec<_> = lib[lib.len() - 2..]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(bottom, vec![rock, rock]);
}

#[test]
fn manual_activation_adds_mana_for_each_creature() {
    let mut t = Table::default();
    let cradle = t.card(
        "",
        "Land",
        None,
        "{T}: Add {G} for each creature you control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let c = g.put(cradle, P0, Zone::Battlefield);
    for _ in 0..3 {
        g.put(bear, P0, Zone::Battlefield);
    }
    g.main();
    use mtg_ir::PrintedCards;
    let ability = g.table.face(cradle, 0).unwrap().abilities[0].id;
    g.act(
        Action::ActivateManaAbility {
            source: c,
            ability,
            color: None,
        },
        &[],
        &[],
    );
    assert_eq!(g.engine.state.player(P0).mana.total(), 3);
}

#[test]
fn two_mana_of_any_one_color_pays_a_double_pip() {
    let mut t = Table::default();
    let vault = t.card("", "Land", None, "{T}: Add two mana of any one color.");
    let ogre = t.card("{R}{R}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.put(vault, P0, Zone::Battlefield);
    let o = g.put(ogre, P0, Zone::Hand);
    let actions = g.main();
    assert!(actions.contains(&Action::Cast { object: o }));
    g.cast(o, &[]);
    assert!(g.find(ogre).is_some());
}

#[test]
fn prevent_combat_damage_to_and_by_target_creature() {
    let mut t = Table::default();
    let fog = t.card(
        "{W}",
        "Instant",
        None,
        "Prevent all combat damage that would be dealt to and dealt by target creature this \
         turn.",
    );
    let ogre = t.card("{3}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(ogre, P0, Zone::Battlefield);
    let theirs = g.put(ogre, P1, Zone::Battlefield);
    let f = g.put(fog, P0, Zone::Hand);
    g.main();
    g.cast(f, &[Target::Object(mine)]);
    g.combat(&[mine], &[(theirs, mine)], &[], &[]);
    assert_eq!(g.engine.state.objects[&mine].damage, 0, "none dealt to it");
    assert_eq!(
        g.engine.state.objects[&theirs].damage, 0,
        "none dealt by it"
    );
}

#[test]
fn group_pump_then_untap_them() {
    let mut t = Table::default();
    let rally = t.card(
        "{1}{W}",
        "Instant",
        None,
        "Creatures you control get +1/+1 until end of turn. Untap them.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let a = g.put(bear, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let foe = g.put(bear, P1, Zone::Battlefield);
    let r = g.put(rally, P0, Zone::Hand);
    g.main();
    for id in [a, b, foe] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    g.cast(r, &[]);
    assert!(!g.engine.state.objects[&a].tapped && !g.engine.state.objects[&b].tapped);
    assert!(g.engine.state.objects[&foe].tapped, "not one of them");
    assert_eq!(g.pt(a), (3, 3));
}

#[test]
fn assigns_combat_damage_equal_to_toughness() {
    let mut t = Table::default();
    let wall = t.card(
        "{1}{W}",
        "Creature — Bear",
        Some((1, 4)),
        "This creature assigns combat damage equal to its toughness rather than its power.",
    );
    let mut g = Game::new(t);
    let w = g.put(wall, P0, Zone::Battlefield);
    g.main();
    g.combat(&[w], &[], &[], &[]);
    assert_eq!(g.life(P1), 16, "4 damage, not 1");
    // Blocked by a 3/3: the blocker dies.
    let mut t = Table::default();
    let wall = t.card(
        "{1}{W}",
        "Creature — Bear",
        Some((1, 4)),
        "This creature assigns combat damage equal to its toughness rather than its power.",
    );
    let ogre = t.card("{3}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    let w = g.put(wall, P0, Zone::Battlefield);
    let o = g.put(ogre, P1, Zone::Battlefield);
    g.main();
    g.combat(&[w], &[(o, w)], &[], &[]);
    assert!(!g.engine.state.objects.contains_key(&o));
}

#[test]
fn spell_shuffles_itself_into_its_library() {
    let mut t = Table::default();
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 5 damage to any target. Shuffle ~ into its owner's library.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 15);
    assert_eq!(g.count(Zone::Graveyard, P0), 0, "not in the graveyard");
    assert_eq!(
        g.count(Zone::Library, P0),
        library + 1,
        "back in the library"
    );
}

#[test]
fn owner_chooses_top_or_bottom() {
    for top in [true, false] {
        let mut t = Table::default();
        let tuck = t.card(
            "{1}{U}",
            "Instant",
            None,
            "Target creature's owner puts it on their choice of the top or bottom of their \
             library.",
        );
        let ogre = t.card("{3}", "Creature — Bear", Some((3, 3)), "");
        let mut g = Game::new(t);
        g.lands(2);
        let o = g.put(ogre, P1, Zone::Battlefield);
        let s = g.put(tuck, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: s },
            &[Target::Object(o)],
            &[Answer::Bool(top)],
        );
        let lib = g
            .engine
            .state
            .objects_in(mtg_core::ZoneRef::of(Zone::Library, P1));
        let at = if top { lib[0] } else { lib[lib.len() - 1] };
        assert_eq!(g.engine.state.objects[&at].card, ogre);
    }
}

#[test]
fn that_token_is_the_one_just_made() {
    let mut t = Table::default();
    let call = t.card(
        "{R}",
        "Sorcery",
        None,
        "Create a 3/1 red Elemental creature token. That token gains haste.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let c = g.put(call, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .unwrap();
    assert!(g.has(token, mtg_core::Keyword::Haste));
    g.combat(&[token], &[], &[], &[]);
    assert_eq!(g.life(P1), 17);
}

#[test]
fn damage_cant_be_prevented_beats_a_shield() {
    for unpreventable in [false, true] {
        let mut t = Table::default();
        let shield = t.card(
            "{W}",
            "Instant",
            None,
            "Prevent all damage that would be dealt to target creature this turn.",
        );
        let text = if unpreventable {
            "Damage can't be prevented this turn. ~ deals 2 damage to any target."
        } else {
            "~ deals 2 damage to any target."
        };
        let shot = t.card("{R}", "Instant", None, text);
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let b = g.put(bear, P1, Zone::Battlefield);
        let s = g.put(shield, P0, Zone::Hand);
        let x = g.put(shot, P0, Zone::Hand);
        g.main();
        g.cast(s, &[Target::Object(b)]);
        g.cast(x, &[Target::Object(b)]);
        assert_eq!(g.engine.state.objects.contains_key(&b), !unpreventable);
    }
}

#[test]
fn skip_your_next_turn() {
    let mut t = Table::default();
    let deal = t.card(
        "{U}",
        "Sorcery",
        None,
        "Draw four cards. You skip your next turn.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(deal, P0, Zone::Hand);
    g.main();
    g.cast(d, &[]);
    let turn = g.engine.state.turn;
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.engine.state.turn, turn + 1);
    // P0's turn is skipped: P1 goes again.
    g.pending = None;
    g.until(P1, Step::Upkeep);
    assert_eq!(g.engine.state.turn, turn + 2);
    assert_eq!(g.engine.state.active_player, P1);
    g.pending = None;
    g.until(P0, Step::Upkeep);
    assert_eq!(g.engine.state.turn, turn + 3, "then P0 as usual");
}

#[test]
fn life_for_each_creature_destroyed_this_way() {
    let mut t = Table::default();
    let wrath = t.card(
        "{W}",
        "Sorcery",
        None,
        "Destroy all creatures. You gain 1 life for each creature destroyed this way.",
    );
    let bear = t.bear();
    let wall = t.card("{1}", "Creature — Bear", Some((0, 4)), "Indestructible");
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let w = g.put(wall, P1, Zone::Battlefield);
    let s = g.put(wrath, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(
        g.life(P0),
        23,
        "three destroyed; the indestructible one isn't"
    );
    assert!(g.engine.state.objects.contains_key(&w));
}

#[test]
fn creatures_target_player_controls() {
    let mut t = Table::default();
    let sleep = t.card(
        "{U}",
        "Sorcery",
        None,
        "Creatures you control get +1/+1 until end of turn. Tap all creatures target player \
         controls.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(sleep, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Player(P1)]);
    assert!(g.engine.state.objects[&theirs].tapped);
    assert!(!g.engine.state.objects[&mine].tapped);
    assert_eq!(g.pt(mine), (3, 3));
}

#[test]
fn destroy_target_creature_and_target_land() {
    let mut t = Table::default();
    let rain = t.card(
        "{B}",
        "Sorcery",
        None,
        "Destroy target creature and target land.",
    );
    let bear = t.bear();
    let land = t.card("", "Land", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let l = g.put(land, P1, Zone::Battlefield);
    let s = g.put(rain, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(b), Target::Object(l)]);
    assert!(!g.engine.state.objects.contains_key(&b));
    assert!(!g.engine.state.objects.contains_key(&l));
}

#[test]
fn creatures_dealt_damage_this_way_are_exiled_if_they_die() {
    let mut t = Table::default();
    let sweep = t.card(
        "{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to each creature. If a creature dealt damage this way would die \
         this turn, exile it instead.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(bear, P1, Zone::Battlefield);
    let s = g.put(sweep, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let exiled = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.card == bear && o.zone.zone == Zone::Exile)
        .count();
    assert_eq!(exiled, 1, "exiled, not put in the graveyard");
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
}

#[test]
fn an_opponents_creature_that_would_die_is_exiled() {
    let mut t = Table::default();
    let rest = t.card(
        "{1}{W}",
        "Enchantment",
        None,
        "If a creature an opponent controls would die, exile it instead.",
    );
    let wrath = t.card("{W}", "Sorcery", None, "Destroy all creatures.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(rest, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let s = g.put(wrath, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let where_ = |owner| {
        g.engine
            .state
            .objects
            .values()
            .find(|o| o.card == bear && o.owner == owner && o.zone.zone != Zone::Library)
            .map(|o| o.zone.zone)
    };
    assert_eq!(where_(P1), Some(Zone::Exile));
    assert_eq!(where_(P0), Some(Zone::Graveyard));
}

#[test]
fn damage_equal_to_the_cards_in_that_players_hand() {
    let mut t = Table::default();
    let storm = t.card(
        "{R}",
        "Sorcery",
        None,
        "~ deals damage to target player equal to the number of cards in that player's hand.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Hand);
    }
    let s = g.put(storm, P0, Zone::Hand);
    g.main();
    let theirs = g.count(Zone::Hand, P1) as i32;
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 20 - theirs);
    assert!(theirs >= 3);
}

#[test]
fn each_player_discards_their_hand_then_draws_seven() {
    let mut t = Table::default();
    let wheel = t.card(
        "{R}",
        "Sorcery",
        None,
        "Each player discards their hand, then draws seven cards.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    for _ in 0..2 {
        g.put(bear, P0, Zone::Hand);
    }
    for _ in 0..4 {
        g.put(bear, P1, Zone::Hand);
    }
    let s = g.put(wheel, P0, Zone::Hand);
    g.main();
    let (mine, theirs) = (g.count(Zone::Hand, P0) - 1, g.count(Zone::Hand, P1));
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Hand, P0), 7);
    assert_eq!(g.count(Zone::Hand, P1), 7);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        mine + 1,
        "its hand and the spell"
    );
    assert_eq!(g.count(Zone::Graveyard, P1), theirs);
}

#[test]
fn counter_unless_it_pays_one_for_each_card_in_your_graveyard() {
    for (in_graveyard, countered) in [(3, true), (1, false)] {
        let mut t = Table::default();
        let daze = t.card(
            "{U}",
            "Instant",
            None,
            "Counter target spell unless its controller pays {1} for each card in your \
             graveyard.",
        );
        let shock = t.card("{R}", "Instant", None, "~ deals 2 damage to any target.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(3);
        for _ in 0..in_graveyard {
            g.put(bear, P0, Zone::Graveyard);
        }
        let d = g.put(daze, P0, Zone::Hand);
        let s = g.put(shock, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: s }, &[Target::Player(P1)]);
        let spell = g.stack()[0];
        // One land left over to pay with.
        g.act(
            Action::Cast { object: d },
            &[Target::Object(spell)],
            &[Answer::Bool(true)],
        );
        assert_eq!(
            g.life(P1),
            if countered { 20 } else { 18 },
            "{in_graveyard} cards"
        );
    }
}

#[test]
fn an_aura_that_sets_base_power_and_toughness() {
    let mut t = Table::default();
    let frog = t.card(
        "{1}{U}",
        "Enchantment — Aura",
        None,
        "Enchant creature\nEnchanted creature loses all abilities and has base power and \
         toughness 1/1.",
    );
    let dragon = t.card("{4}", "Creature — Bear", Some((4, 4)), "Flying");
    let mut g = Game::new(t);
    let d = g.put(dragon, P1, Zone::Battlefield);
    let a = g.put(frog, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&a).unwrap().attached_to = Some(d);
    g.main();
    assert_eq!(g.pt(d), (1, 1));
    assert!(!g.has(d, mtg_core::Keyword::Flying));
}

#[test]
fn target_creature_other_than_this_one() {
    let mut t = Table::default();
    let shaman = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((1, 1)),
        "{T}: Target creature other than this creature has base power and toughness 0/2 until \
         end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let s = g.put(shaman, P0, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    g.main();
    g.act(activate(s, 0), &[Target::Object(b)], &[]);
    assert_eq!(g.pt(b), (0, 2));
    let face = mtg_ir::PrintedCards::face(&g.table, shaman, 0).unwrap();
    let legal = mtg_engine::targeting::legal_targets(
        &g.engine.state,
        &g.table,
        &face.abilities[0].targets[0],
        s,
        P0,
        &[],
    );
    assert!(!legal.contains(&Target::Object(s)), "not itself");
    assert!(legal.contains(&Target::Object(b)));
}

#[test]
fn draw_for_each_creature_that_died_this_turn() {
    let mut t = Table::default();
    let harvest = t.card(
        "{B}",
        "Sorcery",
        None,
        "Draw a card for each creature that died this turn.",
    );
    let wrath = t.card("{W}", "Sorcery", None, "Destroy all creatures.");
    let bear = t.bear();
    let rock = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    g.put(rock, P1, Zone::Battlefield);
    let w = g.put(wrath, P0, Zone::Hand);
    let h = g.put(harvest, P0, Zone::Hand);
    g.main();
    g.cast(w, &[]);
    let hand = g.count(Zone::Hand, P0);
    g.cast(h, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2);
}

#[test]
fn half_rounded_up_and_down() {
    let mut t = Table::default();
    let drain = t.card(
        "{B}",
        "Sorcery",
        None,
        "Target opponent loses half their life, rounded up.",
    );
    let mill = t.card(
        "{U}",
        "Sorcery",
        None,
        "Target player mills half their library, rounded down.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    g.engine.state.players.get_mut(&P1).unwrap().life = 19;
    let d = g.put(drain, P0, Zone::Hand);
    let m = g.put(mill, P0, Zone::Hand);
    g.main();
    g.cast(d, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 9, "half of 19, rounded up, is 10");
    let library = g.count(Zone::Library, P1);
    g.cast(m, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Library, P1), library - library / 2);
}

#[test]
fn exchange_control_of_two_target_creatures() {
    for theirs in [true, false] {
        let mut t = Table::default();
        let switch = t.card(
            "{U}",
            "Sorcery",
            None,
            "Exchange control of two target creatures.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let mine = g.put(bear, P0, Zone::Battlefield);
        let other = g.put(bear, if theirs { P1 } else { P0 }, Zone::Battlefield);
        let s = g.put(switch, P0, Zone::Hand);
        g.main();
        g.cast(s, &[Target::Object(mine), Target::Object(other)]);
        let controller = |id| mtg_engine::layers::controller(&g.engine.state, id);
        if theirs {
            assert_eq!(controller(mine), Some(P1));
            assert_eq!(controller(other), Some(P0));
        } else {
            assert_eq!(controller(mine), Some(P0));
            assert_eq!(controller(other), Some(P0));
        }
    }
}

#[test]
fn an_x_by_x_token() {
    let mut t = Table::default();
    let ooze = t.card(
        "{X}{G}",
        "Sorcery",
        None,
        "Create an X/X green Elf creature token.",
    );
    let wurm = t.card(
        "{G}",
        "Sorcery",
        None,
        "Create an X/X green Elf creature token, where X is the number of lands you control.",
    );
    let mut g = Game::new(t);
    g.lands(5);
    let o = g.put(ooze, P0, Zone::Hand);
    let w = g.put(wurm, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: o }, &[], &[Answer::Number(3)]);
    g.cast(w, &[]);
    let mut sizes: Vec<(i32, i32)> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|id| g.pt(id))
        .collect();
    sizes.sort();
    assert_eq!(sizes, vec![(3, 3), (5, 5)]);
}

#[test]
fn where_x_is_reaches_a_prevention_shield() {
    let mut t = Table::default();
    let medic = t.card(
        "{1}{W}",
        "Creature — Wizard",
        Some((1, 1)),
        "{T}: Prevent the next X damage that would be dealt to target creature this turn, \
         where X is the number of Wizards on the battlefield.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let bear = t.card("{1}", "Creature — Bear", Some((2, 4)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let m = g.put(medic, P0, Zone::Battlefield);
    g.put(medic, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.act(activate(m, 0), &[Target::Object(b)], &[]);
    g.cast(s, &[Target::Object(b)]);
    assert_eq!(
        g.engine.state.objects[&b].damage, 1,
        "two Wizards prevent 2 of 3"
    );
}

#[test]
fn granted_ward_counters_unless_paid() {
    let mut t = Table::default();
    let shield = t.card(
        "{1}{U}",
        "Enchantment",
        None,
        "Creatures you control have ward {2}.",
    );
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to target creature.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(shield, P1, Zone::Battlefield);
    let b = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: s }, &[Target::Object(b)], &[]);
    assert!(
        g.engine.state.objects.contains_key(&b),
        "couldn't pay {{2}}: the bolt was countered"
    );
}

#[test]
fn a_leyline_begins_the_game_on_the_battlefield() {
    use mtg_engine::{Progress, choice::ChoiceKind};
    let mut t = Table::default();
    let leyline = t.card(
        "{2}{B}{B}",
        "Enchantment",
        None,
        "If this card is in your opening hand, you may begin the game with it on the \
         battlefield.\nCreatures you control get +1/+1.",
    );
    let mut g = Game::new(t);
    g.put(leyline, P0, Zone::Hand);
    g.engine.state.pregame = Some(mtg_engine::state::Pregame::new(vec![P0, P1]));
    let mut asked_leyline = false;
    for _ in 0..50 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::KeepOrMulligan { .. } => {
                    g.engine.answer(&g.table, c.id, Answer::Bool(true)).unwrap();
                }
                ChoiceKind::ChooseObjects { from, .. } => {
                    asked_leyline = true;
                    assert_eq!(c.who, P0);
                    let all = from.clone();
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(all))
                        .unwrap();
                }
                _ => break,
            },
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    }
    assert!(asked_leyline);
    assert!(
        g.engine
            .state
            .battlefield()
            .iter()
            .any(|id| g.engine.state.objects[id].card == leyline),
        "on the battlefield before the first turn"
    );
}
