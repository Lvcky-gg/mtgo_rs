//! WebSocket transport.
//!
//! Behind the `transport` feature. Blocking rather than async: a two-peer game has exactly
//! one connection, so an async runtime would be a large dependency serving no concurrency
//! that exists.
//!
//! WebSocket rather than raw TCP for two reasons that both matter later: it frames messages
//! already, so there is no length-prefix protocol to get wrong, and it traverses the kind of
//! relay a NAT-ed player will need — a relay can forward WebSocket frames without knowing
//! anything about the game.

use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

use crate::wire::{Channel, WireError};

/// Bound TCP connection attempts and WebSocket upgrade I/O in either direction.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Noise messages are at most 32 KiB plus their authentication tag.
const MAX_WEBSOCKET_MESSAGE: usize = 64 * 1024;

fn websocket_config() -> tungstenite::protocol::WebSocketConfig {
    let mut config = tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(MAX_WEBSOCKET_MESSAGE);
    config.max_frame_size = Some(MAX_WEBSOCKET_MESSAGE);
    config
}

/// A [`Channel`] over a WebSocket.
pub struct WsChannel {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl WsChannel {
    /// Accept one incoming connection on an already-bound listener.
    ///
    /// Takes a listener rather than an address so the caller can bind to port 0, learn the
    /// real port, and put it in an invite — which is what a host actually needs to do.
    pub fn accept(listener: &TcpListener) -> Result<Self, WireError> {
        let (stream, _peer) = listener.accept()?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let socket = tungstenite::accept_with_config(
            MaybeTlsStream::Plain(stream),
            Some(websocket_config()),
        )
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => convert(error),
            tungstenite::HandshakeError::Interrupted(_) => WireError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "websocket handshake timed out",
            )),
        })?;
        let channel = Self { socket };
        channel.set_io_timeout(None)?;
        Ok(channel)
    }

    /// Connect directly to a host, with ten-second TCP and upgrade I/O timeouts.
    /// Hostname lookup uses the system resolver; each resolved address gets an attempt.
    pub fn connect(host: &str, port: u16) -> Result<Self, WireError> {
        Self::connect_with_timeout(host, port, HANDSHAKE_TIMEOUT)
    }

    fn connect_with_timeout(host: &str, port: u16, timeout: Duration) -> Result<Self, WireError> {
        // Brackets belong in an IPv6 URI, but the resolver expects the bare host.
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        let authority = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let url = format!("ws://{authority}/");
        let uri = url
            .parse::<tungstenite::http::Uri>()
            .map_err(|e| WireError::Malformed(e.to_string()))?;
        let addresses = (host, port).to_socket_addrs()?;
        Self::connect_addresses(uri, addresses, timeout)
    }

    /// Connect through an HTTPS tunnel, validating its TLS certificate.
    /// The path and query are retained for providers that route by URL.
    pub fn connect_url(url: &str) -> Result<Self, WireError> {
        let uri = url
            .parse::<tungstenite::http::Uri>()
            .map_err(|e| WireError::Malformed(e.to_string()))?;
        let default_port = match uri.scheme_str() {
            Some("wss") => 443,
            Some("ws") => 80,
            _ => {
                return Err(WireError::Malformed(
                    "expected a ws:// or wss:// URL".into(),
                ));
            }
        };
        let host = uri
            .host()
            .ok_or_else(|| WireError::Malformed("tunnel URL has no host".into()))?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let addresses = (host, uri.port_u16().unwrap_or(default_port)).to_socket_addrs()?;
        Self::connect_addresses(uri, addresses, HANDSHAKE_TIMEOUT)
    }

    fn stream(&self) -> &TcpStream {
        match self.socket.get_ref() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::Rustls(stream) => &stream.sock,
            _ => unreachable!("only plain and rustls transports are enabled"),
        }
    }

    fn connect_addresses(
        uri: tungstenite::http::Uri,
        addresses: impl IntoIterator<Item = SocketAddr>,
        timeout: Duration,
    ) -> Result<Self, WireError> {
        let mut last_error = WireError::Io(std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            "host resolved to no addresses",
        ));
        for address in addresses {
            let stream = match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => stream,
                Err(error) => {
                    last_error = WireError::Io(error);
                    continue;
                }
            };
            stream.set_read_timeout(Some(timeout))?;
            stream.set_write_timeout(Some(timeout))?;
            // Keep the original WebSocket: rebuilding it from the TCP stream would lose
            // application bytes buffered alongside the HTTP upgrade response.
            let mut request = uri.clone().into_client_request().map_err(convert)?;
            request
                .headers_mut()
                .insert("ngrok-skip-browser-warning", "1".parse().unwrap());
            let connector = if uri.scheme_str() == Some("wss") {
                let roots = rustls::RootCertStore::from_iter(
                    webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
                );
                let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .map_err(|e| WireError::Malformed(e.to_string()))?
                .with_root_certificates(roots)
                .with_no_client_auth();
                Some(tungstenite::Connector::Rustls(std::sync::Arc::new(config)))
            } else {
                None
            };
            let upgrade = tungstenite::client_tls_with_config(
                request,
                stream,
                Some(websocket_config()),
                connector,
            )
            .map_err(|error| match error {
                tungstenite::HandshakeError::Failure(error) => convert(error),
                tungstenite::HandshakeError::Interrupted(_) => WireError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "websocket handshake timed out",
                )),
            });
            let (socket, _) = match upgrade {
                Ok(upgraded) => upgraded,
                Err(error) => {
                    last_error = error;
                    continue;
                }
            };
            let channel = Self { socket };
            channel.set_io_timeout(None)?;
            return Ok(channel);
        }
        Err(last_error)
    }

    /// The address this channel is connected to, for logging.
    pub fn peer(&self) -> Option<std::net::SocketAddr> {
        self.stream().peer_addr().ok()
    }

    /// Bound blocking reads and writes during connection setup. Clear this before play,
    /// when a player may legitimately take longer to answer a choice.
    pub fn set_io_timeout(&self, timeout: Option<Duration>) -> Result<(), WireError> {
        self.stream().set_read_timeout(timeout)?;
        self.stream().set_write_timeout(timeout)?;
        Ok(())
    }

    /// Close politely, so the other side sees a clean end rather than a reset.
    pub fn close(&mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.flush();
    }
}

impl Channel for WsChannel {
    fn send(&mut self, frame: &[u8]) -> Result<(), WireError> {
        self.socket
            .send(Message::Binary(frame.to_vec().into()))
            .map_err(convert)?;
        self.socket.flush().map_err(convert)
    }

    fn recv(&mut self) -> Result<Vec<u8>, WireError> {
        loop {
            match self.socket.read().map_err(convert)? {
                Message::Binary(b) => return Ok(b.to_vec()),
                // Accepted for a human debugging with a generic client; the codec decides
                // whether the contents make sense.
                Message::Text(t) => return Ok(t.as_bytes().to_vec()),
                Message::Close(_) => return Err(WireError::Closed),
                // Ping/pong are handled by tungstenite; keep waiting for real content.
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            }
        }
    }
}

fn convert(e: tungstenite::Error) -> WireError {
    match e {
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            WireError::Closed
        }
        tungstenite::Error::Io(io) => WireError::Io(io),
        other => WireError::Malformed(other.to_string()),
    }
}

/// Bind a listener on an ephemeral port and report which one.
///
/// A host needs the real port before it can issue an invite, so binding and learning the
/// port are one step.
pub fn bind_ephemeral() -> Result<(TcpListener, u16), WireError> {
    let listener = TcpListener::bind(("0.0.0.0", 0))?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn read_upgrade_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 8192);
        }
        String::from_utf8(request).unwrap()
    }

    #[test]
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback fixes this type.
    fn tunnel_url_keeps_routing_and_bypasses_the_provider_browser_page() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("ws://{}/game?q=42", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(
                        request.uri().path_and_query().unwrap().as_str(),
                        "/game?q=42"
                    );
                    assert_eq!(request.headers()["ngrok-skip-browser-warning"], "1");
                    Ok(response)
                },
            )
            .unwrap();
            socket
                .send(Message::Binary(b"through tunnel".to_vec().into()))
                .unwrap();
        });
        let mut channel = WsChannel::connect_url(&url).unwrap();
        channel
            .set_io_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(channel.recv().unwrap(), b"through tunnel");
        server.join().unwrap();
    }

    #[test]
    fn secure_tunnels_attempt_tls_and_do_not_downgrade_to_plaintext() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("wss://{}/", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = [0; 5];
            stream.read_exact(&mut header).unwrap();
            assert_eq!(header[0], 0x16, "TLS handshake, never plaintext HTTP");
        });
        assert!(WsChannel::connect_url(&url).is_err());
        server.join().unwrap();
    }

    #[test]
    fn tunnel_urls_require_a_websocket_scheme() {
        for url in [
            "https://localhost/",
            "file:///tmp/game",
            "localhost:80",
            "ws:///game",
        ] {
            assert!(WsChannel::connect_url(url).is_err());
        }
    }

    #[test]
    fn another_resolved_address_is_tried_after_upgrade_rejection() {
        let rejecting = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let accepting = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addresses = [
            rejecting.local_addr().unwrap(),
            accepting.local_addr().unwrap(),
        ];
        let rejector = std::thread::spawn(move || {
            let (mut stream, _) = rejecting.accept().unwrap();
            read_upgrade_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let acceptor = std::thread::spawn(move || {
            let mut channel = WsChannel::accept(&accepting).unwrap();
            channel.send(b"second address").unwrap();
        });
        let uri = format!("ws://{}/", addresses[1]).parse().unwrap();
        let mut channel =
            WsChannel::connect_addresses(uri, addresses, Duration::from_secs(2)).unwrap();
        channel
            .set_io_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(channel.peer(), Some(addresses[1]));
        assert_eq!(channel.recv().unwrap(), b"second address");
        rejector.join().unwrap();
        acceptor.join().unwrap();
    }

    #[test]
    fn no_resolved_addresses_returns_an_address_error() {
        let result = WsChannel::connect_addresses(
            "ws://localhost:1/".parse().unwrap(),
            [],
            Duration::from_secs(2),
        );
        assert!(matches!(result, Err(WireError::Io(error))
            if error.kind() == std::io::ErrorKind::AddrNotAvailable));
    }

    #[test]
    fn a_silent_websocket_server_times_out() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let result = WsChannel::connect_with_timeout("127.0.0.1", port, Duration::from_millis(50));
        done_tx.send(()).unwrap();
        server.join().unwrap();
        assert!(matches!(result, Err(WireError::Io(error))
            if matches!(error.kind(), std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut)));
    }

    #[test]
    fn a_message_sent_with_the_upgrade_response_is_preserved() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Read exactly the HTTP request so the response and first frame can be
            // deliberately delivered together in one write.
            let request = read_upgrade_request(&mut stream);
            let key = request
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("sec-websocket-key")
                        .then_some(value.trim())
                })
                .unwrap();
            let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
            let mut response = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").into_bytes();
            response.extend_from_slice(&[0x82, 5]); // final, unmasked binary frame
            response.extend_from_slice(b"hello");
            stream.write_all(&response).unwrap();
        });
        let mut channel = WsChannel::connect("127.0.0.1", port).unwrap();
        assert_eq!(channel.stream().read_timeout().unwrap(), None);
        assert_eq!(channel.stream().write_timeout().unwrap(), None);
        channel
            .set_io_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(channel.recv().unwrap(), b"hello");
        server.join().unwrap();
    }
}
