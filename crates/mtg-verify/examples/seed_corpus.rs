//! Explicit maintenance tool: run only when intentionally creating/migrating fixtures.
use mtg_core::{AbilityId, CardId, CardType, ManaCost, PlayerId, Step, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};
use mtg_ir::{
    Ability, AbilityKind, CardFace, Effect, ObjectFilter, Selector, Value,
    trigger::{EventPattern, Trigger, TriggerTiming},
};
use mtg_verify::{
    campaign,
    scenario::{
        self, GameScenario, Metadata, ScenarioAction, ScenarioAssertions, ScenarioObject,
        ScenarioPlayer, ScenarioState,
    },
};
use std::{collections::BTreeMap, fs, path::Path};

fn face(name: &str, kind: CardType, ability: Option<AbilityKind>) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![kind],
        subtypes: vec![],
        supertypes: vec![],
        power: (kind == CardType::Creature).then_some(2),
        toughness: (kind == CardType::Creature).then_some(2),
        loyalty: None,
        abilities: ability
            .into_iter()
            .map(|kind| Ability {
                id: AbilityId(0),
                kind,
                targets: vec![],
                source_text: None,
            })
            .collect(),
        oracle_text: None,
        colors: None,
    }
}
fn gain() -> Effect {
    Effect::GainLife {
        who: Selector::You,
        amount: Value::Fixed(3),
    }
}
fn object(card: u32, owner: u8, zone: Zone) -> ScenarioObject {
    let player = PlayerId(owner);
    ScenarioObject {
        card: CardId(card),
        owner: player,
        controller: None,
        zone: if zone.is_shared() {
            ZoneRef::shared(zone)
        } else {
            ZoneRef::of(zone, player)
        },
        tapped: false,
        damage: 0,
        counters: BTreeMap::new(),
    }
}
fn base(players: u8, cards: Vec<CardFace>, mut objects: Vec<ScenarioObject>) -> GameScenario {
    for p in 0..players {
        for _ in 0..8 {
            objects.push(object(0, p, Zone::Library));
        }
    }
    let mut result = GameScenario {
        format_version: scenario::FORMAT_VERSION,
        rules_version: scenario::RULES_VERSION.into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        card_db_hash: String::new(),
        seed: 42,
        metadata: Metadata::default(),
        cards,
        subtypes: vec![],
        initial_state: ScenarioState {
            players: (0..players)
                .map(|p| ScenarioPlayer {
                    id: PlayerId(p),
                    life: 20,
                    poison: 0,
                    mana: Default::default(),
                })
                .collect(),
            active_player: PlayerId(0),
            turn: 2,
            step: Step::PrecombatMain,
            objects,
            commanders: BTreeMap::new(),
        },
        actions: vec![],
        expected: ScenarioAssertions::default(),
    };
    result.card_db_hash = result.card_hash();
    result
}
fn write(name: &str, scenario: &GameScenario) {
    let (report, recorded) = scenario::run(scenario, true).unwrap();
    assert!(report.pass, "{name}: {}", report.message);
    fs::write(
        Path::new("tests/replays").join(name),
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();
}
fn scripted(mut artifact: GameScenario, casts: usize, expected_life: i32) -> GameScenario {
    let (mut engine, cards) = artifact.setup().unwrap();
    let mut cast = 0;
    for _ in 0..32 {
        let choice = scenario::next_choice(&mut engine, &cards).unwrap().unwrap();
        if cast == casts
            && engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty()
        {
            break;
        }
        let answer = match &choice.kind {
            ChoiceKind::Priority { legal } if cast < casts => {
                if let Some(action) = legal
                    .actions
                    .iter()
                    .find(|a| matches!(a, Action::Cast { .. }))
                {
                    cast += 1;
                    Answer::Action(action.clone())
                } else {
                    Answer::Pass
                }
            }
            ChoiceKind::Priority { .. } => Answer::Pass,
            _ => mtg_policy::well_formed(&choice, &engine.view_for(choice.who)),
        };
        artifact.actions.push(ScenarioAction {
            who: choice.who,
            answer: answer.clone(),
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert_eq!(cast, casts);
    artifact.expected.life.insert(PlayerId(0), expected_life);
    artifact
}

fn combat_recording(mut artifact: GameScenario) -> GameScenario {
    let (mut engine, cards) = artifact.setup().unwrap();
    for _ in 0..24 {
        let choice = scenario::next_choice(&mut engine, &cards).unwrap().unwrap();
        let answer = match &choice.kind {
            ChoiceKind::Priority { .. } => Answer::Pass,
            ChoiceKind::DeclareAttackers { eligible, .. } => Answer::Objects(eligible.clone()),
            _ => mtg_policy::well_formed(&choice, &engine.view_for(choice.who)),
        };
        artifact.actions.push(ScenarioAction {
            who: choice.who,
            answer: answer.clone(),
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    scenario::next_choice(&mut engine, &cards).unwrap();
    assert!(
        engine.state.players[&PlayerId(1)].life < 20,
        "combat fixture must actually deal combat damage"
    );
    artifact
}
fn main() {
    let creature = face("Synthetic 2/2", CardType::Creature, None);
    let instant = face(
        "Synthetic gain three",
        CardType::Instant,
        Some(AbilityKind::SpellEffect(gain())),
    );
    let mut basic = scripted(
        base(
            2,
            vec![creature.clone(), instant.clone()],
            vec![object(1, 0, Zone::Hand)],
        ),
        1,
        23,
    );
    basic.metadata.description = "Synthetic scripted casting: one free gain-three spell; independently expected life 20+3=23".into();
    basic.metadata.rules = vec!["601".into(), "608".into(), "119.3".into()];
    write("basic_casting.json", &basic);
    let mut stack = scripted(
        base(
            2,
            vec![creature.clone(), instant],
            vec![object(1, 0, Zone::Hand), object(1, 0, Zone::Hand)],
        ),
        2,
        26,
    );
    stack.metadata.description = "Synthetic scripted stack: two free instants announced before either resolves; independent total life 26".into();
    write("stack_interaction.json", &stack);
    let mut triggered = creature.clone();
    triggered.abilities.push(Ability {
        id: AbilityId(0),
        targets: vec![],
        source_text: None,
        kind: AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                functions_from: Zone::Battlefield,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: gain(),
        },
    });
    let mut trigger = scripted(
        base(
            2,
            vec![creature.clone(), triggered],
            vec![object(1, 0, Zone::Hand)],
        ),
        1,
        23,
    );
    trigger.metadata.description =
        "Synthetic creature casting and enters trigger; independently expected life 23".into();
    write("enters_trigger.json", &trigger);
    for (name, players, objects, description) in [
        (
            "combat.json",
            2,
            vec![
                object(0, 0, Zone::Battlefield),
                object(0, 1, Zone::Battlefield),
            ],
            "Synthetic legal-action campaign with opposing creatures; combat choices recorded",
        ),
        (
            "multiplayer_priority.json",
            4,
            vec![],
            "Synthetic four-player legal-action campaign exercising turn and priority rotation",
        ),
    ] {
        let source = base(players, vec![creature.clone()], objects);

        let mut replay = if players == 2 {
            combat_recording(source)
        } else {
            let campaign = campaign::semantic(&source, 7045, 16).unwrap();
            assert!(campaign.failure.is_none(), "{:?}", campaign.failure);
            campaign.reproduction
        };
        replay.metadata.description = description.into();
        write(name, &replay);
    }
    let mut elimination = base(4, vec![creature.clone()], vec![]);
    elimination.initial_state.players[3].life = 0;
    let campaign = campaign::semantic(&elimination, 7045, 12).unwrap();
    assert!(campaign.failure.is_none(), "{:?}", campaign.failure);
    let mut replay = campaign.reproduction;
    replay.metadata.description = "Synthetic four-player game: zero-life fourth player is eliminated before priority rotation".into();
    write("multiplayer_elimination.json", &replay);

    let source = base(2, vec![creature], vec![]);
    let campaign = campaign::semantic(&source, 42, 1).unwrap();
    assert!(campaign.failure.is_none());
    let mut regression = campaign.reproduction;
    regression.initial_state.objects.clear();
    regression.expected = ScenarioAssertions::default();
    regression.metadata = Metadata {
        issue: Some("local_canonical_life_omission".into()),
        description: "Confirmed verification-harness regression: DiagnosticState omitted player life, allowing changed life to produce the same canonical digest. One priority pass is sufficient to detect the corrected state difference; this is not a historical Magic-engine bug.".into(),
        rules: vec!["119".into()],
        first_affected: None,
        fixed_in: Some("working-tree diagnostic snapshot fix".into()),
    };
    let (report, recorded) = scenario::run(&regression, true).unwrap();
    assert!(report.pass);
    fs::write(
        "tests/regressions/issue_local_canonical_life_omission.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();

    // Independently minimized from semantic seed 484: two passes, no cards
    // in zones, one previously eliminated seat between surviving players.
    let mut regression = base(4, recorded.cards.clone(), vec![]);
    regression.seed = 484;
    regression.initial_state.objects.clear();
    regression.initial_state.active_player = PlayerId(1);
    regression.initial_state.players[3].life = 0;
    regression.actions = [PlayerId(1), PlayerId(2)]
        .into_iter()
        .map(|who| ScenarioAction {
            who,
            answer: Answer::Pass,
            expected_choice: None,
            expected_state: None,
            expected_digest: None,
        })
        .collect();
    regression.metadata = Metadata {
        issue: Some("local_priority_eliminated_player".into()),
        description: "Confirmed engine bug independently minimized from semantic seed 484: after P1 and P2 pass, eliminated P3 incorrectly held state priority. The surviving P0 must receive both the priority choice and state priority. Two answers, no zone objects.".into(),
        rules: vec!["117.3d".into(), "800.4".into()],
        first_affected: None,
        fixed_in: Some("working-tree next-player and priority-state fix".into()),
    };
    let (report, recorded) = scenario::run(&regression, true).unwrap();
    assert!(report.pass);
    fs::write(
        "tests/regressions/issue_local_priority_eliminated_player.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();

    let creature = face("Synthetic 2/2", CardType::Creature, None);
    let mut commander = creature.clone();
    commander.name = "Synthetic legendary commander".into();
    commander.supertypes.push(mtg_core::Supertype::Legendary);
    let mut source = base(
        4,
        vec![creature, commander],
        vec![object(1, 0, Zone::Command)],
    );
    for player in &mut source.initial_state.players {
        player.life = 40;
    }
    source
        .initial_state
        .commanders
        .insert(PlayerId(0), CardId(1));
    let mut replay = scripted(source, 1, 40);
    replay.metadata.description = "Synthetic four-player Commander setup: legendary commander cast from shared command zone at 40 life; not a full Commander game".into();
    replay.metadata.rules = vec!["400.1".into(), "903.8".into()];
    write("commander_cast.json", &replay);
    replay
        .initial_state
        .objects
        .retain(|object| object.zone.zone == Zone::Command);
    replay.actions.truncate(1);
    replay.expected = ScenarioAssertions::default();
    replay.metadata = Metadata {
        issue: Some("local_shared_command_zone_casting".into()),
        description: "Confirmed engine representation bug: shared Command objects were uncastable because legal-action lookup used player-private Command instances. Independent golden tests failed before the engine normalized casting and return destinations to shared Command.".into(),
        rules: vec!["400.1".into(), "903.8".into()], first_affected: None,
        fixed_in: Some("working-tree shared command-zone consistency fix".into()),
    };
    let (report, recorded) = scenario::run(&replay, true).unwrap();
    assert!(report.pass);
    fs::write(
        "tests/regressions/issue_local_shared_command_zone_casting.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();

    let mut dying = object(1, 1, Zone::Battlefield);
    dying.controller = Some(PlayerId(0));
    dying.damage = 2;
    let mut source = base(4, recorded.cards.clone(), vec![dying]);
    for player in &mut source.initial_state.players {
        player.life = 40;
    }
    source
        .initial_state
        .commanders
        .insert(PlayerId(1), CardId(1));
    source.actions = [
        (PlayerId(1), Answer::Bool(false)),
        (PlayerId(0), Answer::Pass),
        (PlayerId(1), Answer::Pass),
    ]
    .into_iter()
    .map(|(who, answer)| ScenarioAction {
        who,
        answer,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
    })
    .collect();
    source.metadata.description = "Synthetic four-player Commander game: opponent-controlled commander dies, its owner declines command-zone return, priority proceeds without repeating the option".into();
    source.metadata.rules = vec!["903.9a".into(), "704.3".into()];
    write("commander_declines_return.json", &source);
    source
        .initial_state
        .objects
        .retain(|object| object.zone.zone == Zone::Battlefield);
    source.actions.truncate(1);
    source.metadata.issue = Some("local_forced_commander_return".into());
    source.metadata.description = "Confirmed rules bug: engine automatically returned graveyard/exile commanders and suppressed the owner's CR903.9a option. Opponent-controlled commander dies; owner declines and card must remain in owner's graveyard.".into();
    source.metadata.fixed_in = Some("working-tree optional commander-return SBA choices".into());
    let (report, recorded) = scenario::run(&source, true).unwrap();
    assert!(report.pass);
    fs::write(
        "tests/regressions/issue_local_forced_commander_return.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();
}
