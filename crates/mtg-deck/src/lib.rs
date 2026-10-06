//! Deck import.
//!
//! Decks come in by paste. There is no collection to own cards against and no
//! trading — you build a list on Moxfield, Archidekt, Arena or wherever, copy it,
//! and paste it in. That decision removes an entire subsystem (ownership, packs,
//! trading, an economy) and changes what "legal deck" means: a deck is checked
//! against a *format*, never against what you own.
//!
//! # One parser, not five
//!
//! Every site's export is the same idea wearing different punctuation:
//!
//! ```text
//! 4 Emberwing Scout (ABC) 152         Arena, Moxfield
//! 4x Emberwing Scout                  Archidekt, TappedOut
//! 4 Emberwing Scout                   MTGGoldfish, plain text
//! SB: 4 Emberwing Scout               older tooling
//! ```
//!
//! So rather than a parser per site with a format picker the user has to get
//! right, [`parse`] accepts all of it at once: quantity prefix or suffix `x`, an
//! optional set and collector number, Archidekt's `[Category]` tags, Moxfield's
//! `*F*` foil markers, explicit section headers, and MTGGoldfish's convention of a
//! blank line before the sideboard. Pasting works without the user telling the
//! program where the text came from, which is the whole point.
//!
//! # Names are resolved separately
//!
//! [`parse`] does not touch the card database. It returns names as written, and
//! [`Resolver`] maps them to oracle ids afterwards. Keeping those apart means the
//! parser is testable with no data loaded, and name matching — which is the part
//! that actually goes wrong, thanks to accents, split cards and double-faced
//! cards — is one function with its own tests.

mod interchange;
pub mod parse;
pub mod resolve;

pub use parse::{Entry, ParsedDeck, Section, parse};
pub use resolve::{ResolveError, ResolvedDeck, Resolver};
