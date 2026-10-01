//! Session orchestration.
//!
//! # One authoritative peer
//!
//! Hidden information forces the choice. A symmetric lockstep simulation, as a
//! deterministic game would normally use, cannot work here: neither client can
//! simulate a turn without knowing cards it is not allowed to know. So one peer
//! runs the engine and the other sends intents and receives [views].
//!
//! The host is *not* privileged in the game — it is the same engine either peer
//! runs — and the asymmetry is bounded by [`crate::fairness`]. The alternative,
//! reproducing MTGO's always-on central server, would trade the whole
//! offline-by-design premise for a cheat guarantee that commit-reveal mostly
//! provides anyway.
//!
//! # The wire is small on purpose
//!
//! ```text
//!   guest                                             host
//!     │                                                 │
//!     │──── Join { invite, guest_key, commitment } ─────►│  verify signature,
//!     │                                                 │  burn nonce
//!     │◄─── Accepted { host_commitment } ───────────────│
//!     │──── Reveal { seed } ───────────────────────────►│  combine seeds, shuffle
//!     │◄─── View(PlayerView) + Choice ─────────────────│
//!     │──── Answer { choice_id, answer } ─────────────►│  engine.apply(..)
//!     │◄─── Events(since) + View + Choice ────────────│
//! ```
//!
//! Only three message kinds carry game data: a redacted view, a choice, and an
//! answer. The guest never receives events it is not entitled to, because events
//! are filtered through the same projection as state.
//!
//! # Reconnect
//!
//! Every message carries the event sequence number it reflects. A guest that drops
//! reconnects with the last id it saw and receives the gap. There is no separate
//! resynchronisation protocol, and there is no state to reconcile, because the
//! engine is a pure function of its log — which is the practical payoff of keeping
//! the engine free of I/O and ambient randomness.
//!
//! [views]: mtg_engine_view
//! [mtg_engine_view]: https://docs.rs/mtg-engine

use serde::{Deserialize, Serialize};

use crate::{
    Identity,
    fairness::{Commitment, Seed},
    identity::PublicKey,
    invite::{Invite, InviteError},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct SessionId(pub [u8; 16]);

impl SessionId {
    pub fn random() -> Self {
        let mut bytes = [0u8; 16];
        crate::fairness::fill_random(&mut bytes);
        SessionId(bytes)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostRole {
    /// Runs the engine and holds full state.
    Host,
    /// Sends intents, receives redacted views.
    Guest,
}

/// Why a join was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JoinError {
    /// The invite's signature is not this host's, or the token was altered.
    BadInvite(InviteError),
    /// The invite is for a session this host is not running.
    WrongSession,
    /// The invite has already been used. Each one admits exactly one player.
    AlreadyRedeemed,
    /// A message arrived out of order — a seed before the commitments were exchanged.
    OutOfOrder,
    /// Someone already joined this session.
    Occupied,
}

// ---- messages ----------------------------------------------------------

/// Messages a guest sends.
///
/// Note what is absent: there is no message by which a guest asserts a state change. A
/// guest can only answer a question the host asked, so a modified guest client cannot do
/// anything the rules do not already permit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GuestMessage {
    Join {
        invite: Invite,
        guest_key: PublicKey,
        /// Commitment to the guest's shuffle seed and decklist.
        commitment: Commitment,
    },
    /// Sent once both commitments have been exchanged.
    Reveal {
        seed: Seed,
    },
    /// An answer to a choice the host asked. Opaque here because the shapes belong to the
    /// engine, and the transport has no business knowing them.
    Answer {
        choice_id: u64,
        answer_bytes: Vec<u8>,
    },
    /// Resume after a drop, asking for everything after `last_seen`.
    Resume {
        session: SessionId,
        last_seen: u64,
    },
    RequestTakeback {
        to_event: u64,
        reason: Box<str>,
    },
    Concede,
    /// End-of-game disclosure, so the host's play can be verified.
    Disclose(crate::fairness::Disclosure),
}

/// Messages a host sends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HostMessage {
    Accepted {
        host_key: PublicKey,
        commitment: Commitment,
    },
    Rejected {
        reason: Box<str>,
    },
    Reveal {
        seed: Seed,
    },
    /// A redacted snapshot. Sent on connect and after a takeback; ordinary play sends
    /// `Events` instead, because a diff is smaller than a state.
    Snapshot {
        at_event: u64,
        view_bytes: Vec<u8>,
    },
    Events {
        from: u64,
        event_bytes: Vec<u8>,
    },
    Ask {
        choice_id: u64,
        choice_bytes: Vec<u8>,
    },
    TakebackDecision {
        to_event: u64,
        granted: bool,
    },
    GameOver {
        winner: Option<u8>,
    },
    Disclose(crate::fairness::Disclosure),
}

// ---- the handshake, as a state machine ---------------------------------

/// How far the handshake has got.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Waiting for someone to present an invite.
    AwaitingJoin,
    /// Commitments exchanged; waiting for the seed.
    AwaitingReveal,
    /// Both seeds known and combined. The shuffle can be derived.
    Ready,
    /// The handshake failed and this session is over.
    Failed(JoinError),
}

/// The host side of the handshake.
///
/// Pure: it takes a message and returns what to send. No sockets, no clock — `now_unix` is
/// passed in — so the whole protocol is testable without a network, which is the same split
/// the engine and the importer use.
pub struct Host {
    session: SessionId,
    /// The nonce of the invite this host issued. Burned on first successful join, which is
    /// what makes an invite single-use.
    nonce: [u8; 16],
    redeemed: bool,
    my_seed: Seed,
    my_commitment: Commitment,
    guest: Option<PublicKey>,
    guest_commitment: Option<Commitment>,
    shuffle: Option<Seed>,
    phase: Phase,
}

impl Host {
    pub fn new(
        session: SessionId,
        nonce: [u8; 16],
        my_seed: Seed,
        my_commitment: Commitment,
    ) -> Self {
        Self {
            session,
            nonce,
            redeemed: false,
            my_seed,
            my_commitment,
            guest: None,
            guest_commitment: None,
            shuffle: None,
            phase: Phase::AwaitingJoin,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The combined shuffle seed, once both sides have revealed.
    pub fn shuffle_seed(&self) -> Option<Seed> {
        self.shuffle
    }

    pub fn guest_key(&self) -> Option<PublicKey> {
        self.guest
    }

    /// Handle a guest message, returning what to send back.
    pub fn on_guest(&mut self, msg: &GuestMessage, now_unix: u64) -> Vec<HostMessage> {
        match msg {
            GuestMessage::Join {
                invite,
                guest_key,
                commitment,
            } => {
                if let Err(e) = self.check_join(invite, now_unix) {
                    self.phase = Phase::Failed(e);
                    return vec![HostMessage::Rejected {
                        reason: describe(e).into(),
                    }];
                }

                // Burn the nonce only once the invite has fully checked out, so a failed
                // attempt does not consume a legitimate player's invite.
                self.redeemed = true;
                self.guest = Some(*guest_key);
                self.guest_commitment = Some(*commitment);
                self.phase = Phase::AwaitingReveal;

                // The host's commitment goes out only now — after the guest's has arrived.
                // That ordering is the whole point: neither side knows the other's seed when
                // committing, so neither can steer the shuffle.
                vec![HostMessage::Accepted {
                    host_key: invite.host_key,
                    commitment: self.my_commitment,
                }]
            }

            GuestMessage::Reveal { seed } => {
                if self.phase != Phase::AwaitingReveal {
                    self.phase = Phase::Failed(JoinError::OutOfOrder);
                    return vec![HostMessage::Rejected {
                        reason: describe(JoinError::OutOfOrder).into(),
                    }];
                }
                self.shuffle = Some(Seed::combine(self.my_seed, *seed));
                self.phase = Phase::Ready;
                vec![HostMessage::Reveal { seed: self.my_seed }]
            }

            // Everything else belongs to a running game, not the handshake.
            _ => Vec::new(),
        }
    }

    fn check_join(&self, invite: &Invite, now_unix: u64) -> Result<(), JoinError> {
        invite.verify(now_unix).map_err(JoinError::BadInvite)?;
        if invite.session != self.session {
            return Err(JoinError::WrongSession);
        }
        if invite.nonce != self.nonce {
            // A valid signature over a different nonce means a different invite — one this
            // host is not currently offering.
            return Err(JoinError::AlreadyRedeemed);
        }
        if self.redeemed {
            return Err(JoinError::AlreadyRedeemed);
        }
        if self.guest.is_some() {
            return Err(JoinError::Occupied);
        }
        Ok(())
    }
}

/// The guest side of the handshake.
pub struct Guest {
    my_seed: Seed,
    my_commitment: Commitment,
    host_key: Option<PublicKey>,
    host_commitment: Option<Commitment>,
    shuffle: Option<Seed>,
    phase: Phase,
}

impl Guest {
    pub fn new(my_seed: Seed, my_commitment: Commitment) -> Self {
        Self {
            my_seed,
            my_commitment,
            host_key: None,
            host_commitment: None,
            shuffle: None,
            phase: Phase::AwaitingJoin,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn shuffle_seed(&self) -> Option<Seed> {
        self.shuffle
    }

    pub fn host_commitment(&self) -> Option<Commitment> {
        self.host_commitment
    }

    /// The opening message. Checks the invite locally first, so an expired or tampered link
    /// fails before anything is sent.
    pub fn join(
        &mut self,
        identity: &Identity,
        invite: &Invite,
        now_unix: u64,
    ) -> Result<GuestMessage, JoinError> {
        invite.verify(now_unix).map_err(JoinError::BadInvite)?;
        self.phase = Phase::AwaitingReveal;
        Ok(GuestMessage::Join {
            invite: invite.clone(),
            guest_key: identity.public(),
            commitment: self.my_commitment,
        })
    }

    /// Handle a host message, returning what to send back.
    pub fn on_host(&mut self, msg: &HostMessage) -> Vec<GuestMessage> {
        match msg {
            HostMessage::Accepted {
                host_key,
                commitment,
            } => {
                self.host_key = Some(*host_key);
                self.host_commitment = Some(*commitment);
                // Safe to reveal now: the host is committed, so it cannot choose its seed in
                // response to this one.
                vec![GuestMessage::Reveal { seed: self.my_seed }]
            }

            HostMessage::Reveal { seed } => {
                self.shuffle = Some(Seed::combine(self.my_seed, *seed));
                self.phase = Phase::Ready;
                Vec::new()
            }

            HostMessage::Rejected { .. } => {
                self.phase = Phase::Failed(JoinError::BadInvite(InviteError::Malformed));
                Vec::new()
            }

            _ => Vec::new(),
        }
    }
}

fn describe(e: JoinError) -> &'static str {
    match e {
        JoinError::BadInvite(InviteError::Expired) => "that invite has expired",
        JoinError::BadInvite(InviteError::BadSignature) => "that invite is not valid",
        JoinError::BadInvite(_) => "that invite could not be read",
        JoinError::WrongSession => "that invite is for a different game",
        JoinError::AlreadyRedeemed => "that invite has already been used",
        JoinError::OutOfOrder => "unexpected message",
        JoinError::Occupied => "someone has already joined",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fairness::Disclosure;
    use crate::invite::Endpoint;

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

        fn disclosure(&self) -> Disclosure {
            Disclosure {
                seed: self.seed,
                salt: self.salt,
                decklist: self.decklist.clone(),
            }
        }
    }

    fn invite_for(host: &Party, session: SessionId) -> Invite {
        Invite::issue(
            &host.identity,
            session,
            vec![Endpoint::Direct {
                host: "127.0.0.1".into(),
                port: 47100,
            }],
            NOW + 600,
        )
    }

    /// Run the handshake to completion, returning both sides.
    fn handshake(host_party: &Party, guest_party: &Party) -> (Host, Guest, Invite) {
        let session = SessionId([3u8; 16]);
        let invite = invite_for(host_party, session);

        let mut host = Host::new(
            session,
            invite.nonce,
            host_party.seed,
            host_party.commitment(),
        );
        let mut guest = Guest::new(guest_party.seed, guest_party.commitment());

        let join = guest
            .join(&guest_party.identity, &invite, NOW)
            .expect("invite is good");
        for reply in host.on_guest(&join, NOW) {
            for back in guest.on_host(&reply) {
                for last in host.on_guest(&back, NOW) {
                    guest.on_host(&last);
                }
            }
        }

        (host, guest, invite)
    }

    #[test]
    fn a_full_handshake_leaves_both_sides_ready() {
        let (host, guest, _) = handshake(&Party::new(1), &Party::new(2));
        assert_eq!(host.phase(), Phase::Ready);
        assert_eq!(guest.phase(), Phase::Ready);
    }

    #[test]
    fn both_sides_derive_the_same_shuffle() {
        // If these disagreed the game would desynchronise on the first draw.
        let (host, guest, _) = handshake(&Party::new(1), &Party::new(2));
        assert_eq!(host.shuffle_seed(), guest.shuffle_seed());
        assert!(host.shuffle_seed().is_some());
    }

    #[test]
    fn the_shuffle_is_neither_players_own_seed() {
        // The guarantee: neither side controls the outcome.
        let (h, g) = (Party::new(1), Party::new(2));
        let (host, _guest, _) = handshake(&h, &g);
        let combined = host.shuffle_seed().unwrap();
        assert_ne!(combined, h.seed);
        assert_ne!(combined, g.seed);
    }

    #[test]
    fn the_host_learns_the_guests_key() {
        let guest_party = Party::new(2);
        let (host, _, _) = handshake(&Party::new(1), &guest_party);
        assert_eq!(host.guest_key(), Some(guest_party.identity.public()));
    }

    #[test]
    fn the_host_commits_only_after_the_guest_has() {
        // The ordering that makes commit-reveal work. If the host published its commitment
        // first, the guest could pick a seed in response to it.
        let (h, g) = (Party::new(1), Party::new(2));
        let session = SessionId([3u8; 16]);
        let invite = invite_for(&h, session);
        let mut host = Host::new(session, invite.nonce, h.seed, h.commitment());
        let mut guest = Guest::new(g.seed, g.commitment());

        assert_eq!(
            guest.host_commitment(),
            None,
            "nothing known before joining"
        );

        let join = guest.join(&g.identity, &invite, NOW).unwrap();
        let replies = host.on_guest(&join, NOW);
        assert!(matches!(replies[0], HostMessage::Accepted { .. }));
        guest.on_host(&replies[0]);
        assert_eq!(guest.host_commitment(), Some(h.commitment()));
    }

    // ---- invites are single use and specific --------------------------

    #[test]
    fn the_same_invite_cannot_be_used_twice() {
        let (h, g) = (Party::new(1), Party::new(2));
        let (mut host, _, invite) = handshake(&h, &g);

        let second = Party::new(3);
        let mut gatecrasher = Guest::new(second.seed, second.commitment());
        let join = gatecrasher.join(&second.identity, &invite, NOW).unwrap();
        let replies = host.on_guest(&join, NOW);

        assert!(
            matches!(replies.first(), Some(HostMessage::Rejected { .. })),
            "a burned invite must not admit a second player"
        );
    }

    #[test]
    fn an_expired_invite_is_refused_by_both_sides() {
        let (h, g) = (Party::new(1), Party::new(2));
        let session = SessionId([3u8; 16]);
        let invite = Invite::issue(&h.identity, session, Vec::new(), NOW - 1);

        // The guest checks locally first, so nothing is even sent.
        let mut guest = Guest::new(g.seed, g.commitment());
        assert!(matches!(
            guest.join(&g.identity, &invite, NOW),
            Err(JoinError::BadInvite(InviteError::Expired))
        ));

        // And the host refuses it even if a modified client sends it anyway.
        let mut host = Host::new(session, invite.nonce, h.seed, h.commitment());
        let forced = GuestMessage::Join {
            invite,
            guest_key: g.identity.public(),
            commitment: g.commitment(),
        };
        assert!(matches!(
            host.on_guest(&forced, NOW).first(),
            Some(HostMessage::Rejected { .. })
        ));
        assert_eq!(
            host.phase(),
            Phase::Failed(JoinError::BadInvite(InviteError::Expired))
        );
    }

    #[test]
    fn a_tampered_invite_is_refused() {
        let (h, g) = (Party::new(1), Party::new(2));
        let session = SessionId([3u8; 16]);
        let mut invite = invite_for(&h, session);
        invite.not_after = NOW + 99_999;

        let mut host = Host::new(session, invite.nonce, h.seed, h.commitment());
        let forced = GuestMessage::Join {
            invite,
            guest_key: g.identity.public(),
            commitment: g.commitment(),
        };
        assert!(matches!(
            host.on_guest(&forced, NOW).first(),
            Some(HostMessage::Rejected { .. })
        ));
    }

    #[test]
    fn an_invite_for_another_session_is_refused() {
        let (h, g) = (Party::new(1), Party::new(2));
        let invite = invite_for(&h, SessionId([9u8; 16]));

        let mut host = Host::new(SessionId([3u8; 16]), invite.nonce, h.seed, h.commitment());
        let join = GuestMessage::Join {
            invite,
            guest_key: g.identity.public(),
            commitment: g.commitment(),
        };
        host.on_guest(&join, NOW);
        assert_eq!(host.phase(), Phase::Failed(JoinError::WrongSession));
    }

    #[test]
    fn an_invite_from_a_previous_session_of_the_same_host_is_refused() {
        // Same host, same session id, but a different nonce: a stale link.
        let (h, g) = (Party::new(1), Party::new(2));
        let session = SessionId([3u8; 16]);
        let old = invite_for(&h, session);
        let current = invite_for(&h, session);
        assert_ne!(old.nonce, current.nonce);

        let mut host = Host::new(session, current.nonce, h.seed, h.commitment());
        let join = GuestMessage::Join {
            invite: old,
            guest_key: g.identity.public(),
            commitment: g.commitment(),
        };
        host.on_guest(&join, NOW);
        assert_eq!(host.phase(), Phase::Failed(JoinError::AlreadyRedeemed));
    }

    // ---- ordering ------------------------------------------------------

    #[test]
    fn a_seed_sent_before_joining_is_refused() {
        // Otherwise a guest could see the host's seed without having committed to its own.
        let h = Party::new(1);
        let mut host = Host::new(SessionId([3u8; 16]), [0u8; 16], h.seed, h.commitment());

        let premature = GuestMessage::Reveal {
            seed: Seed::random(),
        };
        let replies = host.on_guest(&premature, NOW);

        assert!(matches!(
            replies.first(),
            Some(HostMessage::Rejected { .. })
        ));
        assert_eq!(host.phase(), Phase::Failed(JoinError::OutOfOrder));
        assert_eq!(host.shuffle_seed(), None, "no shuffle was derived");
    }

    // ---- verification after the game ----------------------------------

    #[test]
    fn an_honest_disclosure_matches_the_commitment_that_was_exchanged() {
        let (h, g) = (Party::new(1), Party::new(2));
        let (_host, guest, _) = handshake(&h, &g);
        let host_commitment = guest.host_commitment().expect("exchanged");
        assert!(
            h.disclosure().matches(&host_commitment),
            "the host's end-of-game disclosure should verify"
        );
    }

    #[test]
    fn a_host_that_swaps_its_decklist_afterwards_is_caught() {
        // The point of committing to the decklist rather than only the seed.
        let (h, g) = (Party::new(1), Party::new(2));
        let (_host, guest, _) = handshake(&h, &g);
        let host_commitment = guest.host_commitment().unwrap();

        let mut lying = h.disclosure();
        lying.decklist.push((99, 1));
        assert!(!lying.matches(&host_commitment));
    }

    #[test]
    fn a_host_that_claims_a_different_seed_afterwards_is_caught() {
        let (h, g) = (Party::new(1), Party::new(2));
        let (_host, guest, _) = handshake(&h, &g);
        let host_commitment = guest.host_commitment().unwrap();

        let mut lying = h.disclosure();
        lying.seed = Seed::random();
        assert!(!lying.matches(&host_commitment));
    }

    #[test]
    fn a_rejection_leaves_the_guest_knowing_it_failed() {
        let g = Party::new(2);
        let mut guest = Guest::new(g.seed, g.commitment());
        guest.on_host(&HostMessage::Rejected {
            reason: "nope".into(),
        });
        assert!(matches!(guest.phase(), Phase::Failed(_)));
        assert_eq!(guest.shuffle_seed(), None);
    }

    #[test]
    fn every_rejection_reason_is_explainable() {
        // A refused join should say why, not just fail.
        for e in [
            JoinError::BadInvite(InviteError::Expired),
            JoinError::BadInvite(InviteError::BadSignature),
            JoinError::WrongSession,
            JoinError::AlreadyRedeemed,
            JoinError::OutOfOrder,
            JoinError::Occupied,
        ] {
            assert!(!describe(e).is_empty(), "{e:?} has no message");
        }
    }
}
