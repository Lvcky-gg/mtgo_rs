//! Importing card data.
//!
//! The client ships **no card data**. Names, rules text, types and images all come from
//! Scryfall's bulk export, fetched once into the local database (see `mtg-store`). Two
//! consequences, and the second is why this is worth the trouble: nothing in this
//! repository redistributes someone else's data, and a new set works without a release,
//! because only *behaviour* needs authoring.
//!
//! # Parsing is separate from fetching
//!
//! [`import`] reads from any [`std::io::Read`], so the whole pipeline is testable with a
//! handful of bytes and no network. Downloading is a thin edge behind the `fetch`
//! feature. That split is not only for tests: it means a user can import from a file they
//! already have, and it keeps an HTTP client out of builds that do not need one.
//!
//! # What is imported, and what is not
//!
//! Printed *characteristics* — name, cost, types, power, toughness. **Not** ability
//! behaviour: an `mtg-ir` ability tree cannot be derived from English rules text, and
//! pretending otherwise would produce cards that look playable and are not. Behaviour is
//! authored separately in `mtg-cards`, keyed by oracle id, and a card with no authored
//! behaviour is importable, searchable and deck-legal but has no abilities the engine
//! will act on. Making that gap visible is the point: see [`import::ImportReport`].

pub mod convert;
pub mod fetch;
pub mod import;
pub mod library;
pub mod manacost;
pub mod scryfall;
pub mod typeline;

pub use import::{ImportReport, import};
pub use library::CardLibrary;
pub mod compile;
