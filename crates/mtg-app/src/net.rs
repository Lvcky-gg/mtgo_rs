//! Hosting and joining a match over the network, from the app's side.
//!
//! The protocol pieces already exist — invite links, the encrypted channel, the commit-reveal
//! shuffle, and the match exchange in `mtg_session::matches`. This strings them together for
//! the two buttons on the menu, and reports progress to the window as it goes.
//!
//! # Reachability
//!
//! A host listens on all interfaces and puts its LAN address, and the loopback address for
//! testing on one machine, in the invite. Internet hosting uses an embedded ngrok HTTPS
//! tunnel. Noise still authenticates the host and encrypts game messages end to end.

use std::net::{IpAddr, TcpListener, UdpSocket};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::time::Duration;

use mtg_net::{
    Guest, Host, Identity, Invite, SessionId, WireError,
    fairness::{Commitment, Seed, fill_random},
    invite::Endpoint,
    noise::NoiseChannel,
    wire::{authenticated_host_handshake, guest_handshake},
    ws::WsChannel,
};
use mtg_session::{
    game::{CardSource, DeckSpec, MatchSettings},
    matches::{MatchEnd, Seat, guest_match, host_match},
};
use mtg_store::{Store, StoredIdentity};

use crate::seat::MatchEvent;

/// How long an invite link stays valid.
const INVITE_LIFETIME: u64 = 60 * 60;
/// Identity and lobby setup should fail if the peer stops responding.
const SETUP_IO_TIMEOUT: Duration = Duration::from_secs(10);

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// This installation's identity: loaded from the database, or made and saved on first use.
/// The public key is the account, so it is kept rather than regenerated per game.
pub fn identity() -> Result<Identity, String> {
    let store: Store = crate::decks::create_store()?;
    identity_from_store(&store)
}

fn identity_from_store(store: &Store) -> Result<Identity, String> {
    let stored = match store.identity().map_err(|e| e.to_string())? {
        Some(stored) => stored,
        None => {
            let fresh = Identity::generate();
            store
                .identity_or_insert(&StoredIdentity {
                    seed: fresh.to_backup(),
                    public: fresh.public().0,
                })
                .map_err(|e| e.to_string())?
        }
    };
    let identity = Identity::from_seed(&stored.seed);
    if identity.public().0 != stored.public {
        return Err("the saved identity's public key does not match its seed".into());
    }
    Ok(identity)
}

/// The address other machines on the LAN reach this one by. Found by asking the OS which
/// interface it would route through; a UDP "connect" sends nothing.
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket
        .local_addr()
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_loopback())
}

/// A commitment to a seed and deck, as the handshake requires. The deck is committed by
/// position and count; its contents travel separately in the match exchange.
fn commit(seed: &Seed, deck: &DeckSpec) -> Commitment {
    let mut salt = [0u8; 16];
    fill_random(&mut salt);
    let list: Vec<(u32, u8)> = deck
        .main
        .iter()
        .enumerate()
        .map(|(i, (_, n))| (i as u32, *n))
        .collect();
    Commitment::of(seed, &salt, &list)
}

/// Everything a host needs besides its seat: who it is, where cards come from, and where to
/// listen. Passed in rather than read from the default database, so a test can use its own.
pub struct HostConfig<'a> {
    pub settings: MatchSettings,
    pub deck: DeckSpec,
    /// 0 for any free port.
    pub port: u16,
    /// A public address to put in the invite, for play over the internet.
    pub public_address: Option<String>,
    /// None for LAN hosting; an ngrok authtoken for an automatic internet tunnel.
    pub tunnel_token: Option<String>,
    /// Optional assigned domain for ngrok accounts requiring an explicit endpoint.
    pub tunnel_domain: Option<String>,
    /// Name shown to players on the same network.
    pub name: String,
    pub identity: &'a Identity,
    pub source: &'a dyn CardSource,
}

/// Host a match: listen, publish an invite, wait for a guest, then play.
pub fn host(
    config: HostConfig<'_>,
    seat: &mut dyn Seat,
    events: &Sender<MatchEvent>,
    cancel: &Arc<AtomicBool>,
) -> Result<MatchEnd, String> {
    let HostConfig {
        settings,
        deck,
        port,
        public_address: extra_address,
        tunnel_token,
        tunnel_domain,
        name,
        identity,
        source,
    } = config;
    let listener = TcpListener::bind(("0.0.0.0", port))
        .map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    check_cancel(cancel)?;
    let mut tunnel = if let Some(token) = tunnel_token {
        let _ = events.send(MatchEvent::Status("opening your internet tunnel…".into()));
        Some(crate::tunnel::Tunnel::start(
            port,
            &token,
            tunnel_domain.as_deref(),
            cancel,
        )?)
    } else {
        None
    };
    check_cancel(cancel)?;
    let session = SessionId::random();
    let mut endpoints = vec![Endpoint::Mdns {
        instance: mtg_net::discovery::instance(session).into(),
    }];
    if let Some(tunnel) = &tunnel {
        endpoints.push(Endpoint::Tunnel {
            url: tunnel.url().into(),
        });
    }
    if let Some(public) = extra_address.filter(|a| !a.trim().is_empty()) {
        endpoints.push(Endpoint::Direct {
            host: public.trim().into(),
            port,
        });
    }
    if let Some(lan) = lan_address() {
        endpoints.push(Endpoint::Direct {
            host: lan.to_string().into(),
            port,
        });
    }
    endpoints.push(Endpoint::Direct {
        host: "127.0.0.1".into(),
        port,
    });

    let invite = Invite::issue(identity, session, endpoints, now() + INVITE_LIFETIME);
    let advertisement = match mtg_net::discovery::Advertisement::publish(&invite, port, &name) {
        Ok(advertisement) => Some(advertisement),
        Err(_) => {
            let _ = events.send(MatchEvent::Status(
                "Nearby discovery is unavailable. Share the invite link instead.".into(),
            ));
            None
        }
    };
    let _ = events.send(MatchEvent::Invite(invite.to_url()));
    let _ = events.send(MatchEvent::Status("waiting for someone to join…".into()));

    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let (mut secure, shuffle) = wait_for_guest_with_tunnel(
        &listener,
        identity,
        &invite,
        &deck,
        events,
        cancel,
        tunnel.as_mut(),
    )?;
    drop(advertisement);
    let guest = secure.peer().fingerprint().to_grouped_string();
    let _ = events.send(MatchEvent::Status(format!("playing {guest}")));

    play_with_cancel(&mut secure, cancel, |secure| {
        host_match(secure, settings, deck, source, shuffle, seat).map_err(|e| e.to_string())
    })
}

/// Failed attempts may be retried only before a valid Join consumes the invite.
#[cfg(test)]
fn wait_for_guest(
    listener: &TcpListener,
    identity: &Identity,
    invite: &Invite,
    deck: &DeckSpec,
    events: &Sender<MatchEvent>,
    cancel: &AtomicBool,
) -> Result<(NoiseChannel<WsChannel>, Seed), String> {
    wait_for_guest_with_tunnel(listener, identity, invite, deck, events, cancel, None)
}

fn wait_for_guest_with_tunnel(
    listener: &TcpListener,
    identity: &Identity,
    invite: &Invite,
    deck: &DeckSpec,
    events: &Sender<MatchEvent>,
    cancel: &AtomicBool,
    mut tunnel: Option<&mut crate::tunnel::Tunnel>,
) -> Result<(NoiseChannel<WsChannel>, Seed), String> {
    let seed = Seed::random();
    let commitment = commit(&seed, deck);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if tunnel.as_mut().is_some_and(|tunnel| !tunnel.is_running()) {
            return Err(
                "Your internet tunnel stopped. Check your connection and host again.".into(),
            );
        }
        invite
            .verify(now())
            .map_err(|e| format!("the invite cannot be used: {e:?}"))?;
        let plain = match WsChannel::accept(listener) {
            Ok(channel) => channel,
            Err(WireError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                let _ = events.send(MatchEvent::Status(format!("a connection failed: {e}")));
                continue;
            }
        };
        let _ = events.send(MatchEvent::Status(
            "someone connected — securing the connection…".into(),
        ));
        let setup = plain
            .set_io_timeout(Some(SETUP_IO_TIMEOUT))
            .and_then(|()| NoiseChannel::respond(plain, identity));
        let mut secure = match setup {
            Ok(channel) => channel,
            Err(e) => {
                let _ = events.send(MatchEvent::Status(format!(
                    "a connection failed: {e}; waiting for someone to join…"
                )));
                continue;
            }
        };
        let mut handshake = Host::new(invite.session, invite.nonce, seed, commitment);
        let peer = secure.peer();
        let shuffle = match authenticated_host_handshake(&mut secure, &mut handshake, now(), peer) {
            Ok(shuffle) => shuffle,
            // A validated Join burns the nonce, even if the peer disconnects before
            // revealing. Do not reset that state and admit a second guest.
            Err(e) if handshake.guest_key().is_some() => return Err(e.to_string()),
            Err(e) => {
                let _ = events.send(MatchEvent::Status(format!(
                    "a connection failed: {e}; waiting for someone to join…"
                )));
                continue;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        secure
            .inner_mut()
            .set_io_timeout(None)
            .map_err(|e| e.to_string())?;
        return Ok((secure, shuffle));
    }
}

/// Join a match from a pasted invite link.
pub fn join(
    link: &str,
    deck: DeckSpec,
    identity: &Identity,
    seat: &mut dyn Seat,
    events: &Sender<MatchEvent>,
) -> Result<MatchEnd, String> {
    join_with_cancel(link, deck, identity, seat, events, &AtomicBool::new(false))
}

/// Join while observing cancellation between connection setup operations.
pub fn join_with_cancel(
    link: &str,
    deck: DeckSpec,
    identity: &Identity,
    seat: &mut dyn Seat,
    events: &Sender<MatchEvent>,
    cancel: &AtomicBool,
) -> Result<MatchEnd, String> {
    check_cancel(cancel)?;
    let invite = Invite::from_url(link.trim())
        .map_err(|e| format!("that is not a valid invite link: {e:?}"))?;
    invite
        .verify(now())
        .map_err(|e| format!("the invite cannot be used: {e:?}"))?;

    let mut secure = connect_to_host(&invite, identity, events, cancel)?;
    check_cancel(cancel)?;
    let seed = Seed::random();
    let mut handshake = Guest::new(seed, commit(&seed, &deck));
    guest_handshake(&mut secure, &mut handshake, identity, &invite, now())
        .map_err(|e| e.to_string())?;
    check_cancel(cancel)?;
    secure
        .inner_mut()
        .set_io_timeout(None)
        .map_err(|e| e.to_string())?;
    let _ = events.send(MatchEvent::Status(format!(
        "connected to {}; sending your deck…",
        invite.host_key.fingerprint().to_grouped_string()
    )));

    play_with_cancel(&mut secure, cancel, |secure| {
        guest_match(secure, deck, seat).map_err(|e| e.to_string())
    })
}

/// Leaving interrupts a blocked peer read as well as the local seat, so all owned
/// resources (including an internet tunnel) can be dropped promptly.
fn play_with_cancel<T>(
    secure: &mut NoiseChannel<WsChannel>,
    cancel: &AtomicBool,
    play: impl FnOnce(&mut NoiseChannel<WsChannel>) -> Result<T, String>,
) -> Result<T, String> {
    let socket = secure
        .inner_mut()
        .shutdown_handle()
        .map_err(|e| e.to_string())?;
    std::thread::scope(|scope| {
        let (finished, waiting) = std::sync::mpsc::channel::<()>();
        scope.spawn(move || {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    let _ = socket.shutdown(std::net::Shutdown::Both);
                    break;
                }
                match waiting.recv_timeout(Duration::from_millis(100)) {
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
        });
        let result = play(secure);
        drop(finished);
        check_cancel(cancel)?;
        result
    })
}

/// Only an authenticated endpoint counts as reaching the invited host. No Join is
/// sent while trying addresses, so a failed attempt cannot consume the invite.
fn connect_to_host(
    invite: &Invite,
    identity: &Identity,
    events: &Sender<MatchEvent>,
    cancel: &AtomicBool,
) -> Result<NoiseChannel<WsChannel>, String> {
    check_cancel(cancel)?;
    let mut last_error = String::from("the invite lists no direct address");
    for endpoint in &invite.endpoints {
        check_cancel(cancel)?;
        invite
            .verify(now())
            .map_err(|e| format!("the invite cannot be used: {e:?}"))?;
        let (address, connection) = match endpoint {
            Endpoint::Direct { host, port } => {
                let address = format!("{host}:{port}");
                let _ = events.send(MatchEvent::Status("connecting to the host…".into()));
                (address, WsChannel::connect(host, *port))
            }
            Endpoint::Tunnel { url } => {
                let _ = events.send(MatchEvent::Status(
                    "connecting through the internet tunnel…".into(),
                ));
                (url.to_string(), WsChannel::connect_url(url))
            }
            Endpoint::Mdns { instance } => {
                let _ = events.send(MatchEvent::Status("looking for the host nearby…".into()));
                let found = resolve_nearby(instance, cancel)?;
                let Some((address, connection)) = found else {
                    continue;
                };
                (address, Ok(connection))
            }
            _ => continue,
        };
        let attempt = connection.and_then(|plain| {
            if cancel.load(Ordering::Relaxed) {
                return Err(WireError::Refused("cancelled".into()));
            }
            let _ = events.send(MatchEvent::Status("securing the connection…".into()));
            plain.set_io_timeout(Some(SETUP_IO_TIMEOUT))?;
            NoiseChannel::initiate(plain, identity, invite.host_key)
        });
        check_cancel(cancel)?;
        match attempt {
            Ok(channel) => return Ok(channel),
            Err(error) => last_error = format!("{address}: {error}"),
        }
    }
    Err(format!("could not reach the host ({last_error})"))
}

fn resolve_nearby(
    instance: &str,
    cancel: &AtomicBool,
) -> Result<Option<(String, WsChannel)>, String> {
    let Ok(mut browser) = mtg_net::discovery::Browser::new() else {
        return Ok(None);
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        check_cancel(cancel)?;
        browser.poll();
        if let Some(game) = browser.find(instance) {
            for address in &game.addresses {
                check_cancel(cancel)?;
                let connection = WsChannel::connect(&address.to_string(), game.port);
                if let Ok(connection) = connection {
                    return Ok(Some((format!("{address}:{}", game.port), connection)));
                }
            }
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(None)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_net::wire::{Channel, TypedChannel};

    #[test]
    fn first_use_saves_a_consistent_identity_and_reuses_it() {
        let store = Store::in_memory().unwrap();
        let first = identity_from_store(&store).unwrap();
        let saved = store.identity().unwrap().unwrap();
        assert_eq!(saved.seed, first.to_backup());
        assert_eq!(saved.public, first.public().0);
        assert_eq!(
            identity_from_store(&store).unwrap().public(),
            first.public()
        );
        assert_eq!(store.identity().unwrap(), Some(saved));
    }

    #[test]
    fn mismatched_saved_identity_is_reported_and_preserved() {
        let store = Store::in_memory().unwrap();
        let actual = Identity::from_seed(&[7; 32]);
        let mut saved = StoredIdentity {
            seed: actual.to_backup(),
            public: actual.public().0,
        };
        saved.public[0] ^= 1;
        store.put_identity(&saved).unwrap();
        assert!(
            identity_from_store(&store)
                .err()
                .unwrap()
                .contains("does not match")
        );
        assert_eq!(store.identity().unwrap(), Some(saved));
    }

    #[test]
    fn cancelled_join_stops_before_parsing_or_connecting() {
        let (events, receiver) = std::sync::mpsc::channel();
        let result = join_with_cancel(
            "not a link",
            DeckSpec::default(),
            &Identity::generate(),
            &mut crate::seat::BotSeat::default(),
            &events,
            &AtomicBool::new(true),
        );
        assert!(matches!(result, Err(error) if error == "cancelled"));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn cancellation_during_setup_prevents_trying_the_next_address() {
        let first = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let second = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        second.set_nonblocking(true).unwrap();
        let identity = Identity::generate();
        let invite = Invite::issue(
            &identity,
            SessionId::random(),
            vec![
                Endpoint::Direct {
                    host: "127.0.0.1".into(),
                    port: first.local_addr().unwrap().port(),
                },
                Endpoint::Direct {
                    host: "127.0.0.1".into(),
                    port: second.local_addr().unwrap().port(),
                },
            ],
            now() + 60,
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let peer_cancel = Arc::clone(&cancel);
        let server = std::thread::spawn(move || {
            let channel = WsChannel::accept(&first).unwrap();
            peer_cancel.store(true, Ordering::Relaxed);
            drop(channel);
        });
        let (events, _receiver) = std::sync::mpsc::channel();
        let result = connect_to_host(&invite, &Identity::generate(), &events, &cancel);
        assert!(matches!(result, Err(error) if error == "cancelled"));
        server.join().unwrap();
        assert!(matches!(second.accept(), Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock));
    }

    fn identity_server(identity: Identity) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let channel = WsChannel::accept(&listener).unwrap();
            channel
                .set_io_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            NoiseChannel::respond(channel, &identity).unwrap();
        });
        (port, worker)
    }

    #[test]
    fn leaving_interrupts_a_blocked_peer_read() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = Identity::generate();
        let host_key = host.public();
        let (release, waiting) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let plain = WsChannel::accept(&listener).unwrap();
            plain.set_io_timeout(Some(Duration::from_secs(2))).unwrap();
            let _secure = NoiseChannel::respond(plain, &host).unwrap();
            let _ = waiting.recv_timeout(Duration::from_secs(3));
        });
        let plain = WsChannel::connect("127.0.0.1", port).unwrap();
        plain.set_io_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut secure = NoiseChannel::initiate(plain, &Identity::generate(), host_key).unwrap();
        secure.inner_mut().set_io_timeout(None).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancel);
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            trigger.store(true, Ordering::Relaxed);
        });
        let start = std::time::Instant::now();
        let result = play_with_cancel(&mut secure, &cancel, |secure| {
            secure.recv().map_err(|e| e.to_string())
        });
        drop(release);
        server.join().unwrap();
        canceller.join().unwrap();
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "leave should not wait for the silent opponent"
        );
    }

    #[test]
    #[ignore = "requires NGROK_AUTHTOKEN and connects to the public ngrok service"]
    fn a_live_ngrok_tunnel_plays_a_complete_match() {
        let token = std::env::var("NGROK_AUTHTOKEN")
            .expect("set NGROK_AUTHTOKEN to run this explicit live test");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = Arc::new(AtomicBool::new(false));
        let tunnel = crate::tunnel::Tunnel::start(
            port,
            &token,
            std::env::var("NGROK_DOMAIN").ok().as_deref(),
            &cancel,
        )
        .unwrap();
        // Only the public tunnel is advertised, so LAN fallback cannot mask a failure.
        let identity = Identity::generate();
        let invite = Invite::issue(
            &identity,
            SessionId::random(),
            vec![Endpoint::Tunnel {
                url: tunnel.url().into(),
            }],
            now() + 60,
        );
        let link = invite.to_url();
        listener.set_nonblocking(true).unwrap();
        let host_cancel = Arc::clone(&cancel);
        let host = std::thread::spawn(move || {
            let deck = crate::decks::demo_deck();
            let (events, _) = std::sync::mpsc::channel();
            let (mut channel, shuffle) =
                wait_for_guest(&listener, &identity, &invite, &deck, &events, &host_cancel)?;
            host_match(
                &mut channel,
                MatchSettings {
                    format: mtg_session::game::Format::Constructed,
                    best_of: 1,
                },
                deck,
                &crate::decks::LocalSource::new(None),
                shuffle,
                &mut crate::seat::BotSeat::default(),
            )
            .map_err(|e| e.to_string())
        });
        let (events, _) = std::sync::mpsc::channel();
        let guest = join(
            &link,
            crate::decks::demo_deck(),
            &Identity::generate(),
            &mut crate::seat::BotSeat::default(),
            &events,
        );
        cancel.store(true, Ordering::Relaxed);
        let hosted = host.join().unwrap();
        let guest = guest.expect("guest played through the public tunnel");
        let hosted = hosted.expect("host played through the public tunnel");
        assert_eq!(hosted.score, guest.score);
        assert_eq!(hosted.winner, guest.winner);
        drop(tunnel);
    }

    #[test]
    fn a_tunnel_authenticates_the_invited_host() {
        let host = Identity::generate();
        let issuer = Identity::from_seed(&host.to_backup());
        let (port, server) = identity_server(host);
        let invite = Invite::issue(
            &issuer,
            SessionId::random(),
            vec![Endpoint::Tunnel {
                url: format!("ws://127.0.0.1:{port}/").into(),
            }],
            now() + 60,
        );
        let (events, _) = std::sync::mpsc::channel();
        let channel = connect_to_host(
            &invite,
            &Identity::generate(),
            &events,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(channel.peer(), issuer.public());
        server.join().unwrap();
    }

    #[test]
    fn an_impostor_tunnel_is_rejected_before_a_direct_fallback() {
        let host = Identity::generate();
        let issuer = Identity::from_seed(&host.to_backup());
        let (wrong_port, wrong_server) = identity_server(Identity::generate());
        let (port, server) = identity_server(host);
        let invite = Invite::issue(
            &issuer,
            SessionId::random(),
            vec![
                Endpoint::Tunnel {
                    url: format!("ws://127.0.0.1:{wrong_port}/").into(),
                },
                Endpoint::Direct {
                    host: "127.0.0.1".into(),
                    port,
                },
            ],
            now() + 60,
        );
        let (events, _) = std::sync::mpsc::channel();
        let channel = connect_to_host(
            &invite,
            &Identity::generate(),
            &events,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(channel.peer(), issuer.public());
        wrong_server.join().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn join_tries_the_next_address_after_an_impostor() {
        let host = Identity::generate();
        let invite_identity = Identity::from_seed(&host.to_backup());
        let (wrong_port, wrong_server) = identity_server(Identity::generate());
        let (host_port, host_server) = identity_server(host);
        let invite = Invite::issue(
            &invite_identity,
            SessionId::random(),
            vec![
                Endpoint::Direct {
                    host: "127.0.0.1".into(),
                    port: wrong_port,
                },
                Endpoint::Direct {
                    host: "127.0.0.1".into(),
                    port: host_port,
                },
            ],
            now() + 60,
        );
        let (events, _receiver) = std::sync::mpsc::channel();
        let channel = connect_to_host(
            &invite,
            &Identity::generate(),
            &events,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(channel.peer(), invite.host_key);
        wrong_server.join().unwrap();
        host_server.join().unwrap();
    }

    #[test]
    fn an_impostor_is_never_returned_as_the_invited_host() {
        let identity = Identity::generate();
        let (port, server) = identity_server(Identity::generate());
        let invite = Invite::issue(
            &identity,
            SessionId::random(),
            vec![Endpoint::Direct {
                host: "127.0.0.1".into(),
                port,
            }],
            now() + 60,
        );
        let (events, _receiver) = std::sync::mpsc::channel();
        let result = connect_to_host(
            &invite,
            &Identity::generate(),
            &events,
            &AtomicBool::new(false),
        );
        assert!(matches!(result, Err(error) if error.contains("not the host")));
        server.join().unwrap();
    }

    #[test]
    fn an_invite_without_direct_addresses_has_a_clear_error() {
        let identity = Identity::generate();
        let invite = Invite::issue(&identity, SessionId::random(), Vec::new(), now() + 60);
        let (events, _receiver) = std::sync::mpsc::channel();
        let result = connect_to_host(
            &invite,
            &Identity::generate(),
            &events,
            &AtomicBool::new(false),
        );
        assert!(matches!(result, Err(error) if error.contains("no direct address")));
    }

    fn waiting_host() -> (u16, Invite, std::thread::JoinHandle<Result<Seed, String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let identity = Identity::generate();
        let invite = Invite::issue(&identity, SessionId::random(), Vec::new(), now() + 60);
        let host_invite = invite.clone();
        let worker = std::thread::spawn(move || {
            let (events, _receiver) = std::sync::mpsc::channel();
            wait_for_guest(
                &listener,
                &identity,
                &host_invite,
                &DeckSpec::default(),
                &events,
                &AtomicBool::new(false),
            )
            .map(|(_, seed)| seed)
        });
        (port, invite, worker)
    }

    fn connect_guest(port: u16, invite: &Invite, identity: &Identity) -> NoiseChannel<WsChannel> {
        let plain = WsChannel::connect("127.0.0.1", port).unwrap();
        plain.set_io_timeout(Some(Duration::from_secs(2))).unwrap();
        NoiseChannel::initiate(plain, identity, invite.host_key).unwrap()
    }

    fn join_guest(port: u16, invite: &Invite) -> Seed {
        let identity = Identity::generate();
        let mut channel = connect_guest(port, invite, &identity);
        let seed = Seed::random();
        let mut guest = Guest::new(seed, commit(&seed, &DeckSpec::default()));
        guest_handshake(&mut channel, &mut guest, &identity, invite, now()).unwrap()
    }

    #[test]
    fn invalid_noise_attempt_does_not_close_the_lobby() {
        let (port, invite, host) = waiting_host();
        let mut invalid = WsChannel::connect("127.0.0.1", port).unwrap();
        invalid
            .set_io_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        invalid.send(b"invalid Noise handshake").unwrap();
        assert!(invalid.recv().is_err());
        let guest_seed = join_guest(port, &invite);
        assert_eq!(host.join().unwrap().unwrap(), guest_seed);
    }

    #[test]
    fn rejected_invite_does_not_consume_the_open_lobby() {
        let (port, invite, host) = waiting_host();
        let identity = Identity::generate();
        let mut invalid = connect_guest(port, &invite, &identity);
        let other_invite = Invite::issue(&identity, SessionId::random(), Vec::new(), now() + 60);
        let seed = Seed::random();
        let mut guest = Guest::new(seed, commit(&seed, &DeckSpec::default()));
        assert!(matches!(
            guest_handshake(&mut invalid, &mut guest, &identity, &other_invite, now()),
            Err(WireError::Refused(_))
        ));
        drop(invalid);
        let guest_seed = join_guest(port, &invite);
        assert_eq!(host.join().unwrap().unwrap(), guest_seed);
    }

    #[test]
    fn a_forged_lobby_identity_is_refused_without_consuming_the_invite() {
        let (port, invite, host) = waiting_host();
        let transport_identity = Identity::generate();
        let forged_identity = Identity::generate();
        let mut channel = connect_guest(port, &invite, &transport_identity);
        let seed = Seed::random();
        let mut guest = Guest::new(seed, commit(&seed, &DeckSpec::default()));
        let result = guest_handshake(&mut channel, &mut guest, &forged_identity, &invite, now());
        assert!(matches!(result, Err(WireError::Refused(error))
            if error.contains("does not match the authenticated peer")));
        drop(channel);
        let guest_seed = join_guest(port, &invite);
        assert_eq!(host.join().unwrap().unwrap(), guest_seed);
    }

    #[test]
    fn disconnect_after_accepted_join_does_not_reopen_the_invite() {
        let (port, invite, host) = waiting_host();
        let identity = Identity::generate();
        let mut channel = connect_guest(port, &invite, &identity);
        let seed = Seed::random();
        let mut guest = Guest::new(seed, commit(&seed, &DeckSpec::default()));
        let join = guest.join(&identity, &invite, now()).unwrap();
        channel.send_msg(&join).unwrap();
        let accepted: mtg_net::session::HostMessage = channel.recv_msg().unwrap();
        assert!(matches!(
            accepted,
            mtg_net::session::HostMessage::Accepted { .. }
        ));
        drop(channel);
        assert!(host.join().unwrap().is_err());
    }

    #[test]
    fn cancelled_lobby_returns_before_accepting() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let identity = Identity::generate();
        let invite = Invite::issue(&identity, SessionId::random(), Vec::new(), now() + 60);
        let (events, _receiver) = std::sync::mpsc::channel();
        let result = wait_for_guest(
            &listener,
            &identity,
            &invite,
            &DeckSpec::default(),
            &events,
            &AtomicBool::new(true),
        );
        assert!(matches!(result, Err(message) if message == "cancelled"));
    }
}
