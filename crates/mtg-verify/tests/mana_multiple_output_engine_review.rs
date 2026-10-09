//! Independent engine propositions specified before any replay checkpoints are recorded.
use mtg_core::{CardId, Color, ManaSymbol, ObjectId, PlayerId, Zone, ZoneRef};
use mtg_engine::{Action, Answer, ChoiceKind};
use mtg_ir::{AbilityKind, Effect, ManaOutput, Value};
use mtg_verify::scenario::{GameScenario, ScenarioAction, ZoneAssertion, next_choice, run};
use std::{collections::BTreeMap, path::PathBuf};
fn base() -> GameScenario {
    let mut s = GameScenario::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/regressions/issue_local_mana_colorless_alternative.json"),
    )
    .unwrap();
    for action in &mut s.actions {
        action.expected_choice = None;
        action.expected_state = None;
        action.expected_digest = None;
    }
    s.expected.final_state = None;
    s.expected.final_digest = None;
    s
}
fn record(s: &GameScenario, filename: &str) {
    let (report, artifact) = run(s, true).unwrap();
    assert!(report.pass, "{}", report.message);
    if std::env::var_os("MTGO_RECORD_MULTIMANA_FIXTURES").is_some() {
        use std::io::Write;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/regressions")
            .join(filename);
        std::fs::File::create_new(path)
            .unwrap()
            .write_all(&serde_json::to_vec_pretty(&artifact).unwrap())
            .unwrap();
    }
}
#[test]
fn engine_selects_colorless_instead_of_larger_green_ability() {
    let mut s = base();
    let AbilityKind::Activated {
        effect: Effect::AddMana { produces, .. },
        ..
    } = &mut s.cards[0].abilities[0].kind
    else {
        panic!("mana source")
    };
    *produces = vec![ManaOutput::Repeated {
        amount: Value::Fixed(2),
        output: Box::new(ManaOutput::Colored(Color::Green)),
    }];
    s.card_db_hash = s.card_hash();
    s.metadata.issue = Some("local_mana_larger_output_hides_colorless".into());
    s.metadata.description="A permanent with tap-for-GG and tap-for-C must offer the C-cost life-gain spell, select only its C ability, tap once, spend exactly C, and resolve for three life.".into();
    s.metadata.fixed_in = Some("working-tree whole-ability source alternatives".into());
    // Inherited independent expectations: life23, land battlefield, spell graveyard.
    assert_eq!(s.expected.life[&PlayerId(0)], 23);
    let (mut e, cards) = s.setup().unwrap();
    let q = next_choice(&mut e, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = &q.kind else {
        panic!("priority")
    };
    assert!(legal.actions.contains(&Action::Cast {
        object: ObjectId(2)
    }));
    for action in &s.actions {
        let q = next_choice(&mut e, &cards).unwrap().unwrap();
        assert_eq!(q.who, action.who);
        e.answer(&cards, q.id, action.answer.clone()).unwrap();
        let _ = next_choice(&mut e, &cards).unwrap();
    }
    assert_eq!(e.state.player(PlayerId(0)).life, 23);
    assert!(e.state.objects[&ObjectId(1)].tapped);
    assert_eq!(e.state.player(PlayerId(0)).mana.amounts, [0; 6]);
    let produced: Vec<_> = e
        .log
        .iter()
        .filter_map(|e| match e.event {
            mtg_core::Event::ManaAdded { color, amount, .. } => Some((color, amount)),
            _ => None,
        })
        .collect();
    assert_eq!(produced, vec![(None, 1)]);
    record(&s, "issue_local_mana_larger_output_hides_colorless.json");
}
#[test]
fn engine_does_not_offer_white_blue_cost_from_two_mana_of_one_color() {
    let mut s = base();
    s.cards[0].abilities.truncate(1);
    let AbilityKind::Activated {
        effect: Effect::AddMana { produces, .. },
        ..
    } = &mut s.cards[0].abilities[0].kind
    else {
        panic!("mana source")
    };
    *produces = vec![ManaOutput::Repeated {
        amount: Value::Fixed(2),
        output: Box::new(ManaOutput::AnyOf(vec![Color::White, Color::Blue])),
    }];
    s.cards[1].mana_cost.symbols = vec![
        ManaSymbol::Colored(Color::White),
        ManaSymbol::Colored(Color::Blue),
    ];
    s.card_db_hash = s.card_hash();
    s.metadata.issue = Some("local_mana_repeated_choice_cannot_split".into());
    s.metadata.description="A permanent adds two mana of one chosen color, white or blue. It cannot pay WU; the life-gain spell must remain unavailable, stay in hand, and gain no life.".into();
    s.metadata.rules = vec![
        "106.3".into(),
        "107.4a".into(),
        "107.5".into(),
        "601.2h".into(),
        "605.1a".into(),
    ];
    s.metadata.fixed_in = Some("working-tree correlated whole-activation color choices".into());
    s.actions = vec![ScenarioAction {
        who: PlayerId(0),
        answer: Answer::Pass,
        expected_rejection: false,
        expected_choice: None,
        expected_state: None,
        expected_digest: None,
    }];
    s.expected.life.insert(PlayerId(0), 20);
    s.expected.zones = vec![
        ZoneAssertion {
            zone: ZoneRef::of(Zone::Hand, PlayerId(0)),
            card: Some(CardId(1)),
            owner: Some(PlayerId(0)),
            count: 1,
            counters: BTreeMap::new(),
        },
        ZoneAssertion {
            zone: ZoneRef::shared(Zone::Battlefield),
            card: Some(CardId(0)),
            owner: Some(PlayerId(0)),
            count: 1,
            counters: BTreeMap::new(),
        },
    ];
    let (mut e, cards) = s.setup().unwrap();
    let q = next_choice(&mut e, &cards).unwrap().unwrap();
    let ChoiceKind::Priority { legal } = &q.kind else {
        panic!("priority")
    };
    assert!(!legal.actions.contains(&Action::Cast {
        object: ObjectId(2)
    }));
    assert!(!e.state.objects[&ObjectId(1)].tapped);
    assert_eq!(e.state.player(PlayerId(0)).mana.amounts, [0; 6]);
    // Freeze the independently reviewed legal-choice proposition, then record.
    s.actions[0].expected_choice = Some(q.kind);
    record(&s, "issue_local_mana_repeated_choice_cannot_split.json");
}
