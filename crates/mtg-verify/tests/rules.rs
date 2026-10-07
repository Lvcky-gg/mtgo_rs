mod support;

use mtg_core::{Cause, Event, Zone, ZoneRef};
use mtg_engine::Engine;
use support::*;

/// CR 704.5f, contrasted with destruction under CR 704.5g/702.12.
/// Independent expectation: indestructible cannot prevent zero-toughness death.
#[test]
fn cr_704_5f_zero_toughness_is_an_owner_graveyard_move_even_if_indestructible() {
    let cards = Cards::creature(0, true);
    let mut state = main_state();
    let original = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&original).unwrap().controller = P0;
    let mut engine = Engine::new(state);
    stabilize(&mut engine, &cards);
    assert!(!engine.state.objects.contains_key(&original));
    let graveyard = engine.state.objects_in(ZoneRef::of(Zone::Graveyard, P1));
    assert_eq!(graveyard.len(), 1);
    assert_ne!(
        graveyard[0], original,
        "zone changes create new object identities"
    );
    assert!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .is_empty()
    );
    let changes: Vec<_> = engine
        .log
        .iter()
        .filter(|entry| {
            matches!(entry.event,
        Event::ZoneChange { object, .. } if object == original)
        })
        .collect();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].cause, Cause::StateBasedAction);
    assert!(
        matches!(changes[0].event, Event::ZoneChange { from, to, .. }
        if from == ZoneRef::shared(Zone::Battlefield)
        && to == ZoneRef::of(Zone::Graveyard, P1))
    );
    assert!(!engine.log.iter().any(|entry| matches!(entry.event,
        Event::Regenerated { object } if object == original)));
}

#[test]
fn cr_704_5g_indestructible_prevents_lethal_damage_destruction() {
    let cards = Cards::creature(2, true);
    let mut state = main_state();
    let original = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&original).unwrap().damage = 2;
    let mut engine = Engine::new(state);
    stabilize(&mut engine, &cards);
    assert!(engine.state.objects.contains_key(&original));
    assert!(
        engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .is_empty()
    );
}

/// CR 117.3d and 800.4: priority belongs to participants still in the game.
/// Two consecutive departed seats must not create a skipped-player choice.
#[test]
fn cr_117_800_priority_rotation_excludes_consecutive_eliminated_players() {
    use mtg_core::PlayerId;
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
        state::GameState,
    };
    let seats = [P0, P1, PlayerId(2), PlayerId(3)];
    let mut state = GameState::new(&seats, 20);
    state.turn = 2;
    state.step = mtg_core::Step::PrecombatMain;
    state.active_player = P0;
    state.priority = Some(P0);
    for player in seats {
        for _ in 0..8 {
            state.place(CREATURE, player, ZoneRef::of(Zone::Library, player));
        }
    }
    state.players.get_mut(&P1).unwrap().life = 0;
    state.players.get_mut(&PlayerId(2)).unwrap().life = 0;
    let mut engine = Engine::new(state);
    let cards = Cards::creature(2, false);
    let mut priorities = vec![];
    let mut reached_next_surviving_turn = false;
    for _ in 0..1000 {
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("two survivors must continue playing"),
            Progress::NeedsChoice(choice) => {
                reached_next_surviving_turn |= engine.state.active_player == PlayerId(3);
                if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                    assert!(
                        !engine.state.players[&choice.who].has_lost,
                        "eliminated player received a priority choice"
                    );
                    assert_eq!(
                        engine.state.priority,
                        Some(choice.who),
                        "rules priority holder must agree with public decision holder"
                    );
                    priorities.push(choice.who);
                    if priorities.len() == 24 {
                        break;
                    }
                }
                let answer = if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                    Answer::Pass
                } else {
                    mtg_policy::well_formed(&choice, &engine.view_for(choice.who))
                };
                engine.answer(&cards, choice.id, answer).unwrap();
            }
        }
    }
    assert_eq!(priorities.len(), 24);
    assert!(
        reached_next_surviving_turn,
        "turn progression must skip both eliminated seats"
    );
    assert_eq!(&priorities[..2], &[P0, PlayerId(3)]);
    assert!(engine.state.players[&P1].has_lost);
    assert!(engine.state.players[&PlayerId(2)].has_lost);
}
