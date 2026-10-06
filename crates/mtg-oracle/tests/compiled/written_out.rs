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
