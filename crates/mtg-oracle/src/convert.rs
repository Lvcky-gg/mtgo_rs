//! Turning Scryfall rows into stored rows, and stored rows into engine card faces.
//!
//! Two conversions, in opposite directions and for different reasons:
//!
//! - [`to_stored`] narrows a Scryfall object to what the database keeps.
//! - [`to_engine_face`] widens a stored row into the [`mtg_ir::CardFace`] the rules engine
//!   plays with, parsing the cost and type line on the way.
//!
//! Engine conversion compiles supported rules text into abilities. Playability is
//! checked separately by the strict compiler gate, including the card's layout.

use mtg_core::Subtype;
use mtg_ir::CardFace;
use mtg_store::{FaceRow, StoredCard};

use crate::{manacost, scryfall, typeline};

/// Narrow a Scryfall row to what the store keeps.
pub fn to_stored(card: &scryfall::Card) -> Option<StoredCard> {
    let oracle_uuid = card.oracle_id.clone()?;

    let faces = card
        .faces()
        .iter()
        .map(|f| FaceRow {
            name: f.name.clone(),
            mana_cost: f.mana_cost.clone().unwrap_or_default(),
            type_line: f.type_line.clone().unwrap_or_default(),
            oracle_text: f.oracle_text.clone(),
            power: f.power.clone(),
            toughness: f.toughness.clone(),
            loyalty: f.loyalty.clone(),
            colors: f.colors.clone().unwrap_or_default().join(""),
            image_uri: f.image_uris.as_ref().and_then(|i| i.normal.clone()),
            scryfall_id: None,
        })
        .collect();

    Some(StoredCard {
        oracle_uuid,
        name: card.name.clone(),
        layout: card.layout.clone().unwrap_or_else(|| "normal".into()),
        faces,
    })
}

/// Resolves subtype names to interned ids. Implemented by the store.
pub trait Subtypes {
    fn intern(&self, name: &str) -> Option<u16>;
}

/// Widen a stored face into the engine's representation.
///
/// Power and toughness are parsed leniently: a printed `*` or `1+*` is set by a
/// characteristic-defining ability rather than being a number, so it becomes `None` here
/// and the layer system supplies the value once the ability is authored. Storing `0` for
/// `*` would be a quiet lie the engine could not distinguish from a real zero.
pub fn to_engine_face(row: &FaceRow, subtypes: &dyn Subtypes) -> CardFace {
    let (mana_cost, _unparsed) = manacost::parse(&row.mana_cost);
    let types = typeline::parse(&row.type_line);

    let compiled = crate::compile::compile(
        &crate::compile::FaceText {
            name: &row.name,
            card_types: &types.card_types,
            subtypes: &types.subtypes,
            oracle_text: row.oracle_text.as_deref(),
            mana_cost: row.mana_cost.as_str(),
        },
        subtypes,
    );
    CardFace {
        name: row.name.clone().into_boxed_str(),
        mana_cost,
        card_types: types.card_types.clone(),
        subtypes: types
            .subtypes
            .iter()
            .filter_map(|s| subtypes.intern(s).map(Subtype))
            .collect(),
        supertypes: types.supertypes,
        power: numeric(row.power.as_deref()),
        toughness: numeric(row.toughness.as_deref()),
        loyalty: numeric(row.loyalty.as_deref()),
        // Compiled from the rules text line by line (see `compile`). A line the grammar does
        // not fully understand contributes nothing; authored behaviour, where it exists,
        // replaces all of this.
        abilities: compiled.abilities,
        oracle_text: row.oracle_text.clone().map(String::into_boxed_str),
        colors: compiled.colors,
    }
}

/// A printed number, or `None` when the printed value is not one.
fn numeric(s: Option<&str>) -> Option<i32> {
    let s = s?;
    s.parse::<i32>().ok()
}

/// Preserve face relationships without treating an unfamiliar layout as normal.
pub fn to_engine_layout(layout: &str) -> mtg_ir::Layout {
    use mtg_ir::Layout;
    match layout {
        "normal" => Layout::Normal,
        "split" => Layout::Split,
        "adventure" => Layout::Adventure,
        "transform" => Layout::Transforming,
        "modal_dfc" => Layout::ModalDfc,
        "leveler" => Layout::Leveler,
        "saga" => Layout::Saga,
        "class" => Layout::Class,
        "token" => Layout::Token,
        _ => Layout::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoSubtypes;
    impl Subtypes for NoSubtypes {
        fn intern(&self, _: &str) -> Option<u16> {
            None
        }
    }

    struct Counting;
    impl Subtypes for Counting {
        fn intern(&self, name: &str) -> Option<u16> {
            Some(name.len() as u16)
        }
    }

    fn row(type_line: &str, power: Option<&str>, toughness: Option<&str>) -> FaceRow {
        FaceRow {
            name: "Test Face".into(),
            mana_cost: "{1}{W}".into(),
            type_line: type_line.into(),
            power: power.map(str::to_string),
            toughness: toughness.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn a_creature_face_carries_its_types_and_stats() {
        let f = to_engine_face(&row("Creature — Bear", Some("2"), Some("2")), &NoSubtypes);
        assert!(f.card_types.contains(&mtg_core::CardType::Creature));
        assert_eq!((f.power, f.toughness), (Some(2), Some(2)));
        assert_eq!(f.mana_cost.mana_value(), 2);
    }

    #[test]
    fn a_star_power_becomes_none_rather_than_zero() {
        // `*` means a characteristic-defining ability sets it. Storing 0 would be
        // indistinguishable from a genuine 0/0.
        let f = to_engine_face(
            &row("Creature — Elemental", Some("*"), Some("*")),
            &NoSubtypes,
        );
        assert_eq!((f.power, f.toughness), (None, None));
    }

    #[test]
    fn a_compound_star_power_also_becomes_none() {
        let f = to_engine_face(&row("Creature — Elemental", Some("1+*"), None), &NoSubtypes);
        assert_eq!(f.power, None);
    }

    #[test]
    fn a_negative_printed_power_still_parses() {
        let f = to_engine_face(&row("Creature — Ooze", Some("-1"), Some("1")), &NoSubtypes);
        assert_eq!(f.power, Some(-1));
    }

    #[test]
    fn an_imported_face_has_no_abilities() {
        // The important honest gap: rules text is not behaviour.
        let mut r = row("Creature — Bear", Some("2"), Some("2"));
        r.oracle_text = Some("This creature does something.".into());
        let f = to_engine_face(&r, &NoSubtypes);
        assert!(
            f.abilities.is_empty(),
            "behaviour is authored, not imported"
        );
        assert!(
            f.oracle_text.is_some(),
            "but the text is kept, for display and review"
        );
    }

    #[test]
    fn subtypes_are_interned_through_the_store() {
        let f = to_engine_face(
            &row("Creature — Human Wizard", Some("1"), Some("1")),
            &Counting,
        );
        assert_eq!(f.subtypes.len(), 2, "both subtypes interned");
    }

    #[test]
    fn a_single_faced_card_yields_one_face() {
        let card = scryfall::Card {
            oracle_id: Some("abc".into()),
            name: "Single".into(),
            type_line: Some("Instant".into()),
            mana_cost: Some("{R}".into()),
            ..Default::default()
        };
        let stored = to_stored(&card).expect("importable");
        assert_eq!(stored.faces.len(), 1);
        assert_eq!(stored.faces[0].name, "Single");
    }

    #[test]
    fn a_multi_faced_card_yields_each_face() {
        let card = scryfall::Card {
            oracle_id: Some("def".into()),
            name: "Front // Back".into(),
            layout: Some("transform".into()),
            card_faces: Some(vec![
                scryfall::Face {
                    name: "Front".into(),
                    ..Default::default()
                },
                scryfall::Face {
                    name: "Back".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let stored = to_stored(&card).expect("importable");
        assert_eq!(stored.faces.len(), 2);
        assert_eq!(stored.faces[1].name, "Back");
    }

    fn uris(url: &str) -> Option<scryfall::ImageUris> {
        Some(scryfall::ImageUris {
            normal: Some(url.into()),
        })
    }

    #[test]
    fn a_single_faced_card_keeps_its_image_url() {
        let card = scryfall::Card {
            oracle_id: Some("jkl".into()),
            name: "Pictured".into(),
            image_uris: uris("https://cdn.test/pictured.jpg"),
            ..Default::default()
        };
        let stored = to_stored(&card).unwrap();
        assert_eq!(
            stored.faces[0].image_uri.as_deref(),
            Some("https://cdn.test/pictured.jpg")
        );
    }

    #[test]
    fn each_face_of_a_double_faced_card_keeps_its_own_image() {
        let card = scryfall::Card {
            oracle_id: Some("mno".into()),
            name: "Day // Night".into(),
            layout: Some("transform".into()),
            card_faces: Some(vec![
                scryfall::Face {
                    name: "Day".into(),
                    image_uris: uris("https://cdn.test/d.jpg"),
                    ..Default::default()
                },
                scryfall::Face {
                    name: "Night".into(),
                    image_uris: uris("https://cdn.test/n.jpg"),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let stored = to_stored(&card).unwrap();
        assert_eq!(
            stored.faces[0].image_uri.as_deref(),
            Some("https://cdn.test/d.jpg")
        );
        assert_eq!(
            stored.faces[1].image_uri.as_deref(),
            Some("https://cdn.test/n.jpg")
        );
    }

    #[test]
    fn faces_sharing_one_printed_image_inherit_the_cards() {
        // Split and adventure cards: one image at the top level, none per face.
        let card = scryfall::Card {
            oracle_id: Some("pqr".into()),
            name: "Hot // Cold".into(),
            layout: Some("split".into()),
            image_uris: uris("https://cdn.test/split.jpg"),
            card_faces: Some(vec![
                scryfall::Face {
                    name: "Hot".into(),
                    ..Default::default()
                },
                scryfall::Face {
                    name: "Cold".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let stored = to_stored(&card).unwrap();
        assert!(
            stored
                .faces
                .iter()
                .all(|f| f.image_uri.as_deref() == Some("https://cdn.test/split.jpg"))
        );
    }

    #[test]
    fn colors_are_flattened_to_letters() {
        let card = scryfall::Card {
            oracle_id: Some("ghi".into()),
            name: "Two Colors".into(),
            colors: Some(vec!["W".into(), "U".into()]),
            ..Default::default()
        };
        let stored = to_stored(&card).unwrap();
        assert_eq!(stored.faces[0].colors, "WU");
    }
}
