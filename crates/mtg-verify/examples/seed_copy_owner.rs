//! Maintain the independently specified copy-ownership regression only.
use mtg_core::{CardId, CardType, ObjectId, PlayerId, Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};
use mtg_ir::{AbilityKind, Effect, ObjectFilter, Selector, Value, selector::TargetSpec};
use mtg_verify::scenario::{
    GameScenario, Metadata, ScenarioAction, ScenarioAssertions, ScenarioObject, ZoneAssertion, run,
};
use std::{collections::BTreeMap, fs, path::Path};

fn object(card: u32, owner: u8) -> ScenarioObject {
    let owner = PlayerId(owner);
    ScenarioObject {
        card: CardId(card),
        owner,
        controller: None,
        zone: ZoneRef::of(Zone::Hand, owner),
        tapped: false,
        phased_out: false,
        damage: 0,
        counters: BTreeMap::new(),
    }
}
fn main() {
    let mut fixture = GameScenario::load(Path::new("tests/replays/basic_casting.json")).unwrap();
    let mut copy = fixture.cards[1].clone();
    copy.name = "Independent owner regression copy spell".into();
    copy.abilities[0].kind = AbilityKind::SpellEffect(Effect::CopySpell {
        what: Selector::Target { index: 0 },
        may_change_targets: false,
    });
    copy.abilities[0].targets = vec![TargetSpec {
        zone: Zone::Stack,
        filter: ObjectFilter::HasType(CardType::Instant),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    }];
    fixture.cards.push(copy);
    fixture.card_db_hash = fixture.card_hash();
    fixture.initial_state.objects = vec![object(1, 0), object(2, 1)];
    fixture.metadata = Metadata { issue: Some("local_copied_spell_owner".into()),
        description: "Confirmed CR112.2/707.10 ownership bug: P1 copies P0's spell; the stack must contain one original owned by P0 and one copy owned by P1. Before the fix the copy incorrectly retained P0 ownership. Two hands, six engine answers; no seeded stack objects.".into(),
        rules: vec!["112.2".into(), "707.10".into()], first_affected: None,
        fixed_in: Some("working-tree spell-copy ownership fix".into()) };
    fixture.expected = ScenarioAssertions::default();
    fixture.expected.zones = [PlayerId(0), PlayerId(1)]
        .into_iter()
        .map(|owner| ZoneAssertion {
            zone: ZoneRef::shared(Zone::Stack),
            card: Some(CardId(1)),
            owner: Some(owner),
            counters: Default::default(),
            count: 1,
        })
        .collect();
    fixture.actions = [
        (
            PlayerId(0),
            Answer::Action(Action::Cast {
                object: ObjectId(1),
            }),
        ),
        (PlayerId(0), Answer::Pass),
        (
            PlayerId(1),
            Answer::Action(Action::Cast {
                object: ObjectId(2),
            }),
        ),
        (
            PlayerId(1),
            Answer::Targets(vec![vec![Target::Object(ObjectId(3))]]),
        ),
        (PlayerId(1), Answer::Pass),
        (PlayerId(0), Answer::Pass),
    ]
    .into_iter()
    .map(|(who, answer)| ScenarioAction {
        who,
        answer,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
        expected_rejection: false,
    })
    .collect();
    let (report, recorded) = run(&fixture, true).unwrap();
    assert!(report.pass, "{}", report.message);
    fs::write(
        "tests/regressions/issue_local_copied_spell_owner.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();
    // Independent CR 707.10 expected outcome: original gets three life from
    // green mana spent; its copy gets zero while retaining X=3.
    let mut mana = fixture.clone();
    mana.metadata.issue = Some("local_copied_mana_spent".into());
    mana.metadata.description = "CR707.10: a green-paid X=3 spell gains three life for its caster; the copy retains X but no mana was spent to cast it, so its controller gains zero.".into();
    mana.metadata.rules = vec!["707.10".into()];
    mana.cards[1].mana_cost.symbols = vec![mtg_core::ManaSymbol::Variable];
    mana.cards[1].abilities[0].kind = AbilityKind::SpellEffect(Effect::GainLife {
        who: Selector::You,
        amount: Value::ManaSpentOfColor(mtg_core::Color::Green),
    });
    mana.card_db_hash = mana.card_hash();
    mana.initial_state.players[0].mana.amounts[4] = 3;
    mana.actions.clear();
    mana.expected = ScenarioAssertions::default();
    mana.expected.life = [(PlayerId(0), 23), (PlayerId(1), 20)].into();
    mana.expected.zones = vec![
        ZoneAssertion {
            zone: ZoneRef::shared(Zone::Stack),
            card: None,
            owner: None,
            counters: Default::default(),
            count: 0,
        },
        ZoneAssertion {
            zone: ZoneRef::of(Zone::Graveyard, PlayerId(0)),
            card: Some(CardId(1)),
            owner: Some(PlayerId(0)),
            counters: Default::default(),
            count: 1,
        },
        ZoneAssertion {
            zone: ZoneRef::of(Zone::Graveyard, PlayerId(1)),
            card: Some(CardId(2)),
            owner: Some(PlayerId(1)),
            counters: Default::default(),
            count: 1,
        },
    ];
    let (mut engine, cards) = mana.setup().unwrap();
    let mut original_cast = false;
    let mut copy_cast = false;
    for _ in 0..100 {
        let choice = mtg_verify::scenario::next_choice(&mut engine, &cards)
            .unwrap()
            .unwrap();
        if copy_cast
            && engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty()
        {
            break;
        }
        let answer = match &choice.kind {
            mtg_engine::choice::ChoiceKind::Priority { .. } if !original_cast => {
                original_cast = true;
                Answer::Action(Action::Cast {
                    object: ObjectId(1),
                })
            }
            mtg_engine::choice::ChoiceKind::Priority { .. }
                if choice.who == PlayerId(1) && !copy_cast =>
            {
                copy_cast = true;
                Answer::Action(Action::Cast {
                    object: ObjectId(2),
                })
            }
            mtg_engine::choice::ChoiceKind::Priority { .. } => Answer::Pass,
            mtg_engine::choice::ChoiceKind::ChooseX { .. } => Answer::Number(3),
            mtg_engine::choice::ChoiceKind::ChooseTargets { slots, .. } => {
                Answer::Targets(vec![vec![slots[0][0]]])
            }
            _ => choice.default.clone().expect("unsupported fixture choice"),
        };
        engine.answer(&cards, choice.id, answer.clone()).unwrap();
        mana.actions.push(ScenarioAction {
            who: choice.who,
            answer,
            expected_rejection: false,
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
    }
    let (report, recorded) = run(&mana, true).unwrap();
    assert!(report.pass, "{}", report.message);
    fs::write(
        "tests/regressions/issue_local_copied_mana_spent.json",
        serde_json::to_vec_pretty(&recorded).unwrap(),
    )
    .unwrap();
}
