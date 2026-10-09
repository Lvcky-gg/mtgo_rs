//! Explicit creation of the multiplayer combat regression after behavioral assertions pass.
use mtg_core::{CardId, CardType, CounterKind, ManaCost, PlayerId, Step, Target, Zone, ZoneRef};
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
};
use mtg_ir::CardFace;
use mtg_verify::scenario::{
    self, GameScenario, Metadata, ScenarioAction, ScenarioAssertions, ScenarioObject,
    ScenarioPlayer, ScenarioState, ZoneAssertion,
};
use std::{collections::BTreeMap, path::Path};
fn face(name: &str, power: i32, toughness: i32) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Creature],
        subtypes: vec![],
        supertypes: vec![],
        power: Some(power),
        toughness: Some(toughness),
        loyalty: None,
        abilities: vec![],
        oracle_text: None,
        colors: None,
    }
}
fn main() {
    let output = std::env::args().nth(1).expect("pass a new output path");
    assert!(!Path::new(&output).exists(), "refuse to overwrite evidence");
    let mut walker = face("Synthetic pod walker", 0, 0);
    walker.card_types = vec![CardType::Planeswalker];
    walker.power = None;
    walker.toughness = None;
    walker.loyalty = Some(10);
    let mut artifact = GameScenario {
        format_version: scenario::FORMAT_VERSION, rules_version: scenario::RULES_VERSION.into(), engine_version: env!("CARGO_PKG_VERSION").into(), card_db_hash: String::new(), seed: 481, advance_budget: scenario::MAX_ADVANCE_BUDGET,
        metadata: Metadata { issue: Some("local-multiplayer-combat".into()), description: "Four-player combat: split attacks, per-defender blocking, foreign-block rejection and planeswalker damage. Direct life/counter assertions accompany behavioral golden checks.".into(), rules: vec!["802.2a".into(),"802.3".into(),"802.4a".into(),"802.4b".into(),"510".into()], first_affected: Some("0.1.0".into()), fixed_in: Some("2026-10-09 working tree".into()) },
        cards: vec![face("Synthetic pod soldier",2,2),face("Synthetic pod guard",1,3),face("Synthetic pod commander",2,2),walker], subtypes: vec![],
        initial_state: ScenarioState {
            players: (0..4).map(|i| ScenarioPlayer { id: PlayerId(i), life: 40, poison: 0, mana: Default::default() }).collect(), active_player: PlayerId(0), turn: 2, step: Step::BeginCombat,
            objects: [(0,0),(0,0),(2,0),(0,0),(1,1),(1,2),(3,2)].into_iter().map(|(card,owner)| ScenarioObject { card: CardId(card),owner: PlayerId(owner),controller: None,zone: ZoneRef::shared(Zone::Battlefield),tapped:false,phased_out:false,damage:0,counters: if card==3 {BTreeMap::from([(CounterKind::Loyalty,10)])}else{BTreeMap::new()} }).collect(),
            commanders: BTreeMap::from([(PlayerId(0),CardId(2))]),
        }, actions: vec![], expected: ScenarioAssertions { life: BTreeMap::from([(PlayerId(0),40),(PlayerId(1),40),(PlayerId(2),40),(PlayerId(3),38)]), zones: vec![ZoneAssertion { zone: ZoneRef::shared(Zone::Battlefield), card:Some(CardId(3)),owner:Some(PlayerId(2)),count:1,counters:BTreeMap::from([(CounterKind::Loyalty,8)]) }], ..Default::default() },
    };
    artifact.card_db_hash = artifact.card_hash();
    let (mut engine, cards) = artifact.setup().unwrap();
    let mut defenders = vec![];
    for _ in 0..100 {
        let choice = scenario::next_choice(&mut engine, &cards).unwrap().unwrap();
        if engine.state.step == Step::CombatDamage {
            break;
        }
        let answer = match &choice.kind {
            ChoiceKind::DeclareAttackers { .. } => Answer::Attackers(vec![
                (mtg_core::ObjectId(1), Target::Player(PlayerId(1))),
                (mtg_core::ObjectId(2), Target::Player(PlayerId(2))),
                (mtg_core::ObjectId(3), Target::Player(PlayerId(3))),
                (mtg_core::ObjectId(4), Target::Object(mtg_core::ObjectId(7))),
            ]),
            ChoiceKind::DeclareBlockers { .. } => {
                defenders.push(choice.who);
                let (blocker, attacker) = if choice.who == PlayerId(1) {
                    (5, 1)
                } else {
                    (6, 2)
                };
                Answer::Blocks(vec![(
                    mtg_core::ObjectId(blocker),
                    mtg_core::ObjectId(attacker),
                )])
            }
            _ => choice.default.clone().unwrap_or(Answer::Pass),
        };
        artifact.actions.push(ScenarioAction {
            who: choice.who,
            answer: answer.clone(),
            expected_rejection: false,
            expected_choice: Some(choice.kind),
            expected_state: None,
            expected_digest: None,
        });
        engine.answer(&cards, choice.id, answer).unwrap();
    }
    assert_eq!(defenders, [PlayerId(1), PlayerId(2)]);
    assert_eq!(engine.state.step, Step::CombatDamage);
    let (report, recorded) = scenario::run(&artifact, true).unwrap();
    assert!(report.pass, "{}", report.message);
    std::fs::write(output, serde_json::to_vec_pretty(&recorded).unwrap()).unwrap();
    assert!(matches!(engine.advance(&cards), Progress::NeedsChoice(_)));
}
