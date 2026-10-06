//! Owned ngrok tunnel. The embedded agent runs only while its hosted match exists.

use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use ngrok::{
    Session,
    config::Scheme,
    forwarder::Forwarder,
    prelude::{EndpointInfo, ForwarderBuilder, TunnelCloser},
    tunnel::HttpTunnel,
};
use tokio::runtime::Runtime;

pub struct Tunnel {
    runtime: Option<Runtime>,
    forwarder: Forwarder<HttpTunnel>,
    session: Session,
    url: String,
}

/// Startup is bounded and cancellable even when the provider is unreachable.
async fn during_startup<T>(
    future: impl Future<Output = Result<T, String>>,
    cancel: &AtomicBool,
) -> Result<T, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let cancelled = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
    };
    tokio::select! {
        biased;
        _ = cancelled => Err("cancelled".into()),
        result = tokio::time::timeout(Duration::from_secs(30), future) =>
            result.map_err(|_| "Internet hosting timed out. Check your connection and try again.".to_owned())?,
    }
}

impl Tunnel {
    pub fn start(
        port: u16,
        token: &str,
        domain: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err("Paste your ngrok authtoken in hosting setup first.".into());
        }
        // The SDK needs aws-lc compiled in, while card downloads use ring. With both
        // features enabled rustls needs an explicit process default for SDK builders.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let domain = domain
            .filter(|domain| !domain.trim().is_empty())
            .map(normalize_domain)
            .transpose()?;
        let startup = async {
            let session = Session::builder().authtoken(token.trim()).connect().await
                .map_err(|_| "Could not connect to ngrok. Check your token, connection, and ngrok account status.".to_owned())?;
            let upstream =
                url::Url::parse(&format!("http://127.0.0.1:{port}")).map_err(|e| e.to_string())?;
            let mut endpoint = session.http_endpoint();
            endpoint.scheme(Scheme::HTTPS);
            if let Some(domain) = &domain {
                endpoint.domain(domain);
            }
            let forwarder = endpoint
                .listen_and_forward(upstream).await
                .map_err(|_| "Ngrok could not open a tunnel. Check your account limits and close any other tunnel using this account. If your account requires a domain, enter its assigned domain under Advanced connection settings.".to_owned())?;
            let url = websocket_url(forwarder.url())?;
            Ok((session, forwarder, url))
        };
        // Provider errors are deliberately summarized: SDK errors may contain credentials.
        let (session, forwarder, url) = runtime.block_on(during_startup(startup, cancel))?;
        Ok(Self {
            runtime: Some(runtime),
            forwarder,
            session,
            url,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn is_running(&mut self) -> bool {
        !self.forwarder.join().is_finished()
    }
}

/// Accept either the dashboard hostname or its HTTPS URL, without routing or credentials.
fn normalize_domain(input: &str) -> Result<String, String> {
    let input = input.trim();
    let address = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let url = url::Url::parse(&address)
        .map_err(|_| "Enter your ngrok domain from the dashboard.".to_owned())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Enter only your ngrok domain or its HTTPS address.".into());
    }
    url.host_str()
        .map(str::to_owned)
        .ok_or_else(|| "The ngrok domain is missing.".into())
}

fn websocket_url(public_url: &str) -> Result<String, String> {
    let mut url = url::Url::parse(public_url)
        .map_err(|_| "Ngrok returned an invalid tunnel address.".to_owned())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Ngrok did not return a secure tunnel address.".into());
    }
    url.set_scheme("wss")
        .map_err(|_| "Invalid tunnel scheme.".to_owned())?;
    Ok(url.into())
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.block_on(async {
                let _ = tokio::time::timeout(Duration::from_secs(2), async {
                    let _ = self.forwarder.close().await;
                    let _ = self.session.close().await;
                })
                .await;
                self.forwarder.join().abort();
            });
            runtime.shutdown_timeout(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_preserves_routing_and_requires_tls() {
        assert_eq!(
            websocket_url("https://game.example/path?q=1").unwrap(),
            "wss://game.example/path?q=1"
        );
        for invalid in [
            "http://game.example",
            "tcp://game.example:123",
            "https://secret@game.example",
            "garbage",
        ] {
            assert!(websocket_url(invalid).is_err());
        }
    }

    #[test]
    fn dashboard_domains_accept_https_and_reject_credentials_or_routing() {
        for input in ["my-game.ngrok-free.app", "https://my-game.ngrok-free.app/"] {
            assert_eq!(normalize_domain(input).unwrap(), "my-game.ngrok-free.app");
        }
        for input in [
            "https://secret@game.example",
            "http://game.example",
            "https://game.example/path",
            "game.example?q=1",
        ] {
            assert!(normalize_domain(input).is_err());
        }
    }

    #[test]
    fn cancellation_interrupts_a_stalled_provider() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let trigger = std::sync::Arc::clone(&cancel);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            trigger.store(true, Ordering::Relaxed);
        });
        let result = runtime.block_on(during_startup(
            std::future::pending::<Result<(), String>>(),
            &cancel,
        ));
        assert_eq!(result.unwrap_err(), "cancelled");
        worker.join().unwrap();
    }
}
