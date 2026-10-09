//! CR 800.4 — what happens when a player leaves a multiplayer game: to their
//! objects (800.4a/d), to "each player" and "each opponent", and to combat damage
//! (800.4e). Expectations come from the Comprehensive Rules text
//! (2026-09-25 release), not from recorded production output.
mod support;

use mtg_core::{CardId, CardType, ManaCost, ObjectId, PlayerId, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    layers::{PrintedCards, layer},
    state::{AffectedSet, CastContext, ContinuousEffect, GameState},
};
use mtg_ir::{CardFace, Selector, effect::Duration, effect::Modification};
use support::{CREATURE, Cards, P0, P1};

const P2: PlayerId = PlayerId(2);
const MOURNER: CardId = CardId(1);
const HERALD: CardId = CardId(2);
const CENSUS: CardId = CardId(3);
const STRIKER: CardId = CardId(4);
const SOLDIER: CardId = CardId(5);
const DOOM: CardId = CardId(6);

/// The plain 2/2 creature plus 2/2 creatures compiled from rules text:
/// - Mourner: "When this creature dies, each player gains 1 life."
/// - Herald: "{0}: Each player creates a 1/1 white Soldier creature token."
/// - Census: "{0}: You gain 1 life for each opponent you have."
/// - Striker: "Lifelink"
struct Faces {
    creature: Cards,
    compiled: Vec<(CardId, CardFace)>,
}

struct NoSubtypes;
impl mtg_oracle::convert::Subtypes for NoSubtypes {
    fn intern(&self, name: &str) -> Option<u16> {
        (name == "Soldier").then_some(0)
    }
}

fn compiled_creature(name: &str, text: &str) -> CardFace {
    let compiled = mtg_oracle::compile::compile(
        &mtg_oracle::compile::FaceText {
            name,
            card_types: &[CardType::Creature],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "",
        },
        &NoSubtypes,
    );
    assert!(compiled.unparsed.is_empty(), "{:?}", compiled.unparsed);
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Creature],
        subtypes: vec![],
        supertypes: vec![],
        power: Some(2),
        toughness: Some(2),
        loyalty: None,
        abilities: compiled.abilities,
        oracle_text: Some(text.into()),
        colors: None,
    }
}

/// Token specs must be registered as card faces before the engine can create them,
/// as the card loader does for real cards.
fn register_tokens(face: &mut CardFace, as_card: CardId) {
    for ability in &mut face.abilities {
        if let mtg_ir::AbilityKind::Activated {
            effect: mtg_ir::Effect::CreateToken { token, .. },
            ..
        } = &mut ability.kind
        {
            token.card = Some(as_card);
        }
    }
}

impl Faces {
    fn new() -> Self {
        let mut herald = compiled_creature(
            "Herald",
            "{0}: Each player creates a 1/1 white Soldier creature token.",
        );
        register_tokens(&mut herald, SOLDIER);
        let mut soldier = compiled_creature("Soldier", "");
        soldier.power = Some(1);
        soldier.toughness = Some(1);
        Self {
            creature: Cards::creature(2, false),
            compiled: vec![
                (
                    MOURNER,
                    compiled_creature(
                        "Mourner",
                        "When this creature dies, each player gains 1 life.",
                    ),
                ),
                (HERALD, herald),
                (SOLDIER, soldier),
                (
                    CENSUS,
                    compiled_creature("Census", "{0}: You gain 1 life for each opponent you have."),
                ),
                (STRIKER, compiled_creature("Striker", "Lifelink")),
                (
                    DOOM,
                    compiled_creature("Doom", "{0}: Target player loses the game."),
                ),
            ],
        }
    }
}

impl PrintedCards for Faces {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        match self.compiled.iter().find(|(id, _)| *id == card) {
            Some((_, f)) => Some(f),
            None => self.creature.face(card, face),
        }
    }

    fn subtype_name(&self, _subtype: mtg_core::Subtype) -> Option<&str> {
        None
    }
}

fn three_players() -> GameState {
    let mut state = GameState::new(&[P0, P1, P2], 20);
    state.turn = 2;
    state.step = mtg_core::Step::PrecombatMain;
    state.active_player = P0;
    state.priority = Some(P0);
    for p in [P0, P1, P2] {
        for _ in 0..5 {
            state.place(CREATURE, p, ZoneRef::of(Zone::Library, p));
        }
    }
    state
}

/// Advance to the next decision, which must not end the game.
fn next_choice(engine: &mut Engine, cards: &dyn PrintedCards) -> mtg_engine::Choice {
    for _ in 0..200 {
        match engine.advance(cards) {
            Progress::NeedsChoice(choice) => return choice,
            Progress::GameOver { .. } => panic!("two players remain; the game must continue"),
            _ => {}
        }
    }
    panic!("engine did not reach a decision");
}

/// No ordered zone lists an object that no longer exists, and every object in an
/// ordered zone is listed there exactly once.
fn assert_zone_index_consistent(state: &GameState) {
    for (zone, order) in &state.zone_order {
        for id in order {
            let obj = state
                .objects
                .get(id)
                .unwrap_or_else(|| panic!("{zone:?} lists missing object {id:?}"));
            assert_eq!(obj.zone, *zone, "{id:?} indexed under the wrong zone");
        }
    }
    for (id, obj) in &state.objects {
        if obj.zone.zone.is_ordered() {
            let listed = state
                .zone_order
                .get(&obj.zone)
                .map_or(0, |o| o.iter().filter(|x| *x == id).count());
            assert_eq!(listed, 1, "{id:?} in {:?} indexed {listed} times", obj.zone);
        }
    }
}

fn owned_by(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state
        .objects
        .values()
        .filter(|o| o.owner == player)
        .map(|o| o.id)
        .collect()
}

fn stack_object(
    state: &mut GameState,
    card: CardId,
    owner: PlayerId,
    controller: PlayerId,
) -> ObjectId {
    let id = state.place(card, owner, ZoneRef::shared(Zone::Stack));
    let obj = state.objects.get_mut(&id).unwrap();
    obj.controller = controller;
    obj.cast_context = Some(CastContext::default());
    id
}

/// CR 800.4a: "all objects owned by that player leave the game" — in every zone,
/// including the shared battlefield, stack and exile.
#[test]
fn cr_800_4a_owned_objects_in_every_zone_leave_the_game() {
    let mut state = three_players();
    for zone in [
        ZoneRef::shared(Zone::Battlefield),
        ZoneRef::shared(Zone::Exile),
        ZoneRef::shared(Zone::Command),
        ZoneRef::of(Zone::Hand, P1),
        ZoneRef::of(Zone::Graveyard, P1),
    ] {
        state.place(CREATURE, P1, zone);
    }
    stack_object(&mut state, CREATURE, P1, P1);
    let survivors: Vec<_> = state
        .objects
        .values()
        .filter(|o| o.owner != P1)
        .map(|o| o.id)
        .collect();
    let mine = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    state.players.get_mut(&P1).unwrap().life = 0;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    let choice = next_choice(&mut engine, &cards);

    assert!(engine.state.players[&P1].has_lost);
    assert_eq!(
        owned_by(&engine.state, P1),
        vec![],
        "P1's objects remain in the game"
    );
    for id in survivors.iter().chain([&mine]) {
        assert!(
            engine.state.objects.contains_key(id),
            "{id:?} owned by a remaining player was disturbed"
        );
    }
    assert_eq!(
        engine.state.objects[&mine].zone,
        ZoneRef::shared(Zone::Battlefield)
    );
    assert_zone_index_consistent(&engine.state);
    assert_ne!(choice.who, P1);
}

/// CR 800.4a: "any effects which give that player control of any objects ... end".
/// The permanent stays where it is, under its owner's control, as the same object.
#[test]
fn cr_800_4a_control_effects_for_the_departed_player_end() {
    let mut state = three_players();
    let stolen = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    let id = state.new_object_id();
    let timestamp = state.bump();
    state.continuous.push(ContinuousEffect {
        id,
        source: stolen,
        affected: AffectedSet::Fixed(vec![stolen]),
        modification: Modification::Control(Selector::You),
        duration: Duration::Permanent,
        timestamp,
        layer: layer::CONTROL,
        ability: None,
        controller: Some(P1),
    });
    state.players.get_mut(&P1).unwrap().life = 0;
    let cards = Faces::new();
    assert_eq!(mtg_engine::layers::controller(&state, stolen), Some(P1));
    let mut engine = Engine::new(state);
    next_choice(&mut engine, &cards);

    let obj = engine
        .state
        .objects
        .get(&stolen)
        .expect("a control effect ending is not a zone change");
    assert_eq!(obj.zone, ZoneRef::shared(Zone::Battlefield));
    assert_eq!(
        mtg_engine::layers::controller(&engine.state, stolen),
        Some(P0)
    );
    assert_zone_index_consistent(&engine.state);
}

/// CR 800.4a: "if there are any objects still controlled by that player, those
/// objects are exiled" — a permanent put onto the battlefield under the departed
/// player's control, with no effect to end.
#[test]
fn cr_800_4a_objects_still_controlled_by_the_departed_player_are_exiled() {
    let mut state = three_players();
    let borrowed = state.place(CREATURE, P2, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&borrowed).unwrap().controller = P1;
    state.players.get_mut(&P1).unwrap().life = 0;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    next_choice(&mut engine, &cards);

    assert!(
        !engine.state.objects.contains_key(&borrowed),
        "exile is a zone change: a new object"
    );
    let exiled: Vec<_> = engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .into_iter()
        .filter(|id| engine.state.objects[id].owner == P2)
        .collect();
    assert_eq!(
        exiled.len(),
        1,
        "P2's card is exiled, not removed from the game"
    );
    assert!(
        engine
            .state
            .battlefield()
            .iter()
            .all(|id| engine.state.objects[id].owner != P2)
    );
    assert_zone_index_consistent(&engine.state);
}

/// CR 800.4a on the stack: a departed player's ability ceases to exist even when its
/// source belongs to someone else, and a spell they control but don't own is exiled.
#[test]
fn cr_800_4a_departed_players_stack_objects() {
    let mut state = three_players();
    let source = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    let ability = stack_object(&mut state, CREATURE, P0, P1);
    state.objects.get_mut(&ability).unwrap().cast_context = Some(CastContext {
        ability: Some(mtg_core::AbilityId(0)),
        source: Some(source),
        ..Default::default()
    });
    let spell = stack_object(&mut state, CREATURE, P2, P1);
    state.players.get_mut(&P1).unwrap().life = 0;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    next_choice(&mut engine, &cards);

    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
    );
    assert!(!engine.state.objects.contains_key(&ability));
    assert!(!engine.state.objects.contains_key(&spell));
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .iter()
            .any(|id| engine.state.objects[id].owner == P2),
        "the spell's card goes to exile"
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .iter()
            .all(|id| engine.state.objects[id].owner != P0),
        "an ability is not a card: it ceases to exist rather than being exiled"
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P2))
            .is_empty()
    );
    assert!(engine.state.objects.contains_key(&source));
    assert_zone_index_consistent(&engine.state);
}

/// CR 800.4a: "This is not a state-based action. It happens as soon as the player
/// leaves the game. If the player who left the game had priority at the time they
/// left, priority passes to the next player in turn order who's still in the game."
#[test]
fn cr_800_4a_conceding_with_priority_removes_objects_and_passes_priority() {
    let mut state = three_players();
    let theirs = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    let first = next_choice(&mut engine, &cards);
    assert_eq!(first.who, P0);
    engine.answer(&cards, first.id, Answer::Pass).unwrap();
    let second = next_choice(&mut engine, &cards);
    assert_eq!(second.who, P1);
    assert!(matches!(second.kind, ChoiceKind::Priority { .. }));
    engine
        .answer(&cards, second.id, Answer::Action(Action::Concede))
        .unwrap();
    let third = next_choice(&mut engine, &cards);

    assert!(engine.state.players[&P1].has_lost);
    assert!(!engine.state.objects.contains_key(&theirs));
    assert_eq!(owned_by(&engine.state, P1), vec![]);
    assert_eq!(third.who, P2);
    assert_zone_index_consistent(&engine.state);
}

/// CR 800.4d: "If a triggered ability that would be controlled by a player who has
/// left the game would be put onto the stack, it isn't put on the stack." A creature
/// P2 owns but P1 controls dies in the same state-based-action batch in which P1
/// loses: the card stays in the game, in P2's graveyard, but its dies trigger was
/// P1's.
#[test]
fn cr_800_4d_departed_players_triggers_are_not_put_on_the_stack() {
    let mut state = three_players();
    let mourner = state.place(MOURNER, P2, ZoneRef::shared(Zone::Battlefield));
    let obj = state.objects.get_mut(&mourner).unwrap();
    obj.controller = P1;
    obj.damage = 2;
    state.players.get_mut(&P1).unwrap().life = 0;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    let choice = next_choice(&mut engine, &cards);

    assert!(matches!(choice.kind, ChoiceKind::Priority { .. }));
    assert!(
        engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
    );
    assert_eq!(engine.state.players[&P0].life, 20);
    assert_eq!(engine.state.players[&P2].life, 20);
    assert_eq!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P2))
            .len(),
        1,
        "the card itself remains in the game"
    );
}

/// Control check of the trigger above: with its controller still in the game, the
/// same dies trigger does go on the stack.
#[test]
fn control_departed_trigger_fixture_triggers_for_a_remaining_player() {
    let mut state = three_players();
    let mourner = state.place(MOURNER, P2, ZoneRef::shared(Zone::Battlefield));
    let obj = state.objects.get_mut(&mourner).unwrap();
    obj.controller = P1;
    obj.damage = 2;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    next_choice(&mut engine, &cards);
    assert_eq!(
        engine.state.objects_in(ZoneRef::shared(Zone::Stack)).len(),
        1
    );
}

/// A player who has left the game: no longer in it, holding nothing.
fn departed(state: &mut GameState, player: PlayerId) {
    state.players.get_mut(&player).unwrap().has_lost = true;
    let theirs: Vec<_> = owned_by(state, player);
    for id in theirs {
        let zone = state.objects.remove(&id).unwrap().zone;
        if let Some(order) = state.zone_order.get_mut(&zone) {
            order.retain(|x| *x != id);
        }
    }
}

/// Activate P0's only activated ability of `source`, then pass until the stack is
/// empty again.
fn activate_and_resolve(engine: &mut Engine, cards: &Faces, source: ObjectId) {
    let ability = cards
        .face(engine.state.objects[&source].card, 0)
        .unwrap()
        .abilities
        .iter()
        .find(|a| matches!(a.kind, mtg_ir::AbilityKind::Activated { .. }))
        .unwrap()
        .id;
    let choice = next_choice(engine, cards);
    assert_eq!(choice.who, P0);
    engine
        .answer(
            cards,
            choice.id,
            Answer::Action(Action::ActivateAbility { source, ability }),
        )
        .unwrap();
    for _ in 0..20 {
        let choice = next_choice(engine, cards);
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            return;
        }
        assert_ne!(choice.who, P1, "a departed player was asked something");
        let answer = if matches!(choice.kind, ChoiceKind::Priority { .. }) {
            Answer::Pass
        } else {
            mtg_policy::well_formed(&choice, &engine.view_for(choice.who))
        };
        engine.answer(cards, choice.id, answer).unwrap();
    }
    panic!("the ability did not resolve");
}

/// CR 800.4a/800.4d: a player who has left is no longer one of "each player" — no
/// token is created for them (no object would be owned by them).
#[test]
fn cr_800_4_each_player_means_players_still_in_the_game() {
    let mut state = three_players();
    let herald = state.place(HERALD, P0, ZoneRef::shared(Zone::Battlefield));
    departed(&mut state, P1);
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    activate_and_resolve(&mut engine, &cards, herald);

    let tokens = |p| {
        engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| engine.state.objects[id].is_token && engine.state.objects[id].owner == p)
            .count()
    };
    assert_eq!((tokens(P0), tokens(P1), tokens(P2)), (1, 0, 1));
}

/// A departed player is no longer anyone's opponent: P0 has one opponent left.
#[test]
fn cr_800_4_departed_player_is_no_longer_an_opponent() {
    let mut state = three_players();
    let census = state.place(CENSUS, P0, ZoneRef::shared(Zone::Battlefield));
    departed(&mut state, P1);
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    activate_and_resolve(&mut engine, &cards, census);
    assert_eq!(engine.state.players[&P0].life, 21);
}

/// Drive one combat in which P0's lifelinking Striker attacks P1 unblocked. With
/// `concede`, P1 concedes while holding priority in the declare blockers step.
fn lifelink_attack(concede: bool) -> Engine {
    let mut state = three_players();
    let striker = state.place(STRIKER, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&striker).unwrap().summoning_sick = false;
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    let mut conceded = false;
    for _ in 0..200 {
        if engine.state.step == mtg_core::Step::EndCombat
            || engine.state.step == mtg_core::Step::PostcombatMain
        {
            assert_eq!(
                conceded, concede,
                "the fixture never reached the concession"
            );
            return engine;
        }
        let choice = next_choice(&mut engine, &cards);
        let answer = match &choice.kind {
            ChoiceKind::DeclareAttackers { .. } => {
                Answer::Attackers(vec![(striker, mtg_core::Target::Player(P1))])
            }
            ChoiceKind::DeclareBlockers { .. } => Answer::Blocks(vec![]),
            ChoiceKind::Priority { .. }
                if concede
                    && choice.who == P1
                    && engine.state.step == mtg_core::Step::DeclareBlockers =>
            {
                conceded = true;
                Answer::Action(Action::Concede)
            }
            ChoiceKind::Priority { .. } => Answer::Pass,
            _ => mtg_policy::well_formed(&choice, &engine.view_for(choice.who)),
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    panic!("combat did not finish");
}

/// CR 800.4e: "If combat damage would be assigned to a player who has left the game,
/// that damage isn't assigned." An unblocked attacker whose defending player left
/// deals no damage, so lifelink gains nothing.
#[test]
fn cr_800_4e_no_combat_damage_is_assigned_to_a_departed_player() {
    let engine = lifelink_attack(true);
    assert!(engine.state.players[&P1].has_lost);
    assert_eq!(
        engine.state.players[&P0].life, 20,
        "lifelink from unassigned damage"
    );
    assert!(!engine.log.iter().any(|e| matches!(
        e.event,
        mtg_core::Event::DamageDealtToPlayer { player, .. } if player == P1
    )));
}

/// Control for the fixture above: the same attack against a player still in the game
/// deals 2 and gains 2.
#[test]
fn control_lifelink_attack_against_a_remaining_player() {
    let engine = lifelink_attack(false);
    assert_eq!(engine.state.players[&P1].life, 18);
    assert_eq!(engine.state.players[&P0].life, 22);
}

fn control_effect(state: &mut GameState, object: ObjectId, by: PlayerId) {
    let id = state.new_object_id();
    let timestamp = state.bump();
    state.continuous.push(ContinuousEffect {
        id,
        source: object,
        affected: AffectedSet::Fixed(vec![object]),
        modification: Modification::Control(Selector::You),
        duration: Duration::Permanent,
        timestamp,
        layer: layer::CONTROL,
        ability: None,
        controller: Some(by),
    });
}

/// CR 800.4a ends only the effects giving the departed player control: an earlier
/// effect giving a remaining player control applies again.
#[test]
fn cr_800_4a_other_players_control_effect_persists() {
    let mut state = three_players();
    let object = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    control_effect(&mut state, object, P2);
    control_effect(&mut state, object, P0);
    assert_eq!(mtg_engine::layers::controller(&state, object), Some(P0));
    let cards = Faces::new();
    let mut engine = Engine::new(state);
    let first = next_choice(&mut engine, &cards);
    engine
        .answer(&cards, first.id, Answer::Action(Action::Concede))
        .unwrap();
    next_choice(&mut engine, &cards);
    assert!(engine.state.objects.contains_key(&object));
    assert_eq!(
        mtg_engine::layers::controller(&engine.state, object),
        Some(P2)
    );
    assert_eq!(engine.state.continuous.len(), 1);
}

/// CR 113.7a / 800.4a: an ability on the stack belongs to no one. When its source's
/// owner leaves, the ability another player activated stays and resolves — counting
/// opponents as they are when it resolves.
#[test]
fn cr_800_4a_remaining_players_ability_outlives_its_sources_owner() {
    let mut state = three_players();
    let census = state.place(CENSUS, P2, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&census).unwrap().controller = P0;
    let cards = Faces::new();
    let ability = cards.face(CENSUS, 0).unwrap().abilities[0].id;
    let mut engine = Engine::new(state);
    let first = next_choice(&mut engine, &cards);
    engine
        .answer(
            &cards,
            first.id,
            Answer::Action(Action::ActivateAbility {
                source: census,
                ability,
            }),
        )
        .unwrap();
    let mut conceded = false;
    for _ in 0..20 {
        let choice = next_choice(&mut engine, &cards);
        if engine
            .state
            .objects_in(ZoneRef::shared(Zone::Stack))
            .is_empty()
        {
            break;
        }
        let answer = if choice.who == P2 && !conceded {
            conceded = true;
            Answer::Action(Action::Concede)
        } else {
            Answer::Pass
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert!(conceded);
    assert!(!engine.state.objects.contains_key(&census));
    assert_eq!(engine.state.players[&P0].life, 21, "one opponent left: P1");
}

/// CR 104.3e / 800.4a: a player who loses to an effect leaves as it resolves, taking
/// their objects with them; the other two play on.
#[test]
fn cr_800_4a_losing_to_an_effect_removes_the_players_objects() {
    let mut state = three_players();
    let doom = state.place(DOOM, P0, ZoneRef::shared(Zone::Battlefield));
    let theirs = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    let cards = Faces::new();
    let ability = cards.face(DOOM, 0).unwrap().abilities[0].id;
    let mut engine = Engine::new(state);
    let first = next_choice(&mut engine, &cards);
    engine
        .answer(
            &cards,
            first.id,
            Answer::Action(Action::ActivateAbility {
                source: doom,
                ability,
            }),
        )
        .unwrap();
    for _ in 0..20 {
        let choice = next_choice(&mut engine, &cards);
        if engine.state.players[&P1].has_lost
            && engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty()
        {
            break;
        }
        let answer = match choice.kind {
            ChoiceKind::ChooseTargets { .. } => {
                Answer::Targets(vec![vec![mtg_core::Target::Player(P1)]])
            }
            _ => Answer::Pass,
        };
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert!(engine.state.players[&P1].has_lost);
    assert!(!engine.state.objects.contains_key(&theirs));
    assert_eq!(owned_by(&engine.state, P1), vec![]);
    assert_zone_index_consistent(&engine.state);
}
