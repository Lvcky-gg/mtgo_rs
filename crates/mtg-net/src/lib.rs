//! Session establishment and transport.
//!
//! There is **no server**. No account service, no matchmaker, no lobby, nothing
//! to sign up for and nothing to go offline. A game is two peers that found each
//! other, and the only thing standing in for an account system is a keypair
//! generated on first run.
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
pub mod noise;
#[cfg(feature = "transport")]
pub mod ws;

pub use identity::{Fingerprint, Friend, Identity, PublicKey, SignatureBytes};
pub use invite::{Invite, InviteError};
pub use session::{Guest, Host, HostRole, SessionId};
pub use wire::{Channel, MemoryChannel, WireError};
