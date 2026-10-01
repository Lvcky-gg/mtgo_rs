//! Encrypting the channel.
//!
//! The wire carries hidden information — a redacted view still tells you things, and a relay
//! sits in the middle by design. So the channel is encrypted, and it is encrypted in a way
//! that does not require the relay to be trusted or a certificate to exist.
//!
//! # Why Noise `NN` plus a signature, rather than `XX`
//!
//! Noise's mutually-authenticated patterns want static Diffie–Hellman keys. This client's
//! identity is **Ed25519**, for signing invites; using the same key material for X25519 is
//! poor practice, and carrying a second long-term key per player is a second thing to back
//! up and lose.
//!
//! So: `NN` (ephemeral keys only) provides the encrypted channel, and authentication is
//! bolted on by **channel binding** — each side signs the Noise handshake hash with its
//! Ed25519 identity key and sends that inside the encrypted channel. The handshake hash is
//! unique to this connection, so a signature over it cannot be replayed onto another one,
//! and a man-in-the-middle ends up with two different hashes and cannot produce a signature
//! that satisfies either side. That is the standard construction, and it keeps one key type.
//!
//! # What this does and does not protect
//!
//! It gives confidentiality and integrity against anyone carrying the bytes, including a
//! relay, and it proves who is on the other end. It does **not** protect against the peer
//! themselves: the host legitimately knows the whole game state, and bounding *that* is
//! [`crate::fairness`]'s job, not this module's.

use snow::{HandshakeState, TransportState};

use crate::{
    identity::{Identity, PublicKey, SignatureBytes},
    wire::{Channel, WireError},
};

/// Ephemeral-only pattern: authentication comes from the signature below, not from static
/// DH keys.
const PARAMS: &str = "Noise_NN_25519_ChaChaPoly_BLAKE2s";

/// Domain separation, so an identity signature over a channel binding can never be mistaken
/// for a signature over an invite.
const BINDING_DOMAIN: &[u8] = b"mtgors-channel-binding-v1";

/// Noise has a 65535-byte message limit; frames are chunked below it with room for the tag.
const MAX_PLAINTEXT: usize = 32 * 1024;
/// Leave ample room for card tables and snapshots while bounding hostile frames.
const MAX_FRAME_SIZE: usize = 32 * 1024 * 1024;
const MAX_CHUNKS: u32 = (MAX_FRAME_SIZE / MAX_PLAINTEXT) as u32;

/// An encrypted, mutually authenticated channel.
pub struct NoiseChannel<C: Channel> {
    inner: C,
    transport: TransportState,
    /// Who is on the other end, proven by signature.
    peer: PublicKey,
}

impl<C: Channel> NoiseChannel<C> {
    /// The authenticated identity of the peer.
    pub fn peer(&self) -> PublicKey {
        self.peer
    }

    /// Configure the underlying transport, for example to clear setup timeouts before
    /// a match begins. Application messages must go through this encrypted channel.
    pub fn inner_mut(&mut self) -> &mut C {
        &mut self.inner
    }

    /// Open a channel as the initiator (the guest, who dialled).
    ///
    /// `expect` is the host key from the invite. Checking it here is what makes the invite an
    /// authenticated address rather than just a hint: a relay that redirected the connection
    /// cannot produce this signature.
    pub fn initiate(
        mut inner: C,
        identity: &Identity,
        expect: PublicKey,
    ) -> Result<Self, WireError> {
        let mut hs = builder()?.build_initiator().map_err(noise_err)?;
        let mut buf = vec![0u8; 65535];

        // -> e
        let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
        inner.send(&buf[..n])?;

        // <- e, ee
        let msg = inner.recv()?;
        hs.read_message(&msg, &mut buf).map_err(noise_err)?;

        let binding = binding_material(&hs);
        let mut transport = hs.into_transport_mode().map_err(noise_err)?;

        // Prove who we are over this specific channel, then require the same of the peer.
        send_encrypted(
            &mut inner,
            &mut transport,
            &binding_payload(identity, &binding),
        )?;
        let their_sig = recv_encrypted(&mut inner, &mut transport)?;
        let peer = verify_binding(&their_sig, &binding, Some(expect))?;

        Ok(Self {
            inner,
            transport,
            peer,
        })
    }

    /// Open a channel as the responder (the host, who is listening).
    ///
    /// The host does not know in advance who is calling, so it learns the guest's key from
    /// the signature rather than checking it against an expectation.
    pub fn respond(mut inner: C, identity: &Identity) -> Result<Self, WireError> {
        let mut hs = builder()?.build_responder().map_err(noise_err)?;
        let mut buf = vec![0u8; 65535];

        // -> e
        let msg = inner.recv()?;
        hs.read_message(&msg, &mut buf).map_err(noise_err)?;

        // <- e, ee
        let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
        inner.send(&buf[..n])?;

        let binding = binding_material(&hs);
        let mut transport = hs.into_transport_mode().map_err(noise_err)?;

        // The initiator signs first, so the responder learns who it is talking to before
        // revealing anything about itself beyond the ephemeral key.
        let their_sig = recv_encrypted(&mut inner, &mut transport)?;
        let peer = verify_binding(&their_sig, &binding, None)?;
        send_encrypted(
            &mut inner,
            &mut transport,
            &binding_payload(identity, &binding),
        )?;

        Ok(Self {
            inner,
            transport,
            peer,
        })
    }
}

impl<C: Channel> Channel for NoiseChannel<C> {
    fn send(&mut self, frame: &[u8]) -> Result<(), WireError> {
        if frame.len() > MAX_FRAME_SIZE {
            return Err(WireError::Malformed(
                "frame exceeds the 32 MiB limit".into(),
            ));
        }
        // Noise caps a message at 65535 bytes, and a game state snapshot can exceed that, so
        // a frame is chunked with a count in front rather than silently failing on a big one.
        let chunks: Vec<&[u8]> = frame.chunks(MAX_PLAINTEXT).collect();
        let count = chunks.len().max(1) as u32;
        let mut header = [0u8; 4];
        header.copy_from_slice(&count.to_le_bytes());
        send_encrypted(&mut self.inner, &mut self.transport, &header)?;

        for chunk in chunks {
            send_encrypted(&mut self.inner, &mut self.transport, chunk)?;
        }
        if frame.is_empty() {
            send_encrypted(&mut self.inner, &mut self.transport, &[])?;
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8>, WireError> {
        let header = recv_encrypted(&mut self.inner, &mut self.transport)?;
        let count = u32::from_le_bytes(
            header
                .as_slice()
                .try_into()
                .map_err(|_| WireError::Malformed("chunk header must be four bytes".into()))?,
        );
        // A hostile peer must not be able to make the receiver allocate unboundedly.
        if count == 0 || count > MAX_CHUNKS {
            return Err(WireError::Malformed("implausible chunk count".into()));
        }

        let mut out = Vec::new();
        for _ in 0..count {
            let chunk = recv_encrypted(&mut self.inner, &mut self.transport)?;
            if chunk.len() > MAX_FRAME_SIZE - out.len() {
                return Err(WireError::Malformed(
                    "frame exceeds the 32 MiB limit".into(),
                ));
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }
}

fn builder() -> Result<snow::Builder<'static>, WireError> {
    let params = PARAMS
        .parse()
        .map_err(|_| WireError::Malformed("bad noise params".into()))?;
    Ok(snow::Builder::new(params))
}

/// The bytes both sides sign: a domain tag plus this connection's handshake hash.
fn binding_material(hs: &HandshakeState) -> Vec<u8> {
    let mut out = Vec::with_capacity(BINDING_DOMAIN.len() + 64);
    out.extend_from_slice(BINDING_DOMAIN);
    out.extend_from_slice(hs.get_handshake_hash());
    out
}

/// Check a peer's channel-binding signature.
///
/// The signature is over this connection's handshake hash, so it proves both *who* the peer
/// is and that they are on *this* channel. A man-in-the-middle sees two different hashes and
/// cannot satisfy either side.
fn verify_binding(
    payload: &[u8],
    binding: &[u8],
    expect: Option<PublicKey>,
) -> Result<PublicKey, WireError> {
    // key ‖ signature
    if payload.len() != 32 + 64 {
        // The initiator sends only a signature; the responder needs the key too. Both are
        // sent as key ‖ signature for symmetry, so a short payload is malformed.
        return Err(WireError::Malformed("bad channel binding".into()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&payload[..32]);
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&payload[32..]);

    let peer = PublicKey(key);
    if let Some(expected) = expect
        && peer != expected
    {
        return Err(WireError::Refused(
            "the peer is not the host named in the invite".into(),
        ));
    }
    if !peer.verify(binding, &SignatureBytes(sig)) {
        return Err(WireError::Refused(
            "the peer could not prove its identity".into(),
        ));
    }
    Ok(peer)
}

fn send_encrypted<C: Channel>(
    inner: &mut C,
    transport: &mut TransportState,
    plaintext: &[u8],
) -> Result<(), WireError> {
    let mut buf = vec![0u8; plaintext.len() + 16];
    let n = transport
        .write_message(plaintext, &mut buf)
        .map_err(noise_err)?;
    inner.send(&buf[..n])
}

fn recv_encrypted<C: Channel>(
    inner: &mut C,
    transport: &mut TransportState,
) -> Result<Vec<u8>, WireError> {
    let ciphertext = inner.recv()?;
    if !(16..=65535).contains(&ciphertext.len()) {
        return Err(WireError::Malformed(
            "invalid encrypted message size".into(),
        ));
    }
    let mut buf = vec![0u8; ciphertext.len().max(16)];
    let n = transport
        .read_message(&ciphertext, &mut buf)
        .map_err(noise_err)?;
    buf.truncate(n);
    Ok(buf)
}

fn noise_err(e: snow::Error) -> WireError {
    WireError::Malformed(format!("encryption failed: {e:?}"))
}

/// The payload each side sends to bind its identity to the channel: key then signature.
pub fn binding_payload(identity: &Identity, binding: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&identity.public().0);
    out.extend_from_slice(&identity.sign(binding).0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::MemoryChannel;

    fn pair() -> (NoiseChannel<MemoryChannel>, NoiseChannel<MemoryChannel>) {
        let host = Identity::generate();
        let guest = Identity::generate();
        let key = host.public();
        let (host_end, guest_end) = MemoryChannel::pair();
        let responder = std::thread::spawn(move || NoiseChannel::respond(host_end, &host));
        let initiator = NoiseChannel::initiate(guest_end, &guest, key).unwrap();
        (responder.join().unwrap().unwrap(), initiator)
    }

    #[test]
    fn malformed_chunk_headers_are_rejected_before_reading_chunks() {
        for header in [
            vec![],
            vec![1, 0, 0],
            vec![1, 0, 0, 0, 42],
            0u32.to_le_bytes().to_vec(),
            (MAX_CHUNKS + 1).to_le_bytes().to_vec(),
        ] {
            let (mut receiver, mut sender) = pair();
            send_encrypted(&mut sender.inner, &mut sender.transport, &header).unwrap();
            drop(sender);
            assert!(matches!(receiver.recv(), Err(WireError::Malformed(_))));
        }
    }

    #[test]
    fn invalid_ciphertext_sizes_are_rejected() {
        for size in [0, 15, 65536] {
            let (mut receiver, mut sender) = pair();
            sender.inner.send(&vec![0; size]).unwrap();
            assert!(matches!(receiver.recv(), Err(WireError::Malformed(message))
                if message == "invalid encrypted message size"));
        }
    }

    #[test]
    fn oversized_send_does_not_corrupt_the_next_frame() {
        let (mut receiver, mut sender) = pair();
        assert!(matches!(
            sender.send(&vec![0; MAX_FRAME_SIZE + 1]),
            Err(WireError::Malformed(_))
        ));
        sender.send(b"still connected").unwrap();
        assert_eq!(receiver.recv().unwrap(), b"still connected");
    }

    #[test]
    fn oversized_reassembly_is_rejected() {
        let (mut receiver, mut sender) = pair();
        let producer = std::thread::spawn(move || {
            send_encrypted(
                &mut sender.inner,
                &mut sender.transport,
                &MAX_CHUNKS.to_le_bytes(),
            )
            .unwrap();
            // A peer can encrypt larger chunks than our sender normally emits.
            let chunk = vec![0; 65535 - 16];
            for _ in 0..MAX_CHUNKS {
                if send_encrypted(&mut sender.inner, &mut sender.transport, &chunk).is_err() {
                    break;
                }
            }
        });
        assert!(matches!(receiver.recv(), Err(WireError::Malformed(message))
            if message == "frame exceeds the 32 MiB limit"));
        drop(receiver);
        producer.join().unwrap();
    }
}
