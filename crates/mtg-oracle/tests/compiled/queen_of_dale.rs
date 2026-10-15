use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const QUEEN: &str = "Whenever an opponent casts their first noncreature spell each turn, you recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)";

fn recruits(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            mtg_engine::layers::compute(&g.engine.state, &g.table, *id)
                .is_some_and(|c| c.name.as_ref() == "Human Soldier")
        })
        .collect()
}

#[test]
fn queen_recruits_only_for_each_opponents_first_noncreature_spell_each_turn() {
    let mut t = Table::default();
    let queen = t.card("{0}", "Legendary Creature", Some((2, 4)), QUEEN);
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let creature = t.card("{0}", "Creature", Some((1, 1)), "");
    let discard = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.put(queen, P0, Zone::Battlefield);
    let own = g.put(spell, P0, Zone::Hand);
    let creature = g.put(creature, P1, Zone::Hand);
    let first = g.put(spell, P1, Zone::Hand);
    let second = g.put(spell, P1, Zone::Hand);
    let later = g.put(spell, P1, Zone::Hand);
    let discard1 = g.put(discard, P0, Zone::Hand);
    let discard2 = g.put(discard, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    g.cast(own, &[]);
    assert_eq!(
        g.count(Zone::Library, P0),
        library,
        "own spells do not trigger Queen"
    );
    g.until(P1, Step::PrecombatMain);
    cast_and_keep_priority(&mut g, creature, None);
    assert!(recruits(&g).is_empty());
    cast_and_keep_priority(&mut g, first, Some(discard1));
    assert_eq!(recruits(&g).len(), 1);
    assert_eq!(g.count(Zone::Library, P0), library - 1);
    cast_and_keep_priority(&mut g, second, None);
    assert_eq!(recruits(&g).len(), 1);
    assert_eq!(g.count(Zone::Library, P0), library - 1);
    // The same opponent can trigger Queen again on someone else's turn.
    g.until(P0, Step::PrecombatMain);
    g.drive(|_, _| None, |g| g.engine.state.priority == Some(P1));
    cast_and_keep_priority(&mut g, later, Some(discard2));
    assert_eq!(recruits(&g).len(), 2);
}

#[test]
fn recruiting_discards_a_land_without_making_a_token() {
    let mut t = Table::default();
    let queen = t.card("{0}", "Legendary Creature", Some((2, 4)), QUEEN);
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let land = t.mountain();
    let mut g = Game::new(t);
    g.put(queen, P0, Zone::Battlefield);
    let land = g.put(land, P0, Zone::Hand);
    let spell = g.put(spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    let before = g.count(Zone::Hand, P0);
    let library = g.count(Zone::Library, P0);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![land])],
    );
    assert!(recruits(&g).is_empty());
    assert_eq!(g.count(Zone::Hand, P0), before);
    assert_eq!(g.count(Zone::Library, P0), library - 1);
    assert!(!g.engine.state.objects.contains_key(&land));
}

#[test]
fn recruit_recognizes_a_nonland_discarded_into_exile_by_madness() {
    let mut t = Table::default();
    let queen = t.card("{0}", "Legendary Creature", Some((2, 4)), QUEEN);
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let madness = t.card("{3}", "Sorcery", None, "You gain 4 life.\nMadness {2}");
    let mut g = Game::new(t);
    g.put(queen, P0, Zone::Battlefield);
    let madness = g.put(madness, P0, Zone::Hand);
    let spell = g.put(spell, P1, Zone::Hand);
    g.until(P1, Step::PrecombatMain);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![madness])],
    );
    assert_eq!(recruits(&g).len(), 1);
    assert_eq!(
        g.life(P0),
        20,
        "the unaffordable madness spell was not cast"
    );
}

#[test]
fn queen_counts_each_opponent_separately_and_recruit_still_discards_when_drawing_is_prevented() {
    use mtg_core::PlayerId;
    use mtg_engine::state::PlayerState;
    let mut t = Table::default();
    let queen = t.card("{0}", "Legendary Creature", Some((2, 4)), QUEEN);
    let narset = t.card(
        "{0}",
        "Enchantment",
        None,
        "Each opponent can't draw more than one card each turn.",
    );
    let spell = t.card("{0}", "Instant", None, "You gain 1 life.");
    let discard = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    let p2 = PlayerId(2);
    g.engine.state.players.insert(p2, PlayerState::new(p2, 20));
    g.engine.state.turn_order.push(p2);
    g.put(queen, P0, Zone::Battlefield);
    g.put(narset, P1, Zone::Battlefield);
    let first = g.put(spell, P1, Zone::Hand);
    let second = g.put(spell, p2, Zone::Hand);
    let discard1 = g.put(discard, P0, Zone::Hand);
    let discard2 = g.put(discard, P0, Zone::Hand);
    g.main();
    let library = g.count(Zone::Library, P0);
    g.drive(|_, _| None, |g| g.engine.state.priority == Some(P1));
    cast_and_keep_priority(&mut g, first, Some(discard1));
    g.drive(|_, _| None, |g| g.engine.state.priority == Some(p2));
    cast_and_keep_priority(&mut g, second, Some(discard2));
    assert_eq!(
        recruits(&g).len(),
        2,
        "each opponent has a first noncreature spell"
    );
    assert_eq!(
        g.count(Zone::Library, P0),
        library,
        "Narset prevented both recruit draws"
    );
    assert!(!g.engine.state.objects.contains_key(&discard1));
    assert!(!g.engine.state.objects.contains_key(&discard2));
}

fn cast_and_keep_priority(
    g: &mut Game,
    spell: mtg_core::ObjectId,
    discard: Option<mtg_core::ObjectId>,
) {
    let prompt = g.pending.take().unwrap();
    let caster = prompt.who;
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    g.drive(
        |_, c| match c.kind {
            ChoiceKind::ChooseObjects { .. } => discard.map(|id| Answer::Objects(vec![id])),
            _ => None,
        },
        |g| g.engine.state.priority == Some(caster) && g.stack().is_empty(),
    );
}
