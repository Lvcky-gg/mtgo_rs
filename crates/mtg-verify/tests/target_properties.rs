use mtg_core::{AbilityId, CardId, CardType, ManaCost, ObjectId, PlayerId, Target, Zone, ZoneRef};
use mtg_engine::{state::GameState, targeting};
use mtg_ir::{
    Ability, AbilityKind, CardFace, ObjectFilter, PrintedCards, Selector, Value, ability::Keyword,
    selector::TargetSpec,
};
use mtg_verify::{
    differential::compare,
    target_reference::{Candidate, Relation, TargetOracle, TargetProblem},
};
use proptest::prelude::*;

struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        if face == 0 {
            self.0.get(card.0 as usize)
        } else {
            None
        }
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn face(candidate: &Candidate) -> CardFace {
    let mut keywords = Vec::new();
    if candidate.shroud {
        keywords.push(Keyword::Shroud);
    }
    if candidate.hexproof {
        keywords.push(Keyword::Hexproof);
    }
    CardFace {
        name: "Scoped targeting fixture".into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![if candidate.creature {
            CardType::Creature
        } else {
            CardType::Artifact
        }],
        subtypes: vec![],
        supertypes: vec![],
        power: candidate.creature.then_some(2),
        toughness: candidate.creature.then_some(2),
        loyalty: None,
        oracle_text: None,
        colors: None,
        abilities: keywords
            .into_iter()
            .enumerate()
            .map(|(i, keyword)| Ability {
                id: AbilityId(i as u16),
                kind: AbilityKind::Keyword(keyword),
                targets: vec![],
                source_text: None,
            })
            .collect(),
    }
}
fn relation_filter(relation: Relation, owned: bool) -> Option<ObjectFilter> {
    let selector = match relation {
        Relation::Any => return None,
        Relation::You => Selector::You,
        Relation::Opponent => Selector::Opponents,
    };
    Some(if owned {
        ObjectFilter::OwnedBy(Box::new(selector))
    } else {
        ObjectFilter::ControlledBy(Box::new(selector))
    })
}
fn check(mut problem: TargetProblem) {
    let mut state = GameState::new(&[PlayerId(0), PlayerId(1)], 20);
    let mut cards = Cards(problem.candidates.iter().map(face).collect());
    for (i, candidate) in problem.candidates.iter_mut().enumerate() {
        let actual = state.place(
            CardId(i as u32),
            candidate.owner,
            if candidate.zone.is_shared() {
                ZoneRef::shared(candidate.zone)
            } else {
                ZoneRef::of(candidate.zone, candidate.owner)
            },
        );
        assert_eq!(actual, candidate.id);
        let object = state.objects.get_mut(&actual).unwrap();
        object.controller = candidate.controller;
        object.phased_out = candidate.phased_out;
    }
    let mut source_face = face(&candidate(0, problem.source_controller.0, false, false));
    source_face.card_types = vec![CardType::Instant];
    source_face.power = None;
    source_face.toughness = None;
    let source_card = CardId(cards.0.len() as u32);
    cards.0.push(source_face);
    let source = state.place(
        source_card,
        problem.source_controller,
        ZoneRef::shared(Zone::Stack),
    );
    let mut filters = vec![ObjectFilter::HasType(CardType::Creature)];
    filters.extend(relation_filter(problem.owner, true));
    filters.extend(relation_filter(problem.controller, false));
    let spec = TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::And(filters),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::Fixed(1),
        up_to: false,
        distinct_from_other_targets: problem.distinct,
    };
    let already: Vec<_> = problem
        .already
        .iter()
        .copied()
        .map(Target::Object)
        .collect();
    let mut actual: Vec<_> = targeting::legal_targets(
        &state,
        &cards,
        &spec,
        source,
        problem.source_controller,
        &already,
    )
    .into_iter()
    .filter_map(|target| {
        if let Target::Object(id) = target {
            Some(id)
        } else {
            None
        }
    })
    .collect();
    actual.sort();
    let comparison = compare(&TargetOracle, &problem, Ok(actual));
    assert!(
        comparison.is_ok(),
        "problem={problem:?}; disagreement={comparison:?}"
    );
}
fn candidate(id: u32, controller: u8, shroud: bool, hexproof: bool) -> Candidate {
    Candidate {
        id: ObjectId(id),
        zone: Zone::Battlefield,
        owner: PlayerId(0),
        controller: PlayerId(controller),
        creature: true,
        phased_out: false,
        shroud,
        hexproof,
    }
}
#[test]
fn shroud_blocks_both_sides_but_hexproof_allows_its_controller() {
    for source_controller in [PlayerId(0), PlayerId(1)] {
        check(TargetProblem {
            source_controller,
            owner: Relation::Any,
            controller: Relation::Any,
            distinct: false,
            already: vec![],
            candidates: vec![
                candidate(1, 0, true, false),
                candidate(2, 0, false, true),
                candidate(3, 1, false, true),
            ],
        });
    }
}
#[test]
fn phased_out_creatures_do_not_exist_for_target_selection() {
    let mut object = candidate(1, 0, false, false);
    object.phased_out = true;
    check(TargetProblem {
        source_controller: PlayerId(0),
        owner: Relation::Any,
        controller: Relation::Any,
        distinct: false,
        already: vec![],
        candidates: vec![object],
    });
}
fn relation(n: u8) -> Relation {
    match n {
        0 => Relation::Any,
        1 => Relation::You,
        _ => Relation::Opponent,
    }
}
proptest! {
    #[test]
    fn production_creature_targets_match_independent_scoped_decision_table(
        source in 0u8..2, owner_filter in 0u8..3, controller_filter in 0u8..3, distinct in any::<bool>(),
        objects in prop::collection::vec((0u8..2,0u8..2,0u8..3,any::<bool>(),any::<bool>(),any::<bool>(),any::<bool>(),any::<bool>()),0..12),
    ) {
        let mut already = vec![];
        let candidates = objects.into_iter().enumerate().map(|(i,(owner,controller,zone,creature,phased_out,shroud,hexproof,chosen))| {
            let id=ObjectId(i as u32+1); if chosen { already.push(id); }
            Candidate { id, owner: PlayerId(owner), controller: PlayerId(controller), zone: match zone { 0=>Zone::Battlefield,1=>Zone::Hand,_=>Zone::Graveyard }, creature, phased_out, shroud, hexproof }
        }).collect();
        check(TargetProblem { source_controller:PlayerId(source),owner:relation(owner_filter),controller:relation(controller_filter),distinct,already,candidates });
    }
}

#[test]
fn independent_model_rejects_unsupported_players_and_duplicate_objects() {
    use mtg_verify::differential::RulesOracle;
    let mut input = TargetProblem {
        source_controller: PlayerId(2),
        owner: Relation::Any,
        controller: Relation::Any,
        distinct: false,
        already: vec![],
        candidates: vec![],
    };
    assert!(TargetOracle.evaluate(&input).is_err());
    input.source_controller = PlayerId(0);
    input.candidates = vec![candidate(1, 0, false, false); 2];
    assert!(TargetOracle.evaluate(&input).is_err());
}

#[test]
fn distinct_slots_exclude_previous_choices_only_when_requested() {
    for distinct in [false, true] {
        check(TargetProblem {
            source_controller: PlayerId(0),
            owner: Relation::Any,
            controller: Relation::Any,
            distinct,
            already: vec![ObjectId(1)],
            candidates: vec![candidate(1, 0, false, false), candidate(2, 0, false, false)],
        });
    }
}

#[test]
fn ownership_and_control_filters_remain_independent() {
    let mut first = candidate(1, 1, false, false);
    first.owner = PlayerId(0);
    let mut second = candidate(2, 0, false, false);
    second.owner = PlayerId(1);
    for (owner, controller) in [
        (Relation::You, Relation::Any),
        (Relation::Any, Relation::You),
        (Relation::Opponent, Relation::You),
    ] {
        check(TargetProblem {
            source_controller: PlayerId(0),
            owner,
            controller,
            distinct: false,
            already: vec![],
            candidates: vec![first.clone(), second.clone()],
        });
    }
}

#[test]
fn canonical_phased_out_target_regression_preserves_human_asserted_priority() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/regressions/issue_local_phased_out_targeting.json");
    let scenario = mtg_verify::scenario::GameScenario::load(&fixture).unwrap();
    let (report, _) = mtg_verify::scenario::run(&scenario, false).unwrap();
    assert!(
        report.pass,
        "{} at {:?}",
        report.message, report.first_divergent_action
    );
}
