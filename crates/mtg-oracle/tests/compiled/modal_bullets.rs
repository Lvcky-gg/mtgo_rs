//! Shapes found inside modal staples' bullets.

use super::harness::*;
use mtg_core::{Keyword, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn tokens_counted_from_a_targeted_players_creatures() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a number of 1/1 red Warrior creature tokens equal to the number of creatures target player controls.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Battlefield);
    }
    g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = g.engine.state.battlefield().len();
    g.cast(s, &[Target::Player(P1)]);
    assert_eq!(g.engine.state.battlefield().len(), before + 3);
}

#[test]
fn you_and_target_opponent_each_draw() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "You and target opponent each draw two cards.",
    );
    let mut g = Game::new(t);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let (mine, theirs) = (g.count(Zone::Hand, P0), g.count(Zone::Hand, P1));
    g.cast(s, &[Target::Player(P1)]);
    // The spell left the hand; two came in.
    assert_eq!(g.count(Zone::Hand, P0), mine + 1);
    assert_eq!(g.count(Zone::Hand, P1), theirs + 2);
}

#[test]
fn protection_from_each_color_at_the_end_of_a_keyword_list() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Instant",
        None,
        "Creatures you control gain lifelink, indestructible, and protection from each color until end of turn.",
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
    let mine = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    let b = g.put(bolt, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert!(g.has(mine, Keyword::Lifelink) && g.has(mine, Keyword::Indestructible));
    // A red spell can't target it any more: with no other creature, it isn't offered.
    assert!(!g.main().contains(&Action::Cast { object: b }));
}

#[test]
fn those_creatures_are_the_ones_that_got_counters() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Put a +1/+1 counter on each creature you control. Those creatures gain vigilance until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.pt(a), (3, 3));
    assert!(g.has(a, Keyword::Vigilance));
    assert!(!g.has(theirs, Keyword::Vigilance));
}

#[test]
fn up_to_two_creature_cards_from_hand() {
    let mut t = Table::default();
    let spell = t.card(
        "{0}",
        "Sorcery",
        None,
        "Put up to two creature cards from your hand onto the battlefield.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let a = g.put(bear, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Hand);
    let c = g.put(bear, P0, Zone::Hand);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    let before = g.engine.state.battlefield().len();
    g.act(
        Action::Cast { object: s },
        &[],
        &[Answer::Objects(vec![a, b])],
    );
    assert_eq!(g.engine.state.battlefield().len(), before + 2);
    assert!(g.engine.state.objects.contains_key(&c));
}

#[test]
fn a_prowess_token_gets_bigger_when_you_cast_a_noncreature_spell() {
    let mut t = Table::default();
    let make = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create a 1/1 white Monk creature token with prowess.",
    );
    let cantrip = t.card("{0}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    let s = g.put(make, P0, Zone::Hand);
    let c = g.put(cantrip, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    let monk = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].card.0 >= mtg_ir::walk::TOKEN_CARD_BASE)
        .unwrap();
    assert_eq!(g.pt(monk), (1, 1));
    g.main();
    g.cast(c, &[]);
    assert_eq!(g.pt(monk), (2, 2));
}

#[test]
fn for_each_spell_youve_cast_this_turn() {
    let mut t = Table::default();
    let a = t.card("{0}", "Instant", None, "You gain 1 life.");
    let b = t.card(
        "{0}",
        "Sorcery",
        None,
        "You gain 1 life for each spell you've cast this turn.",
    );
    let mut g = Game::new(t);
    let first = g.put(a, P0, Zone::Hand);
    let second = g.put(b, P0, Zone::Hand);
    g.main();
    g.cast(first, &[]);
    g.main();
    g.cast(second, &[]);
    // 1, then 2 for two spells cast this turn.
    assert_eq!(g.life(P0), 23);
}

#[test]
fn mana_spent_on_the_spell_that_triggered() {
    let mut t = Table::default();
    let watcher = t.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever you cast a spell, you gain X life, where X is the amount of mana spent to cast that spell.",
    );
    let spell = t.card("{2}{G}", "Sorcery", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(3);
    g.put(watcher, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert_eq!(g.life(P0), 23);
}
