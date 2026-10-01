//! Two peers connecting over a real socket.
//!
//! Everything else in this crate is tested without a network, which is the right default —
//! but at some point the claim "two people can play over a connection" has to be checked
//! against an actual connection. These tests bind a real port on loopback.

#![cfg(feature = "transport")]

use std::net::TcpListener;

use mtg_net::{
    Identity,
    fairness::{Commitment, Seed, shuffle_order},
    invite::{Endpoint, Invite},
    session::{Guest, Host, SessionId},
    wire::{TypedChannel, WireError, guest_handshake, host_handshake},
    ws::{WsChannel, bind_ephemeral},
};

const NOW: u64 = 1_700_000_000;

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
            decklist: vec![(1, 4), (2, 20)],
        }
    }
    fn commitment(&self) -> Commitment {
        Commitment::of(&self.seed, &self.salt, &self.decklist)
    }
}

/// Host a game on an ephemeral port and return the invite plus the bound listener.
fn host_a_game(host: &Party) -> (TcpListener, Invite, SessionId) {
    let (listener, port) = bind_ephemeral().expect("bind");
    let session = SessionId::random();
    // The invite carries the real port, which is why binding and issuing are one step.
    let invite = Invite::issue(
        &host.identity,
        session,
        vec![Endpoint::Direct {
            host: "127.0.0.1".into(),
            port,
        }],
        NOW + 600,
    );
    (listener, invite, session)
}

fn direct_endpoint(invite: &Invite) -> (String, u16) {
    invite
        .endpoints
        .iter()
        .find_map(|e| match e {
            Endpoint::Direct { host, port } => Some((host.to_string(), *port)),
            _ => None,
        })
        .expect("a direct endpoint")
}

#[test]
fn two_peers_shake_hands_over_a_real_socket() {
    let host_party = Party::new(1);
    let guest_party = Party::new(2);
    let (listener, invite, session) = host_a_game(&host_party);

    let (host_seed, host_commitment) = (host_party.seed, host_party.commitment());
    let nonce = invite.nonce;

    let host_thread = std::thread::spawn(move || {
        let mut channel = WsChannel::accept(&listener)?;
        let mut host = Host::new(session, nonce, host_seed, host_commitment);
        let seed = host_handshake(&mut channel, &mut host, NOW)?;
        channel.close();
        Ok::<_, WireError>((seed, host.guest_key()))
    });

    let (host_addr, port) = direct_endpoint(&invite);
    let guest_thread = std::thread::spawn(move || {
        let mut channel = WsChannel::connect(&host_addr, port)?;
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        let seed = guest_handshake(
            &mut channel,
            &mut guest,
            &guest_party.identity,
            &invite,
            NOW,
        )?;
        Ok::<_, WireError>((seed, guest_party.identity.public()))
    });

    let (host_seed, learned_guest_key) = host_thread.join().unwrap().expect("host completed");
    let (guest_seed, guest_key) = guest_thread.join().unwrap().expect("guest completed");

    // The point of the whole exercise: a shared, unrigged shuffle over a real connection.
    assert_eq!(
        host_seed, guest_seed,
        "both peers must agree on the shuffle seed"
    );
    assert_eq!(
        shuffle_order(&host_seed, 60),
        shuffle_order(&guest_seed, 60),
        "and therefore on the deck order"
    );
    assert_eq!(
        learned_guest_key,
        Some(guest_key),
        "the host learned who joined"
    );
}

#[test]
fn the_shuffle_is_not_either_players_own_seed() {
    // Over a real connection, not just in the state machine: neither side controls it.
    let host_party = Party::new(3);
    let guest_party = Party::new(4);
    let (listener, invite, session) = host_a_game(&host_party);

    let (own_host_seed, commitment, nonce) =
        (host_party.seed, host_party.commitment(), invite.nonce);
    let own_guest_seed = guest_party.seed;

    let h = std::thread::spawn(move || {
        let mut channel = WsChannel::accept(&listener)?;
        let mut host = Host::new(session, nonce, own_host_seed, commitment);
        host_handshake(&mut channel, &mut host, NOW)
    });

    let (addr, port) = direct_endpoint(&invite);
    let g = std::thread::spawn(move || {
        let mut channel = WsChannel::connect(&addr, port)?;
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        guest_handshake(
            &mut channel,
            &mut guest,
            &guest_party.identity,
            &invite,
            NOW,
        )
    });

    let combined = h.join().unwrap().expect("host");
    g.join().unwrap().expect("guest");

    assert_ne!(combined, own_host_seed);
    assert_ne!(combined, own_guest_seed);
}

#[test]
fn a_guest_presenting_an_invite_for_another_session_is_refused() {
    // The host is running one session; the invite names another. Over a real socket the
    // guest should get a refusal, not a hang.
    let host_party = Party::new(5);
    let guest_party = Party::new(6);
    let (listener, invite, _session) = host_a_game(&host_party);

    let (seed, commitment, nonce) = (host_party.seed, host_party.commitment(), invite.nonce);
    let h = std::thread::spawn(move || {
        let mut channel = WsChannel::accept(&listener)?;
        // A different session id than the invite was issued for.
        let mut host = Host::new(SessionId([0xAB; 16]), nonce, seed, commitment);
        host_handshake(&mut channel, &mut host, NOW)
    });

    let (addr, port) = direct_endpoint(&invite);
    let g = std::thread::spawn(move || {
        let mut channel = WsChannel::connect(&addr, port)?;
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        guest_handshake(
            &mut channel,
            &mut guest,
            &guest_party.identity,
            &invite,
            NOW,
        )
    });

    assert!(h.join().unwrap().is_err(), "host should refuse");
    let guest_result = g.join().unwrap();
    match guest_result {
        Err(WireError::Refused(reason)) => {
            assert!(
                reason.contains("different game"),
                "should say why: {reason}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_invite_link_survives_being_pasted() {
    // The realistic flow: the host prints a link, the guest pastes it, and the port and key
    // come out of the link rather than being configured.
    let host_party = Party::new(7);
    let guest_party = Party::new(8);
    let (listener, invite, session) = host_a_game(&host_party);

    let link = invite.to_url();
    let (seed, commitment, nonce) = (host_party.seed, host_party.commitment(), invite.nonce);

    let h = std::thread::spawn(move || {
        let mut channel = WsChannel::accept(&listener)?;
        let mut host = Host::new(session, nonce, seed, commitment);
        host_handshake(&mut channel, &mut host, NOW)
    });

    let g = std::thread::spawn(move || {
        // Everything the guest knows comes from the pasted text.
        let parsed = Invite::from_url(&link).expect("link parses");
        parsed.verify(NOW).expect("link is valid");
        let (addr, port) = parsed
            .endpoints
            .iter()
            .find_map(|e| match e {
                Endpoint::Direct { host, port } => Some((host.to_string(), *port)),
                _ => None,
            })
            .expect("endpoint");

        let mut channel = WsChannel::connect(&addr, port)?;
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        guest_handshake(
            &mut channel,
            &mut guest,
            &guest_party.identity,
            &parsed,
            NOW,
        )
    });

    let host_seed = h.join().unwrap().expect("host");
    let guest_seed = g.join().unwrap().expect("guest");
    assert_eq!(host_seed, guest_seed);
}

#[test]
fn game_messages_travel_over_the_same_channel_after_the_handshake() {
    // The handshake is not the end: the connection then carries views, choices and answers.
    use mtg_net::session::{GuestMessage, HostMessage};

    let host_party = Party::new(9);
    let guest_party = Party::new(10);
    let (listener, invite, session) = host_a_game(&host_party);
    let (seed, commitment, nonce) = (host_party.seed, host_party.commitment(), invite.nonce);

    let h = std::thread::spawn(move || {
        let mut channel = WsChannel::accept(&listener)?;
        let mut host = Host::new(session, nonce, seed, commitment);
        host_handshake(&mut channel, &mut host, NOW)?;

        // Ask a question, expect an answer.
        channel.send_msg(&HostMessage::Ask {
            choice_id: 42,
            choice_bytes: vec![9, 9],
        })?;
        let reply: GuestMessage = channel.recv_msg()?;
        Ok::<_, WireError>(reply)
    });

    let (addr, port) = direct_endpoint(&invite);
    let g = std::thread::spawn(move || {
        let mut channel = WsChannel::connect(&addr, port)?;
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        guest_handshake(
            &mut channel,
            &mut guest,
            &guest_party.identity,
            &invite,
            NOW,
        )?;

        let ask: HostMessage = channel.recv_msg()?;
        let id = match ask {
            HostMessage::Ask { choice_id, .. } => choice_id,
            other => panic!("expected a question, got {other:?}"),
        };
        channel.send_msg(&GuestMessage::Answer {
            choice_id: id,
            answer_bytes: vec![1],
        })?;
        Ok::<_, WireError>(())
    });

    let reply = h.join().unwrap().expect("host");
    g.join().unwrap().expect("guest");

    match reply {
        GuestMessage::Answer {
            choice_id,
            answer_bytes,
        } => {
            assert_eq!(
                choice_id, 42,
                "the answer is for the question that was asked"
            );
            assert_eq!(answer_bytes, vec![1]);
        }
        other => panic!("expected an answer, got {other:?}"),
    }
}

#[test]
fn a_silent_peer_cannot_hold_the_noise_handshake_forever() {
    use mtg_net::noise::NoiseChannel;
    use std::time::Duration;

    let (listener, port) = bind_ephemeral().unwrap();
    let host = std::thread::spawn(move || {
        let channel = WsChannel::accept(&listener).unwrap();
        channel
            .set_io_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        match NoiseChannel::respond(channel, &Identity::generate()) {
            Err(WireError::Io(error)) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )),
            _ => panic!("a silent peer should time out during identity setup"),
        }
    });
    let _silent_peer = WsChannel::connect("127.0.0.1", port).unwrap();
    host.join().unwrap();
}

#[test]
fn setup_timeout_can_be_cleared_before_waiting_for_a_player() {
    use mtg_net::{noise::NoiseChannel, wire::Channel};
    use std::time::Duration;

    let identity = Identity::generate();
    let key = identity.public();
    let (listener, port) = bind_ephemeral().unwrap();
    let host = std::thread::spawn(move || {
        let channel = WsChannel::accept(&listener).unwrap();
        channel
            .set_io_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut secure = NoiseChannel::respond(channel, &identity).unwrap();
        secure
            .inner_mut()
            .set_io_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        secure.inner_mut().set_io_timeout(None).unwrap();
        secure.send(b"ready").unwrap();
        assert_eq!(secure.recv().unwrap(), b"player answered");
    });
    let channel = WsChannel::connect("127.0.0.1", port).unwrap();
    let mut secure = NoiseChannel::initiate(channel, &Identity::generate(), key).unwrap();
    assert_eq!(secure.recv().unwrap(), b"ready");
    std::thread::sleep(Duration::from_millis(100));
    secure.send(b"player answered").unwrap();
    host.join().unwrap();
}
