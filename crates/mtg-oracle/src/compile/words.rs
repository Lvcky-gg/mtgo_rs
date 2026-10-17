//! Text normalisation and the small vocabulary every other part of the compiler shares.
//!
//! Parsers here take a `&str` and return what they recognised plus the rest of the input,
//! or `None`. Nothing is ever skipped: a word the grammar does not know fails the parse,
//! which is what keeps "understood" honest.

use mtg_core::{CardType, Color, CounterKind, Keyword};

/// Rules text made uniform before parsing.
///
/// - Reminder text (anything in parentheses) is removed.
/// - The card's own name, its short name ("Urza" for "Urza, Lord High Artificer"), and
///   the modern self-references ("this creature", "this spell", …) all become `~`.
/// - Everything is lowercased, and the typographic minus sign becomes `-`.
pub fn normalise(text: &str, name: &str) -> String {
    let mut s = strip_reminder(text);
    if !name.is_empty() {
        s = s.replace(name, "~");
        // "Loran of the Third Path" → "Loran", but only where the text uses it of the card
        // itself: "Knight of the New Coalition" also creates a "Knight creature token".
        if !name.contains(", ")
            && let Some((short, _)) = name.split_once(" of the ")
            && short.len() >= 3
            && !short.contains(' ')
        {
            for verb in [
                " enters", " attacks", " blocks", " dies", " deals", " gets", " gains", " has ",
                " becomes", " can't", " doesn't", " leaves", "'s ",
            ] {
                s = s.replace(&format!("{short}{verb}"), &format!("~{verb}"));
            }
            for lead in [
                "on ",
                "of ",
                "untap ",
                "tap ",
                "exile ",
                "sacrifice ",
                "return ",
            ] {
                s = s.replace(&format!("{lead}{short}."), &format!("{lead}~."));
            }
        }
        // "Urza, Lord High Artificer" → "Urza".
        if let Some((short, _)) = name.split_once(", ").or_else(|| name.split_once(" the "))
            && short.len() >= 3
        {
            s = s.replace(short, "~");
        }
    }
    let s = s.to_lowercase().replace('−', "-");
    // Inside double quotes "this creature" is whatever has the quoted ability — a token or
    // the creature it's granted to, not this card — so it's left for `quoted_abilities`.
    let s: String = s
        .split('"')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                part.to_string()
            } else {
                self_references(part)
            }
        })
        .collect::<Vec<_>>()
        .join("\"");
    s.lines()
        .map(|line| instead_if_first(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// "~ deals 4 damage instead if you control three or more artifacts." (Galvanic Blast) is
/// read as "if you control three or more artifacts, ~ deals 4 damage instead.", the order
/// the compiler knows. An ability word before it stays in front.
fn instead_if_first(line: &str) -> String {
    if !line.contains(" instead if ") || line.contains('"') {
        return line.to_string();
    }
    line.split_inclusive(". ")
        .map(|sentence| {
            let (text, end) = match sentence.strip_suffix(". ") {
                Some(t) => (t, ". "),
                None => match sentence.strip_suffix('.') {
                    Some(t) => (t, "."),
                    None => (sentence, ""),
                },
            };
            let Some((body, cond)) = text.split_once(" instead if ") else {
                return sentence.to_string();
            };
            let (lead, body) = match body.rsplit_once(" — ") {
                Some((word, body)) => (format!("{word} — "), body),
                None => (String::new(), body),
            };
            if body.starts_with("if ") || body.contains(": ") || cond.contains(" instead") {
                return sentence.to_string();
            }
            format!("{lead}if {cond}, {body} instead{end}")
        })
        .collect()
}

/// "this creature", "this spell", … as `~`.
pub fn self_references(text: &str) -> String {
    let mut s = text.to_string();
    for this in [
        "this creature",
        "this artifact",
        "this enchantment",
        "this land",
        "this permanent",
        "this spell",
        "this card",
        "this equipment",
        "this aura",
        "this vehicle",
        "this planeswalker",
        "this saga",
        "this class",
        "this spacecraft",
        "this planet",
    ] {
        s = s.replace(this, "~");
    }
    s
}

/// A line with every parenthesised span removed.
pub fn strip_reminder(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0u32;
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    // Removing "(…)" leaves a doubled or trailing space behind.
    let mut tidy = String::with_capacity(out.len());
    for part in out.split(' ').filter(|p| !p.is_empty()) {
        if !tidy.is_empty() {
            tidy.push(' ');
        }
        tidy.push_str(part);
    }
    tidy.replace(" .", ".").replace(" ,", ",")
}

/// A small number written as a word or digits: "a", "an", "one" … "ten", "3".
pub fn number(s: &str) -> Option<(i32, &str)> {
    const WORDS: &[(&str, i32)] = &[
        ("an", 1),
        ("a", 1),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("eighteen", 18),
        ("nineteen", 19),
        ("twenty", 20),
        ("thirty", 30),
        ("forty", 40),
        ("fifty", 50),
    ];
    for (w, n) in WORDS {
        if let Some(rest) = s.strip_prefix(w)
            && word_boundary(rest)
        {
            return Some((*n, rest));
        }
    }
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let n = s[..digits].parse().ok()?;
    let rest = &s[digits..];
    word_boundary(rest).then_some((n, rest))
}

fn word_boundary(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with([' ', ',', '.', '/', ':', '—'])
}

/// A signed power/toughness change: "+2/+2", "-1/-1", "+3/+0".
pub fn pt_delta(s: &str) -> Option<((i32, i32), &str)> {
    let (p, rest) = signed(s)?;
    let rest = rest.strip_prefix('/')?;
    let (t, rest) = signed(rest)?;
    Some(((p, t), rest))
}

fn signed(s: &str) -> Option<(i32, &str)> {
    let (neg, rest) = match s.as_bytes().first()? {
        b'+' => (false, &s[1..]),
        b'-' => (true, &s[1..]),
        _ => return None,
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let n: i32 = rest[..digits].parse().ok()?;
    Some((if neg { -n } else { n }, &rest[digits..]))
}

/// An unsigned "N/N" for tokens.
pub fn pt(s: &str) -> Option<((i32, i32), &str)> {
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let p: i32 = s[..digits].parse().ok()?;
    let rest = s[digits..].strip_prefix('/')?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let t: i32 = rest[..digits].parse().ok()?;
    Some(((p, t), &rest[digits..]))
}

/// Energy symbols: "{e}{e}{e}" is three (CR 107.14).
pub fn energy(s: &str) -> Option<(i32, &str)> {
    let mut n = 0;
    let mut r = s;
    while let Some(rest) = r.strip_prefix("{e}") {
        n += 1;
        r = rest;
    }
    (n > 0).then_some((n, r))
}

/// Keywords the engine enforces and that take no parameter.
pub fn keyword(word: &str) -> Option<Keyword> {
    Some(match word {
        "flying" => Keyword::Flying,
        "phasing" => Keyword::Phasing,
        "reach" => Keyword::Reach,
        "first strike" => Keyword::FirstStrike,
        "double strike" => Keyword::DoubleStrike,
        "deathtouch" => Keyword::Deathtouch,
        "lifelink" => Keyword::Lifelink,
        "trample" => Keyword::Trample,
        "vigilance" => Keyword::Vigilance,
        "haste" => Keyword::Haste,
        "defender" => Keyword::Defender,
        "menace" => Keyword::Menace,
        "hexproof" => Keyword::Hexproof,
        "shroud" => Keyword::Shroud,
        "indestructible" => Keyword::Indestructible,
        "flash" => Keyword::Flash,
        "shadow" => Keyword::Shadow,
        "fear" => Keyword::Fear,
        "intimidate" => Keyword::Intimidate,
        "horsemanship" => Keyword::Horsemanship,
        "skulk" => Keyword::Skulk,
        "ascend" => Keyword::Ascend,
        "plainswalk" => Keyword::Plainswalk,
        "islandwalk" => Keyword::Islandwalk,
        "swampwalk" => Keyword::Swampwalk,
        "mountainwalk" => Keyword::Mountainwalk,
        "forestwalk" => Keyword::Forestwalk,
        "wither" => Keyword::Wither,
        "infect" => Keyword::Infect,
        "convoke" => Keyword::Convoke,
        "improvise" => Keyword::Improvise,
        "delve" => Keyword::Delve,
        "rebound" => Keyword::Rebound,
        "split second" => Keyword::SplitSecond,
        "flanking" => Keyword::Flanking,
        "myriad" => Keyword::Myriad,
        "start your engines!" => Keyword::StartYourEngines,
        "daybound" => Keyword::Daybound,
        "nightbound" => Keyword::Nightbound,
        "changeling" => Keyword::Changeling,
        "umbra armor" | "totem armor" => Keyword::UmbraArmor,
        _ => return None,
    })
}

/// A keyword at the start of `s`, longest spelling first.
pub fn keyword_prefix(s: &str) -> Option<(Keyword, &str)> {
    if let Some(r) = s.strip_prefix("toxic ") {
        let (n, rest) = number(r)?;
        if (1..=255).contains(&n) {
            return Some((Keyword::Toxic(n as u8), rest));
        }
        return None;
    }
    const ALL: &[&str] = &[
        "split second",
        "horsemanship",
        "changeling",
        "wither",
        "infect",
        "convoke",
        "improvise",
        "mountainwalk",
        "intimidate",
        "plainswalk",
        "islandwalk",
        "forestwalk",
        "swampwalk",
        "shadow",
        "skulk",
        "ascend",
        "fear",
        "first strike",
        "double strike",
        "indestructible",
        "deathtouch",
        "vigilance",
        "lifelink",
        "hexproof",
        "defender",
        "trample",
        "flying",
        "shroud",
        "menace",
        "reach",
        "haste",
        "flash",
    ];
    for w in ALL {
        if let Some(rest) = s.strip_prefix(w)
            && word_boundary(rest)
        {
            return Some((keyword(w)?, rest));
        }
    }
    None
}

/// "flying", "flying and trample", "flying, vigilance, and lifelink".
pub fn keyword_list(s: &str) -> Option<(Vec<Keyword>, &str)> {
    let (first, mut rest) = keyword_prefix(s)?;
    let mut out = vec![first];
    loop {
        let next = rest
            .strip_prefix(", and ")
            .or_else(|| rest.strip_prefix(" and "))
            .or_else(|| rest.strip_prefix(", "));
        match next.and_then(keyword_prefix) {
            Some((k, r)) => {
                out.push(k);
                rest = r;
            }
            None => return Some((out, rest)),
        }
    }
}

/// A colored mana symbol's letter: "w" in "{w}".
pub fn color_symbol(letter: &str) -> Option<Color> {
    Some(match letter {
        "w" => Color::White,
        "u" => Color::Blue,
        "b" => Color::Black,
        "r" => Color::Red,
        "g" => Color::Green,
        _ => return None,
    })
}

pub fn color(word: &str) -> Option<Color> {
    Some(match word {
        "white" => Color::White,
        "blue" => Color::Blue,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        _ => return None,
    })
}

/// A card type as a noun, singular or plural. Returns the type and whether it was plural.
pub fn card_type(word: &str) -> Option<(CardType, bool)> {
    Some(match word {
        "creature" => (CardType::Creature, false),
        "creatures" => (CardType::Creature, true),
        "artifact" => (CardType::Artifact, false),
        "artifacts" => (CardType::Artifact, true),
        "enchantment" => (CardType::Enchantment, false),
        "enchantments" => (CardType::Enchantment, true),
        "land" => (CardType::Land, false),
        "lands" => (CardType::Land, true),
        "planeswalker" => (CardType::Planeswalker, false),
        "planeswalkers" => (CardType::Planeswalker, true),
        "instant" => (CardType::Instant, false),
        "instants" => (CardType::Instant, true),
        "sorcery" => (CardType::Sorcery, false),
        "sorceries" => (CardType::Sorcery, true),
        _ => return None,
    })
}

/// Split off the first word.
pub fn first_word(s: &str) -> (&str, &str) {
    let end = s.find([' ', ',', '.', ':']).unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// CR 207.2c ability words. They have no rules meaning, so "Landfall — Whenever …" is
/// exactly "Whenever …".
pub fn strip_ability_word(line: &str) -> &str {
    const WORDS: &[&str] = &[
        "adamant",
        "addendum",
        "alliance",
        "avoidance",
        "battalion",
        "bloodrush",
        "celebration",
        "channel",
        "chroma",
        "cohort",
        "constellation",
        "converge",
        "corrupted",
        "council's dilemma",
        "coven",
        "delirium",
        "descend 4",
        "descend 8",
        "disappear",
        "domain",
        "eerie",
        "eminence",
        "enrage",
        "fateful hour",
        "fathomless descent",
        "ferocious",
        "flurry",
        "formidable",
        "grandeur",
        "hellbent",
        "heroic",
        "imprint",
        "infusion",
        "inspired",
        "join forces",
        "kinship",
        "landfall",
        "lieutenant",
        "magecraft",
        "metalcraft",
        "morbid",
        "pack tactics",
        "opus",
        "paradox",
        "parley",
        "radiance",
        "renew",
        "raid",
        "rally",
        "revolt",
        "secret council",
        "scions' secretary",
        "vivid",
        "beacon of hope",
        "keen senses",
        "blood chalice",
        "rapacious hunger",
        "martyrdom",
        "poison breath",
        "medicus ministorum",
        "cure wounds",
        "dispel magic",
        "gentle repose",
        "sort inventory",
        "pick a perk",
        "check map",
        "foil their scheme",
        "learn their secrets",
        "distract the guard",
        "hide",
        "rouse the party",
        "set off traps",
        "charge them",
        "befriend them",
        "journey on",
        "make camp",
        "fight the current",
        "find a crossing",
        "form a party",
        "start a brawl",
        "intimidate them",
        "fend them off",
        "shrieking gargoyles",
        "spell mastery",
        "strive",
        "survival",
        "sweep",
        "tempting offer",
        "threshold",
        "undergrowth",
        "valiant",
        "void",
        "will of the council",
    ];
    for w in WORDS {
        if let Some(rest) = line.strip_prefix(w).and_then(|r| r.strip_prefix(" — ")) {
            return rest;
        }
    }
    // Any other flavor word (CR 207.2c): "Venom Blast — …", "Treasure Hunter — …". Not a
    // keyword whose dash introduces a cost ("Boast — {1}{R}: …") or a condition ("Max speed
    // — …"), not a saga chapter ("I, II — …"), and only a few plain words.
    if let Some((word, rest)) = line.split_once(" — ")
        && !word.is_empty()
        && word.split(' ').count() <= 5
        && word
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, ' ' | '\'' | '-' | ',' | '!' | '.'))
        && !word
            .chars()
            .all(|c| matches!(c.to_ascii_lowercase(), 'i' | 'v' | 'x' | ',' | ' '))
        && ![
            "max speed",
            "boast",
            "exhaust",
            "power-up",
            "forecast",
            "level up",
            "choose one",
            "choose two",
            "ward",
        ]
        .iter()
        .any(|k| word.to_lowercase().starts_with(k))
    {
        return rest;
    }
    line
}

/// Counter kinds with no rules of their own (CR 122.1): only what cards say about them
/// matters. Their position here is their number, so the list is append-only. Kinds with
/// rules attached — stun, shield, lore, level, keyword counters — are deliberately absent.
const PLAIN_COUNTERS: &[&str] = &[
    "charge",
    "time",
    "oil",
    "age",
    "quest",
    "fade",
    "ice",
    "spore",
    "fuse",
    "hatchling",
    "page",
    "study",
    "verse",
    "wish",
    "blood",
    "bounty",
    "brick",
    "coin",
    "corpse",
    "credit",
    "crystal",
    "depletion",
    "doom",
    "dream",
    "egg",
    "flood",
    "gem",
    "glyph",
    "gold",
    "growth",
    "healing",
    "hit",
    "hourglass",
    "hunger",
    "incarnation",
    "infection",
    "ki",
    "knowledge",
    "luck",
    "manifestation",
    "mine",
    "mining",
    "muster",
    "net",
    "omen",
    "ore",
    "pain",
    "petal",
    "pin",
    "plague",
    "polyp",
    "pressure",
    "prey",
    "pupa",
    "rust",
    "scream",
    "shell",
    "silver",
    "sleep",
    "slime",
    "slumber",
    "soot",
    "soul",
    "spark",
    "storage",
    "strife",
    "task",
    "theft",
    "tide",
    "tower",
    "training",
    "trap",
    "unity",
    "velocity",
    "vitality",
    "void",
    "vortex",
    "wage",
    "winch",
    "wind",
    "arrow",
    "aim",
    "cage",
    "carrion",
    "delay",
    "despair",
    "devotion",
    "divinity",
    "echo",
    "elixir",
    "eon",
    "eyeball",
    "fate",
    "feather",
    "filibuster",
    "fungus",
    "hoofprint",
    "hour",
    "intervention",
    "isolation",
    "javelin",
    "landmark",
    "magnet",
    "mask",
    "matrix",
    "mire",
    "music",
    "night",
    "paralyzation",
    "petrification",
    "phylactery",
    "stash",
    "treasure",
    "vow",
    "voyage",
    // Appended: indices are the counters' identities.
    "lore",
    "level",
    "burden",
    "plan",
    "flame",
    "fetch",
    "memory",
    "judgment",
    "intel",
    "ingenuity",
    "hatching",
    "fire",
    "dread",
    "collection",
    "blight",
    "blaze",
    "story",
    "valor",
    "point",
    "wreck",
    "supply",
    "spite",
    "sin",
    "shred",
    "stall",
    "unlock",
];

/// "+1/+1 counter", "-1/-1 counter", "charge counter", "loyalty counter".
pub fn counter(s: &str) -> Option<(CounterKind, &str)> {
    for (name, keyword) in [
        ("flying", Keyword::Flying),
        ("first strike", Keyword::FirstStrike),
        ("double strike", Keyword::DoubleStrike),
        ("deathtouch", Keyword::Deathtouch),
        ("lifelink", Keyword::Lifelink),
        ("hexproof", Keyword::Hexproof),
        ("indestructible", Keyword::Indestructible),
        ("menace", Keyword::Menace),
        ("reach", Keyword::Reach),
        ("trample", Keyword::Trample),
        ("vigilance", Keyword::Vigilance),
        ("haste", Keyword::Haste),
        ("shadow", Keyword::Shadow),
    ] {
        if let Some(rest) = s
            .strip_prefix(name)
            .and_then(|r| r.strip_prefix(" counter"))
        {
            return Some((CounterKind::Keyword(keyword), rest));
        }
    }
    if let Some(r) = s.strip_prefix("+1/+1 counter") {
        return Some((CounterKind::PlusOnePlusOne, r));
    }
    if let Some(r) = s.strip_prefix("-1/-1 counter") {
        return Some((CounterKind::MinusOneMinusOne, r));
    }
    if let Some(r) = s.strip_prefix("loyalty counter") {
        return Some((CounterKind::Loyalty, r));
    }
    if let Some(r) = s.strip_prefix("stun counter") {
        return Some((CounterKind::Stun, r));
    }
    if let Some(r) = s.strip_prefix("shield counter") {
        return Some((CounterKind::Shield, r));
    }
    let (w, r) = first_word(s);
    let r = r.strip_prefix(" counter")?;
    let i = PLAIN_COUNTERS.iter().position(|c| *c == w)?;
    Some((CounterKind::Other(i as u16), r))
}

#[cfg(test)]
mod tests {

    #[test]
    fn flavor_words_are_stripped_but_not_costs_or_chapters() {
        let strip = super::strip_ability_word;
        assert_eq!(
            strip("Venom Blast — Creatures your opponents control enter tapped."),
            "Creatures your opponents control enter tapped."
        );
        assert_eq!(strip("II, III — Draw a card."), "II, III — Draw a card.");
        assert_eq!(
            strip("Boast — {1}{R}: Draw a card."),
            "Boast — {1}{R}: Draw a card."
        );
        assert_eq!(
            strip("Max speed — Whenever you attack, draw a card."),
            "Max speed — Whenever you attack, draw a card."
        );
    }

    use super::*;

    #[test]
    fn self_references_become_a_tilde() {
        assert_eq!(
            normalise("When Grizzly Bears enters, draw a card.", "Grizzly Bears"),
            "when ~ enters, draw a card."
        );
        assert_eq!(
            normalise("When this creature enters, draw a card.", "X"),
            "when ~ enters, draw a card."
        );
        assert_eq!(
            normalise("Urza deals 1 damage.", "Urza, Lord of Tests"),
            "~ deals 1 damage."
        );
    }

    #[test]
    fn reminder_text_goes_and_leaves_no_gap() {
        assert_eq!(normalise("Scry 1. (Look at the top card.)", "X"), "scry 1.");
        assert_eq!(
            normalise("Trample (It can.) and more", "X"),
            "trample and more"
        );
    }

    #[test]
    fn numbers_and_deltas() {
        assert_eq!(number("two cards"), Some((2, " cards")));
        assert_eq!(number("a card"), Some((1, " card")));
        assert_eq!(number("another"), None, "not 'an' followed by 'other'");
        assert_eq!(number("12 damage"), Some((12, " damage")));
        assert_eq!(pt_delta("+2/-1 until"), Some(((2, -1), " until")));
        assert_eq!(pt("3/3 green"), Some(((3, 3), " green")));
    }

    #[test]
    fn keyword_lists() {
        assert_eq!(
            keyword_list("flying, vigilance, and lifelink until").map(|(k, r)| (k.len(), r)),
            Some((3, " until"))
        );
        assert_eq!(
            keyword_list("first strike and trample.").map(|(k, _)| k),
            Some(vec![Keyword::FirstStrike, Keyword::Trample])
        );
        assert_eq!(
            keyword_list("flying and toxic 2 until end of turn"),
            Some((
                vec![Keyword::Flying, Keyword::Toxic(2)],
                " until end of turn"
            ))
        );
        assert_eq!(
            keyword_prefix("toxic 255."),
            Some((Keyword::Toxic(255), "."))
        );
        for invalid in ["toxic", "toxic 0", "toxic -1", "toxic 256", "toxic 2foo"] {
            assert_eq!(keyword_prefix(invalid), None, "{invalid}");
        }
    }
}
