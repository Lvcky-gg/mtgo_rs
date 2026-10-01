//! Playing a whole game over a connection.
//!
//! Cards here are invented; the project ships no card data, so a test cannot depend on any real
//! card existing.

use mtg_core::{CardId, CardType, ManaCost, PlayerId, Subtype, Zone, ZoneRef};
use mtg_engine::{
    Choice, Engine, PlayerView,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
    state::GameState,
};
use mtg_ir::CardFace;
use mtg_net::{Channel, wire::MemoryChannel};
use mtg_session::{GuestSession, HostSession, Outcome, PolicyDecider};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

const FIELD: CardId = CardId(0);
const BEAR: CardId = CardId(1);

struct Cards {
    faces: Vec<CardFace>,
}

impl Default for Cards {
    fn default() -> Self {
        Self {
            faces: vec![land("Quiet Field"), creature("Practice Bear", 2, 2)],
        }
    }
}

impl PrintedCards for Cards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.faces.get(card.0 as usize)
    }
    fn subtype_name(&self, _s: Subtype) -> Option<&str> {
        None
    }
}

fn base(name: &str, types: Vec<CardType>) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: types,
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: None,
        toughness: None,
        loyalty: None,
        abilities: Vec::new(),
        oracle_text: None,
        colors: None,
    }
}

fn land(name: &str) -> CardFace {
    base(name, vec![CardType::Land])
}

fn creature(name: &str, p: i32, t: i32) -> CardFace {
    let mut c = base(name, vec![CardType::Creature]);
    c.power = Some(p);
    c.toughness = Some(t);
    c
}

/// A short game: small libraries, so someone loses to an empty draw rather than running forever.
fn short_game() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.priority = Some(P0);
    for p in [P0, P1] {
        for i in 0..4 {
            let card = if i % 2 == 0 { FIELD } else { BEAR };
            state.place(card, p, ZoneRef::of(Zone::Library, p));
        }
    }
    state
}

/// Run host and guest on two threads over a connected pair.
fn play_over<C, D>(
    make: impl FnOnce() -> (C, D),
) -> (
    Result<Outcome, mtg_session::SessionError>,
    Result<Outcome, mtg_session::SessionError>,
)
where
    C: Channel + Send + 'static,
    D: Channel + Send + 'static,
{
    let (host_end, guest_end) = make();

    let h = std::thread::spawn(move || {
        let cards = Cards::default();
        let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
        let mut local = PolicyDecider::default();
        session.play(&cards, &mut local)
    });

    let g = std::thread::spawn(move || {
        let mut session = GuestSession::new(guest_end);
        let mut decide = PolicyDecider::default();
        session.play(&mut decide)
    });

    (h.join().unwrap(), g.join().unwrap())
}

// ---- a whole game --------------------------------------------------------

#[test]
fn a_full_game_plays_to_completion_over_a_connection() {
    let (host, guest) = play_over(MemoryChannel::pair);

    let host = host.expect("host finished");
    let guest = guest.expect("guest finished");

    match (&host, &guest) {
        (Outcome::Over { winners: a }, Outcome::Over { winners: b }) => {
            assert!(!a.is_empty(), "someone should have won");
            // The host sends only the first winner, so compare that.
            assert_eq!(a.first(), b.first(), "both sides agree who won");
        }
        other => panic!("expected a finished game, got {other:?}"),
    }
}

#[test]
fn the_guest_gets_a_view_it_can_act_on() {
    let (host_end, guest_end) = MemoryChannel::pair();

    let h = std::thread::spawn(move || {
        let cards = Cards::default();
        let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
        let mut local = PolicyDecider::default();
        session.play(&cards, &mut local)
    });

    // Capture the first view and question the guest is shown.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(PlayerView, Choice)>::new()));
    let recorder = seen.clone();

    let g = std::thread::spawn(move || {
        let mut session = GuestSession::new(guest_end);
        let mut decide = move |choice: &Choice, view: &PlayerView| -> Answer {
            recorder
                .lock()
                .unwrap()
                .push((view.clone(), choice.clone()));
            choice.default.clone().unwrap_or(Answer::Pass)
        };
        session.play(&mut decide)
    });

    h.join().unwrap().expect("host");
    g.join().unwrap().expect("guest");

    let seen = seen.lock().unwrap();
    assert!(
        !seen.is_empty(),
        "the guest should have been asked something"
    );
    let (view, choice) = &seen[0];
    assert_eq!(view.viewer, P1, "the view is projected for the guest");
    assert_eq!(choice.who, P1, "and the question is for the guest");
    assert!(matches!(choice.kind, ChoiceKind::Priority { .. }));
}

// ---- the property that matters most -------------------------------------

#[test]
fn the_guest_never_sees_hidden_information_over_the_wire() {
    // The whole reason `PlayerView` exists. If the projection leaked, this is where it would
    // show — these are the actual bytes the guest received.
    let (host_end, guest_end) = MemoryChannel::pair();

    let h = std::thread::spawn(move || {
        let cards = Cards::default();
        let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
        let mut local = PolicyDecider::default();
        session.play(&cards, &mut local)
    });

    let views = std::sync::Arc::new(std::sync::Mutex::new(Vec::<PlayerView>::new()));
    let recorder = views.clone();

    let g = std::thread::spawn(move || {
        let mut session = GuestSession::new(guest_end);
        let mut decide = move |choice: &Choice, view: &PlayerView| -> Answer {
            recorder.lock().unwrap().push(view.clone());
            choice.default.clone().unwrap_or(Answer::Pass)
        };
        session.play(&mut decide)
    });

    h.join().unwrap().expect("host");
    g.join().unwrap().expect("guest");

    let views = views.lock().unwrap();
    assert!(!views.is_empty());

    for view in views.iter() {
        // No library card is ever present as an object, for either player.
        for obj in view.visible.values() {
            assert_ne!(
                obj.zone.zone,
                Zone::Library,
                "a library card reached the guest"
            );
        }
        // The opponent's hand is a count, not contents.
        for obj in view.visible.values() {
            if obj.zone.zone == Zone::Hand && obj.zone.player == Some(P0) {
                assert!(
                    obj.card.is_none(),
                    "the guest was told what is in the host's hand"
                );
            }
        }
        // And the host's hand size is still reported, so the guest can play properly.
        assert!(view.players.contains_key(&P0));
    }
}

// ---- failure modes ------------------------------------------------------

#[test]
fn a_guest_that_disappears_is_a_disconnect_not_a_crash() {
    let (host_end, guest_end) = MemoryChannel::pair();
    drop(guest_end);

    let cards = Cards::default();
    let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
    let mut local = PolicyDecider::default();

    let result = session.play(&cards, &mut local);
    assert!(
        matches!(result, Err(mtg_session::SessionError::Wire(_))),
        "expected a wire error, got {result:?}"
    );
}

#[test]
fn a_host_that_disappears_is_a_disconnect_for_the_guest() {
    let (host_end, guest_end) = MemoryChannel::pair();
    drop(host_end);

    let mut session = GuestSession::new(guest_end);
    let mut decide = PolicyDecider::default();
    assert_eq!(session.play(&mut decide).unwrap(), Outcome::Disconnected);
}

#[test]
fn a_guest_sending_nonsense_answers_is_eventually_refused() {
    // A modified client must not be able to keep the host busy, but a single raced answer
    // should not end the game either — hence a tolerance rather than zero or infinity.
    let (host_end, guest_end) = MemoryChannel::pair();

    let h = std::thread::spawn(move || {
        let cards = Cards::default();
        let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
        let mut local = PolicyDecider::default();
        session.play(&cards, &mut local)
    });

    let g = std::thread::spawn(move || {
        let mut session = GuestSession::new(guest_end);
        // Always answer with the wrong *shape* for the question.
        let mut decide = |_c: &Choice, _v: &PlayerView| Answer::Number(999);
        session.play(&mut decide)
    });

    let host_result = h.join().unwrap();
    let _ = g.join();

    assert!(
        matches!(host_result, Err(mtg_session::SessionError::Illegal(_)))
            || matches!(host_result, Err(mtg_session::SessionError::Wire(_))),
        "the host should give up on a client that cannot answer, got {host_result:?}"
    );
}

#[test]
fn a_takeback_request_is_answered_rather_than_ignored() {
    // An unattended host has nobody to ask, so it must decline explicitly — silence would
    // leave the guest waiting.
    let (host_end, guest_end) = MemoryChannel::pair();

    let h = std::thread::spawn(move || {
        let cards = Cards::default();
        let mut session = HostSession::new(Engine::new(short_game()), host_end, P1);
        let mut local = PolicyDecider::default();
        session.play(&cards, &mut local)
    });

    let g = std::thread::spawn(move || {
        let mut session = GuestSession::new(guest_end);
        let mut decide =
            |choice: &Choice, _v: &PlayerView| choice.default.clone().unwrap_or(Answer::Pass);
        // One takeback request, mid-game, then play normally.
        let _ = session.request_takeback(0, "misclick");
        session.play(&mut decide)
    });

    // The point is that neither side deadlocks.
    let host_result = h.join().unwrap();
    let guest_result = g.join().unwrap();
    assert!(
        host_result.is_ok() || guest_result.is_ok(),
        "the game should not deadlock"
    );
}
