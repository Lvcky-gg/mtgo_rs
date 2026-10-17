//! Turning the real game into one the searcher is allowed to have seen.
//!
//! A search plays futures out on a copy of the [`Engine`], and the engine knows everything:
//! both libraries in order, the opponent's hand, the seed of every shuffle still to come. Searching
//! that copy as it is would be cheating — the bot would hold its counterspell because it "knows"
//! the top card of the opponent's library.
//!
//! So each copy is *determinized* first (the standard trick for imperfect-information search,
//! as in Information Set MCTS): every card the searcher cannot see is dealt again at random
//! from the pool of cards it might be, and the random generator is reseeded. Many copies,
//! each dealt differently, average over what the hidden cards could be instead of what they are.
//!
//! What the searcher may know is exactly what [`mtg_engine::view::project`] shows it. A card
//! whose identity the view withholds — in a hand or a library — is pooled with its owner's other
//! withheld cards and dealt back out. Pools are per owner and cover hand and library together,
//! so a dealt hand is a fair draw from what is left of that player's deck. Knowing the deck list
//! is the one assumption: the same thing a player knows after game one, or from the metagame.
//!
//! Face-down permanents keep their identity for now. Morphs are rare in the decks played here.

use mtg_core::{CardId, PlayerId, Zone, ZoneRef};
use mtg_engine::{Engine, state::Rng};

/// Deal every card `viewer` cannot see again, and reseed the game's randomness.
pub fn determinize(engine: &mut Engine, viewer: PlayerId, rng: &mut Rng) {
    let view = mtg_engine::view::project(&engine.state, viewer);
    let state = &mut engine.state;
    let players: Vec<PlayerId> = state.players.keys().copied().collect();
    for owner in players {
        let hidden: Vec<_> = [Zone::Hand, Zone::Library]
            .into_iter()
            .flat_map(|zone| {
                state.objects_in(ZoneRef {
                    zone,
                    player: Some(owner),
                })
            })
            .filter(|id| view.visible.get(id).is_none_or(|o| o.card.is_none()))
            .filter(|id| state.objects.get(id).is_some_and(|o| !o.is_token))
            .collect();
        if hidden.len() < 2 {
            continue;
        }
        let mut identities: Vec<CardId> = hidden.iter().map(|id| state.objects[id].card).collect();
        rng.shuffle(&mut identities);
        for (id, card) in hidden.iter().zip(identities) {
            if let Some(o) = state.objects.get_mut(id) {
                o.card = card;
                o.face = 0;
            }
        }
    }
    let mut seed = [0u8; 32];
    for chunk in seed.chunks_mut(8) {
        chunk.copy_from_slice(&rng.next_u64().to_le_bytes());
    }
    state.rng = Rng::from_seed(&seed);
    // Characteristics are memoised by generation; the hidden cards just changed identity.
    state.bump();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_searchers_own_hand_and_the_board_are_never_redealt() {
        let state = two_hands_and_libraries();
        let mut engine = Engine::new(state.clone());
        let me = PlayerId(0);
        let mine = ZoneRef {
            zone: Zone::Hand,
            player: Some(me),
        };
        let before: Vec<_> = engine
            .state
            .objects_in(mine)
            .iter()
            .map(|id| engine.state.objects[id].card)
            .collect();
        let mut rng = Rng::from_seed(&[7; 32]);
        determinize(&mut engine, me, &mut rng);
        let after: Vec<_> = engine
            .state
            .objects_in(mine)
            .iter()
            .map(|id| engine.state.objects[id].card)
            .collect();
        assert_eq!(before, after);
        // The opponent's hand and library hold the same cards, in some order.
        let them = PlayerId(1);
        let pool = |s: &mtg_engine::GameState| {
            let mut cards: Vec<CardId> = [Zone::Hand, Zone::Library]
                .into_iter()
                .flat_map(|zone| {
                    s.objects_in(ZoneRef {
                        zone,
                        player: Some(them),
                    })
                })
                .map(|id| s.objects[&id].card)
                .collect();
            cards.sort();
            cards
        };
        assert_eq!(pool(&state), pool(&engine.state));
    }

    /// Two players, each with a hand of three and a library of ten distinct cards.
    fn two_hands_and_libraries() -> mtg_engine::GameState {
        let mut state = mtg_engine::GameState::new(&[PlayerId(0), PlayerId(1)], 20);
        let mut next = 0u32;
        for p in [PlayerId(0), PlayerId(1)] {
            for (zone, n) in [(Zone::Hand, 3), (Zone::Library, 10)] {
                for _ in 0..n {
                    next += 1;
                    state.place(
                        CardId(next),
                        p,
                        ZoneRef {
                            zone,
                            player: Some(p),
                        },
                    );
                }
            }
        }
        state
    }
}
