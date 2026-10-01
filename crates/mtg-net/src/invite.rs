//! Invite tokens.
//!
//! An invite is a **signed capability**, not a lookup key. Everything needed to
//! join is inside the token, which is what lets invites work with no directory
//! service: the host signs a small struct, encodes it as a URL, and the player
//! sends that URL over whatever channel they already use to talk to each other.
//!
//! ```text
//! mtgors://join/AQTF9K2M...   ← base32, ~120 chars
//!            │
//!            └── { session, host_key, endpoints[], not_after, nonce, sig }
//! ```
//!
//! Properties this buys, each of which would otherwise need a server:
//!
//! - **Offline issuance.** Generating an invite needs no network.
//! - **Unforgeable.** The signature is over the whole body, so endpoints cannot be
//!   rewritten to point a player at someone else.
//! - **Expiring and single-use.** `not_after` bounds it in time, and the host burns
//!   `nonce` on first successful join, so a leaked invite is not a standing
//!   entrance.
//! - **Authenticating in both directions.** The token carries the host's public
//!   key, so the joiner confirms it reached the intended host, and the joiner
//!   proves its own key during the handshake.
//!
//! An invite is *not* a secret in the cryptographic sense: anyone holding it can
//! attempt to join until it expires or is used. That matches the intent — it is a
//! "here, play with me" link, not a credential — but it does mean the token should
//! not be posted publicly.

use serde::{Deserialize, Serialize};

use crate::{
    identity::{PublicKey, SignatureBytes},
    session::SessionId,
};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Invite {
    pub session: SessionId,
    /// The host's public key: both the address to verify against and the identity
    /// the joiner is agreeing to play.
    pub host_key: PublicKey,
    /// Where to try connecting, in order. The joiner races them.
    pub endpoints: Vec<Endpoint>,
    /// Unix seconds. Short by default — an invite is for right now.
    pub not_after: u64,
    /// Burned by the host on first successful join.
    pub nonce: [u8; 16],
    /// Ed25519 over the canonical encoding of every field above.
    pub signature: SignatureBytes,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Endpoint {
    /// A directly reachable address. The common case on a LAN.
    Direct { host: Box<str>, port: u16 },
    /// Discover the host by name on the local network. Needs nothing configured
    /// and covers playing with someone in the same room.
    Mdns { instance: Box<str> },
    /// A relay the *user* chose to run or trust, which forwards bytes it cannot
    /// read. Opt-in, and the only reason it exists is NAT.
    Relay { url: Box<str> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InviteError {
    Malformed,
    BadSignature,
    Expired,
    /// The nonce was already used.
    AlreadyRedeemed,
    /// The token is for a session this host is not running.
    UnknownSession,
}

/// Prepended when signing, never transmitted. Ensures a signature over an invite can
/// never be replayed as a signature over anything else this key signs.
const DOMAIN: &[u8] = b"mtgors-invite-v1";

impl Invite {
    pub const URL_SCHEME: &'static str = "mtgors";

    /// Create and sign an invite.
    pub fn issue(
        identity: &crate::Identity,
        session: SessionId,
        endpoints: Vec<Endpoint>,
        not_after: u64,
    ) -> Self {
        let mut nonce = [0u8; 16];
        crate::fairness::fill_random(&mut nonce);

        let mut invite = Self {
            session,
            host_key: identity.public(),
            endpoints,
            not_after,
            nonce,
            signature: crate::SignatureBytes([0u8; 64]),
        };
        invite.signature = identity.sign(&invite.signing_payload());
        invite
    }

    /// The bytes covered by the signature.
    ///
    /// Canonical and length-prefixed so no two distinct invites can share an encoding — an
    /// ambiguous encoding would let a signature be transplanted onto a different invite.
    /// Domain-separated so a signature made here can never be replayed as a signature over
    /// anything else this key signs.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&self.body());
        out
    }

    /// The invite's fields, canonically encoded. This is what travels; the domain tag is
    /// prepended only for signing, since sending a constant on every link is waste.
    fn body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(112);
        out.extend_from_slice(&self.session.0);
        out.extend_from_slice(&self.host_key.0);
        out.extend_from_slice(&(self.endpoints.len() as u16).to_le_bytes());
        for e in &self.endpoints {
            match e {
                Endpoint::Direct { host, port } => {
                    out.push(0);
                    push_str(&mut out, host);
                    out.extend_from_slice(&port.to_le_bytes());
                }
                Endpoint::Mdns { instance } => {
                    out.push(1);
                    push_str(&mut out, instance);
                }
                Endpoint::Relay { url } => {
                    out.push(2);
                    push_str(&mut out, url);
                }
            }
        }
        out.extend_from_slice(&self.not_after.to_le_bytes());
        out.extend_from_slice(&self.nonce);
        out
    }

    /// Whether the signature is the host key's, and the invite is still in date.
    ///
    /// Does **not** check the nonce: that is per-session state the host holds, not a
    /// property of the token, so it is checked by [`crate::session`] at redemption.
    pub fn verify(&self, now_unix: u64) -> Result<(), InviteError> {
        if !self
            .host_key
            .verify(&self.signing_payload(), &self.signature)
        {
            return Err(InviteError::BadSignature);
        }
        if self.is_expired(now_unix) {
            return Err(InviteError::Expired);
        }
        Ok(())
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix > self.not_after
    }

    /// The pasteable link.
    ///
    /// Base32 rather than base64: it survives case-insensitive transports, contains no
    /// characters a chat client will treat as punctuation, and can be read aloud.
    pub fn to_url(&self) -> String {
        let encoded = data_encoding::BASE32_NOPAD.encode(&self.encode());
        format!("{}://join/{}", Self::URL_SCHEME, encoded)
    }

    /// Parse a link, or anything that looks like one.
    ///
    /// Tolerant of a missing scheme and of case, because these arrive through chat clients
    /// that helpfully "fix" URLs.
    pub fn from_url(text: &str) -> Result<Self, InviteError> {
        let trimmed = text.trim();
        let body = trimmed
            .rsplit_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or(trimmed);
        let bytes = data_encoding::BASE32_NOPAD
            .decode(body.to_ascii_uppercase().as_bytes())
            .map_err(|_| InviteError::Malformed)?;
        Self::decode(&bytes)
    }

    /// The body followed by the signature.
    fn encode(&self) -> Vec<u8> {
        let mut out = self.body();
        out.extend_from_slice(&self.signature.0);
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self, InviteError> {
        let mut r = Reader { bytes, at: 0 };
        let session = SessionId(r.take_array::<16>()?);
        let host_key = PublicKey(r.take_array::<32>()?);

        let count = u16::from_le_bytes(r.take_array::<2>()?);
        let mut endpoints = Vec::with_capacity(count as usize);
        for _ in 0..count {
            endpoints.push(match r.take_u8()? {
                0 => {
                    let host = r.take_str()?;
                    let port = u16::from_le_bytes(r.take_array::<2>()?);
                    Endpoint::Direct { host, port }
                }
                1 => Endpoint::Mdns {
                    instance: r.take_str()?,
                },
                2 => Endpoint::Relay { url: r.take_str()? },
                _ => return Err(InviteError::Malformed),
            });
        }

        let not_after = u64::from_le_bytes(r.take_array::<8>()?);
        let nonce = r.take_array::<16>()?;
        let signature = crate::SignatureBytes(r.take_array::<64>()?);

        // Trailing bytes mean the encoding was not the one this build produces, and a
        // permissive parser here would accept two encodings of the same invite.
        if r.at != bytes.len() {
            return Err(InviteError::Malformed);
        }

        Ok(Self {
            session,
            host_key,
            endpoints,
            not_after,
            nonce,
            signature,
        })
    }
}

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// A minimal cursor. Every read is bounds-checked, so a truncated or hostile token is a
/// `Malformed` error rather than a panic.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], InviteError> {
        let end = self.at.checked_add(n).ok_or(InviteError::Malformed)?;
        let slice = self.bytes.get(self.at..end).ok_or(InviteError::Malformed)?;
        self.at = end;
        Ok(slice)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], InviteError> {
        let slice = self.take(N)?;
        slice.try_into().map_err(|_| InviteError::Malformed)
    }

    fn take_u8(&mut self) -> Result<u8, InviteError> {
        Ok(self.take(1)?[0])
    }

    fn take_str(&mut self) -> Result<Box<str>, InviteError> {
        let len = u16::from_le_bytes(self.take_array::<2>()?) as usize;
        let bytes = self.take(len)?;
        let s = std::str::from_utf8(bytes).map_err(|_| InviteError::Malformed)?;
        Ok(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    const NOW: u64 = 1_700_000_000;

    fn session() -> SessionId {
        SessionId([7u8; 16])
    }

    fn endpoints() -> Vec<Endpoint> {
        vec![
            Endpoint::Direct {
                host: "192.168.1.20".into(),
                port: 47100,
            },
            Endpoint::Mdns {
                instance: "kitchen-laptop".into(),
            },
            Endpoint::Relay {
                url: "wss://relay.example/mtgors".into(),
            },
        ]
    }

    #[test]
    fn an_issued_invite_verifies() {
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        assert_eq!(invite.verify(NOW), Ok(()));
    }

    #[test]
    fn it_round_trips_through_its_link() {
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);

        let url = invite.to_url();
        assert!(url.starts_with("mtgors://join/"));

        let back = Invite::from_url(&url).expect("decodes");
        assert_eq!(back, invite);
        assert_eq!(back.verify(NOW), Ok(()));
    }

    #[test]
    fn a_link_survives_being_mangled_by_a_chat_client() {
        // Lower-cased, whitespace-padded, scheme stripped: all things that happen in
        // practice, and none of which should break a paste.
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        let url = invite.to_url();

        let body = url.rsplit_once('/').unwrap().1.to_string();
        for variant in [
            url.to_lowercase(),
            format!("  {url}  "),
            body.clone(),
            body.to_lowercase(),
        ] {
            assert_eq!(
                Invite::from_url(&variant).as_ref(),
                Ok(&invite),
                "{variant}"
            );
        }
    }

    #[test]
    fn an_expired_invite_is_refused() {
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW - 1);
        assert_eq!(invite.verify(NOW), Err(InviteError::Expired));
    }

    #[test]
    fn a_tampered_endpoint_invalidates_the_signature() {
        // The attack this prevents: rewriting an endpoint to point the joiner at you.
        let host = Identity::generate();
        let mut invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        invite.endpoints[0] = Endpoint::Direct {
            host: "10.0.0.1".into(),
            port: 1,
        };
        assert_eq!(invite.verify(NOW), Err(InviteError::BadSignature));
    }

    #[test]
    fn a_tampered_expiry_invalidates_the_signature() {
        let host = Identity::generate();
        let mut invite = Invite::issue(&host, session(), endpoints(), NOW - 1);
        invite.not_after = NOW + 10_000;
        assert_eq!(invite.verify(NOW), Err(InviteError::BadSignature));
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        let host = Identity::generate();
        let impostor = Identity::generate();
        let mut invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        invite.signature = impostor.sign(&invite.signing_payload());
        assert_eq!(invite.verify(NOW), Err(InviteError::BadSignature));
    }

    #[test]
    fn swapping_the_host_key_is_refused() {
        // Replacing the key *and* the signature still fails, because the key is inside the
        // signed body — so the token names exactly one host.
        let host = Identity::generate();
        let impostor = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);

        let mut forged = invite.clone();
        forged.host_key = impostor.public();
        assert_eq!(forged.verify(NOW), Err(InviteError::BadSignature));
    }

    #[test]
    fn garbage_is_malformed_not_a_panic() {
        for text in [
            "",
            "not base32 at all!!",
            "mtgors://join/",
            "AAAA",
            "mtgors://join/AAAAAAAA",
        ] {
            assert!(
                matches!(Invite::from_url(text), Err(InviteError::Malformed)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_truncated_token_is_malformed() {
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        let full = invite.encode();
        for cut in [1, 16, 40, full.len() - 1] {
            let partial = data_encoding::BASE32_NOPAD.encode(&full[..cut]);
            assert!(
                Invite::from_url(&partial).is_err(),
                "truncated at {cut} should not decode"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        // Otherwise the same invite would have more than one valid encoding.
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);
        let mut bytes = invite.encode();
        bytes.push(0);
        let text = data_encoding::BASE32_NOPAD.encode(&bytes);
        assert_eq!(Invite::from_url(&text), Err(InviteError::Malformed));
    }

    #[test]
    fn an_invite_with_no_endpoints_still_works() {
        // Valid, and means "find me by other means" — it still names and authenticates a
        // host.
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), Vec::new(), NOW + 600);
        let back = Invite::from_url(&invite.to_url()).unwrap();
        assert!(back.endpoints.is_empty());
        assert_eq!(back.verify(NOW), Ok(()));
    }

    #[test]
    fn the_domain_tag_is_signed_but_not_transmitted() {
        let host = Identity::generate();
        let invite = Invite::issue(&host, session(), endpoints(), NOW + 600);

        assert!(
            invite.signing_payload().starts_with(DOMAIN),
            "the domain must be covered by the signature"
        );
        assert!(
            !invite.encode().starts_with(DOMAIN),
            "but sending a constant on every link is waste"
        );
        // And it still verifies after a round trip, so decode reconstructs it.
        assert_eq!(
            Invite::from_url(&invite.to_url()).unwrap().verify(NOW),
            Ok(())
        );
    }

    #[test]
    fn two_invites_from_the_same_host_differ() {
        // The nonce makes each one single-use, so they must not collide.
        let host = Identity::generate();
        let a = Invite::issue(&host, session(), endpoints(), NOW + 600);
        let b = Invite::issue(&host, session(), endpoints(), NOW + 600);
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.to_url(), b.to_url());
    }

    #[test]
    fn a_link_is_short_enough_to_paste() {
        let host = Identity::generate();
        let invite = Invite::issue(
            &host,
            session(),
            vec![Endpoint::Direct {
                host: "192.168.1.20".into(),
                port: 47100,
            }],
            NOW + 600,
        );
        let url = invite.to_url();
        // The signature (64 bytes) and host key (32) dominate and are irreducible for
        // Ed25519; the rest is a session id, one endpoint, an expiry and a nonce. Base32
        // inflates by 8/5. Around 260 characters is the floor, so this guards against
        // accidental bloat rather than asserting a tight bound.
        assert!(url.len() < 300, "link was {} chars: {url}", url.len());
    }
}
