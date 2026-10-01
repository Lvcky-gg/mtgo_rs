//! The in-memory card library the engine plays from.
//!
//! SQLite is the durable store and the search index; the rules engine gets a snapshot
//! loaded into memory at startup. That split is forced by [`PrintedCards`], which returns
//! *references* — but it is the right shape anyway: characteristics are read constantly
//! (the layer system recomputes them on demand), and a query per read would make the
//! engine's hot path an I/O path.

use std::collections::BTreeMap;

use mtg_core::{CardId, Subtype};
use mtg_ir::{CardFace, PrintedCards};
use mtg_store::Store;

use crate::convert;

/// Every imported card, indexed by interned oracle id.
pub struct CardLibrary {
    /// Indexed by oracle id. Sparse ids are tolerated with empty entries rather than a
    /// map, because lookup is on the engine's hot path.
    faces: Vec<Vec<CardFace>>,
    layouts: Vec<mtg_ir::Layout>,
    subtype_names: Vec<String>,
    /// Behaviour authored for an oracle id, keyed by it. An imported card with no entry
    /// here has no abilities — see the crate docs.
    authored: BTreeMap<u32, Vec<mtg_ir::Ability>>,
    /// Faces for the tokens the cards create, numbered from
    /// [`mtg_ir::walk::TOKEN_CARD_BASE`].
    tokens: Vec<CardFace>,
}

impl CardLibrary {
    /// Load every card from the store.
    pub fn load(store: &Store) -> Result<Self, mtg_store::StoreError> {
        let subtypes = store.subtypes()?;
        let highest_subtype = subtypes.iter().map(|(id, _)| *id).max().unwrap_or(0);
        let mut subtype_names = vec![String::new(); highest_subtype as usize + 1];
        for (id, name) in subtypes {
            subtype_names[id as usize] = name;
        }

        let table = SubtypeTable {
            names: &subtype_names,
        };

        let cards = store.all_cards()?;
        let highest = cards.iter().map(|(id, _)| *id).max().unwrap_or(0);
        let mut faces = vec![Vec::new(); highest as usize + 1];

        let mut layouts = vec![mtg_ir::Layout::Unknown; highest as usize + 1];
        for (oracle, card) in cards {
            layouts[oracle as usize] = convert::to_engine_layout(&card.layout);
            faces[oracle as usize] = card
                .faces
                .iter()
                .map(|row| convert::to_engine_face(row, &table))
                .collect();
        }

        let tokens = mtg_ir::walk::register_tokens(
            faces.iter_mut().flatten(),
            mtg_ir::walk::TOKEN_CARD_BASE,
        );
        Ok(Self {
            faces,
            layouts,
            subtype_names,
            authored: BTreeMap::new(),
            tokens,
        })
    }

    /// Attach authored behaviour to a card.
    ///
    /// Import supplies printed characteristics; this supplies the abilities the engine
    /// acts on. Kept separate because one comes from Scryfall and the other is written by
    /// hand, and conflating them would make a re-import silently discard authored work.
    pub fn author(&mut self, oracle: CardId, abilities: Vec<mtg_ir::Ability>) {
        if let Some(faces) = self.faces.get_mut(oracle.0 as usize)
            && let Some(front) = faces.first_mut()
        {
            front.abilities = abilities.clone();
            let first = mtg_ir::walk::TOKEN_CARD_BASE + self.tokens.len() as u32;
            let made = mtg_ir::walk::register_tokens(std::iter::once(front), first);
            self.tokens.extend(made);
        }
        self.authored.insert(oracle.0, abilities);
    }

    /// How many cards are loaded.
    pub fn len(&self) -> usize {
        self.faces.iter().filter(|f| !f.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many loaded cards have authored behaviour.
    ///
    /// The coverage number the roadmap tracks: cards imported is how many exist, this is
    /// how many actually do anything.
    pub fn authored_count(&self) -> usize {
        self.authored.len()
    }
}

impl PrintedCards for CardLibrary {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        if let Some(i) = card.0.checked_sub(mtg_ir::walk::TOKEN_CARD_BASE) {
            return (face == 0).then(|| self.tokens.get(i as usize)).flatten();
        }
        self.faces.get(card.0 as usize)?.get(face as usize)
    }

    fn layout(&self, card: CardId) -> mtg_ir::Layout {
        if card.0 >= mtg_ir::walk::TOKEN_CARD_BASE {
            return mtg_ir::Layout::Token;
        }
        self.layouts
            .get(card.0 as usize)
            .copied()
            .unwrap_or(mtg_ir::Layout::Unknown)
    }

    fn subtype_name(&self, subtype: Subtype) -> Option<&str> {
        self.subtype_names
            .get(subtype.0 as usize)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    fn subtype_named(&self, name: &str) -> Option<Subtype> {
        let i = self.subtype_names.iter().position(|s| s == name)?;
        Some(Subtype(u16::try_from(i).ok()?))
    }
}

/// Resolves subtype names against an already-loaded table, for conversion.
struct SubtypeTable<'a> {
    names: &'a [String],
}

impl convert::Subtypes for SubtypeTable<'_> {
    fn intern(&self, name: &str) -> Option<u16> {
        self.names.iter().position(|n| n == name).map(|i| i as u16)
    }
}

/// Resolves deck-list names against the store.
///
/// The ladder is deliberately ordered from strict to forgiving, and the loose rungs exist
/// because of what actually goes wrong in pasted lists: accents typed as ASCII, ligatures,
/// smart quotes from a web page, and a double-faced card written as only its front half.
pub struct StoreResolver<'a>(pub &'a Store);

impl mtg_deck::Resolver for StoreResolver<'_> {
    fn exact(&self, name: &str) -> Option<CardId> {
        self.0.find_exact(name).ok().flatten().map(CardId)
    }

    fn normalised(&self, key: &str) -> Option<CardId> {
        self.0.find_normalised(key).ok().flatten().map(CardId)
    }

    fn by_face_name(&self, name: &str) -> Option<CardId> {
        self.0.find_by_face_name(name).ok().flatten().map(CardId)
    }

    fn suggest(&self, name: &str, limit: usize) -> Vec<(CardId, String)> {
        self.0
            .suggest(name, limit)
            .unwrap_or_default()
            .into_iter()
            .map(|(id, n)| (CardId(id), n))
            .collect()
    }
}
