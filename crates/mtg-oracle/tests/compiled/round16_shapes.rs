//! "If it's an artifact creature, …" (Blacksmith's Skill) and "Whenever you create one or
//! more creature tokens, …" (Staff of the Storyteller).

use super::harness::*;
use mtg_core::{Target, Zone};

fn skill_on(type_line: &str) -> (i32, i32) {
    let mut t = Table::default();
    let c = t.card("{0}", type_line, Some((1, 1)), "");
    let skill = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +1/+1 until end of turn. If it's an artifact creature, it gets +2/+2 until end of turn.",
    );
    let mut g = Game::new(t);
    let id = g.put(c, P0, Zone::Battlefield);
    let s = g.put(skill, P0, Zone::Hand);
    g.main();
    g.cast(s, &[Target::Object(id)]);
    g.pt(id)
}

#[test]
fn if_its_an_artifact_creature() {
    assert_eq!(skill_on("Artifact Creature — Golem"), (4, 4));
    assert_eq!(skill_on("Creature — Bear"), (2, 2));
}

#[test]
fn whenever_you_create_creature_tokens() {
    let mut t = Table::default();
    let staff = t.card(
        "{0}",
        "Artifact",
        None,
        "Whenever you create one or more creature tokens, put a story counter on this artifact.",
    );
    let soldiers = t.card(
        "{0}",
        "Sorcery",
        None,
        "Create two 1/1 white Soldier creature tokens.",
    );
    let treasure = t.card("{0}", "Sorcery", None, "Create a Treasure token.");
    let mut g = Game::new(t);
    let st = g.put(staff, P0, Zone::Battlefield);
    let a = g.put(soldiers, P0, Zone::Hand);
    let b = g.put(treasure, P0, Zone::Hand);
    g.main();
    let counters = |g: &Game| g.engine.state.objects[&st].counters.values().sum::<i32>();
    g.cast(b, &[]);
    assert_eq!(counters(&g), 0, "a Treasure isn't a creature");
    g.cast(a, &[]);
    assert_eq!(counters(&g), 1, "once for the batch");
}

#[test]
fn map_token_explores() {
    let mut t = Table::default();
    let bear = t.bear();
    let spell = t.card("{0}", "Sorcery", None, "Create a Map token.");
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let s = g.put(spell, P0, Zone::Hand);
    g.lands(1);
    g.main();
    g.cast(s, &[]);
    let map = *g
        .engine
        .state
        .battlefield()
        .iter()
        .find(|id| g.engine.state.objects[id].card.0 >= mtg_ir::walk::TOKEN_CARD_BASE)
        .expect("a Map");
    let before = g.count(Zone::Hand, P0) + g.pt(b).0 as usize;
    g.act(activate(map, 0), &[Target::Object(b)], &[]);
    assert!(!g.engine.state.objects.contains_key(&map), "sacrificed");
    let after = g.count(Zone::Hand, P0) + g.pt(b).0 as usize;
    assert_eq!(after, before + 1, "a land to hand or a +1/+1 counter");
}

#[test]
fn until_end_of_turn_life_total_cant_change() {
    let mut t = Table::default();
    let bear = t.bear();
    let flare = t.card(
        "{0}",
        "Instant",
        None,
        "Until end of turn, your life total can't change, and permanents you control gain hexproof and indestructible.",
    );
    let drain = t.card("{0}", "Sorcery", None, "You lose 3 life.");
    let mut g = Game::new(t);
    let b = g.put(bear, P0, Zone::Battlefield);
    let f = g.put(flare, P0, Zone::Hand);
    let d = g.put(drain, P0, Zone::Hand);
    g.main();
    g.cast(f, &[]);
    assert!(g.has(b, mtg_core::Keyword::Hexproof));
    assert!(g.has(b, mtg_core::Keyword::Indestructible));
    g.cast(d, &[]);
    assert_eq!(g.life(P0), 20);
}
