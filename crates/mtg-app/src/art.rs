//! Card images: resolved by name, fetched once from Scryfall's CDN, cached on disk.
//!
//! The UI asks for a card's image by the name it is drawing. On first sight the name goes to a
//! shared worker pool, which finds a URL, downloads, decodes, caches validated bytes,
//! and hands back pixels; the UI turns those into a texture. Until then — and forever, for a card
//! with no image — the card is drawn as its text panel, so a slow or absent network changes how
//! the board looks, never whether it works.
//!
//! # Which names have images
//!
//! A name is looked up only when it is known to be a real card: either the local database has it
//! (imported from Scryfall), or the user's art-name file maps it to one. An invented card with no
//! mapping costs nothing — no request is made for a name that cannot exist.
//!
//! The art-name file is the user's, never the repository's: the project ships no card names
//! (ADR-007), so pointing the invented demo cards at real printings is a local choice.
//!
//! ```text
//! # ~/.config/mtgo_rs/art.txt — shown name = real card name
//! Training Field = Plains
//! ```
//!
//! # Mana symbols
//!
//! The same workers fetch Scryfall's symbol SVGs (`{W}`, `{T}`, `{2/U}`…) into
//! `images/symbols/` and rasterizes them. There are a few dozen in all, so after the first
//! game every one is on disk. The UI draws a plain coloured disc in the meantime.
//!
//! # Being a good citizen of someone else's CDN
//!
//! Workers share a gate that spaces requests to Scryfall's API, while CDN images can download
//! in parallel. Requests use a descriptive `User-Agent`, and valid disk caches are checked
//! before the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use mtg_store::Store;
use sha2::Digest;

/// Identifies this client to Scryfall, as their guidelines ask.
const USER_AGENT: &str = concat!("mtgo_rs/", env!("CARGO_PKG_VERSION"), " (personal client)");

/// Scryfall asks for 50–100 ms between requests to its API. Its image hosts
/// (`*.scryfall.io`) have no such limit.
const REQUEST_GAP: Duration = Duration::from_millis(100);

/// Downloads running at once. Images come from Scryfall's CDN, which is built for this; a few
/// in parallel is what makes a page of search results fill in quickly.
const WORKERS: usize = 4;

/// Where Scryfall serves symbol images, by file name (see [`crate::mana_text::symbol_file`]).
const SYMBOL_BASE: &str = "https://svgs.scryfall.io/card-symbols/";

/// Symbols are rasterized at this many pixels square: sharp at twice the size they are drawn.
const SYMBOL_PIXELS: u32 = 64;

/// An image larger than this is not a card image.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Bound decoded card textures as well as their compressed downloads.
const MAX_IMAGE_DIMENSION: u32 = 2048;
const MAX_DECODE_BYTES: u64 = 16 * 1024 * 1024;

/// The user's mapping from shown names to real card names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtNames {
    map: BTreeMap<String, String>,
}

impl ArtNames {
    /// Parse `shown = real` lines. Blank lines and `#` comments are ignored, as is any line
    /// without an `=` — a typo should lose one mapping, not the file.
    pub fn parse(text: &str) -> Self {
        let map = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(shown, real)| (shown.trim().to_string(), real.trim().to_string()))
            .filter(|(shown, real)| !shown.is_empty() && !real.is_empty())
            .collect();
        Self { map }
    }

    /// Read the file, or nothing if there is none: having no mappings is the normal case.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// `$XDG_CONFIG_HOME/mtgo_rs/art.txt`, or `~/.config/mtgo_rs/art.txt`.
    pub fn default_path() -> PathBuf {
        xdg_dir("XDG_CONFIG_HOME", ".config")
            .join("mtgo_rs")
            .join("art.txt")
    }

    /// The real name a shown name is mapped to, if any.
    pub fn real_name(&self, shown: &str) -> Option<&str> {
        self.map.get(shown).map(String::as_str)
    }
}

/// `$XDG_CACHE_HOME/mtgo_rs/images`, or `~/.cache/mtgo_rs/images`.
pub fn default_cache_dir() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache")
        .join("mtgo_rs")
        .join("images")
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(fallback)
    })
}

/// The cache file for a complete real card name, with a stable bounded filename.
/// A digest preserves distinctions that punctuation replacement would collapse.
pub fn cache_file(dir: &Path, real_name: &str) -> PathBuf {
    let digest = sha2::Sha256::digest(real_name.as_bytes());
    let mut filename = String::with_capacity(68);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        filename.push(HEX[usize::from(byte >> 4)] as char);
        filename.push(HEX[usize::from(byte & 15)] as char);
    }
    filename.push_str(".jpg");
    dir.join(filename)
}

/// Scryfall's by-name image endpoint: redirects to the CDN image for the card's default
/// printing. Used when the database has no URL for a name, e.g. before any import.
pub fn named_image_url(real_name: &str) -> String {
    format!(
        "https://api.scryfall.com/cards/named?exact={}&format=image&version=normal",
        percent_encode(real_name)
    )
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Where to download a shown name's image from, or `None` when it has no image to find.
///
/// Pure, so the "never ask the network about an invented card" rule is testable: a URL comes
/// back only if the database knows the real name or the user mapped it.
pub fn source_url(
    mapped: bool,
    db_uri: Option<String>,
    db_knows: bool,
    real_name: &str,
) -> Option<String> {
    match db_uri {
        Some(uri) => Some(uri),
        None if mapped || db_knows => Some(named_image_url(real_name)),
        None => None,
    }
}

/// Where the worker looks things up and keeps what it downloads.
pub struct ArtConfig {
    pub names: ArtNames,
    /// The local card database, if one has been imported.
    pub db: Option<PathBuf>,
    pub cache_dir: PathBuf,
}

impl ArtConfig {
    /// The standard locations. A database that does not exist is not created.
    pub fn standard() -> Self {
        let db = mtg_store::default_path();
        Self {
            names: ArtNames::load(&ArtNames::default_path()),
            db: db.exists().then_some(db),
            cache_dir: default_cache_dir(),
        }
    }
}

enum Slot {
    Pending,
    Ready(egui::TextureHandle),
    /// No image exists, or it could not be fetched. Not retried this session.
    Missing,
}

/// Something the worker can fetch.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Art {
    /// A card, by the name the UI shows.
    Card(String),
    /// A mana symbol, by its code without braces: `"W"`, `"2/U"`.
    Symbol(String),
}

/// Work waiting for a downloader. A stack, not a queue: the newest request is for what is on
/// screen now, so it goes first, and a page scrolled past no longer holds up the one in view.
#[derive(Default)]
struct Jobs {
    pending: std::sync::Mutex<(Vec<Art>, bool)>,
    wake: std::sync::Condvar,
}

impl Jobs {
    fn push(&self, art: Art) {
        if let Ok(mut guard) = self.pending.lock() {
            guard.0.push(art);
            self.wake.notify_one();
        }
    }

    /// The newest job, waiting for one; `None` once closed.
    fn pop(&self) -> Option<Art> {
        let mut guard = self.pending.lock().ok()?;
        loop {
            if guard.1 {
                return None;
            }
            if let Some(art) = guard.0.pop() {
                return Some(art);
            }
            guard = self.wake.wait(guard).ok()?;
        }
    }

    fn close(&self) {
        if let Ok(mut guard) = self.pending.lock() {
            guard.1 = true;
            self.wake.notify_all();
        }
    }
}

/// Spaces out requests to Scryfall's API across every downloader.
#[derive(Default)]
struct ApiGate(std::sync::Mutex<Option<std::time::Instant>>);

impl ApiGate {
    fn wait_turn(&self, url: &str) {
        if !url.contains("api.scryfall.com") {
            return;
        }
        if let Ok(mut last) = self.0.lock() {
            if let Some(then) = *last {
                let since = then.elapsed();
                if since < REQUEST_GAP {
                    std::thread::sleep(REQUEST_GAP - since);
                }
            }
            *last = Some(std::time::Instant::now());
        }
    }
}

/// The UI's side: textures by what they depict, and the work list the downloaders share.
pub struct CardArt {
    jobs: std::sync::Arc<Jobs>,
    ready: Receiver<(Art, Option<egui::ColorImage>)>,
    slots: BTreeMap<Art, Slot>,
}

impl CardArt {
    /// Start the worker. `ctx` is used to wake the window when an image arrives.
    pub fn start(ctx: egui::Context, config: ArtConfig) -> Self {
        let jobs = std::sync::Arc::new(Jobs::default());
        let (done, ready) = std::sync::mpsc::channel();
        let config = std::sync::Arc::new(config);
        let gate = std::sync::Arc::new(ApiGate::default());

        for i in 0..WORKERS {
            let (jobs, done, ctx, config, gate) = (
                jobs.clone(),
                done.clone(),
                ctx.clone(),
                config.clone(),
                gate.clone(),
            );
            std::thread::Builder::new()
                .name(format!("card-art-{i}"))
                .spawn(move || worker(&config, &jobs, &gate, done, ctx))
                .expect("spawn a card art thread");
        }

        Self {
            jobs,
            ready,
            slots: BTreeMap::new(),
        }
    }

    /// The texture for a shown card name, if it has arrived. Asks for it the first time.
    pub fn get(&mut self, ctx: &egui::Context, name: &str) -> Option<&egui::TextureHandle> {
        self.request(ctx, Art::Card(name.to_string()))
    }

    /// The texture for a mana symbol code (`"G"`, `"W/U"`), if it has arrived.
    pub fn symbol(&mut self, ctx: &egui::Context, code: &str) -> Option<&egui::TextureHandle> {
        self.request(ctx, Art::Symbol(code.to_string()))
    }

    fn request(&mut self, ctx: &egui::Context, art: Art) -> Option<&egui::TextureHandle> {
        while let Ok((arrived, image)) = self.ready.try_recv() {
            let slot = match image {
                Some(image) => {
                    let name = format!("{arrived:?}");
                    Slot::Ready(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
                }
                None => Slot::Missing,
            };
            self.slots.insert(arrived, slot);
        }

        if !self.slots.contains_key(&art) {
            self.jobs.push(art.clone());
            self.slots.insert(art.clone(), Slot::Pending);
        }

        match self.slots.get(&art) {
            Some(Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }
}

impl Drop for CardArt {
    fn drop(&mut self) {
        self.jobs.close();
    }
}

fn worker(
    config: &ArtConfig,
    jobs: &Jobs,
    gate: &ApiGate,
    done: Sender<(Art, Option<egui::ColorImage>)>,
    ctx: egui::Context,
) {
    // Opened here, not on the UI thread: a lookup is I/O, and the connection never needs sharing.
    let store = config.db.as_deref().and_then(|p| Store::open(p).ok());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();

    while let Some(job) = jobs.pop() {
        let image = match &job {
            Art::Card(shown) => load(config, store.as_ref(), &agent, gate, shown),
            Art::Symbol(code) => load_symbol(&config.cache_dir, &agent, gate, code),
        };
        if done.send((job, image)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// The image for a shown name: from a valid cache, else downloaded and decoded.
fn load(
    config: &ArtConfig,
    store: Option<&Store>,
    agent: &ureq::Agent,
    gate: &ApiGate,
    shown: &str,
) -> Option<egui::ColorImage> {
    let mapped = config.names.real_name(shown);
    let real = mapped.unwrap_or(shown);

    let path = cache_file(&config.cache_dir, real);
    load_cached_image(&path, decode, || {
        let db_uri = store.and_then(|s| s.image_uri(real).ok().flatten());
        let db_knows = store.is_some_and(|s| {
            s.find_exact(real).ok().flatten().is_some()
                || s.find_by_face_name(real).ok().flatten().is_some()
        });
        let url = source_url(mapped.is_some(), db_uri, db_knows, real)?;
        gate.wait_turn(&url);
        match fetch(agent, &url) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                eprintln!("card art for {real:?}: {error}");
                None
            }
        }
    })
}

/// A symbol image: from a valid cached SVG, else downloaded and rasterized.
fn load_symbol(
    cache_dir: &Path,
    agent: &ureq::Agent,
    gate: &ApiGate,
    code: &str,
) -> Option<egui::ColorImage> {
    let file = crate::mana_text::symbol_file(code)?;
    let path = symbol_cache_file(cache_dir, &file);
    load_cached_image(
        &path,
        |svg| rasterize(svg, SYMBOL_PIXELS),
        || {
            let url = format!("{SYMBOL_BASE}{file}.svg");
            gate.wait_turn(&url);
            match fetch(agent, &url) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    eprintln!("mana symbol {{{code}}}: {error}");
                    None
                }
            }
        },
    )
}

/// Only decoded images are cache hits; invalid downloads must not poison future loads.
fn load_cached_image(
    path: &Path,
    decode_image: impl Fn(&[u8]) -> Option<egui::ColorImage>,
    download: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<egui::ColorImage> {
    use std::io::Read;
    if let Ok(file) = std::fs::File::open(path) {
        let mut bytes = Vec::new();
        if file
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_ok()
            && bytes.len() as u64 <= MAX_IMAGE_BYTES
            && let Some(image) = decode_image(&bytes)
        {
            return Some(image);
        }
    }
    let bytes = download()?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    let image = decode_image(&bytes)?;
    write_cache(path, &bytes);
    Some(image)
}

/// Where a symbol's SVG is cached, by Scryfall file name.
pub fn symbol_cache_file(cache_dir: &Path, file: &str) -> PathBuf {
    cache_dir.join("symbols").join(format!("{file}.svg"))
}

/// Written beside and renamed into place, so an interrupted write never leaves a truncated file
/// that later reads as a cached image.
fn write_cache(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_ok() {
        let serial = NEXT_WRITE.fetch_add(1, Ordering::Relaxed);
        let partial = path.with_extension(format!("{}.{}.part", std::process::id(), serial));
        let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
        else {
            return;
        };
        let written = file.write_all(bytes).is_ok();
        drop(file);
        if !written || std::fs::rename(&partial, path).is_err() {
            let _ = std::fs::remove_file(&partial);
        }
    }
}

/// Render an SVG into a square image, scaled to fit.
fn rasterize(svg: &[u8], pixels: u32) -> Option<egui::ColorImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = pixels as f32 / size.width().max(size.height());
    let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia's pixels are premultiplied, which is what this constructor expects.
    Some(egui::ColorImage::from_rgba_premultiplied(
        [pixels as usize, pixels as usize],
        pixmap.data(),
    ))
}

fn fetch(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "image/*, */*;q=0.8")
        .call()
        .map_err(|e| e.to_string())?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_IMAGE_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// Decode on the worker, so a large JPEG never costs the UI a frame.
fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_cache_entries_are_repaired_and_invalid_downloads_are_not_saved() {
        let dir = std::env::temp_dir().join(format!("mtgo-cache-repair-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = cache_file(&dir, "Repair Image");
        let corrupt = b"not an image";
        std::fs::write(&path, corrupt).unwrap();
        assert!(load_cached_image(&path, decode, || Some(b"also invalid".to_vec())).is_none());
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);

        let mut png = Vec::new();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let image = load_cached_image(&path, decode, || Some(png.clone())).unwrap();
        assert_eq!(image.size, [1, 1]);
        assert_eq!(std::fs::read(&path).unwrap(), png);
        assert!(
            load_cached_image(&path, decode, || panic!("valid cache must not download")).is_some()
        );

        // Oversized cache files are bounded before decoding and fall back to download.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        drop(file);
        let image = load_cached_image(&path, decode, || Some(png.clone())).unwrap();
        assert_eq!(image.size, [1, 1]);
        assert_eq!(std::fs::read(&path).unwrap(), png);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_cache_writes_leave_one_complete_file_and_no_partials() {
        let dir = std::env::temp_dir().join(format!(
            "mtgo-cache-writes-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = cache_file(&dir, "Shared Image");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8u8)
            .map(|byte| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    write_cache(&path, &vec![byte; 128 * 1024]);
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 128 * 1024);
        assert!(bytes[0] < 8);
        assert!(bytes.iter().all(|byte| *byte == bytes[0]));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn small_compressed_images_with_excessive_dimensions_are_rejected() {
        for (width, height) in [(MAX_IMAGE_DIMENSION + 1, 1), (1, MAX_IMAGE_DIMENSION + 1)] {
            let mut bytes = Vec::new();
            image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            assert!(bytes.len() < MAX_IMAGE_BYTES as usize);
            assert!(decode(&bytes).is_none());
        }
    }

    #[test]
    fn the_name_file_maps_shown_names_and_ignores_noise() {
        let names = ArtNames::parse(
            "# a comment\n\
             \n\
             Training Field = Real Land\n\
             no equals sign here\n\
             Rust Adder=Real Snake  \n\
             = nothing shown\n",
        );
        assert_eq!(names.real_name("Training Field"), Some("Real Land"));
        assert_eq!(names.real_name("Rust Adder"), Some("Real Snake"));
        assert_eq!(names.real_name("Siege Colossus"), None);
        assert_eq!(names.map.len(), 2);
    }

    #[test]
    fn a_missing_name_file_is_no_mappings() {
        let names = ArtNames::load(Path::new("/nonexistent/mtgo_rs/art.txt"));
        assert_eq!(names, ArtNames::default());
    }

    #[test]
    fn an_invented_unmapped_card_never_reaches_the_network() {
        assert_eq!(source_url(false, None, false, "Invented Name"), None);
    }

    #[test]
    fn a_recorded_url_is_preferred_over_the_by_name_endpoint() {
        let url = source_url(true, Some("https://cdn.test/x.jpg".into()), true, "X");
        assert_eq!(url.as_deref(), Some("https://cdn.test/x.jpg"));
    }

    #[test]
    fn a_known_card_without_a_recorded_url_falls_back_to_its_name() {
        // An import from before image URLs were stored, or a mapping with no database.
        for (mapped, knows) in [(true, false), (false, true)] {
            let url = source_url(mapped, None, knows, "Some Card").unwrap();
            assert!(
                url.starts_with("https://api.scryfall.com/cards/named?exact=Some%20Card&"),
                "{url}"
            );
        }
    }

    #[test]
    fn names_are_encoded_for_a_query_string() {
        assert_eq!(percent_encode("Fire // Ice"), "Fire%20%2F%2F%20Ice");
        assert_eq!(percent_encode("Æther Vial"), "%C3%86ther%20Vial");
    }

    #[test]
    fn a_cache_file_is_one_safe_path_component() {
        let dir = Path::new("/cache");
        let path = cache_file(dir, "Fire // Ice");
        assert_eq!(path.parent(), Some(dir));
        assert_eq!(path.file_name().unwrap().len(), 68);
        assert_eq!(cache_file(dir, "Fire // Ice"), path);
        assert_eq!(cache_file(dir, "../../etc/passwd").parent(), Some(dir));
    }

    #[test]
    fn cache_names_distinguish_punctuation_and_bound_long_unicode_names() {
        let dir = Path::new("/cache");
        assert_ne!(cache_file(dir, "Sun/Moon"), cache_file(dir, "Sun Moon"));
        assert_ne!(cache_file(dir, "Sun-Moon"), cache_file(dir, "Sun Moon"));
        let path = cache_file(dir, &"Æ".repeat(1000));
        assert_eq!(path.parent(), Some(dir));
        assert_eq!(path.file_name().unwrap().len(), 68);
    }

    #[test]
    fn a_symbol_svg_rasterizes_to_a_square_image() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <circle cx="50" cy="50" r="50" fill="#00ff00"/></svg>"##;
        let image = rasterize(svg, 32).expect("renders");
        assert_eq!(image.size, [32, 32]);
        let centre = image.pixels[16 * 32 + 16];
        assert_eq!(
            (centre.r(), centre.g(), centre.a()),
            (0, 255, 255),
            "the fill is drawn"
        );
        assert_eq!(image.pixels[0].a(), 0, "outside the circle is transparent");
    }

    #[test]
    fn a_cached_symbol_is_read_without_a_network() {
        let dir = std::env::temp_dir().join(format!("mtgo_rs-sym-test-{}", std::process::id()));
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>"#;
        write_cache(&symbol_cache_file(&dir, "WU"), svg);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(1)))
            .build()
            .into();
        assert_eq!(
            load_symbol(&dir, &agent, &ApiGate::default(), "W/U")
                .unwrap()
                .size,
            [SYMBOL_PIXELS as usize; 2]
        );
        assert_eq!(
            load_symbol(&dir, &agent, &ApiGate::default(), "../bad"),
            None,
            "never a path outside the cache"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cached_image_is_read_without_a_network() {
        let dir = std::env::temp_dir().join(format!("mtgo_rs-art-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // A 1×1 PNG, written where the cache would have put it.
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(cache_file(&dir, "Real Land"), &png).unwrap();

        let config = ArtConfig {
            names: ArtNames::parse("Training Field = Real Land"),
            db: None,
            cache_dir: dir.clone(),
        };
        // An agent that cannot connect anywhere: a cache hit must not need it.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(1)))
            .build()
            .into();

        let image = load(&config, None, &agent, &ApiGate::default(), "Training Field")
            .expect("served from the cache");
        assert_eq!(image.size, [1, 1]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[test]
    fn the_newest_request_is_served_first() {
        let jobs = Jobs::default();
        jobs.push(Art::Card("old".into()));
        jobs.push(Art::Card("new".into()));
        assert_eq!(jobs.pop(), Some(Art::Card("new".into())));
        assert_eq!(jobs.pop(), Some(Art::Card("old".into())));
    }

    #[test]
    fn a_closed_queue_releases_its_workers() {
        let jobs = std::sync::Arc::new(Jobs::default());
        let waiting = {
            let jobs = jobs.clone();
            std::thread::spawn(move || jobs.pop())
        };
        std::thread::sleep(Duration::from_millis(20));
        jobs.close();
        assert_eq!(
            waiting.join().unwrap(),
            None,
            "a worker waiting for work stops"
        );
    }

    #[test]
    fn only_the_api_is_spaced_out() {
        let gate = ApiGate::default();
        let start = std::time::Instant::now();
        for _ in 0..5 {
            gate.wait_turn("https://cards.scryfall.io/normal/front/x.jpg");
        }
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "the image CDN is not rate-limited"
        );
        gate.wait_turn("https://api.scryfall.com/cards/named?exact=X");
        gate.wait_turn("https://api.scryfall.com/cards/named?exact=Y");
        assert!(
            start.elapsed() >= REQUEST_GAP,
            "two API calls are a gap apart"
        );
    }
}
