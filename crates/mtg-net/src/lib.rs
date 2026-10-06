//! Session establishment and transport.
//!
//! A game is two peers that found each other. Player identity is a keypair
//! generated on first run, with no game account service or matchmaker. LAN games
//! can discover each other locally; an optional provider tunnel makes the host
//! reachable over the internet without changing the encrypted peer protocol.
//!
//! What that leaves to solve:
//!
//! - **Identity without accounts** — [`identity`].
//! - **Invites without a directory** — [`invite`].
//! - **Hidden information with one authoritative peer** — [`session`].
//! - **A shuffle neither side can rig** — [`fairness`].

pub mod fairness;
pub mod identity;
pub mod invite;
pub mod session;
pub mod wire;

#[cfg(feature = "transport")]
pub mod discovery;
#[cfg(feature = "transport")]
pub mod noise;
#[cfg(feature = "transport")]
pub mod ws;

pub use identity::{Fingerprint, Friend, Identity, PublicKey, SignatureBytes};
pub use invite::{Invite, InviteError};
pub use session::{Guest, Host, HostRole, SessionId};
pub use wire::{Channel, MemoryChannel, WireError};
