//! CR 802.3–4 and 510: each attacker has a defender; blocks belong to that defender.
#[path = "../../mtg-engine/tests/common/mod.rs"]
mod common;
use mtg_core::{CardId, CardType, CounterKind, ObjectId, PlayerId, Step, Target, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    choice::{Answer, ChoiceKind},
    state::GameState,
};
use mtg_ir::{CardFace, PrintedCards};
struct Cards(Vec<CardFace>);
impl PrintedCards for Cards {
    fn face(&self, card: CardId, _: u8) -> Option<&CardFace> {
        self.0.get(card.0 as usize)
    }
    fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
        None
    }
}
fn ready(state: &mut GameState, card: u32, player: u8) -> ObjectId {
    let id = state.place(
        CardId(card),
        PlayerId(player),
        ZoneRef::shared(Zone::Battlefield),
    );
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}
#[test]
fn attacks_to_three_players_and_a_walker_have_separate_blocks_and_damage() {
    let mut walker = common::creature("Pod Walker", 0, 0, &[]);
    walker.card_types = vec![CardType::Planeswalker];
    walker.power = None;
    walker.toughness = None;
    walker.loyalty = Some(10);
    let cards = Cards(vec![
        common::creature("Pod Soldier", 2, 2, &[]),
        common::creature("Pod Guard", 1, 3, &[]),
        common::creature("Pod Commander", 2, 2, &[]),
        walker,
    ]);
    let mut state = GameState::new(&[PlayerId(0), PlayerId(1), PlayerId(2), PlayerId(3)], 40);
    state.turn = 2;
    state.step = Step::BeginCombat;
    state.priority = Some(PlayerId(0));
    let a = ready(&mut state, 0, 0);
    let b = ready(&mut state, 0, 0);
    let c = ready(&mut state, 2, 0);
    let d = ready(&mut state, 0, 0);
    let g1 = ready(&mut state, 1, 1);
    let g2 = ready(&mut state, 1, 2);
    let walker = ready(&mut state, 3, 2);
    state
        .objects
        .get_mut(&walker)
        .unwrap()
        .counters
        .insert(CounterKind::Loyalty, 10);
    state.commander.commanders.insert(PlayerId(0), CardId(2));
    let mut engine = Engine::new(state);
    let mut defenders_asked = vec![];
    let mut declared = false;
    for _ in 0..2000 {
        if engine.state.step == Step::PostcombatMain {
            break;
        }
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("combat must not end this game"),
            Progress::NeedsChoice(choice) => {
                let answer = match &choice.kind {
                    ChoiceKind::DeclareAttackers { defenders, .. } => {
                        assert_eq!(
                            defenders,
                            &[
                                Target::Player(PlayerId(1)),
                                Target::Player(PlayerId(2)),
                                Target::Player(PlayerId(3)),
                                Target::Object(walker)
                            ]
                        );
                        declared = true;
                        Answer::Attackers(vec![
                            (a, Target::Player(PlayerId(1))),
                            (b, Target::Player(PlayerId(2))),
                            (c, Target::Player(PlayerId(3))),
                            (d, Target::Object(walker)),
                        ])
                    }
                    ChoiceKind::DeclareBlockers { eligible, .. } => {
                        defenders_asked.push(choice.who);
                        if choice.who == PlayerId(1) {
                            assert_eq!(eligible, &vec![(g1, vec![a])]);
                            Answer::Blocks(vec![(g1, a)])
                        } else {
                            assert_eq!(choice.who, PlayerId(2));
                            assert_eq!(eligible, &vec![(g2, vec![b, d])]);
                            assert!(!engine.state.combat.was_blocked.contains(&b));
                            let before = engine.log.len();
                            assert!(
                                engine
                                    .answer(&cards, choice.id, Answer::Blocks(vec![(g2, a)]))
                                    .is_err(),
                                "cannot block a creature attacking another player"
                            );
                            assert_eq!(engine.log.len(), before);
                            assert!(
                                matches!(engine.advance(&cards), Progress::NeedsChoice(ref retry) if retry.id == choice.id)
                            );
                            Answer::Blocks(vec![(g2, b)])
                        }
                    }
                    ChoiceKind::Priority { .. } => {
                        if engine.state.step == Step::DeclareBlockers {
                            assert_eq!(defenders_asked, [PlayerId(1), PlayerId(2)]);
                        }
                        choice.default.clone().unwrap_or(Answer::Pass)
                    }
                    _ => choice.default.clone().unwrap_or(Answer::Pass),
                };
                engine.answer(&cards, choice.id, answer).unwrap();
            }
        }
    }
    assert!(declared);
    assert_eq!(engine.state.step, Step::PostcombatMain);
    assert_eq!(defenders_asked, [PlayerId(1), PlayerId(2)]);
    assert_eq!(engine.state.player(PlayerId(1)).life, 40);
    assert_eq!(engine.state.player(PlayerId(2)).life, 40);
    assert_eq!(engine.state.player(PlayerId(3)).life, 38);
    assert_eq!(
        engine.state.objects[&walker].counters[&CounterKind::Loyalty],
        8
    );
    assert_eq!(
        engine
            .state
            .commander
            .damage
            .get(&(PlayerId(3), PlayerId(0))),
        Some(&2)
    );
}
