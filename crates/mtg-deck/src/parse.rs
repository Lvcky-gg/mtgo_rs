//! The tolerant line parser.

/// Which pile a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Main,
    Sideboard,
    /// Commander, or Brawl's. Kept separate because it is not part of the 99.
    Commander,
    Companion,
    /// Archidekt and Moxfield both export a maybeboard; it is parsed and ignored
    /// rather than treated as a parse failure.
    Maybeboard,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub count: u32,
    /// The name exactly as written, minus decorations. Resolution happens later.
    pub name: String,
    /// Set code, when the export included one. A hint for picking printing/art,
    /// never used for identity.
    pub set: Option<String>,
    pub collector_number: Option<String>,
    pub section: Section,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ParsedDeck {
    pub entries: Vec<Entry>,
    /// Lines that looked like content but could not be parsed, kept so the import
    /// UI can show them rather than silently dropping cards.
    pub unparsed: Vec<String>,
}

impl ParsedDeck {
    pub fn count_in(&self, section: Section) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.section == section)
            .map(|e| e.count)
            .sum()
    }
}

/// Parse a pasted decklist in any of the common export shapes.
///
/// Never fails: anything unrecognised lands in [`ParsedDeck::unparsed`] for the
/// UI to surface. A paste that is 95% good should import 95% of the deck and say
/// what it could not read, not refuse the whole thing.
pub fn parse(input: &str) -> ParsedDeck {
    let mut out = ParsedDeck::default();
    let mut section = Section::Main;
    // MTGGoldfish and several others separate the sideboard with a blank line and
    // no header. Only the *first* blank line after real content means that, and
    // only if no explicit header was ever seen.
    let mut saw_explicit_header = false;
    let mut saw_content = false;
    let mut blank_run = false;

    for raw in input.lines() {
        let line = raw.trim();

        if line.is_empty() {
            if saw_content && !saw_explicit_header && !blank_run && section == Section::Main {
                section = Section::Sideboard;
            }
            blank_run = true;
            continue;
        }
        blank_run = false;

        if is_comment(line) {
            // A comment can still be a section header: `// Sideboard`.
            if let Some(s) = section_header(strip_comment(line)) {
                section = s;
                saw_explicit_header = true;
            }
            continue;
        }

        if let Some(s) = section_header(line) {
            section = s;
            saw_explicit_header = true;
            continue;
        }

        // `SB: 4 Cardname` — a per-line section marker.
        let (line, forced) = match line.strip_prefix("SB:") {
            Some(rest) => (rest.trim(), Some(Section::Sideboard)),
            None => (line, None),
        };

        match parse_entry(line, forced.unwrap_or(section)) {
            Some(e) => {
                saw_content = true;
                out.entries.push(e);
            }
            None => out.unparsed.push(raw.to_string()),
        }
    }

    out
}

fn is_comment(line: &str) -> bool {
    line.starts_with("//") || line.starts_with('#')
}

fn strip_comment(line: &str) -> &str {
    line.trim_start_matches(['/', '#']).trim()
}

/// Recognise a standalone section header, case-insensitively.
fn section_header(line: &str) -> Option<Section> {
    let key = line.trim_end_matches([':', ' ']).to_ascii_lowercase();
    match key.as_str() {
        "deck" | "main" | "maindeck" | "main deck" => Some(Section::Main),
        "sideboard" | "side" | "sb" => Some(Section::Sideboard),
        "commander" | "commanders" => Some(Section::Commander),
        "companion" => Some(Section::Companion),
        "maybeboard" | "considering" => Some(Section::Maybeboard),
        _ => None,
    }
}

/// Parse one card line.
///
/// Handles `4 Name`, `4x Name`, `Name x4`, and a trailing `(SET) 123`, plus the
/// decorations exporters add: Archidekt's `[Category]` and `^flags^`, Moxfield's
/// `*F*`.
fn parse_entry(line: &str, section: Section) -> Option<Entry> {
    let line = strip_decorations(line);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let (count, rest) = split_count(line)?;
    let (name, set, collector_number) = split_set_hint(rest);

    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }

    Some(Entry {
        count,
        name,
        set,
        collector_number,
        section,
    })
}

/// Remove exporter-specific decorations that carry no identity.
fn strip_decorations(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth_square = 0usize;
    let mut in_caret = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Archidekt category: `[Ramp,Land]`
            '[' => depth_square += 1,
            ']' => depth_square = depth_square.saturating_sub(1),
            // Archidekt flags: `^Have,#aabbcc^`
            '^' => in_caret = !in_caret,
            // Moxfield foil/etched markers: `*F*`, `*E*`
            '*' if matches!(chars.peek(), Some('F' | 'E' | 'f' | 'e')) => {
                chars.next();
                if chars.peek() == Some(&'*') {
                    chars.next();
                }
            }
            _ if depth_square == 0 && !in_caret => out.push(c),
            _ => {}
        }
    }
    out
}

/// Pull the quantity off either end.
fn split_count(line: &str) -> Option<(u32, &str)> {
    // Leading: `4 Name` or `4x Name`
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        let rest = &line[digits.len()..];
        let rest = rest.strip_prefix(['x', 'X']).unwrap_or(rest);
        let rest = rest.trim_start();
        // Guard against a card name that genuinely begins with a number by
        // requiring separation between the count and the name.
        if rest.len() < line.len() && !rest.is_empty() {
            return Some((digits.parse().ok()?, rest));
        }
    }

    // Trailing: `Name x4`
    if let Some(idx) = line.rfind(['x', 'X'])
        && line[idx + 1..].chars().all(|c| c.is_ascii_digit())
        && !line[idx + 1..].is_empty()
        && line[..idx].ends_with(' ')
    {
        return Some((line[idx + 1..].parse().ok()?, line[..idx].trim_end()));
    }

    // No quantity at all means one copy — plenty of pasted lists omit it.
    Some((1, line))
}

/// Split a trailing `(SET) 123` or `(SET)` hint off the name.
fn split_set_hint(rest: &str) -> (&str, Option<String>, Option<String>) {
    let Some(open) = rest.rfind('(') else {
        return (rest, None, None);
    };
    let Some(close_rel) = rest[open..].find(')') else {
        return (rest, None, None);
    };
    let close = open + close_rel;

    let code = &rest[open + 1..close];
    // Set codes are 3-6 alphanumerics. Anything else is part of the name — several
    // real cards have parenthesised words in them.
    if code.is_empty() || code.len() > 6 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return (rest, None, None);
    }

    let after = rest[close + 1..].trim();
    let collector = (!after.is_empty()).then(|| after.to_string());
    (&rest[..open], Some(code.to_ascii_uppercase()), collector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_and_moxfield_shape() {
        let d = parse("4 Mountain (M19) 152\n2 Island (M19) 265");
        assert_eq!(d.unparsed.len(), 0);
        assert_eq!(d.entries.len(), 2);
        assert_eq!(d.entries[0].count, 4);
        assert_eq!(d.entries[0].name, "Mountain");
        assert_eq!(d.entries[0].set.as_deref(), Some("M19"));
        assert_eq!(d.entries[0].collector_number.as_deref(), Some("152"));
    }

    #[test]
    fn archidekt_shape_with_categories_and_flags() {
        let d = parse("4x Mountain (M19) 152 [Lands] ^Have,#aabbcc^");
        assert_eq!(d.entries[0].count, 4);
        assert_eq!(d.entries[0].name, "Mountain");
        assert_eq!(d.entries[0].set.as_deref(), Some("M19"));
    }

    #[test]
    fn moxfield_foil_marker_is_not_part_of_the_name() {
        let d = parse("1 Mountain (M19) 152 *F*");
        assert_eq!(d.entries[0].name, "Mountain");
        assert_eq!(d.entries[0].count, 1);
    }

    #[test]
    fn explicit_headers_switch_section() {
        let d = parse("Deck\n4 Mountain\n\nSideboard\n2 Island");
        assert_eq!(d.count_in(Section::Main), 4);
        assert_eq!(d.count_in(Section::Sideboard), 2);
    }

    #[test]
    fn blank_line_separates_sideboard_when_no_header_given() {
        // The MTGGoldfish convention.
        let d = parse("4 Mountain\n20 Island\n\n2 Forest");
        assert_eq!(d.count_in(Section::Main), 24);
        assert_eq!(d.count_in(Section::Sideboard), 2);
    }

    #[test]
    fn an_explicit_header_disables_the_blank_line_heuristic() {
        // With headers present, blank lines are just formatting.
        let d = parse("Deck\n\n4 Mountain\n\n20 Island\n\nSideboard\n2 Forest");
        assert_eq!(d.count_in(Section::Main), 24);
        assert_eq!(d.count_in(Section::Sideboard), 2);
    }

    #[test]
    fn sb_prefix_marks_one_line_only() {
        let d = parse("4 Mountain\nSB: 2 Island\n3 Forest");
        assert_eq!(d.count_in(Section::Sideboard), 2);
        assert_eq!(d.count_in(Section::Main), 7);
    }

    #[test]
    fn commander_section_is_kept_apart_from_the_main_deck() {
        let d = parse("Commander\n1 Some Legend\n\nDeck\n99 Mountain");
        assert_eq!(d.count_in(Section::Commander), 1);
        assert_eq!(d.count_in(Section::Main), 99);
    }

    #[test]
    fn comments_are_skipped_but_comment_headers_still_count() {
        let d = parse("// my deck\n4 Mountain\n// Sideboard\n2 Island");
        assert_eq!(d.count_in(Section::Main), 4);
        assert_eq!(d.count_in(Section::Sideboard), 2);
    }

    #[test]
    fn missing_quantity_means_one() {
        let d = parse("Mountain\nIsland");
        assert_eq!(d.count_in(Section::Main), 2);
        assert_eq!(d.entries[0].name, "Mountain");
    }

    #[test]
    fn trailing_quantity_shape() {
        let d = parse("Mountain x4");
        assert_eq!(d.entries[0].count, 4);
        assert_eq!(d.entries[0].name, "Mountain");
    }

    #[test]
    fn parenthesised_text_that_is_not_a_set_code_stays_in_the_name() {
        let d = parse("1 Ashen Rite (the long version)");
        assert_eq!(d.entries[0].name, "Ashen Rite (the long version)");
        assert_eq!(d.entries[0].set, None);
    }

    #[test]
    fn split_card_names_survive_intact() {
        let d = parse("2 Front Half // Back Half (SET) 1");
        assert_eq!(d.entries[0].name, "Front Half // Back Half");
    }
}
