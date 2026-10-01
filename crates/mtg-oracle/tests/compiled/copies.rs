//! Copy effects (CR 707): a permanent that enters as a copy of another.
use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};
use mtg_ir::PrintedCards;

fn clone(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{3}{U}",
        "Creature — Shapeshifter",
        Some((0, 0)),
        "You may have this creature enter as a copy of any creature on the battlefield.",
    )
}

#[test]
fn a_clone_copies_power_toughness_and_enters_triggers() {
    let mut t = Table::default();
    let mystic = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((3, 4)),
        "When this creature enters, draw a card.",
    );
    let clone = clone(&mut t);
    let mut g = Game::new(t);
    g.lands(4);
    let original = g.put(mystic, P0, Zone::Battlefield);
    let c = g.put(clone, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act(
        Action::Cast { object: c },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| **id != original && g.engine.state.objects[id].card == mystic)
        .expect("the clone is on the battlefield as a copy");
    assert_eq!(g.pt(copy), (3, 4));
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "cast the clone (-1), the copied trigger drew (+1)"
    );
}

#[test]
fn a_clone_is_itself_again_once_it_leaves() {
    let mut t = Table::default();
    let bear = t.bear();
    let clone = clone(&mut t);
    let bounce = t.card(
        "{U}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    g.lands(5);
    let original = g.put(bear, P1, Zone::Battlefield);
    let c = g.put(clone, P0, Zone::Hand);
    let b = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: c },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| {
            g.engine.state.objects[id].controller == P0 && g.engine.state.objects[id].card == bear
        })
        .expect("a copy of the bear");
    g.cast(b, &[Target::Object(copy)]);
    let in_hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .into_iter()
        .map(|id| g.engine.state.objects[&id].card)
        .collect::<Vec<_>>();
    assert!(in_hand.contains(&clone), "the clone card, not a bear");
    assert_eq!(g.table.face(clone, 0).unwrap().power, Some(0));
}

#[test]
fn a_clone_that_copies_nothing_is_a_zero_zero() {
    let mut t = Table::default();
    let bear = t.bear();
    let clone = clone(&mut t);
    let mut g = Game::new(t);
    g.lands(4);
    g.put(bear, P1, Zone::Battlefield);
    let c = g.put(clone, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: c }, &[], &[Answer::Objects(vec![])]);
    assert!(g.find(clone).is_none(), "0/0: put into the graveyard");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_token_copy_has_the_originals_values_and_triggers() {
    let mut t = Table::default();
    let mystic = t.card(
        "{2}{U}",
        "Creature — Wizard",
        Some((3, 4)),
        "When this creature enters, draw a card.",
    );
    let echo = t.card(
        "{3}{U}",
        "Sorcery",
        None,
        "Create a token that's a copy of target creature you control.",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let original = g.put(mystic, P0, Zone::Battlefield);
    let spell = g.put(echo, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[Target::Object(original)]);
    let token = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("a token");
    assert_eq!(g.engine.state.objects[&token].card, mystic);
    assert_eq!(g.pt(token), (3, 4));
    assert_eq!(g.count(Zone::Hand, P0), hand, "cast (-1), drew (+1)");
}

#[test]
fn a_creature_that_copies_itself_on_combat_damage() {
    let mut t = Table::default();
    let mite = t.card(
        "{1}{G}",
        "Creature — Bear",
        Some((1, 1)),
        "Whenever this creature deals combat damage to a player, create a token that's a copy of this creature.",
    );
    let mut g = Game::new(t);
    let m = g.put(mite, P0, Zone::Battlefield);
    g.main();
    g.combat(&[m], &[], &[], &[]);
    let copies = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].card == mite)
        .count();
    assert_eq!(copies, 2);
}

#[test]
fn storm_copies_the_spell_once_for_each_spell_before_it() {
    let mut t = Table::default();
    let ping = t.card("{R}", "Instant", None, "~ deals 1 damage to any target.");
    let grapeshot = t.card(
        "{1}{R}",
        "Sorcery",
        None,
        "~ deals 1 damage to any target.\nStorm",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let a = g.put(ping, P0, Zone::Hand);
    let b = g.put(ping, P0, Zone::Hand);
    let s = g.put(grapeshot, P0, Zone::Hand);
    g.main();
    g.cast(a, &[Target::Player(P1)]);
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    // The spell's own target, then a new target for each of the two copies.
    g.cast(
        s,
        &[Target::Player(P1), Target::Player(P1), Target::Player(P1)],
    );
    assert_eq!(g.life(P1), 15, "the original and two copies");
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        3,
        "only the three cards; the copies ceased to exist"
    );
}

#[test]
fn a_copied_spell_may_get_new_targets() {
    let mut t = Table::default();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let twin = t.card(
        "{U}{U}",
        "Instant",
        None,
        "Copy target instant or sorcery spell. You may choose new targets for the copy.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let foe = g.put(bear, P1, Zone::Battlefield);
    let b = g.put(bolt, P0, Zone::Hand);
    let tw = g.put(twin, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: b }, &[Target::Player(P1)]);
    let spell = g.stack()[0];
    g.act(
        Action::Cast { object: tw },
        &[Target::Object(spell), Target::Object(foe)],
        &[],
    );
    assert!(g.find(bear).is_none(), "the copy hit the bear");
    assert_eq!(g.life(P1), 17, "the original hit the player");
}

/// Put cards on top of a player's library, first listed on top.
fn stack_library(g: &mut Game, owner: mtg_core::PlayerId, cards: &[mtg_core::CardId]) {
    let lib = mtg_core::ZoneRef::of(Zone::Library, owner);
    for card in cards.iter().rev() {
        let id = g.put(*card, owner, Zone::Library);
        let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
        order.retain(|o| *o != id);
        order.insert(0, id);
    }
}

#[test]
fn cascade_casts_the_first_cheaper_nonland_for_free() {
    let mut t = Table::default();
    let shaman = t.card("{2}{R}", "Creature — Goblin", Some((3, 3)), "Cascade");
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(shaman, P0, Zone::Hand);
    g.main();
    stack_library(&mut g, P0, &[mountain, mountain, bolt]);
    let library = g.count(Zone::Library, P0);
    g.act(
        Action::Cast { object: s },
        &[Target::Player(P1)],
        &[mtg_engine::choice::Answer::Bool(true)],
    );
    assert_eq!(g.life(P1), 17, "the bolt was cast for free");
    assert!(g.find(shaman).is_some());
    assert_eq!(
        g.count(Zone::Library, P0),
        library - 1,
        "the two lands went back"
    );
    let lib = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Library, P0));
    let bottom: Vec<_> = lib[lib.len() - 2..]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(bottom, vec![mountain, mountain], "on the bottom");
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "the bolt, after resolving");
}

#[test]
fn a_declined_cascade_hit_goes_to_the_bottom_too() {
    let mut t = Table::default();
    let shaman = t.card("{2}{R}", "Creature — Goblin", Some((3, 3)), "Cascade");
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(shaman, P0, Zone::Hand);
    g.main();
    stack_library(&mut g, P0, &[bolt]);
    let library = g.count(Zone::Library, P0);
    g.act(
        Action::Cast { object: s },
        &[],
        &[mtg_engine::choice::Answer::Bool(false)],
    );
    assert_eq!(g.life(P1), 20);
    assert_eq!(g.count(Zone::Library, P0), library);
    let lib = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Library, P0));
    assert_eq!(g.engine.state.objects[lib.last().unwrap()].card, bolt);
}

#[test]
fn eternalize_makes_a_four_four_black_zombie_copy() {
    let mut t = Table::default();
    let knight = t.card(
        "{1}{W}",
        "Creature — Human Knight",
        Some((2, 1)),
        "Flying\nEternalize {1}",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let k = g.put(knight, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(offers(&actions, k));
    let ability = actions
        .iter()
        .find_map(|a| match a {
            mtg_engine::actions::Action::ActivateAbility { source, ability } if *source == k => {
                Some(*ability)
            }
            _ => None,
        })
        .unwrap();
    g.act(activate(k, ability.0), &[], &[]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("a token");
    assert_eq!(g.pt(token), (4, 4));
    assert!(g.has(token, mtg_core::Keyword::Flying), "a copy");
    let ch = g.engine.characteristics(&g.table, token).unwrap().clone();
    assert!(ch.colors.contains(mtg_core::Color::Black));
    assert!(!ch.colors.contains(mtg_core::Color::White));
    assert_eq!(ch.mana_cost.mana_value(), 0, "no mana cost");
    assert_eq!(g.count(Zone::Graveyard, P0), 0, "the card was exiled");
}

#[test]
fn offspring_makes_a_one_one_copy_when_paid() {
    let mut t = Table::default();
    let otter = t.card(
        "{1}{U}",
        "Creature — Otter",
        Some((3, 3)),
        "Flying\nOffspring {1}",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let o = g.put(otter, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: o }, &[], &[Answer::Bool(true)]);
    let token = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_token)
        .expect("the offspring");
    assert_eq!(g.pt(token), (1, 1));
    assert!(g.has(token, mtg_core::Keyword::Flying));
}
