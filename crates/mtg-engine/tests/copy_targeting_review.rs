//! Independent copy review: CR112.2, 601.2c and 707.10c.
//! https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt
mod common;
use common::{creature, creature_target, targeted_instant};
use mtg_core::{CardId, ObjectId, PlayerId, Step, Target, Zone, ZoneRef};
use mtg_engine::{
    Action, Answer, Choice, ChoiceKind, Engine, Progress,
    state::{CastContext, GameState},
};
use mtg_ir::{CardFace, Effect, ObjectFilter, PrintedCards, Selector, selector::TargetSpec};
const A: PlayerId = PlayerId(0);
const B: PlayerId = PlayerId(1);
struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn next(e: &mut Engine, c: &Cards) -> Choice {
    for _ in 0..1000 {
        match e.advance(c) {
            Progress::Continue => (),
            Progress::NeedsChoice(q) => return q,
            Progress::GameOver { .. } => panic!("unexpected game over"),
        }
    }
    panic!("driver stalled")
}
struct Fixture {
    e: Engine,
    cards: Cards,
    q: Choice,
    original: ObjectId,
    creatures: Vec<ObjectId>,
}
fn fixture(specs: Vec<TargetSpec>, groups: Vec<usize>) -> Fixture {
    let stack_spec = TargetSpec {
        zone: Zone::Stack,
        filter: ObjectFilter::IsSpell,
        ..creature_target()
    };
    let cards = Cards(vec![
        creature("Review target", 4, 4, &[]),
        targeted_instant("Original", specs.clone(), Effect::Nothing),
        targeted_instant(
            "Copy",
            vec![stack_spec],
            Effect::CopySpell {
                what: Selector::Target { index: 0 },
                may_change_targets: true,
            },
        ),
    ]);
    let mut s = GameState::new(&[A, B], 20);
    s.turn = 2;
    s.step = Step::PrecombatMain;
    s.active_player = A;
    let creatures: Vec<_> = (0..3)
        .map(|_| s.place(CardId(0), B, ZoneRef::shared(Zone::Battlefield)))
        .collect();
    s.objects.get_mut(&creatures[2]).unwrap().owner = A;
    s.objects.get_mut(&creatures[2]).unwrap().controller = A;
    let original = s.place(CardId(1), B, ZoneRef::shared(Zone::Stack));
    let targets = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            if spec.zone == Zone::Stack {
                Target::Object(s.place(CardId(1), B, ZoneRef::shared(Zone::Stack)))
            } else {
                Target::Object(creatures[i])
            }
        })
        .collect();
    s.objects.get_mut(&original).unwrap().cast_context = Some(CastContext {
        targets,
        target_specs: Some(specs),
        target_groups: Some(groups),
        ..Default::default()
    });
    let copier = s.place(CardId(2), A, ZoneRef::of(Zone::Hand, A));
    let mut e = Engine::new(s);
    let q = next(&mut e, &cards);
    e.answer(
        &cards,
        q.id,
        Answer::Action(Action::Cast { object: copier }),
    )
    .unwrap();
    let q = next(&mut e, &cards);
    e.answer(
        &cards,
        q.id,
        Answer::Targets(vec![vec![Target::Object(original)]]),
    )
    .unwrap();
    let q = loop {
        let q = next(&mut e, &cards);
        if matches!(q.kind, ChoiceKind::ChooseTargets { .. }) {
            break q;
        }
        assert!(matches!(q.kind, ChoiceKind::Priority { .. }));
        e.answer(&cards, q.id, Answer::Pass).unwrap();
    };
    Fixture {
        e,
        cards,
        q,
        original,
        creatures,
    }
}
fn rejected(f: &mut Fixture, answer: Answer) {
    assert!(f.e.answer(&f.cards, f.q.id, answer).is_err());
    assert_eq!(next(&mut f.e, &f.cards).id, f.q.id);
}
#[test]
fn malicious_missing_extra_and_wrong_kind_answers_preserve_copy_prompt() {
    let mut f = fixture(vec![creature_target()], vec![0]);
    rejected(&mut f, Answer::Pass);
    rejected(&mut f, Answer::Targets(vec![]));
    let original = f.original;
    rejected(
        &mut f,
        Answer::Targets(vec![vec![Target::Object(original)]]),
    );
    let t = Target::Object(f.creatures[0]);
    rejected(&mut f, Answer::Targets(vec![vec![t, t]]));
    rejected(&mut f, Answer::Targets(vec![vec![t], vec![t]]));
}
#[test]
fn copy_owner_is_copier_and_original_owner_is_preserved() {
    let mut f = fixture(vec![creature_target()], vec![0]);
    f.e.answer(
        &f.cards,
        f.q.id,
        Answer::Targets(vec![vec![Target::Object(f.creatures[0])]]),
    )
    .unwrap();
    let _ = next(&mut f.e, &f.cards);
    let copy =
        f.e.state
            .objects
            .values()
            .find(|o| o.is_spell_copy)
            .expect("copy exists");
    assert_eq!(copy.owner, A);
    assert_eq!(copy.controller, A);
    assert_eq!(f.e.state.objects[&f.original].owner, B);
}
#[test]
fn same_group_and_explicit_distinct_slots_reject_duplicate_retargeting() {
    for explicit in [false, true] {
        let mut second = creature_target();
        second.distinct_from_other_targets = explicit;
        let mut f = fixture(
            vec![creature_target(), second],
            if explicit { vec![0, 1] } else { vec![0, 0] },
        );
        let t = Target::Object(f.creatures[2]);
        rejected(&mut f, Answer::Targets(vec![vec![t], vec![t]]));
    }
}
#[test]
fn separate_target_occurrences_allow_same_replacement() {
    let mut f = fixture(vec![creature_target(), creature_target()], vec![0, 1]);
    let t = Target::Object(f.creatures[2]);
    f.e.answer(&f.cards, f.q.id, Answer::Targets(vec![vec![t], vec![t]]))
        .unwrap();
    let _ = next(&mut f.e, &f.cards);
    let copy =
        f.e.state
            .objects
            .values()
            .find(|o| o.is_spell_copy)
            .unwrap();
    assert_eq!(copy.cast_context.as_ref().unwrap().targets, vec![t, t]);
}

#[test]
fn retained_target_can_be_illegal_for_new_controller_but_new_target_must_be_legal() {
    let own = TargetSpec {
        filter: ObjectFilter::And(vec![
            ObjectFilter::HasType(mtg_core::CardType::Creature),
            ObjectFilter::ControlledBy(Box::new(Selector::You)),
        ]),
        ..creature_target()
    };
    let mut f = fixture(vec![own], vec![0]);
    // The first two creatures belong to B: only the unchanged original target may be retained
    // by A, whereas another B creature would be an illegal new target.
    let ChoiceKind::ChooseTargets { slots, .. } = &f.q.kind else {
        panic!("retarget prompt")
    };
    assert!(slots[0].contains(&Target::Object(f.creatures[0])));
    assert!(!slots[0].contains(&Target::Object(f.creatures[1])));
    let illegal_new = Target::Object(f.creatures[1]);
    rejected(&mut f, Answer::Targets(vec![vec![illegal_new]]));
    let retained = Target::Object(f.creatures[0]);
    f.e.answer(&f.cards, f.q.id, Answer::Targets(vec![vec![retained]]))
        .unwrap();
    let _ = next(&mut f.e, &f.cards);
    let copy =
        f.e.state
            .objects
            .values()
            .find(|o| o.is_spell_copy)
            .unwrap();
    assert_eq!(copy.cast_context.as_ref().unwrap().targets, vec![retained]);
}

#[test]
fn copy_can_newly_target_original_spell_instead_of_inheriting_original_self_exclusion() {
    let spec = TargetSpec {
        zone: Zone::Stack,
        filter: ObjectFilter::IsSpell,
        ..creature_target()
    };
    let mut f = fixture(vec![spec], vec![0]);
    let ChoiceKind::ChooseTargets { slots, .. } = &f.q.kind else {
        panic!("retarget prompt")
    };
    assert!(
        slots[0].contains(&Target::Object(f.original)),
        "the new copy is distinct from its original; CR115.5 cannot exclude the original"
    );
    f.e.answer(
        &f.cards,
        f.q.id,
        Answer::Targets(vec![vec![Target::Object(f.original)]]),
    )
    .unwrap();
    let _ = next(&mut f.e, &f.cards);
    let copy =
        f.e.state
            .objects
            .values()
            .find(|o| o.is_spell_copy)
            .unwrap();
    assert_eq!(
        copy.cast_context.as_ref().unwrap().targets,
        vec![Target::Object(f.original)]
    );
}

#[test]
fn prospective_copy_owner_drives_owner_dependent_target_filter() {
    let spec = TargetSpec {
        filter: ObjectFilter::And(vec![
            ObjectFilter::HasType(mtg_core::CardType::Creature),
            ObjectFilter::OwnedBy(Box::new(Selector::OwnerOf(Box::new(Selector::SelfSource)))),
        ]),
        ..creature_target()
    };
    let mut f = fixture(vec![spec], vec![0]);
    let ChoiceKind::ChooseTargets { slots, .. } = &f.q.kind else {
        panic!("retarget prompt")
    };
    assert!(
        slots[0].contains(&Target::Object(f.creatures[0])),
        "unchanged target remains selectable"
    );
    assert!(
        slots[0].contains(&Target::Object(f.creatures[2])),
        "new copy is owned by A"
    );
    assert!(
        !slots[0].contains(&Target::Object(f.creatures[1])),
        "other B-owned object cannot be newly targeted"
    );
    assert!(
        !f.e.state.objects.values().any(|o| o.is_spell_copy),
        "preview must not publish a copy"
    );
    let target = Target::Object(f.creatures[2]);
    f.e.answer(&f.cards, f.q.id, Answer::Targets(vec![vec![target]]))
        .unwrap();
    let _ = next(&mut f.e, &f.cards);
    assert_eq!(
        f.e.state
            .objects
            .values()
            .filter(|o| o.is_spell_copy)
            .count(),
        1
    );
}

#[test]
fn rejected_retarget_attempts_do_not_change_result_or_allocate_extra_copies() {
    let mut clean = fixture(vec![creature_target()], vec![0]);
    let mut attacked = fixture(vec![creature_target()], vec![0]);
    let before = format!("{:?}", attacked.e.state);
    let log_before = format!("{:?}", attacked.e.log);
    for _ in 0..8 {
        rejected(&mut attacked, Answer::Targets(vec![]));
    }
    assert_eq!(format!("{:?}", attacked.e.state), before);
    assert_eq!(format!("{:?}", attacked.e.log), log_before);
    for f in [&mut clean, &mut attacked] {
        let target = Target::Object(f.creatures[2]);
        f.e.answer(&f.cards, f.q.id, Answer::Targets(vec![vec![target]]))
            .unwrap();
        let _ = next(&mut f.e, &f.cards);
    }
    assert_eq!(
        format!("{:?}", clean.e.state),
        format!("{:?}", attacked.e.state)
    );
    assert_eq!(
        format!("{:?}", clean.e.log),
        format!("{:?}", attacked.e.log)
    );
}
