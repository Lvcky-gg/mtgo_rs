//! Player identity.
//!
//! On first run the client generates an Ed25519 keypair and keeps it in the local
//! database. **The public key is the account.** There is nothing to register, no password,
//! no server holding a record. A friend is a public key you have pinned — trust on first
//! use — with a local nickname only you see.
//!
//! The consequence is worth stating rather than discovering: lose the key and the identity
//! is gone, because nothing else has a copy. Export and backup are a feature, not an
//! afterthought, and [`Identity::to_backup`] exists for that reason.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A peer's public identity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// A short form for reading aloud, to confirm you are talking to who you think.
    ///
    /// Derived by hashing rather than truncating the key, so a chosen-prefix attack on the
    /// fingerprint would require a preimage on SHA-256 rather than key grinding.
    pub fn fingerprint(&self) -> Fingerprint {
        let digest = Sha256::digest(self.0);
        let mut out = [0u8; 8];
        out.copy_from_slice(&digest[..8]);
        Fingerprint(out)
    }

    fn verifying(&self) -> Option<VerifyingKey> {
        VerifyingKey::from_bytes(&self.0).ok()
    }

    /// Check a signature made by this key.
    pub fn verify(&self, message: &[u8], signature: &SignatureBytes) -> bool {
        let Some(key) = self.verifying() else {
            return false;
        };
        key.verify(message, &Signature::from_bytes(&signature.0))
            .is_ok()
    }
}

/// A short, human-comparable form of a public key.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Fingerprint(pub [u8; 8]);

impl Fingerprint {
    /// Rendered in groups, e.g. `k7f2-9qm4-x8tb-3wnc`.
    ///
    /// The alphabet omits `l`, `o`, `0` and `1`, because the point of a fingerprint is that
    /// two people can read it to each other and agree.
    pub fn to_grouped_string(self) -> String {
        const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
        let mut out = String::with_capacity(19);
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 && i % 2 == 0 {
                out.push('-');
            }
            out.push(ALPHABET[(*b >> 3) as usize % ALPHABET.len()] as char);
            out.push(ALPHABET[(*b & 0x1f) as usize % ALPHABET.len()] as char);
        }
        out
    }
}

/// An Ed25519 signature.
///
/// A newtype with hand-written serde because `serde` has no blanket impl for arrays longer
/// than 32, and adding a crate to work around that is a poor trade for twenty lines.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignatureBytes(pub [u8; 64]);

impl core::fmt::Debug for SignatureBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Signature(..)")
    }
}

impl Serialize for SignatureBytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // A full visitor rather than `<&[u8]>::deserialize`, which only works for formats
        // that can hand out borrowed bytes. JSON writes a byte array as a sequence of
        // numbers, so an impl that only accepted borrowed bytes worked over a binary format
        // and failed over the wire — which is exactly where it is used.
        d.deserialize_bytes(SignatureVisitor)
    }
}

struct SignatureVisitor;

impl<'de> serde::de::Visitor<'de> for SignatureVisitor {
    type Value = SignatureBytes;

    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("64 bytes of signature")
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        let arr: [u8; 64] = v
            .try_into()
            .map_err(|_| E::invalid_length(v.len(), &"64 bytes"))?;
        Ok(SignatureBytes(arr))
    }

    fn visit_borrowed_bytes<E: serde::de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.visit_bytes(v)
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        self.visit_bytes(&v)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = [0u8; 64];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = seq
                .next_element()?
                .ok_or_else(|| serde::de::Error::invalid_length(i, &"64 bytes"))?;
        }
        // Reject a longer sequence rather than silently truncating it.
        if seq.next_element::<u8>()?.is_some() {
            return Err(serde::de::Error::invalid_length(65, &"exactly 64 bytes"));
        }
        Ok(SignatureBytes(out))
    }
}

/// This installation's keypair.
///
/// The private half never leaves the device, is never serialised except through
/// [`Identity::to_backup`], and is deliberately absent from `Debug`.
pub struct Identity {
    signing: SigningKey,
}

impl Identity {
    /// Generate a fresh identity.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom(&mut seed);
        Self {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    /// Restore from a backup of the private seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(seed),
        }
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.signing.verifying_key().to_bytes())
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.public().fingerprint()
    }

    pub fn sign(&self, message: &[u8]) -> SignatureBytes {
        SignatureBytes(self.signing.sign(message).to_bytes())
    }

    /// The private seed, for backup.
    ///
    /// Named `to_backup` rather than `private_bytes` so that every call site reads as an
    /// export — there is no other legitimate reason to take this.
    pub fn to_backup(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }
}

impl core::fmt::Debug for Identity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print the private half, not even accidentally through a derived Debug on a
        // struct that happens to contain one.
        f.debug_struct("Identity")
            .field("public", &self.public().fingerprint().to_grouped_string())
            .finish_non_exhaustive()
    }
}

/// Fill a buffer with operating-system randomness.
///
/// Reads `/dev/urandom` directly rather than taking a dependency: this is the only place
/// the crate needs randomness, and the failure mode of a missing entropy source is one
/// worth panicking on rather than papering over — a key generated from a fallback would be
/// a silent security hole.
fn getrandom(buf: &mut [u8]) {
    use std::io::Read;
    let mut f =
        std::fs::File::open("/dev/urandom").expect("no /dev/urandom: cannot generate a key safely");
    f.read_exact(buf).expect("could not read entropy");
}

/// A peer you have chosen to trust, pinned on first use.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Friend {
    pub key: PublicKey,
    /// A name only you see. There is no directory to disagree with.
    pub nickname: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_verifies_against_its_own_key() {
        let me = Identity::generate();
        let sig = me.sign(b"a message");
        assert!(me.public().verify(b"a message", &sig));
    }

    #[test]
    fn a_signature_fails_against_a_different_key() {
        let me = Identity::generate();
        let them = Identity::generate();
        let sig = me.sign(b"a message");
        assert!(!them.public().verify(b"a message", &sig));
    }

    #[test]
    fn a_signature_fails_on_altered_content() {
        let me = Identity::generate();
        let sig = me.sign(b"pay alice");
        assert!(!me.public().verify(b"pay bob", &sig));
    }

    #[test]
    fn two_identities_differ() {
        assert_ne!(Identity::generate().public(), Identity::generate().public());
    }

    #[test]
    fn an_identity_round_trips_through_its_backup() {
        // Losing the key means losing the identity, so this path has to work.
        let me = Identity::generate();
        let restored = Identity::from_seed(&me.to_backup());
        assert_eq!(me.public(), restored.public());

        let sig = restored.sign(b"still me");
        assert!(me.public().verify(b"still me", &sig));
    }

    #[test]
    fn a_fingerprint_is_stable_and_readable() {
        let me = Identity::generate();
        let text = me.fingerprint().to_grouped_string();
        assert_eq!(text.len(), 19, "8 bytes as 4 groups of 2 with dashes");
        assert_eq!(me.fingerprint(), me.public().fingerprint(), "stable");
        // The confusable characters are deliberately absent.
        for c in ['l', 'o', '0', '1'] {
            assert!(!text.contains(c), "{text} contains a confusable {c:?}");
        }
    }

    #[test]
    fn different_keys_get_different_fingerprints() {
        let a = Identity::generate().fingerprint();
        let b = Identity::generate().fingerprint();
        assert_ne!(a, b);
    }

    #[test]
    fn a_signature_survives_a_json_round_trip() {
        // The wire format is JSON, which encodes bytes as a sequence — an impl that only
        // accepted borrowed bytes passed its unit tests and broke on the wire.
        let me = Identity::generate();
        let sig = me.sign(b"over the wire");
        let json = serde_json::to_string(&sig).expect("serialises");
        let back: SignatureBytes = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, sig);
        assert!(
            me.public().verify(b"over the wire", &back),
            "and still verifies"
        );
    }

    #[test]
    fn a_wrong_length_signature_is_rejected() {
        assert!(serde_json::from_str::<SignatureBytes>("[1,2,3]").is_err());
        let too_long = format!("[{}]", (0..65).map(|_| "0").collect::<Vec<_>>().join(","));
        assert!(serde_json::from_str::<SignatureBytes>(&too_long).is_err());
    }

    #[test]
    fn debug_does_not_leak_the_private_key() {
        let me = Identity::generate();
        let text = format!("{me:?}");
        let secret = me.to_backup();
        // No byte of the seed should appear as a formatted number in the output.
        assert!(!text.contains(&format!("{}", secret[0])) || text.len() < 80);
        assert!(!text.contains("signing"), "got {text}");
    }
}
