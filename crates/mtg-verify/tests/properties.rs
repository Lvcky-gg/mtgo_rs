mod support;

use mtg_core::{CardId, Zone, ZoneRef};
use mtg_engine::{Engine, view};
use proptest::prelude::*;
use support::*;

fn hidden_identity_leaked(
    state: &mtg_engine::state::GameState,
    projected: &view::PlayerView,
) -> bool {
    state
        .objects_in(ZoneRef::of(Zone::Hand, P1))
        .iter()
        .any(|id| {
            projected
                .visible
                .get(id)
                .is_some_and(|object| object.card.is_some())
        })
        || state
            .objects_in(ZoneRef::of(Zone::Library, P1))
            .iter()
            .any(|id| projected.visible.contains_key(id))
}

#[test]
fn hidden_information_checker_detects_one_injected_opponent_hand_identity() {
    let mut state = main_state();
    let secret = state.place(CardId(123), P1, ZoneRef::of(Zone::Hand, P1));
    let mut projected = view::project(&state, P0);
    assert!(!hidden_identity_leaked(&state, &projected));
    projected.visible.get_mut(&secret).unwrap().card = Some(CardId(123));
    assert!(hidden_identity_leaked(&state, &projected));
}

// CI can scale cases using PROPTEST_CASES. Fixed seed via PROPTEST_RNG_SEED
// reproduces campaigns; minimized counterexamples are persisted by proptest.
proptest! {
    #[test]
    fn stable_priority_never_retains_nonpositive_toughness(
        toughness in -32i32..=32,
        indestructible in any::<bool>(),
        opponent_owned in any::<bool>(),
    ) {
        let cards = Cards::creature(toughness, indestructible);
        let mut state = main_state();
        let owner = if opponent_owned { P1 } else { P0 };
        let id = state.place(CREATURE, owner, ZoneRef::shared(Zone::Battlefield));
        state.objects.get_mut(&id).unwrap().controller = P0;
        let mut engine = Engine::new(state);
        stabilize(&mut engine, &cards);
        // Mathematical reference independent of the production SBA evaluator.
        prop_assert_eq!(engine.state.objects.contains_key(&id), toughness > 0);
        prop_assert_eq!(engine.state.objects_in(ZoneRef::of(Zone::Graveyard, owner)).len(),
            usize::from(toughness <= 0));
        prop_assert_eq!(engine.state.objects.len(), 1, "non-token card conservation");
    }

    #[test]
    fn p0_hidden_hand_and_library_identity_noninterference(
        hand_a in prop::collection::vec(1u32..1000000, 0..20),
        hand_b in prop::collection::vec(1u32..1000000, 0..20),
        library in prop::collection::vec(1u32..1000000, 0..30),
    ) {
        let mut state = main_state();
        for card in &hand_a {
            state.place(CardId(*card), P0, ZoneRef::of(Zone::Hand, P0));
        }
        for card in &hand_b {
            state.place(CardId(*card), P1, ZoneRef::of(Zone::Hand, P1));
        }
        for card in &library {
            state.place(CardId(*card), P1, ZoneRef::of(Zone::Library, P1));
        }
        let projected = view::project(&state, P0);
        for id in state.objects_in(ZoneRef::of(Zone::Hand, P0)) {
            prop_assert_eq!(projected.visible[&id].card, Some(state.objects[&id].card));
        }
        prop_assert!(!hidden_identity_leaked(&state, &projected),
            "opponent hand identity or hidden library object id leaked");
        prop_assert_eq!(projected.players[&P1].hand_size as usize, hand_b.len());
        // Stronger than checking individual fields: changing every hidden identity
        // must leave the entire serialized view equal.
        let before = serde_json::to_value(&projected).unwrap();
        for object in state.objects.values_mut() {
            if object.owner == P1 {
                object.card = CardId(object.card.0 + 1000000);
            }
        }
        if let Some(order) = state.zone_order.get_mut(&ZoneRef::of(Zone::Library, P1)) {
            order.reverse();
        }
        let after = serde_json::to_value(view::project(&state, P0)).unwrap();
        prop_assert_eq!(before, after);
    }
}
