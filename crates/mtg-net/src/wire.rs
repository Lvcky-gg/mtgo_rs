//! Framing, channels, and driving the handshake over one.
//!
//! Three layers, separated so each is testable on its own:
//!
//! 1. [`encode`]/[`decode`] — turning a message into bytes. Pure.
//! 2. [`Channel`] — anything that can carry a frame. [`MemoryChannel`] implements it with
//!    no socket at all, so the protocol can be driven end to end in a test.
//! 3. [`host_handshake`]/[`guest_handshake`] — the loop that runs the state machine over a
//!    channel. Generic over the channel, so the same code path is exercised by the
//!    in-memory tests and by a real socket.
//!
//! The wire format is JSON. A binary format would be smaller, but the messages are small
//! and infrequent — a view diff a few times a second — and being able to read a capture
//! while debugging a desync is worth more than the bytes. This is the one place where being
//! able to see what happened matters more than efficiency.

use crate::{
    fairness::Seed,
    identity::{Identity, PublicKey},
    invite::Invite,
    session::{Guest, GuestMessage, Host, HostMessage, Phase},
};

#[derive(Debug)]
pub enum WireError {
    /// The peer closed the connection.
    Closed,
    Io(std::io::Error),
    /// A frame could not be decoded. Either a version mismatch or a hostile peer.
    Malformed(String),
    /// The handshake ended without agreement.
    Refused(String),
    /// The peer sent something valid but unexpected at this point.
    OutOfOrder,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Closed => write!(f, "the connection closed"),
            WireError::Io(e) => write!(f, "connection error: {e}"),
            WireError::Malformed(e) => write!(f, "could not read a message: {e}"),
            WireError::Refused(r) => write!(f, "refused: {r}"),
            WireError::OutOfOrder => write!(f, "unexpected message"),
        }
    }
}

impl std::error::Error for WireError {}

impl From<std::io::Error> for WireError {
    fn from(e: std::io::Error) -> Self {
        WireError::Io(e)
    }
}

/// Serialise a message.
pub fn encode<M: serde::Serialize>(msg: &M) -> Result<Vec<u8>, WireError> {
    serde_json::to_vec(msg).map_err(|e| WireError::Malformed(e.to_string()))
}

/// Deserialise a message.
pub fn decode<M: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<M, WireError> {
    serde_json::from_slice(bytes).map_err(|e| WireError::Malformed(e.to_string()))
}

/// Something that can carry frames between two peers.
///
/// Deliberately byte-oriented and blocking. A two-peer game has one connection and no
/// concurrency to exploit, so an async runtime would be a large dependency serving nothing.
pub trait Channel {
    fn send(&mut self, frame: &[u8]) -> Result<(), WireError>;
    /// Blocks until a frame arrives.
    fn recv(&mut self) -> Result<Vec<u8>, WireError>;
}

/// A borrowed channel is a channel, so one connection can carry a whole match — a lobby
/// exchange, then a game, then another — without being moved into each game's session.
impl<C: Channel + ?Sized> Channel for &mut C {
    fn send(&mut self, frame: &[u8]) -> Result<(), WireError> {
        (**self).send(frame)
    }
    fn recv(&mut self) -> Result<Vec<u8>, WireError> {
        (**self).recv()
    }
}

/// Typed helpers over a channel.
pub trait TypedChannel: Channel {
    fn send_msg<M: serde::Serialize>(&mut self, msg: &M) -> Result<(), WireError> {
        let bytes = encode(msg)?;
        self.send(&bytes)
    }

    fn recv_msg<M: serde::de::DeserializeOwned>(&mut self) -> Result<M, WireError> {
        let bytes = self.recv()?;
        decode(&bytes)
    }
}

impl<C: Channel + ?Sized> TypedChannel for C {}

// ---- an in-memory pair, for tests --------------------------------------

/// One end of a connected pair, backed by channels rather than a socket.
///
/// Exists so the protocol can be driven end to end without binding a port — which keeps the
/// protocol tests fast, deterministic, and runnable anywhere.
pub struct MemoryChannel {
    tx: std::sync::mpsc::Sender<Vec<u8>>,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
}

impl MemoryChannel {
    /// A connected pair.
    pub fn pair() -> (MemoryChannel, MemoryChannel) {
        let (a_tx, b_rx) = std::sync::mpsc::channel();
        let (b_tx, a_rx) = std::sync::mpsc::channel();
        (
            MemoryChannel { tx: a_tx, rx: a_rx },
            MemoryChannel { tx: b_tx, rx: b_rx },
        )
    }
}

impl Channel for MemoryChannel {
    fn send(&mut self, frame: &[u8]) -> Result<(), WireError> {
        self.tx.send(frame.to_vec()).map_err(|_| WireError::Closed)
    }

    fn recv(&mut self) -> Result<Vec<u8>, WireError> {
        self.rx.recv().map_err(|_| WireError::Closed)
    }
}

// ---- driving the handshake ---------------------------------------------

/// Run the host side of the handshake to completion.
///
/// Returns the combined shuffle seed. The loop is small because the state machine holds the
/// rules: this only moves bytes and stops when the machine says it is done.
pub fn host_handshake<C: Channel + ?Sized>(
    channel: &mut C,
    host: &mut Host,
    now_unix: u64,
) -> Result<Seed, WireError> {
    host_handshake_with_peer(channel, host, now_unix, None)
}

/// Run the lobby handshake after transport authentication. A Join must name the
/// identity proven by the encrypted channel, before it can consume the invite.
pub fn authenticated_host_handshake<C: Channel + ?Sized>(
    channel: &mut C,
    host: &mut Host,
    now_unix: u64,
    peer: PublicKey,
) -> Result<Seed, WireError> {
    host_handshake_with_peer(channel, host, now_unix, Some(peer))
}

fn host_handshake_with_peer<C: Channel + ?Sized>(
    channel: &mut C,
    host: &mut Host,
    now_unix: u64,
    peer: Option<PublicKey>,
) -> Result<Seed, WireError> {
    // Bounded rather than `loop`: a peer that keeps sending valid-but-useless messages
    // should not be able to spin the host forever.
    for _ in 0..8 {
        let msg: GuestMessage = channel.recv_msg()?;
        if let GuestMessage::Join { guest_key, .. } = &msg
            && peer.is_some_and(|peer| peer != *guest_key)
        {
            let reason = "the joining identity does not match the authenticated peer".to_owned();
            channel.send_msg(&HostMessage::Rejected {
                reason: reason.clone().into(),
            })?;
            return Err(WireError::Refused(reason));
        }
        for reply in host.on_guest(&msg, now_unix) {
            if let HostMessage::Rejected { reason } = &reply {
                let reason = reason.to_string();
                channel.send_msg(&reply)?;
                return Err(WireError::Refused(reason));
            }
            channel.send_msg(&reply)?;
        }
        if let Phase::Failed(_) = host.phase() {
            return Err(WireError::OutOfOrder);
        }
        if let Some(seed) = host.shuffle_seed() {
            return Ok(seed);
        }
    }
    Err(WireError::OutOfOrder)
}

/// Run the guest side of the handshake to completion.
pub fn guest_handshake<C: Channel + ?Sized>(
    channel: &mut C,
    guest: &mut Guest,
    identity: &Identity,
    invite: &Invite,
    now_unix: u64,
) -> Result<Seed, WireError> {
    // The invite is checked locally first, so a stale link fails before anything is sent.
    let join = guest
        .join(identity, invite, now_unix)
        .map_err(|e| WireError::Refused(format!("{e:?}")))?;
    channel.send_msg(&join)?;

    let mut accepted = false;
    for _ in 0..8 {
        let msg: HostMessage = channel.recv_msg()?;
        if let HostMessage::Rejected { reason } = &msg {
            return Err(WireError::Refused(reason.to_string()));
        }
        match &msg {
            HostMessage::Accepted { host_key, .. } if !accepted => {
                if *host_key != invite.host_key {
                    return Err(WireError::Refused(
                        "the accepting identity does not match the invited host".into(),
                    ));
                }
                accepted = true;
            }
            HostMessage::Reveal { .. } if accepted => {}
            _ => return Err(WireError::OutOfOrder),
        }
        for reply in guest.on_host(&msg) {
            channel.send_msg(&reply)?;
        }
        if let Some(seed) = guest.shuffle_seed() {
            return Ok(seed);
        }
    }
    Err(WireError::OutOfOrder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fairness::Commitment, invite::Endpoint, session::SessionId};

    const NOW: u64 = 1_700_000_000;

    fn scripted_host(
        messages: impl FnOnce(PublicKey, Commitment) -> Vec<HostMessage>,
    ) -> (Result<Seed, WireError>, Vec<GuestMessage>) {
        let host_identity = Identity::generate();
        let guest_identity = Identity::generate();
        let invite = Invite::issue(&host_identity, SessionId::random(), Vec::new(), NOW + 60);
        let seed = Seed::random();
        let commitment = Commitment::of(&seed, &[0; 16], &[]);
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        for message in messages(host_identity.public(), commitment) {
            host_end.send_msg(&message).unwrap();
        }
        let mut guest = Guest::new(seed, commitment);
        let result = guest_handshake(&mut guest_end, &mut guest, &guest_identity, &invite, NOW);
        drop(guest_end);
        let mut received = Vec::new();
        while let Ok(message) = host_end.recv_msg::<GuestMessage>() {
            received.push(message);
        }
        (result, received)
    }

    #[test]
    fn a_host_cannot_reveal_before_accepting_and_committing() {
        let (result, received) = scripted_host(|_, _| {
            vec![HostMessage::Reveal {
                seed: Seed::random(),
            }]
        });
        assert!(matches!(result, Err(WireError::OutOfOrder)));
        assert_eq!(received.len(), 1, "only Join, never the guest seed");
        assert!(matches!(received[0], GuestMessage::Join { .. }));
    }

    #[test]
    fn a_wrong_accepting_identity_is_refused_before_the_guest_reveals() {
        let (result, received) = scripted_host(|_, commitment| {
            vec![HostMessage::Accepted {
                host_key: Identity::generate().public(),
                commitment,
            }]
        });
        assert!(matches!(result, Err(WireError::Refused(reason))
            if reason.contains("does not match the invited host")));
        assert_eq!(received.len(), 1, "only Join, never the guest seed");
    }

    #[test]
    fn a_second_acceptance_cannot_replace_the_hosts_commitment() {
        let (result, received) = scripted_host(|host_key, commitment| {
            vec![
                HostMessage::Accepted {
                    host_key,
                    commitment,
                },
                HostMessage::Accepted {
                    host_key,
                    commitment,
                },
            ]
        });
        assert!(matches!(result, Err(WireError::OutOfOrder)));
        assert_eq!(received.len(), 2, "one Join and one Reveal");
        assert!(matches!(received[1], GuestMessage::Reveal { .. }));
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
                decklist: vec![(1, 4), (2, 20)],
            }
        }
        fn commitment(&self) -> Commitment {
            Commitment::of(&self.seed, &self.salt, &self.decklist)
        }
    }

    #[test]
    fn a_message_round_trips_through_the_codec() {
        let msg = HostMessage::Ask {
            choice_id: 7,
            choice_bytes: vec![1, 2, 3],
        };
        let bytes = encode(&msg).unwrap();
        let back: HostMessage = decode(&bytes).unwrap();
        assert!(matches!(back, HostMessage::Ask { choice_id: 7, .. }));
    }

    #[test]
    fn garbage_decodes_to_an_error_not_a_panic() {
        assert!(decode::<HostMessage>(b"not json").is_err());
        assert!(decode::<HostMessage>(b"{}").is_err());
        assert!(decode::<HostMessage>(&[]).is_err());
    }

    #[test]
    fn an_invite_survives_the_codec() {
        // The invite travels inside a Join message, so its hand-rolled signature bytes have
        // to survive JSON as well as base32.
        let host = Party::new(1);
        let invite = Invite::issue(&host.identity, SessionId([4u8; 16]), Vec::new(), NOW + 60);
        let msg = GuestMessage::Join {
            invite: invite.clone(),
            guest_key: host.identity.public(),
            commitment: host.commitment(),
        };
        let back: GuestMessage = decode(&encode(&msg).unwrap()).unwrap();
        match back {
            GuestMessage::Join { invite: got, .. } => {
                assert_eq!(got, invite);
                assert_eq!(got.verify(NOW), Ok(()));
            }
            _ => panic!("wrong variant"),
        }
    }

    /// Run a full handshake across a channel pair, on two threads.
    fn connect(
        host_party: Party,
        guest_party: Party,
        invite: Invite,
        session: SessionId,
    ) -> (Result<Seed, WireError>, Result<Seed, WireError>) {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let nonce = invite.nonce;

        let h = std::thread::spawn(move || {
            let mut host = Host::new(session, nonce, host_party.seed, host_party.commitment());
            host_handshake(&mut host_end, &mut host, NOW)
        });

        let g = std::thread::spawn(move || {
            let mut guest = Guest::new(guest_party.seed, guest_party.commitment());
            guest_handshake(
                &mut guest_end,
                &mut guest,
                &guest_party.identity,
                &invite,
                NOW,
            )
        });

        (h.join().unwrap(), g.join().unwrap())
    }

    #[test]
    fn two_peers_complete_the_handshake_over_a_channel() {
        let session = SessionId([5u8; 16]);
        let host = Party::new(1);
        let invite = Invite::issue(
            &host.identity,
            session,
            vec![Endpoint::Direct {
                host: "127.0.0.1".into(),
                port: 1,
            }],
            NOW + 600,
        );

        let (host_seed, guest_seed) = connect(host, Party::new(2), invite, session);
        let host_seed = host_seed.expect("host completed");
        let guest_seed = guest_seed.expect("guest completed");

        assert_eq!(
            host_seed, guest_seed,
            "both sides must derive the same shuffle"
        );
    }

    #[test]
    fn the_shuffle_derived_over_the_wire_matches_on_both_sides() {
        // The property that matters in practice: agreeing on a seed is only useful if it
        // produces the same deck order.
        let session = SessionId([6u8; 16]);
        let host = Party::new(3);
        let invite = Invite::issue(&host.identity, session, Vec::new(), NOW + 600);

        let (a, b) = connect(host, Party::new(4), invite, session);
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(
            crate::fairness::shuffle_order(&a, 60),
            crate::fairness::shuffle_order(&b, 60)
        );
    }

    #[test]
    fn an_expired_invite_is_refused_before_anything_is_sent() {
        let session = SessionId([7u8; 16]);
        let host = Party::new(5);
        let invite = Invite::issue(&host.identity, session, Vec::new(), NOW - 1);

        let guest_party = Party::new(6);
        let (mut _host_end, mut guest_end) = MemoryChannel::pair();
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());

        let result = guest_handshake(
            &mut guest_end,
            &mut guest,
            &guest_party.identity,
            &invite,
            NOW,
        );
        assert!(matches!(result, Err(WireError::Refused(_))));
    }

    #[test]
    fn a_second_guest_is_refused_with_a_reason() {
        // An invite admits one player. The second attempt should be told why, not hang.
        let session = SessionId([8u8; 16]);
        let host_party = Party::new(7);
        let invite = Invite::issue(&host_party.identity, session, Vec::new(), NOW + 600);

        let mut host = Host::new(
            session,
            invite.nonce,
            host_party.seed,
            host_party.commitment(),
        );

        // First guest, in process, to burn the invite.
        {
            let (mut host_end, mut guest_end) = MemoryChannel::pair();
            let g = Party::new(8);
            let invite2 = invite.clone();
            let t = std::thread::spawn(move || {
                let mut guest = Guest::new(g.seed, g.commitment());
                guest_handshake(&mut guest_end, &mut guest, &g.identity, &invite2, NOW)
            });
            host_handshake(&mut host_end, &mut host, NOW).expect("first guest joins");
            t.join().unwrap().expect("first guest completes");
        }

        // Second guest, same invite.
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let g2 = Party::new(9);
        let invite3 = invite.clone();
        let t = std::thread::spawn(move || {
            let mut guest = Guest::new(g2.seed, g2.commitment());
            guest_handshake(&mut guest_end, &mut guest, &g2.identity, &invite3, NOW)
        });

        let host_result = host_handshake(&mut host_end, &mut host, NOW);
        let guest_result = t.join().unwrap();

        assert!(
            matches!(host_result, Err(WireError::Refused(_))),
            "{host_result:?}"
        );
        match guest_result {
            Err(WireError::Refused(reason)) => {
                assert!(reason.contains("used"), "reason should explain: {reason}");
            }
            other => panic!("expected a refusal with a reason, got {other:?}"),
        }
    }

    #[test]
    fn a_closed_connection_is_an_error_not_a_hang() {
        let (host_end, mut guest_end) = MemoryChannel::pair();
        drop(host_end);
        let g = Party::new(10);
        let mut guest = Guest::new(g.seed, g.commitment());
        let invite = Invite::issue(&g.identity, SessionId([1u8; 16]), Vec::new(), NOW + 60);

        let result = guest_handshake(&mut guest_end, &mut guest, &g.identity, &invite, NOW);
        assert!(matches!(result, Err(WireError::Closed)), "{result:?}");
    }

    #[test]
    fn a_host_facing_a_silent_peer_does_not_spin_forever() {
        // The bound on the handshake loop: a peer sending nothing useful must not be able to
        // keep the host busy.
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let h = Party::new(11);
        let session = SessionId([2u8; 16]);
        let mut host = Host::new(session, [0u8; 16], h.seed, h.commitment());

        let t = std::thread::spawn(move || {
            // Valid frames, none of which advance the handshake.
            for _ in 0..20 {
                if guest_end.send_msg(&GuestMessage::Concede).is_err() {
                    break;
                }
            }
        });

        let result = host_handshake(&mut host_end, &mut host, NOW);
        assert!(result.is_err(), "should give up rather than loop");
        let _ = t.join();
    }
}
