use super::harness::*;
use mtg_core::{CardType, Color, Keyword, Step, Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};
use mtg_ir::PrintedCards;

const MIRROR: &str = "{T}: Add {R}.\nAs this artifact enters, you may have it become a copy of any creature on the battlefield until end of turn, except it has haste.";
const PRESTON: &str = "Whenever another nontoken creature you control enters, if it wasn't cast, create a token that's a copy of that creature, except it's a 0/1 white Illusion.\n{1}{W}, Sacrifice five Illusions: Exile target nonland permanent.";

fn tokens(g: &Game) -> Vec<mtg_core::ObjectId> {
    g.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| g.engine.state.objects[id].is_token)
        .collect()
}

fn clone_card(t: &mut Table) -> mtg_core::CardId {
    t.card(
        "{0}",
        "Creature — Shapeshifter",
        Some((0, 0)),
        "You may have this creature enter as a copy of any creature on the battlefield.",
    )
}

#[test]
fn cursed_mirror_copies_enters_triggers_has_haste_and_reverts_at_cleanup() {
    let mut t = Table::default();
    let mirror = t.card("{3}", "Artifact", None, MIRROR);
    let creature = t.card(
        "{2}",
        "Creature — Bear",
        Some((3, 4)),
        "When this creature enters, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(3);
    let original = g.put(creature, P1, Zone::Battlefield);
    let spell = g.put(mirror, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let copy = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| *id != original && g.engine.state.objects[id].card == creature)
        .unwrap();
    assert_eq!(g.life(P0), 21);
    assert_eq!(g.pt(copy), (3, 4));
    assert!(g.has(copy, Keyword::Haste));
    g.until(P0, Step::End);
    assert_eq!(g.engine.state.objects[&copy].card, creature);
    g.until(P1, Step::Upkeep);
    assert_eq!(g.engine.state.objects[&copy].card, mirror);
    assert!(
        g.engine
            .characteristics(&g.table, copy)
            .unwrap()
            .has_type(CardType::Artifact)
    );
    assert!(!g.has(copy, Keyword::Haste));
    assert!(
        mtg_engine::abilities::find(&g.engine.state, &g.table, copy, mtg_core::AbilityId(0))
            .is_some()
    );
}

#[test]
fn declining_mirrors_copy_keeps_the_mana_ability() {
    let mut t = Table::default();
    let mirror = t.card("{3}", "Artifact", None, MIRROR);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(mirror, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![])],
    );
    let mirror = g.find(mirror).unwrap();
    assert!(!g.has(mirror, Keyword::Haste));
    assert!(g.engine.state.objects[&mirror].original.is_none());
}

#[test]
fn a_clone_of_mirror_keeps_the_copy_and_copiable_haste_after_cleanup() {
    let mut t = Table::default();
    let mirror = t.card("{0}", "Artifact", None, MIRROR);
    let clone = clone_card(&mut t);
    let bear = t.bear();
    let mut g = Game::new(t);
    let original = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(mirror, P0, Zone::Hand);
    let clone = g.put(clone, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![original])],
    );
    let mirror = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| *id != original)
        .unwrap();
    g.act(
        Action::Cast { object: clone },
        &[],
        &[Answer::Objects(vec![mirror])],
    );
    let copy = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| *id != original && *id != mirror)
        .unwrap();
    assert!(g.has(copy, Keyword::Haste));
    g.until(P1, Step::Upkeep);
    assert_eq!(g.engine.state.objects[&copy].card, bear);
    assert!(g.has(copy, Keyword::Haste));
    assert_eq!(g.pt(copy), (2, 2));
}

#[test]
fn preston_ignores_cast_creatures_but_copies_blinked_creatures_as_white_illusions() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let creature = t.card(
        "{0}",
        "Artifact Creature — Golem",
        Some((3, 4)),
        "When this creature enters, you gain 1 life.",
    );
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    let spell = g.put(creature, P0, Zone::Hand);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert!(tokens(&g).is_empty());
    let original = g.find(creature).unwrap();
    g.cast(blink, &[Target::Object(original)]);
    assert_eq!(
        tokens(&g).len(),
        1,
        "the token must not trigger Preston again"
    );
    let token = tokens(&g)[0];
    assert_eq!(g.pt(token), (0, 1));
    assert_eq!(
        g.life(P0),
        23,
        "original cast, blinked original, and token each trigger"
    );
    let ch = g.engine.characteristics(&g.table, token).unwrap().clone();
    assert_eq!(ch.colors, mtg_core::ColorSet::single(Color::White));
    assert!(ch.has_type(CardType::Artifact));
    let illusion = g.table.subtype_named("Illusion").unwrap();
    let golem = g.table.subtype_named("Golem").unwrap();
    assert!(ch.subtypes.contains(&illusion));
    assert!(!ch.subtypes.contains(&golem));
}

#[test]
fn copies_of_prestons_tokens_inherit_their_exceptions_without_copying_pumps() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let clone = clone_card(&mut t);
    let pump = t.card(
        "{0}",
        "Instant",
        None,
        "Target creature gets +3/+3 until end of turn.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    let original = g.put(bear, P0, Zone::Battlefield);
    let blink = g.put(blink, P0, Zone::Hand);
    let clone = g.put(clone, P0, Zone::Hand);
    let pump = g.put(pump, P0, Zone::Hand);
    g.main();
    g.cast(blink, &[Target::Object(original)]);
    let token = tokens(&g)[0];
    g.cast(pump, &[Target::Object(token)]);
    assert_eq!(g.pt(token), (3, 4));
    g.act(
        Action::Cast { object: clone },
        &[],
        &[Answer::Objects(vec![token])],
    );
    let copy = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| g.engine.state.objects[id].original.is_some())
        .unwrap();
    assert_eq!(g.pt(copy), (0, 1));
    assert_eq!(
        g.engine.characteristics(&g.table, copy).unwrap().colors,
        mtg_core::ColorSet::single(Color::White)
    );
    assert_eq!(
        tokens(&g).len(),
        1,
        "the cast clone doesn't trigger Preston"
    );
}

#[test]
fn cursed_mirror_chooses_a_new_copy_when_blinked_and_restores_its_own_card_when_bounced() {
    let mut t = Table::default();
    let mirror_card = t.card("{0}", "Artifact", None, MIRROR);
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target artifact, then return it to the battlefield under its owner's control.",
    );
    let bounce = t.card(
        "{0}",
        "Instant",
        None,
        "Return target creature to its owner's hand.",
    );
    let mut g = Game::new(t);
    let original = g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(mirror_card, P0, Zone::Hand);
    let blink = g.put(blink, P0, Zone::Hand);
    let bounce = g.put(bounce, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![])],
    );
    let mirror = g.find(mirror_card).unwrap();
    g.act(
        Action::Cast { object: blink },
        &[Target::Object(mirror)],
        &[Answer::Objects(vec![original])],
    );
    let copy = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .find(|id| *id != original)
        .unwrap();
    assert_eq!(g.pt(copy), (2, 2));
    assert!(g.has(copy, Keyword::Haste));
    g.cast(bounce, &[Target::Object(copy)]);
    let returned = g
        .engine
        .state
        .objects
        .values()
        .find(|o| o.card == mirror_card)
        .unwrap();
    assert_eq!(returned.zone.zone, Zone::Hand);
    assert!(!returned.copy_until_end_of_turn);
    assert!(returned.copy_modifications.is_empty());
}

#[test]
fn prestons_fixed_size_overrides_the_copied_characteristic_defining_ability() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let creature = t.card("{0}", "Creature — Avatar", Some((0, 0)), "This creature's power and toughness are each equal to the number of creatures you control.");
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    let original = g.put(creature, P0, Zone::Battlefield);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(original), (2, 2));
    g.cast(blink, &[Target::Object(original)]);
    assert_eq!(tokens(&g).len(), 1);
    let token = tokens(&g)[0];
    assert_eq!(g.pt(token), (0, 1));
    let ch = g.engine.characteristics(&g.table, token).unwrap();
    assert!(
        !ch.abilities.contains(&mtg_core::AbilityId(0)),
        "the defining ability is not copied"
    );
}

#[test]
fn preston_does_not_copy_opponents_creatures() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    let original = g.put(bear, P1, Zone::Battlefield);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    g.cast(blink, &[Target::Object(original)]);
    assert!(tokens(&g).is_empty());
}

#[test]
fn prestons_token_has_its_exceptions_before_entering_triggers_are_checked() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let watcher = t.card(
        "{0}",
        "Enchantment",
        None,
        "Whenever a creature with power 1 or less enters, you gain 1 life.",
    );
    let bear = t.bear();
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    g.put(preston, P0, Zone::Battlefield);
    g.put(watcher, P0, Zone::Battlefield);
    let original = g.put(bear, P0, Zone::Battlefield);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    g.cast(blink, &[Target::Object(original)]);
    assert_eq!(
        g.life(P0),
        21,
        "only the 0/1 Illusion satisfies the enter trigger"
    );
}

#[test]
fn preston_can_sacrifice_five_of_its_illusions_to_exile_a_nonland_permanent() {
    let mut t = Table::default();
    let preston = t.named_card(
        "Preston, the Vanisher",
        "{3}{W}",
        "Legendary Creature — Rabbit Wizard",
        Some((2, 5)),
        PRESTON,
    );
    let bear = t.bear();
    let artifact = t.card("{0}", "Artifact", None, "");
    let blink = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target creature, then return it to the battlefield under its owner's control.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let preston = g.put(preston, P0, Zone::Battlefield);
    let mut original = g.put(bear, P0, Zone::Battlefield);
    let victim = g.put(artifact, P1, Zone::Battlefield);
    let spells: Vec<_> = (0..5).map(|_| g.put(blink, P0, Zone::Hand)).collect();
    g.main();
    for spell in spells {
        g.cast(spell, &[Target::Object(original)]);
        original = g
            .engine
            .state
            .battlefield()
            .into_iter()
            .find(|id| {
                let o = &g.engine.state.objects[id];
                o.card == bear && !o.is_token
            })
            .unwrap();
    }
    let sacrifices = tokens(&g);
    assert_eq!(sacrifices.len(), 5);
    g.act(
        activate(preston, 1),
        &[Target::Object(victim)],
        &[Answer::Objects(sacrifices)],
    );
    assert!(tokens(&g).is_empty());
    assert!(
        g.engine
            .state
            .objects
            .values()
            .any(|o| o.card == artifact && o.zone.zone == Zone::Exile)
    );
    assert!(g.engine.state.objects.contains_key(&original));
}
