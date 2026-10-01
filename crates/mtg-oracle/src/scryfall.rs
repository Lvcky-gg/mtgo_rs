//! The subset of Scryfall's card object this client reads.
//!
//! Deliberately partial and permissive. Every field is optional or defaulted, and unknown
//! fields are ignored, because the upstream schema grows and an import that fails on a
//! field it did not expect would break on every new set. A card that is missing something
//! important is skipped and *reported*, not fatal.

use serde::Deserialize;

/// One entry from the bulk export.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Card {
    /// The printing-independent identity. Absent on a few non-card objects, which is one
    /// of the reasons a row can be skipped.
    #[serde(default)]
    pub oracle_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,

    // Single-faced cards carry these at the top level; multi-faced ones carry them per
    // face instead, which is why both paths exist.
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,

    #[serde(default)]
    pub card_faces: Option<Vec<Face>>,

    /// Every colour in the card's cost and rules text, as letters. What Commander deck
    /// building checks against the commander's (CR 903.4).
    #[serde(default)]
    pub color_identity: Option<Vec<String>>,

    /// Format name to "legal", "not_legal", "restricted" or "banned".
    #[serde(default)]
    pub legalities: Option<std::collections::BTreeMap<String, String>>,

    /// Present at the top level for single-image cards — including split and adventure cards,
    /// whose faces share one printed image.
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
}

/// Image URLs on Scryfall's CDN. Only the size the client draws is read.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ImageUris {
    /// 488×680 JPEG: sharp at board size without the weight of the large scan.
    #[serde(default)]
    pub normal: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Face {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    /// Present per face only for cards with a separate image per face (double-faced cards).
    #[serde(default)]
    pub image_uris: Option<ImageUris>,
}

/// Why a row was not imported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Skip {
    /// No `oracle_id`: not a card.
    NoOracleId,
    /// A language other than English. The engine keys behaviour off oracle identity, and
    /// importing translations would mean many rows per identity.
    NotEnglish,
    /// A layout that is not a playable card — art cards, tokens, and the like.
    NotPlayable,
}

impl Card {
    /// Whether this row should be imported, and if not, why.
    pub fn importable(&self) -> Result<(), Skip> {
        if self.oracle_id.is_none() {
            return Err(Skip::NoOracleId);
        }
        // Absent language is treated as English: the oracle bulk file omits it.
        if self.lang.as_deref().is_some_and(|l| l != "en") {
            return Err(Skip::NotEnglish);
        }
        if let Some(layout) = self.layout.as_deref()
            && matches!(
                layout,
                "art_series"
                    | "token"
                    | "double_faced_token"
                    | "emblem"
                    | "planar"
                    | "scheme"
                    | "vanguard"
                    | "augment"
                    | "host"
            )
        {
            return Err(Skip::NotPlayable);
        }
        Ok(())
    }

    /// The faces to store: the per-face list when there is one, else the card itself.
    ///
    /// A face without its own image inherits the card's, so every face row can be drawn.
    pub fn faces(&self) -> Vec<Face> {
        match &self.card_faces {
            Some(faces) if !faces.is_empty() => faces
                .iter()
                .cloned()
                .map(|mut f| {
                    if f.image_uris.is_none() {
                        f.image_uris = self.image_uris.clone();
                    }
                    f
                })
                .collect(),
            _ => vec![Face {
                name: self.name.clone(),
                mana_cost: self.mana_cost.clone(),
                type_line: self.type_line.clone(),
                oracle_text: self.oracle_text.clone(),
                power: self.power.clone(),
                toughness: self.toughness.clone(),
                loyalty: self.loyalty.clone(),
                colors: self.colors.clone(),
                image_uris: self.image_uris.clone(),
            }],
        }
    }
}
