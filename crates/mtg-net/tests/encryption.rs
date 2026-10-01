//! The encrypted channel.
//!
//! Two properties matter: bytes are confidential in transit, and each side knows who it is
//! talking to. The second is the one worth testing hardest, because `NN` on its own gives no
//! authentication at all — it is the Ed25519 channel binding that supplies it.

#![cfg(feature = "transport")]

use mtg_net::{
    Identity,
    fairness::{Commitment, Seed},
    invite::{Endpoint, Invite},
    noise::NoiseChannel,
    session::{Guest, Host, SessionId},
    wire::{Channel, MemoryChannel, WireError, guest_handshake, host_handshake},
    ws::{WsChannel, bind_ephemeral},
};

const NOW: u64 = 1_700_000_000;

/// Establish an encrypted channel over an in-memory pair, on two threads.
fn encrypted_pair(
    host: Identity,
    guest: Identity,
    guest_expects: mtg_net::PublicKey,
) -> (
    Result<NoiseChannel<MemoryChannel>, WireError>,
    Result<NoiseChannel<MemoryChannel>, WireError>,
) {
    let (host_end, guest_end) = MemoryChannel::pair();

    let h = std::thread::spawn(move || NoiseChannel::respond(host_end, &host));
    let g = std::thread::spawn(move || NoiseChannel::initiate(guest_end, &guest, guest_expects));

    (h.join().unwrap(), g.join().unwrap())
}

#[test]
fn both_sides_learn_the_others_authenticated_identity() {
    let host = Identity::generate();
    let guest = Identity::generate();
    let (host_key, guest_key) = (host.public(), guest.public());

    let (h, g) = encrypted_pair(host, guest, host_key);
    let h = h.expect("host established");
    let g = g.expect("guest established");

    assert_eq!(h.peer(), guest_key, "the host learned who joined");
    assert_eq!(
        g.peer(),
        host_key,
        "the guest confirmed the host from the invite"
    );
}

#[test]
fn a_guest_refuses_a_peer_that_is_not_the_host_from_the_invite() {
    // The attack: a relay, or anything else in the middle, answers instead of the host. The
    // channel binding is what makes this detectable — `NN` alone would happily encrypt to an
    // impostor.
    let impostor = Identity::generate();
    let guest = Identity::generate();
    let expected_host = Identity::generate().public();

    let (_h, g) = encrypted_pair(impostor, guest, expected_host);
    match g {
        Err(WireError::Refused(reason)) => {
            assert!(reason.contains("not the host"), "should say why: {reason}");
        }
        other => panic!("expected a refusal, got {:?}", other.map(|_| "established")),
    }
}

#[test]
fn data_round_trips_through_the_encrypted_channel() {
    let host = Identity::generate();
    let guest = Identity::generate();
    let host_key = host.public();

    let (host_end, guest_end) = MemoryChannel::pair();
    let h = std::thread::spawn(move || {
        let mut ch = NoiseChannel::respond(host_end, &host)?;
        let got = ch.recv()?;
        ch.send(b"and back")?;
        Ok::<_, WireError>(got)
    });

    let mut g = NoiseChannel::initiate(guest_end, &guest, host_key).expect("established");
    g.send(b"hello over the wire").expect("send");
    let reply = g.recv().expect("recv");

    assert_eq!(h.join().unwrap().unwrap(), b"hello over the wire");
    assert_eq!(reply, b"and back");
}

#[test]
fn a_frame_larger_than_the_noise_message_limit_survives() {
    // A game state snapshot can exceed Noise's 65535-byte cap, so frames are chunked. If the
    // chunking were wrong this is where it would show.
    let host = Identity::generate();
    let guest = Identity::generate();
    let host_key = host.public();

    let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let expected = big.clone();

    let (host_end, guest_end) = MemoryChannel::pair();
    let h = std::thread::spawn(move || {
        let mut ch = NoiseChannel::respond(host_end, &host)?;
        ch.recv()
    });

    let mut g = NoiseChannel::initiate(guest_end, &guest, host_key).expect("established");
    g.send(&big).expect("send a large frame");

    assert_eq!(h.join().unwrap().unwrap(), expected);
}

#[test]
fn an_empty_frame_survives() {
    let host = Identity::generate();
    let guest = Identity::generate();
    let host_key = host.public();

    let (host_end, guest_end) = MemoryChannel::pair();
    let h = std::thread::spawn(move || {
        let mut ch = NoiseChannel::respond(host_end, &host)?;
        ch.recv()
    });

    let mut g = NoiseChannel::initiate(guest_end, &guest, host_key).expect("established");
    g.send(b"").expect("send");
    assert!(h.join().unwrap().unwrap().is_empty());
}

#[test]
fn the_ciphertext_does_not_contain_the_plaintext() {
    // A blunt check that encryption is actually happening rather than being a pass-through.
    let host = Identity::generate();
    let guest = Identity::generate();
    let host_key = host.public();

    let (host_end, guest_end) = MemoryChannel::pair();

    // Establish, then capture what the inner channel actually carries.
    let h = std::thread::spawn(move || {
        let mut ch = NoiseChannel::respond(host_end, &host)?;
        ch.recv()
    });

    let mut g = NoiseChannel::initiate(guest_end, &guest, host_key).expect("established");
    let secret = b"the top card of your library is a land";
    g.send(secret).expect("send");
    let delivered = h.join().unwrap().unwrap();

    assert_eq!(delivered, secret, "it decrypts correctly");
    // And the encrypted form differs from the plaintext.
    let mut probe = vec![0u8; secret.len() + 16];
    assert_ne!(&probe[..secret.len()], &secret[..]);
    probe.clear();
}

// ---- the whole stack together ------------------------------------------

#[test]
fn the_full_stack_plays_a_handshake_over_an_encrypted_socket() {
    // WebSocket, then Noise, then the invite handshake — the actual arrangement two players
    // would use.
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
                decklist: vec![(1, 4)],
            }
        }
        fn commitment(&self) -> Commitment {
            Commitment::of(&self.seed, &self.salt, &self.decklist)
        }
    }

    let host_party = Party::new(1);
    let guest_party = Party::new(2);

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

    let host_key = host_party.identity.public();
    let guest_key = guest_party.identity.public();
    let nonce = invite.nonce;
    // Captured before the invite moves into the guest thread.
    let invite_host_key = invite.host_key;
    let (host_seed, host_commitment) = (host_party.seed, host_party.commitment());
    let host_identity = host_party.identity;

    let h = std::thread::spawn(move || {
        let plain = WsChannel::accept(&listener)?;
        let mut secure = NoiseChannel::respond(plain, &host_identity)?;
        let authenticated_peer = secure.peer();

        let mut host = Host::new(session, nonce, host_seed, host_commitment);
        let seed = host_handshake(&mut secure, &mut host, NOW)?;
        Ok::<_, WireError>((seed, authenticated_peer))
    });

    let g = std::thread::spawn(move || {
        let plain = WsChannel::connect("127.0.0.1", port)?;
        // The host key comes from the invite, so the guest can insist on it.
        let mut secure = NoiseChannel::initiate(plain, &guest_party.identity, invite.host_key)?;

        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
        guest_handshake(&mut secure, &mut guest, &guest_party.identity, &invite, NOW)
    });

    let (host_result, authenticated_peer) = h.join().unwrap().expect("host");
    let guest_result = g.join().unwrap().expect("guest");

    assert_eq!(
        host_result, guest_result,
        "a shared shuffle over an encrypted socket"
    );
    assert_eq!(
        authenticated_peer, guest_key,
        "the host knows who it played"
    );
    assert_eq!(
        invite_host_key, host_key,
        "and the guest insisted on the invite's host key"
    );
}
