//! Parsing a printed type line.
//!
//! A type line reads `Supertypes Types — Subtypes`, with an em dash. Everything before
//! the dash is a sequence of supertypes and card types; everything after is subtypes.
//! There is no ambiguity to resolve because the three vocabularies are disjoint — the
//! only real work is recognising the words and tolerating the dash being written several
//! ways.

use mtg_core::{CardType, Supertype};

/// The pieces of a type line.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ParsedTypeLine {
    pub supertypes: Vec<Supertype>,
    pub card_types: Vec<CardType>,
    /// Left as strings: subtypes are interned by the store, and new ones appear with
    /// every set, so there is nothing to gain from an enum here.
    pub subtypes: Vec<String>,
    /// Words before the dash that matched no known type. Kept rather than dropped so an
    /// import can report a card it did not fully understand.
    pub unknown: Vec<String>,
}

/// Parse a type line.
pub fn parse(line: &str) -> ParsedTypeLine {
    let mut out = ParsedTypeLine::default();

    // The dash is an em dash in Scryfall's data, but hyphens and en dashes turn up in
    // hand-written and older data, so all three are accepted.
    let (front, back) = match line.find(['—', '–']).or_else(|| line.find(" - ")) {
        Some(i) => {
            let (a, b) = line.split_at(i);
            (a, Some(b.trim_start_matches(['—', '–', '-', ' '])))
        }
        None => (line, None),
    };

    for word in front.split_whitespace() {
        if let Some(s) = supertype(word) {
            out.supertypes.push(s);
        } else if let Some(t) = card_type(word) {
            out.card_types.push(t);
        } else {
            out.unknown.push(word.to_string());
        }
    }

    if let Some(back) = back {
        out.subtypes = back.split_whitespace().map(str::to_string).collect();
    }

    out
}

fn supertype(word: &str) -> Option<Supertype> {
    Some(match word {
        "Basic" => Supertype::Basic,
        "Legendary" => Supertype::Legendary,
        "Snow" => Supertype::Snow,
        "World" => Supertype::World,
        "Ongoing" => Supertype::Ongoing,
        "Host" => Supertype::Host,
        _ => return None,
    })
}

fn card_type(word: &str) -> Option<CardType> {
    Some(match word {
        "Artifact" => CardType::Artifact,
        "Battle" => CardType::Battle,
        "Creature" => CardType::Creature,
        "Enchantment" => CardType::Enchantment,
        "Instant" => CardType::Instant,
        // Renamed from Tribal; older data still says the old word.
        "Kindred" | "Tribal" => CardType::Kindred,
        "Land" => CardType::Land,
        "Planeswalker" => CardType::Planeswalker,
        "Sorcery" => CardType::Sorcery,
        "Dungeon" => CardType::Dungeon,
        "Plane" => CardType::Plane,
        "Scheme" => CardType::Scheme,
        "Vanguard" => CardType::Vanguard,
        "Conspiracy" => CardType::Conspiracy,
        "Phenomenon" => CardType::Phenomenon,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_type() {
        let t = parse("Instant");
        assert_eq!(t.card_types, vec![CardType::Instant]);
        assert!(t.subtypes.is_empty());
        assert!(t.supertypes.is_empty());
    }

    #[test]
    fn types_and_subtypes_split_on_the_em_dash() {
        let t = parse("Creature — Human Wizard");
        assert_eq!(t.card_types, vec![CardType::Creature]);
        assert_eq!(t.subtypes, vec!["Human", "Wizard"]);
    }

    #[test]
    fn supertypes_come_first() {
        let t = parse("Legendary Creature — Angel");
        assert_eq!(t.supertypes, vec![Supertype::Legendary]);
        assert_eq!(t.card_types, vec![CardType::Creature]);
        assert_eq!(t.subtypes, vec!["Angel"]);
    }

    #[test]
    fn several_card_types_at_once() {
        let t = parse("Artifact Creature — Golem");
        assert_eq!(t.card_types, vec![CardType::Artifact, CardType::Creature]);
    }

    #[test]
    fn several_supertypes_at_once() {
        let t = parse("Legendary Snow Creature — Bear");
        assert_eq!(t.supertypes, vec![Supertype::Legendary, Supertype::Snow]);
        assert_eq!(t.card_types, vec![CardType::Creature]);
    }

    #[test]
    fn basic_lands() {
        let t = parse("Basic Land — Forest");
        assert_eq!(t.supertypes, vec![Supertype::Basic]);
        assert_eq!(t.card_types, vec![CardType::Land]);
        assert_eq!(t.subtypes, vec!["Forest"]);
    }

    #[test]
    fn the_old_name_for_kindred_still_parses() {
        assert_eq!(
            parse("Tribal Instant — Goblin").card_types[0],
            CardType::Kindred
        );
        assert_eq!(
            parse("Kindred Instant — Goblin").card_types[0],
            CardType::Kindred
        );
    }

    #[test]
    fn en_dashes_and_hyphens_are_tolerated() {
        for line in ["Creature – Elf", "Creature - Elf"] {
            let t = parse(line);
            assert_eq!(t.card_types, vec![CardType::Creature], "{line}");
            assert_eq!(t.subtypes, vec!["Elf"], "{line}");
        }
    }

    #[test]
    fn an_unrecognised_word_is_reported_rather_than_dropped() {
        let t = parse("Enchantment Wibble — Aura");
        assert_eq!(t.card_types, vec![CardType::Enchantment]);
        assert_eq!(t.unknown, vec!["Wibble"]);
        assert_eq!(t.subtypes, vec!["Aura"]);
    }

    #[test]
    fn a_planeswalker_keeps_its_subtype() {
        let t = parse("Legendary Planeswalker — Scout");
        assert_eq!(t.card_types, vec![CardType::Planeswalker]);
        assert_eq!(t.subtypes, vec!["Scout"]);
    }

    #[test]
    fn an_empty_line_parses_to_nothing() {
        let t = parse("");
        assert!(t.card_types.is_empty() && t.subtypes.is_empty() && t.unknown.is_empty());
    }
}
