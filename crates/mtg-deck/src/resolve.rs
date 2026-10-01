//! Turning written names into oracle ids.
//!
//! This is where imports actually break, and the failures are boringly
//! predictable: accented letters and ligatures typed as ASCII, split and
//! double-faced cards written with only one half, smart quotes from a web page,
//! and trailing whitespace. So matching is a ladder of increasingly forgiving
//! passes, and it reports *why* it matched rather than only what it matched — an
//! import that silently guessed wrong is worse than one that asks.

use mtg_core::CardId;

use crate::parse::{Entry, ParsedDeck, Section};

/// Looks names up in the local card database. Implemented by `mtg-oracle` so this
/// crate needs no database of its own and stays unit-testable.
pub trait Resolver {
    /// Exact match on the full printed name.
    fn exact(&self, name: &str) -> Option<CardId>;
    /// Match on a normalised key: lowercased, accents folded, punctuation dropped.
    fn normalised(&self, key: &str) -> Option<CardId>;
    /// Match a name that is only one face of a multi-face card.
    fn by_face_name(&self, name: &str) -> Option<CardId>;
    /// Best-effort near matches, for the "did you mean" list.
    fn suggest(&self, name: &str, limit: usize) -> Vec<(CardId, String)>;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedDeck {
    pub main: Vec<(CardId, u32)>,
    pub sideboard: Vec<(CardId, u32)>,
    pub commanders: Vec<CardId>,
    /// Entries that could not be resolved, with suggestions. The import UI shows
    /// these for the player to fix by hand; nothing is guessed.
    pub failed: Vec<ResolveError>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolveError {
    pub entry: Entry,
    pub suggestions: Vec<(CardId, String)>,
}

/// How a name was matched. Surfaced in the import summary so a player can see
/// that a card was matched loosely and check it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatchKind {
    Exact,
    Normalised,
    /// Matched on one face of a multi-face card.
    FaceName,
}

/// Fold a name into a match key: lowercase, strip accents and ligatures, drop
/// punctuation, collapse whitespace.
///
/// The ligature cases are hand-listed rather than pulled from a Unicode
/// normalisation crate. That is a deliberate trade: the full set of characters
/// that appear in card names is small and stable, and a table that can be read in
/// ten seconds beats a dependency whose behaviour has to be trusted.
pub fn normalise(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            'Æ' | 'æ' => out.push_str("ae"),
            'Œ' | 'œ' => out.push_str("oe"),
            'á' | 'à' | 'â' | 'ä' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Å' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => out.push('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => out.push('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => out.push('u'),
            'ñ' | 'Ñ' => out.push('n'),
            'ç' | 'Ç' => out.push('c'),
            // Smart quotes, as pasted from a web page.
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            c if c.is_alphanumeric() => out.extend(c.to_lowercase()),
            c if c.is_whitespace() && !out.ends_with(' ') => out.push(' '),
            c if c.is_whitespace() => {}
            // Punctuation, including the `//` of split cards, is dropped.
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Resolve a parsed list against the card database.
pub fn resolve(parsed: &ParsedDeck, db: &dyn Resolver) -> ResolvedDeck {
    let mut out = ResolvedDeck {
        main: Vec::new(),
        sideboard: Vec::new(),
        commanders: Vec::new(),
        failed: Vec::new(),
    };

    for entry in &parsed.entries {
        // The maybeboard is parsed so it does not become an error, then dropped.
        if entry.section == Section::Maybeboard {
            continue;
        }

        let hit = db
            .exact(&entry.name)
            .or_else(|| db.normalised(&normalise(&entry.name)))
            .or_else(|| {
                // `Front // Back` written when only one face is the card's name.
                entry
                    .name
                    .split("//")
                    .map(str::trim)
                    .find_map(|face| db.by_face_name(face))
            });

        match hit {
            Some(id) => match entry.section {
                Section::Main => out.main.push((id, entry.count)),
                Section::Sideboard => out.sideboard.push((id, entry.count)),
                Section::Commander | Section::Companion => out.commanders.push(id),
                Section::Maybeboard => unreachable!("filtered above"),
            },
            None => out.failed.push(ResolveError {
                entry: entry.clone(),
                suggestions: db.suggest(&entry.name, 5),
            }),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_folds_ligatures_and_accents() {
        assert_eq!(normalise("Ætherling"), "aetherling");
        assert_eq!(normalise("Lim-Dûl's Vault"), "limduls vault");
        assert_eq!(normalise("  Séance  "), "seance");
    }

    #[test]
    fn normalise_drops_the_split_card_separator() {
        assert_eq!(normalise("Front Half // Back Half"), "front half back half");
    }

    #[test]
    fn normalise_collapses_internal_whitespace() {
        assert_eq!(normalise("Ashen    Rite"), "ashen rite");
    }

    struct OnlyNormalised;
    impl Resolver for OnlyNormalised {
        fn exact(&self, _: &str) -> Option<CardId> {
            None
        }
        fn normalised(&self, key: &str) -> Option<CardId> {
            (key == "aetherling").then_some(CardId(7))
        }
        fn by_face_name(&self, _: &str) -> Option<CardId> {
            None
        }
        fn suggest(&self, _: &str, _: usize) -> Vec<(CardId, String)> {
            vec![(CardId(1), "Something Else".into())]
        }
    }

    #[test]
    fn falls_back_from_exact_to_normalised() {
        let parsed = crate::parse("4 AEtherling");
        let r = resolve(&parsed, &OnlyNormalised);
        assert_eq!(r.main, vec![(CardId(7), 4)]);
        assert!(r.failed.is_empty());
    }

    #[test]
    fn unresolvable_entries_carry_suggestions_instead_of_being_dropped() {
        let parsed = crate::parse("1 Not A Real Card");
        let r = resolve(&parsed, &OnlyNormalised);
        assert!(r.main.is_empty());
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].suggestions.len(), 1);
    }
}
