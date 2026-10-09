//! Independent CR702.16e / 615.12 / 702.15b propositions.
//! Primary rules read: https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf
//! Current September TXT was unavailable through the browser during this review.
mod support;
use mtg_core::{AbilityId, CardId, Color, Event, Keyword, Zone, ZoneRef};
use mtg_engine::{
    layers::PrintedCards,
    resolve::{self, ResolveCtx},
};
use mtg_ir::{Ability, AbilityKind, CardFace, Effect, ObjectFilter, Selector, Value};
use support::{P0, P1, main_state};
struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, id: CardId, _: u8) -> Option<&CardFace> {
        self.0.get(id.0 as usize)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn damage_case(color: Color, unpreventable: bool) -> (u32, i32, u32) {
    let mut source = support::Cards::creature(10, false).0;
    source.colors = Some(vec![color]);
    source.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::Keyword(Keyword::Lifelink),
        targets: vec![],
        source_text: None,
    }];
    let mut protected = support::Cards::creature(10, false).0;
    protected.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::Protection {
            from: ObjectFilter::HasColor(Color::Red),
        },
        targets: vec![],
        source_text: None,
    }];
    let cards = Cards(vec![source, protected]);
    let mut state = main_state();
    let source = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
    let recipient = state.place(CardId(1), P1, ZoneRef::shared(Zone::Battlefield));
    let mut rc = ResolveCtx::new(source, P0);
    let mut log = vec![];
    if unpreventable {
        resolve::resolve(
            &mut state,
            &cards,
            &mut log,
            &Effect::DamageCantBePrevented,
            &mut rc,
        )
        .unwrap();
    }
    // Untargeted damage: protection's targeting restriction must not mask its
    // independent damage-prevention behavior.
    let effect = Effect::DealDamage {
        source: Selector::SelfSource,
        to: Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::ControlledBy(Box::new(Selector::Opponents)),
        },
        amount: Value::Fixed(3),
    };
    resolve::resolve(&mut state, &cards, &mut log, &effect, &mut rc).unwrap();
    let recorded = log
        .iter()
        .filter_map(|e| match e.event {
            Event::DamageMarked { object, amount, .. } if object == recipient => Some(amount),
            _ => None,
        })
        .sum();
    (
        state.objects[&recipient].damage,
        state.player(P0).life,
        recorded,
    )
}
#[test]
fn red_untargeted_damage_is_prevented_and_lifelink_gains_nothing() {
    assert_eq!(damage_case(Color::Red, false), (0, 20, 0));
}
#[test]
fn unrelated_blue_damage_is_dealt_and_lifelink_tracks_actual_damage() {
    assert_eq!(damage_case(Color::Blue, false), (3, 23, 3));
}
#[test]
fn unpreventable_red_damage_bypasses_protection_and_gains_actual_lifelink() {
    assert_eq!(damage_case(Color::Red, true), (3, 23, 3));
}

fn prevention_side_effect_case(
    phantom: bool,
    finite: bool,
    unpreventable: bool,
) -> (u32, Vec<Event>) {
    use mtg_core::{CounterKind, DamageShield, Target};
    use mtg_ir::effect::{Modification, Restriction};
    let mut face = support::Cards::creature(10, false).0;
    if phantom {
        face.abilities = vec![Ability {
            id: AbilityId(0),
            targets: vec![],
            source_text: None,
            kind: AbilityKind::Static {
                what: Selector::SelfSource,
                modification: Modification::Restriction(Restriction::PreventDamageRemoveCounter),
                condition: None,
            },
        }];
    }
    let cards = Cards(vec![face]);
    let mut state = main_state();
    let source = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
    let recipient = state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield));
    let counter = if phantom {
        CounterKind::PlusOnePlusOne
    } else {
        CounterKind::Shield
    };
    if !finite {
        state
            .objects
            .get_mut(&recipient)
            .unwrap()
            .counters
            .insert(counter, 2);
    }
    if finite {
        state.damage_shields.push(DamageShield {
            id: 7,
            to: Some(Target::Object(recipient)),
            by: Some(source),
            combat_only: false,
            remaining: Some(5),
        });
    }
    state.damage_unpreventable = unpreventable;
    let mut events = vec![];
    assert_eq!(
        mtg_engine::prevention::prevent(
            &state,
            &cards,
            source,
            Target::Object(recipient),
            0,
            false,
            &mut events
        ),
        0
    );
    assert!(events.is_empty(), "zero damage must not spend counters");
    let amount = mtg_engine::prevention::prevent(
        &state,
        &cards,
        source,
        Target::Object(recipient),
        3,
        false,
        &mut events,
    );
    let second_amount = mtg_engine::prevention::prevent(
        &state,
        &cards,
        source,
        Target::Object(recipient),
        3,
        false,
        &mut events,
    );
    assert_eq!(second_amount, amount);
    if !finite {
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event,
            Event::CountersChanged { object, kind, delta: -1 }
                if *object == recipient && *kind == counter))
                .count(),
            1,
            "one simultaneous batch removes one counter even with multiple sources/shares"
        );
    }
    if !finite {
        assert!(events.iter().any(|e| matches!(e,
            Event::CountersChanged { object, kind, delta: -1 } if *object == recipient && *kind == counter)),
            "CR615.12 retains the applicable prevention effect's counter-removal side effect");
    }
    (amount, events)
}
#[test]
fn unpreventable_damage_still_removes_one_shield_counter() {
    assert_eq!(prevention_side_effect_case(false, false, true).0, 3);
}
#[test]
fn unpreventable_damage_still_removes_phantom_plus_one_counter() {
    assert_eq!(prevention_side_effect_case(true, false, true).0, 3);
}
#[test]
fn unpreventable_damage_does_not_consume_finite_next_damage_shield() {
    let (amount, events) = prevention_side_effect_case(false, true, true);
    assert_eq!(amount, 3);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::DamageShieldUsed { .. }))
    );
}

#[test]
fn ordinary_shield_and_phantom_damage_removes_counter_and_prevents_damage() {
    assert_eq!(prevention_side_effect_case(false, false, false).0, 0);
    assert_eq!(prevention_side_effect_case(true, false, false).0, 0);
}

#[test]
fn actual_engine_combat_respects_protection_and_unpreventable_damage() {
    use mtg_engine::{Answer, ChoiceKind, Engine, Progress};
    for unpreventable in [false, true] {
        let mut source = support::Cards::creature(10, false).0;
        source.power = Some(3);
        source.colors = Some(vec![Color::Red]);
        source.abilities = vec![Ability {
            id: AbilityId(0),
            kind: AbilityKind::Keyword(Keyword::Lifelink),
            targets: vec![],
            source_text: None,
        }];
        let mut protected = support::Cards::creature(10, false).0;
        protected.abilities = vec![Ability {
            id: AbilityId(0),
            kind: AbilityKind::Protection {
                from: ObjectFilter::HasColor(Color::Red),
            },
            targets: vec![],
            source_text: None,
        }];
        let cards = Cards(vec![source, protected]);
        let mut state = main_state();
        state.damage_unpreventable = unpreventable;
        let attacker = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
        state.objects.get_mut(&attacker).unwrap().summoning_sick = false;
        let blocker = state.place(CardId(1), P1, ZoneRef::shared(Zone::Battlefield));
        let mut engine = Engine::new(state);
        let mut reached_damage = false;
        for _ in 0..2000 {
            match engine.advance(&cards) {
                Progress::NeedsChoice(q) => {
                    if engine.state.step == mtg_core::Step::CombatDamage {
                        reached_damage = true;
                        break;
                    }
                    let answer = match q.kind {
                        ChoiceKind::DeclareAttackers { .. } => Answer::Objects(vec![attacker]),
                        ChoiceKind::DeclareBlockers { .. } => {
                            Answer::Blocks(vec![(blocker, attacker)])
                        }
                        _ => q.default.unwrap_or(Answer::Pass),
                    };
                    engine.answer(&cards, q.id, answer).unwrap();
                }
                Progress::Continue => (),
                Progress::GameOver { .. } => panic!("unexpected game over"),
            }
        }
        assert!(reached_damage);
        assert_eq!(
            engine.state.objects[&blocker].damage,
            if unpreventable { 3 } else { 0 }
        );
        assert_eq!(
            engine.state.player(P0).life,
            if unpreventable { 23 } else { 20 }
        );
    }
}

#[test]
fn unpreventable_damage_does_not_override_protection_targeting_restriction() {
    use mtg_core::Target;
    use mtg_ir::selector::TargetSpec;
    let mut source = support::Cards::creature(10, false).0;
    source.colors = Some(vec![Color::Red]);
    let mut protected = support::Cards::creature(10, false).0;
    protected.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::Protection {
            from: ObjectFilter::HasColor(Color::Red),
        },
        targets: vec![],
        source_text: None,
    }];
    let cards = Cards(vec![source, protected]);
    let mut state = main_state();
    let source = state.place(CardId(0), P0, ZoneRef::shared(Zone::Battlefield));
    let protected = state.place(CardId(1), P1, ZoneRef::shared(Zone::Battlefield));
    let spec = TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(mtg_core::CardType::Creature),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    };
    for unpreventable in [false, true] {
        state.damage_unpreventable = unpreventable;
        let legal = mtg_engine::targeting::legal_targets(&state, &cards, &spec, source, P0, &[]);
        assert!(!legal.contains(&Target::Object(protected)));
        assert!(legal.contains(&Target::Object(source)));
    }
}
