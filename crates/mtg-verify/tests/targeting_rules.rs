mod support;

use mtg_core::{AbilityId, CardId, CardType, ObjectId, Target, Zone, ZoneRef};
use mtg_engine::{
    Engine,
    actions::Action,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
};
use mtg_ir::{
    Ability, AbilityKind, CardFace, Effect, ObjectFilter, Selector, Value, effect::ZonePosition,
    selector::TargetSpec,
};
use support::*;

struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _subtype: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn specification(optional: bool) -> TargetSpec {
    TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(CardType::Creature),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: optional,
        distinct_from_other_targets: false,
    }
}
fn instant(name: &str, targets: Vec<TargetSpec>, effect: Effect) -> CardFace {
    let mut face = support::Cards::creature(5, false).0;
    face.name = name.into();
    face.card_types = vec![CardType::Instant];
    face.power = None;
    face.toughness = None;
    face.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::SpellEffect(effect),
        targets,
        source_text: None,
    }];
    face
}
struct Table {
    engine: Engine,
    cards: Cards,
    creatures: [ObjectId; 2],
    artifact: ObjectId,
    spell: ObjectId,
    response: ObjectId,
}
impl Table {
    fn new(slots: usize, optional: bool) -> Self {
        let creature = support::Cards::creature(5, false).0;
        let mut printed_artifact = creature.clone();
        printed_artifact.card_types = vec![CardType::Artifact];
        printed_artifact.power = None;
        printed_artifact.toughness = None;
        let mut effects: Vec<_> = (0..slots)
            .map(|index| Effect::DealDamage {
                source: Selector::SelfSource,
                to: Selector::Target { index: index as u8 },
                amount: Value::Fixed(2),
            })
            .collect();
        effects.push(Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(3),
        });
        let printed_spell = instant(
            "Targeted damage then untargeted life",
            (0..slots).map(|_| specification(optional)).collect(),
            Effect::Sequence(effects),
        );
        let printed_response = instant(
            "Exile target creature",
            vec![specification(false)],
            Effect::MoveZone {
                what: Selector::Target { index: 0 },
                to: Zone::Exile,
                owner_relative_to: None,
                position: ZonePosition::Natural,
                tapped: false,
                face_down: false,
                under_control_of: None,
            },
        );
        let mut state = main_state();
        let creatures = [
            state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield)),
            state.place(CardId(0), P1, ZoneRef::shared(Zone::Battlefield)),
        ];
        let artifact = state.place(CardId(1), P1, ZoneRef::shared(Zone::Battlefield));
        let spell = state.place(CardId(2), P0, ZoneRef::of(Zone::Hand, P0));
        let response = state.place(CardId(3), P1, ZoneRef::of(Zone::Hand, P1));
        Self {
            engine: Engine::new(state),
            cards: Cards(vec![
                creature,
                printed_artifact,
                printed_spell,
                printed_response,
            ]),
            creatures,
            artifact,
            spell,
            response,
        }
    }
    fn next(&mut self) -> mtg_engine::Choice {
        mtg_verify::scenario::next_choice(&mut self.engine, &self.cards)
            .unwrap()
            .unwrap()
    }
    fn answer(&mut self, choice: &mtg_engine::Choice, answer: Answer) {
        self.engine
            .answer(&self.cards, choice.id, answer)
            .unwrap_or_else(|error| panic!("answer rejected for {:?}: {error:?}", choice.kind));
    }
    fn announce(&mut self, targets: Vec<Vec<Target>>) {
        let choice = self.next();
        assert_eq!(choice.who, P0);
        self.answer(&choice, Answer::Action(Action::Cast { object: self.spell }));
        for targets in targets {
            let choice = self.next();
            assert!(matches!(choice.kind, ChoiceKind::ChooseTargets { .. }));
            self.answer(&choice, Answer::Targets(vec![targets]));
        }
    }
    fn exile_in_response(&mut self, target: ObjectId) {
        let own = self.next();
        assert_eq!(own.who, P0);
        self.answer(&own, Answer::Pass);
        let opponent = self.next();
        assert_eq!(opponent.who, P1);
        self.answer(
            &opponent,
            Answer::Action(Action::Cast {
                object: self.response,
            }),
        );
        let targets = self.next();
        assert!(matches!(targets.kind, ChoiceKind::ChooseTargets { .. }));
        self.answer(
            &targets,
            Answer::Targets(vec![vec![Target::Object(target)]]),
        );
    }
    fn resolve_stack(&mut self) {
        for _ in 0..100 {
            let choice = self.next();
            if self
                .engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty()
            {
                return;
            }
            assert!(matches!(choice.kind, ChoiceKind::Priority { .. }));
            self.answer(&choice, Answer::Pass);
        }
        panic!("stack failed to finish");
    }
}

#[test]
fn cr_601_2c_illegal_announcement_target_is_rejected_without_losing_choice() {
    let mut table = Table::new(1, false);
    let choice = table.next();
    table.answer(
        &choice,
        Answer::Action(Action::Cast {
            object: table.spell,
        }),
    );
    let targets = table.next();
    assert!(
        matches!(&targets.kind, ChoiceKind::ChooseTargets { slots, .. }
        if !slots[0].contains(&Target::Object(table.artifact)))
    );
    assert!(
        table
            .engine
            .answer(
                &table.cards,
                targets.id,
                Answer::Targets(vec![vec![Target::Object(table.artifact)]])
            )
            .is_err()
    );
    let retry = table.next();
    assert_eq!(retry.id, targets.id);
    table.answer(
        &retry,
        Answer::Targets(vec![vec![Target::Object(table.creatures[0])]]),
    );
    table.resolve_stack();
    assert_eq!(table.engine.state.objects[&table.creatures[0]].damage, 2);
    assert_eq!(table.engine.state.objects[&table.artifact].damage, 0);
}

#[test]
fn cr_608_2b_all_chosen_targets_illegal_prevents_untargeted_life_gain() {
    let mut table = Table::new(1, false);
    table.announce(vec![vec![Target::Object(table.creatures[0])]]);
    table.exile_in_response(table.creatures[0]);
    table.resolve_stack();
    assert_eq!(table.engine.state.players[&P0].life, 20);
    assert_eq!(
        table
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1
    );
    assert_eq!(
        table
            .engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
}

#[test]
fn cr_608_2b_partial_illegality_affects_remaining_target_and_untargeted_part() {
    let mut table = Table::new(2, false);
    table.announce(
        table
            .creatures
            .iter()
            .map(|id| vec![Target::Object(*id)])
            .collect(),
    );
    table.exile_in_response(table.creatures[0]);
    table.resolve_stack();
    assert!(!table.engine.state.objects.contains_key(&table.creatures[0]));
    assert_eq!(table.engine.state.objects[&table.creatures[1]].damage, 2);
    assert_eq!(table.engine.state.players[&P0].life, 23);
}

#[test]
fn optional_zero_targets_still_resolves_untargeted_part() {
    let mut table = Table::new(1, true);
    table.announce(vec![vec![]]);
    table.resolve_stack();
    assert_eq!(table.engine.state.players[&P0].life, 23);
    assert!(
        table
            .creatures
            .iter()
            .all(|id| table.engine.state.objects[id].damage == 0)
    );
}

#[test]
fn cr_601_required_target_without_any_legal_candidate_cannot_be_cast() {
    let mut table = Table::new(1, false);
    for id in table.creatures {
        table.engine.state.objects.get_mut(&id).unwrap().zone = ZoneRef::of(Zone::Hand, P1);
    }
    let choice = table.next();
    assert!(matches!(&choice.kind, ChoiceKind::Priority { legal }
        if !legal.actions.iter().any(|a| matches!(a, Action::Cast { object } if *object == table.spell))));
    assert!(
        table
            .engine
            .answer(
                &table.cards,
                choice.id,
                Answer::Action(Action::Cast {
                    object: table.spell
                })
            )
            .is_err()
    );
    assert_eq!(
        table.engine.state.objects[&table.spell].zone,
        ZoneRef::of(Zone::Hand, P0)
    );
}

#[test]
fn malformed_target_answers_cannot_bypass_type_existence_or_required_cardinality() {
    for case in 0..5 {
        let mut table = Table::new(1, false);
        let choice = table.next();
        table.answer(
            &choice,
            Answer::Action(Action::Cast {
                object: table.spell,
            }),
        );
        let targets = table.next();
        let selected = match case {
            0 => vec![vec![Target::Player(P1)]],
            1 => vec![vec![Target::Object(ObjectId(999999))]],
            2 => vec![vec![]],
            3 => vec![
                table
                    .creatures
                    .iter()
                    .map(|id| Target::Object(*id))
                    .collect(),
            ],
            _ => table
                .creatures
                .iter()
                .map(|id| vec![Target::Object(*id)])
                .collect(),
        };
        assert!(
            table
                .engine
                .answer(&table.cards, targets.id, Answer::Targets(selected))
                .is_err(),
            "malformed target case {case} must be rejected"
        );
        let retry = table.next();
        assert_eq!(retry.id, targets.id);
        table.answer(
            &retry,
            Answer::Targets(vec![vec![Target::Object(table.creatures[0])]]),
        );
        table.resolve_stack();
        assert_eq!(table.engine.state.players[&P0].life, 23);
    }
}
