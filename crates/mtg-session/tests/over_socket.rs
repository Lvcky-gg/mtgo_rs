//! A whole game over a real, encrypted socket.
//!
//! Everything else is tested over an in-memory pair, which is right for speed — but the claim
//! "two people can play over a connection" has to be checked against an actual connection at
//! least once. This is the full arrangement: WebSocket, Noise, the invite handshake, then a game
//! to its end.

#![cfg(feature = "transport")]

use mtg_core::{CardId, CardType, ManaCost, PlayerId, Subtype, Zone, ZoneRef};
use mtg_engine::{Engine, layers::PrintedCards, state::GameState};
use mtg_ir::CardFace;
use mtg_net::{
    Identity,
    fairness::{Commitment, Seed, shuffle_order},
    invite::{Endpoint, Invite},
    noise::NoiseChannel,
    session::{Guest, Host, SessionId},
    wire::{guest_handshake, host_handshake},
    ws::{WsChannel, bind_ephemeral},
};
use mtg_session::{GuestSession, HostSession, Outcome, PolicyDecider};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const NOW: u64 = 1_700_000_000;

struct Cards {
    faces: Vec<CardFace>,
}

impl Default for Cards {
    fn default() -> Self {
        Self {
            faces: vec![face("Quiet Field", vec![CardType::Land], None)],
        }
    }
}

impl PrintedCards for Cards {
    fn face(&self, card: CardId, _f: u8) -> Option<&CardFace> {
        self.faces.get(card.0 as usize)
    }
    fn subtype_name(&self, _s: Subtype) -> Option<&str> {
        None
    }
}

fn face(name: &str, types: Vec<CardType>, pt: Option<(i32, i32)>) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: types,
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: pt.map(|(p, _)| p),
        toughness: pt.map(|(_, t)| t),
        loyalty: None,
        abilities: Vec::new(),
        oracle_text: None,
        colors: None,
    }
}

fn short_game() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 1;
    state.active_player = P0;
    state.priority = Some(P0);
    for p in [P0, P1] {
        for _ in 0..3 {
            state.place(CardId(0), p, ZoneRef::of(Zone::Library, p));
        }
    }
    state
}

struct Party {
    identity: Identity,
    seed: Seed,
    salt: [u8; 16],
    decklist: Vec<(u32, u8)>,
}

impl Party {
    fn new(salt: u8) -> Self {
        Self {
            identity: Identity::generate(),
            seed: Seed::random(),
            salt: [salt; 16],
            decklist: vec![(0, 3)],
        }
    }
    fn commitment(&self) -> Commitment {
        Commitment::of(&self.seed, &self.salt, &self.decklist)
    }
}

#[test]
fn two_peers_play_a_whole_game_over_an_encrypted_socket() {
    let host_party = Party::new(1);
    let guest_party = Party::new(2);

    // The host binds a port and issues an invite naming it.
    let (listener, port) = bind_ephemeral().expect("bind");
    let session = SessionId::random();
    let invite = Invite::issue(
        &host_party.identity,
        session,
        vec![Endpoint::Direct {
            host: "127.0.0.1".into(),
            port,
        }],
        NOW + 600,
    );

    let link = invite.to_url();
    let nonce = invite.nonce;
    let (host_seed, host_commitment) = (host_party.seed, host_party.commitment());
    let host_identity = host_party.identity;

    let h = std::thread::spawn(move || {
        // Accept, encrypt, shake hands, then play.
        let plain = WsChannel::accept(&listener).expect("accept");
        let mut secure = NoiseChannel::respond(plain, &host_identity).expect("encrypted");
        let authenticated = secure.peer();

        let mut handshake = Host::new(session, nonce, host_seed, host_commitment);
        let shuffle = host_handshake(&mut secure, &mut handshake, NOW).expect("handshake");

        let cards = Cards::default();
        let mut game = HostSession::new(Engine::new(short_game()), secure, P1);
        let mut local = PolicyDecider::default();
        let outcome = game.play(&cards, &mut local).expect("game");

        (outcome, shuffle, authenticated)
    });

    let g = std::thread::spawn(move || {
        // Everything the guest knows comes from the pasted link.
        let parsed = Invite::from_url(&link).expect("link parses");
        parsed.verify(NOW).expect("link valid");
        let (addr, port) = parsed
            .endpoints
            .iter()
            .find_map(|e| match e {
                Endpoint::Direct { host, port } => Some((host.to_string(), *port)),
                _ => None,
            })
            .expect("endpoint");

        let plain = WsChannel::connect(&addr, port).expect("connect");
        let mut secure = NoiseChannel::initiate(plain, &guest_party.identity, parsed.host_key)
            .expect("encrypted");

        let mut handshake = Guest::new(guest_party.seed, guest_party.commitment());
        let shuffle = guest_handshake(
            &mut secure,
            &mut handshake,
            &guest_party.identity,
            &parsed,
            NOW,
        )
        .expect("handshake");

        let mut game = GuestSession::new(secure);
        let mut decide = PolicyDecider::default();
        let outcome = game.play(&mut decide).expect("game");

        (outcome, shuffle, guest_party.identity.public())
    });

    let (host_outcome, host_shuffle, learned_guest) = h.join().unwrap();
    let (guest_outcome, guest_shuffle, guest_key) = g.join().unwrap();

    // The shuffle was agreed and neither side chose it.
    assert_eq!(host_shuffle, guest_shuffle, "agreed shuffle seed");
    assert_eq!(
        shuffle_order(&host_shuffle, 60),
        shuffle_order(&guest_shuffle, 60),
        "and therefore the same deck order"
    );

    // Each side knows who it played.
    assert_eq!(learned_guest, guest_key);

    // And the game actually finished, with both sides agreeing.
    match (&host_outcome, &guest_outcome) {
        (Outcome::Over { winners: a }, Outcome::Over { winners: b }) => {
            assert!(!a.is_empty(), "someone won");
            assert_eq!(a.first(), b.first(), "both sides agree who won");
        }
        other => panic!("expected a finished game, got {other:?}"),
    }
}
