//! A headless client: drives the engine with no UI.
//!
//! This is the reference driver. It consumes exactly what a GUI would — the legal
//! action list and the choice stream — so anything it can do, the real app can do,
//! and a scripted test exercises the same path a human takes. It is also where the
//! auto-answer policy is applied, which makes the difference between "asked" and
//! "handled" visible in the output rather than a claim in a design document.

pub mod cards;
pub mod trace;

pub use cards::DemoCards;
pub use trace::{Report, Settings, run};

use mtg_core::{PlayerId, Step, Zone, ZoneRef};
use mtg_engine::state::GameState;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

/// A two-player opening position.
///
/// Libraries are stocked in a fixed order rather than shuffled: shuffling needs a
/// seeded command (the engine owns no randomness), and the seed exchange belongs to
/// `mtg-net`'s commit-reveal. A fixed order is fine for a demo and makes runs
/// reproducible.
pub fn opening_game() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.step = Step::Untap;
    state.priority = Some(P0);

    for p in [P0, P1] {
        // A deck: lands and creatures alternating, so something happens every turn.
        for i in 0..24 {
            let card = match i % 5 {
                0 | 1 => cards::PLAINS,
                2 => cards::DUMMY,
                3 => cards::BANNER,
                _ => cards::SENTRY,
            };
            state.place(card, p, ZoneRef::of(Zone::Library, p));
        }
        // An opening hand, dealt off the top.
        let library = state.objects_in(ZoneRef::of(Zone::Library, p));
        for id in library.into_iter().take(7) {
            let card = state.objects[&id].card;
            state.objects.remove(&id);
            if let Some(order) = state.zone_order.get_mut(&ZoneRef::of(Zone::Library, p)) {
                order.retain(|o| *o != id);
            }
            state.place(card, p, ZoneRef::of(Zone::Hand, p));
        }
    }
    state
}
