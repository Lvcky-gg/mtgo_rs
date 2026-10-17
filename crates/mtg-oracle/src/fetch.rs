//! Downloading the bulk export.
//!
//! Behind the `fetch` feature, because parsing and importing work from any reader and a
//! build that imports a file the user already has needs no HTTP client.
//!
//! # Being a good citizen of someone else's API
//!
//! Scryfall gives this data away and asks a few things in return, so they are honoured
//! here rather than assumed:
//!
//! - A descriptive `User-Agent`, so they can see who is calling and why.
//! - Don't re-download bulk data more than once a day. The file is regenerated daily and
//!   is hundreds of megabytes; [`needs_refresh`] enforces this against the timestamp
//!   recorded in the local database, and `--force` is the deliberate override.
//! - Stream rather than buffer, so a slow connection does not also mean a large
//!   allocation.

use std::io::Read;

use mtg_store::Store;

/// Identifies this client to Scryfall. Their guidelines ask for something descriptive
/// rather than a default library string.
#[cfg(feature = "fetch")]
const USER_AGENT: &str = concat!("mtgo_rs/", env!("CARGO_PKG_VERSION"), " (personal client)");

/// Which bulk file to use.
///
/// `oracle_cards` has one entry per *oracle identity*, which is exactly the granularity
/// the engine keys behaviour off. `default_cards` has one per printing and is several times
/// larger for no benefit here.
#[cfg(feature = "fetch")]
const BULK_TYPE: &str = "oracle_cards";

#[cfg(feature = "fetch")]
const BULK_INDEX: &str = "https://api.scryfall.com/bulk-data";

/// Seconds in a day: the minimum interval between downloads.
const REFRESH_INTERVAL: u64 = 24 * 60 * 60;

#[derive(Debug)]
pub enum FetchError {
    Http(String),
    /// The manifest had no entry of the expected type.
    NoSuchBulkType(&'static str),
    Store(mtg_store::StoreError),
    Import(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Http(e) => write!(f, "download failed: {e}"),
            FetchError::NoSuchBulkType(t) => write!(f, "no '{t}' bulk file in the manifest"),
            FetchError::Store(e) => write!(f, "{e}"),
            FetchError::Import(e) => write!(f, "import failed: {e}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Whether enough time has passed to justify downloading again.
///
/// Pure, and separate from the download, so the policy is testable without a network.
pub fn needs_refresh(last_import_unix: Option<u64>, now_unix: u64, force: bool) -> bool {
    if force {
        return true;
    }
    match last_import_unix {
        None => true,
        Some(then) => now_unix.saturating_sub(then) >= REFRESH_INTERVAL,
    }
}

/// When the local database was last populated.
pub fn last_import(store: &Store) -> Result<Option<u64>, FetchError> {
    Ok(store
        .meta("last_import_unix")
        .map_err(FetchError::Store)?
        .and_then(|value| value.parse().ok()))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Status of a bulk download/import; byte counts refer to the streamed response.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FetchProgress {
    LookingUp,
    Connecting,
    Importing {
        downloaded: u64,
        total: Option<u64>,
        processed: u64,
        imported: u64,
    },
    Saving,
}

impl FetchProgress {
    pub fn status(&self) -> String {
        match self {
            Self::LookingUp => "Checking Scryfall's bulk-data manifest…".into(),
            Self::Connecting => "Connecting to Scryfall's card download…".into(),
            Self::Importing {
                downloaded,
                total,
                processed,
                imported,
            } => {
                let received = *downloaded as f64 / 1_000_000.0;
                let bytes = match total {
                    Some(total) => format!(
                        "{received:.1} / {:.1} MB received",
                        *total as f64 / 1_000_000.0
                    ),
                    None => format!("{received:.1} MB received"),
                };
                format!("Importing cards: {imported} imported, {processed} read • {bytes}")
            }
            Self::Saving => "Saving card database…".into(),
        }
    }
}

/// Bounded batches let the network run ahead while the importer parses and writes.
/// Dropping the reader disconnects the worker, including when an import fails early.
#[cfg(feature = "fetch")]
struct DownloadReader {
    chunks: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    current: std::io::Cursor<Vec<u8>>,
}

#[cfg(feature = "fetch")]
impl DownloadReader {
    fn start<R: Read + Send + 'static>(
        mut source: R,
    ) -> (Self, std::sync::Arc<std::sync::atomic::AtomicU64>) {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
            mpsc,
        };
        // Four queued 256 KiB batches plus one at either end: <= 1.5 MiB.
        let (sender, chunks) = mpsc::sync_channel(4);
        let bytes = Arc::new(AtomicU64::new(0));
        let received = bytes.clone();
        std::thread::spawn(move || {
            loop {
                let mut chunk = vec![0; 256 * 1024];
                let mut used = 0;
                let mut failure = None;
                while used < chunk.len() {
                    match source.read(&mut chunk[used..]) {
                        Ok(0) => break,
                        Ok(n) => {
                            used += n;
                            received.fetch_add(n as u64, Ordering::Relaxed);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                chunk.truncate(used);
                if used > 0 && sender.send(Ok(chunk)).is_err() {
                    break;
                }
                if let Some(e) = failure {
                    let _ = sender.send(Err(e));
                    break;
                }
                if used == 0 {
                    break;
                }
            }
        });
        (
            Self {
                chunks,
                current: std::io::Cursor::new(Vec::new()),
            },
            bytes,
        )
    }
}

#[cfg(feature = "fetch")]
impl Read for DownloadReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.current.read(buf)?;
            if n != 0 {
                return Ok(n);
            }
            match self.chunks.recv() {
                Ok(chunk) => self.current = std::io::Cursor::new(chunk?),
                Err(_) => return Ok(0),
            }
        }
    }
}

#[cfg(feature = "fetch")]
fn download_agent() -> ureq::Agent {
    use std::time::Duration;
    ureq::Agent::config_builder()
        .timeout_resolve(Some(Duration::from_secs(10)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(10 * 60)))
        .build()
        .into()
}

/// Download the bulk export and import it, unless it was fetched recently.
///
/// Returns `None` when the download was skipped as too recent.
#[cfg(feature = "fetch")]
pub fn fetch_and_import(
    store: &mut Store,
    force: bool,
) -> Result<Option<crate::ImportReport>, FetchError> {
    fetch_and_import_with_progress(store, force, &mut |_| {})
}

/// Same refresh policy as [`fetch_and_import`], with stage and row-count updates.
#[cfg(feature = "fetch")]
pub fn fetch_and_import_with_progress(
    store: &mut Store,
    force: bool,
    progress: &mut dyn FnMut(FetchProgress),
) -> Result<Option<crate::ImportReport>, FetchError> {
    let now = now_unix();
    if store.card_count().map_err(FetchError::Store)? > 0
        && !needs_refresh(last_import(store)?, now, force)
    {
        return Ok(None);
    }

    progress(FetchProgress::LookingUp);
    let agent = download_agent();
    let uri = bulk_download_uri(&agent)?;
    progress(FetchProgress::Connecting);

    // The file is gzip-compressed JSON Lines now, and was a JSON array before; `import`
    // detects which, so accept either.
    let response = agent
        .get(&uri)
        .header("User-Agent", USER_AGENT)
        .header(
            "Accept",
            "application/gzip, application/json;q=0.9, */*;q=0.5",
        )
        .call()
        .map_err(|e| FetchError::Http(e.to_string()))?;

    // Download on a separate worker while the caller decompresses and imports.
    // The bounded queue keeps memory use independent of the export size.
    let total = response
        .headers()
        .get("Content-Length")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok());
    let (reader, bytes) = DownloadReader::start(response.into_body().into_reader());
    let report = crate::import::import_with_timestamp(
        store,
        reader,
        &mut |update| {
            use crate::import::ImportProgress;
            progress(match update {
                ImportProgress::Reading {
                    processed,
                    imported,
                } => FetchProgress::Importing {
                    downloaded: bytes.load(std::sync::atomic::Ordering::Relaxed),
                    total,
                    processed,
                    imported,
                },
                ImportProgress::Committing => FetchProgress::Saving,
            });
        },
        Some(now),
    )
    .map_err(FetchError::Import)?;

    Ok(Some(report))
}

/// Ask the manifest where the bulk file currently lives.
///
/// The URI changes whenever the file is regenerated, so it must be looked up rather than
/// hardcoded.
#[cfg(feature = "fetch")]
fn bulk_download_uri(agent: &ureq::Agent) -> Result<String, FetchError> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        data: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        #[serde(rename = "type")]
        kind: String,
        /// Gzip-compressed JSON Lines — what Scryfall serves since 2026.
        #[serde(default)]
        jsonl_download_uri: Option<String>,
        /// The original single-array file, for as long as it is still offered.
        #[serde(default)]
        download_uri: Option<String>,
    }

    let manifest: Manifest = agent
        .get(BULK_INDEX)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| FetchError::Http(e.to_string()))?
        .body_mut()
        .read_json()
        .map_err(|e| FetchError::Http(e.to_string()))?;

    manifest
        .data
        .into_iter()
        .find(|e| e.kind == BULK_TYPE)
        .and_then(|e| e.jsonl_download_uri.or(e.download_uri))
        .ok_or(FetchError::NoSuchBulkType(BULK_TYPE))
}

/// Import from a reader, recording the time so the refresh interval applies to it too.
pub fn import_from<R: Read>(
    store: &mut Store,
    reader: R,
) -> Result<crate::ImportReport, FetchError> {
    import_from_with_progress(store, reader, &mut |_| {})
}

/// File imports share progress reporting and the successful-import refresh timestamp.
pub fn import_from_with_progress<R: Read>(
    store: &mut Store,
    reader: R,
    progress: &mut dyn FnMut(crate::import::ImportProgress),
) -> Result<crate::ImportReport, FetchError> {
    crate::import::import_with_timestamp(store, reader, progress, Some(now_unix()))
        .map_err(FetchError::Import)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "fetch")]
    #[test]
    fn download_batches_preserve_bytes_and_count_network_progress() {
        let data: Vec<u8> = (0..900_000).map(|i| (i % 251) as u8).collect();
        let (mut reader, bytes) = DownloadReader::start(std::io::Cursor::new(data.clone()));
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, data);
        assert_eq!(
            bytes.load(std::sync::atomic::Ordering::Relaxed),
            data.len() as u64
        );
        assert_eq!(reader.read(&mut [0]).unwrap(), 0);
    }

    #[cfg(feature = "fetch")]
    #[test]
    fn download_runs_ahead_and_stops_when_importer_drops() {
        use std::sync::mpsc;
        use std::time::Duration;
        struct Source {
            reads: mpsc::Sender<()>,
            dropped: mpsc::Sender<()>,
        }
        impl Read for Source {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.reads.send(()).unwrap();
                buf.fill(b'x');
                Ok(buf.len())
            }
        }
        impl Drop for Source {
            fn drop(&mut self) {
                let _ = self.dropped.send(());
            }
        }
        let (reads, rx) = mpsc::channel();
        let (dropped, done) = mpsc::channel();
        let (reader, _) = DownloadReader::start(Source { reads, dropped });
        // No consumer reads: the worker fills four slots and blocks on the fifth.
        for _ in 0..5 {
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        assert!(rx.try_recv().is_err());
        drop(reader);
        done.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[cfg(feature = "fetch")]
    #[test]
    fn download_errors_reach_importer_after_buffered_bytes() {
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0 {
                    return Err(std::io::Error::other("connection lost"));
                }
                self.0 = true;
                buf[..3].copy_from_slice(b"abc");
                Ok(3)
            }
        }
        let (mut reader, _) = DownloadReader::start(Broken(false));
        let mut actual = Vec::new();
        let error = reader.read_to_end(&mut actual).unwrap_err();
        assert_eq!(actual, b"abc");
        assert_eq!(error.to_string(), "connection lost");
    }

    #[test]
    fn refresh_metadata_distinguishes_missing_values_from_database_errors() {
        let store = Store::in_memory().unwrap();
        assert_eq!(last_import(&store).unwrap(), None);
        store.set_meta("last_import_unix", "123").unwrap();
        assert_eq!(last_import(&store).unwrap(), Some(123));
        store.set_meta("last_import_unix", "invalid").unwrap();
        assert_eq!(last_import(&store).unwrap(), None);
        store.conn().execute_batch("DROP TABLE meta").unwrap();
        assert!(matches!(last_import(&store), Err(FetchError::Store(_))));
    }

    #[test]
    fn a_fresh_database_needs_a_download() {
        assert!(needs_refresh(None, 1_000_000, false));
    }

    #[test]
    fn a_recent_import_is_not_repeated() {
        // Scryfall asks callers not to re-download bulk data more than daily.
        let then = 1_000_000;
        assert!(!needs_refresh(Some(then), then + 60, false));
        assert!(!needs_refresh(
            Some(then),
            then + REFRESH_INTERVAL - 1,
            false
        ));
    }

    #[test]
    fn a_day_later_it_is_refreshed() {
        let then = 1_000_000;
        assert!(needs_refresh(Some(then), then + REFRESH_INTERVAL, false));
    }

    #[test]
    fn force_overrides_the_interval() {
        let then = 1_000_000;
        assert!(needs_refresh(Some(then), then + 1, true));
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_trigger_a_download() {
        // Saturating arithmetic: a backwards clock should not look like a year has passed.
        assert!(!needs_refresh(Some(2_000_000), 1_000_000, false));
    }
}
