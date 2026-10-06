//! Structured deck interchange with names, quantities, and explicit deck sections.
use crate::parse::{Entry, ParsedDeck, Section};

fn section(value: &str) -> Option<Section> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "main" | "mainboard" | "deck" => Some(Section::Main),
        "side" | "sideboard" => Some(Section::Sideboard),
        "commander" | "commanders" => Some(Section::Commander),
        "companion" => Some(Section::Companion),
        "maybe" | "maybeboard" => Some(Section::Maybeboard),
        _ => None,
    }
}

pub(crate) fn detect(input: &str) -> Option<ParsedDeck> {
    let input = input.trim_start_matches('\u{feff}').trim();
    if input.starts_with(['{', '[']) {
        return Some(json(input));
    }
    let header = input.lines().next()?;
    let delimiter = if header.contains('\t') { '\t' } else { ',' };
    let columns = csv_rows(header, delimiter).ok()?;
    let columns = columns.first()?;
    if columns
        .iter()
        .any(|s| s.trim().eq_ignore_ascii_case("name"))
        && columns.iter().any(|s| {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "count" | "quantity" | "qty"
            )
        })
    {
        return Some(csv(input, delimiter));
    }
    None
}

fn json(input: &str) -> ParsedDeck {
    let mut out = ParsedDeck::default();
    let parsed = serde_json::from_str::<serde_json::Value>(input);
    let rows = match &parsed {
        Ok(serde_json::Value::Array(rows)) => Some(rows),
        Ok(root) => root.get("entries").and_then(|v| v.as_array()),
        _ => None,
    };
    let Some(rows) = rows else {
        out.unparsed.push(
            "JSON deck must contain an entries array (or be an array of card entries).".into(),
        );
        return out;
    };
    for row in rows {
        let entry = (|| {
            let name = row.get("name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            let count = row.get("count").or_else(|| row.get("quantity"))?;
            let count = count
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .or_else(|| count.as_str()?.parse::<u32>().ok())?;
            let pile = match row.get("section") {
                None => Section::Main,
                Some(value) => section(value.as_str()?)?,
            };
            Some(Entry {
                count,
                name: name.into(),
                section: pile,
                set: row.get("set").and_then(|v| v.as_str()).map(str::to_owned),
                collector_number: row
                    .get("collector_number")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            })
        })();
        match entry {
            Some(entry) => out.entries.push(entry),
            None => out.unparsed.push(row.to_string()),
        }
    }
    out
}

fn csv(input: &str, delimiter: char) -> ParsedDeck {
    let mut out = ParsedDeck::default();
    let rows = match csv_rows(input, delimiter) {
        Ok(rows) => rows,
        Err(error) => {
            out.unparsed.push(error.into());
            return out;
        }
    };
    let Some(header) = rows.first() else {
        return out;
    };
    let find = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)))
    };
    let name = find(&["name"]).unwrap();
    let count = find(&["count", "quantity", "qty"]).unwrap();
    let pile = find(&["section", "board"]);
    let set = find(&["set"]);
    let collector = find(&["collector_number", "collector number"]);
    for row in rows.iter().skip(1) {
        if row.iter().all(|s| s.trim().is_empty()) {
            continue;
        }
        let entry = (|| {
            if row.len() != header.len() {
                return None;
            }
            let name = row.get(name)?.trim();
            if name.is_empty() {
                return None;
            }
            let hint = |i: Option<usize>| {
                i.and_then(|i| row.get(i))
                    .filter(|s| !s.is_empty())
                    .cloned()
            };
            Some(Entry {
                count: row.get(count)?.trim().parse().ok()?,
                name: name.into(),
                section: section(
                    pile.and_then(|i| row.get(i))
                        .map(String::as_str)
                        .unwrap_or("main"),
                )?,
                set: hint(set),
                collector_number: hint(collector),
            })
        })();
        match entry {
            Some(entry) => out.entries.push(entry),
            None => out.unparsed.push(format!("CSV row: {row:?}")),
        }
    }
    out
}

/// Quoted fields can contain commas, newlines, and escaped double quotes.
fn csv_rows(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, &'static str> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !closed => quoted = true,
            c if c == delimiter => {
                row.push(std::mem::take(&mut field));
                closed = false;
            }
            '\n' | '\r' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                closed = false;
            }
            _ if closed || c == '"' => return Err("Malformed quoted CSV field."),
            _ => field.push(c),
        }
    }
    if quoted {
        return Err("Unterminated quoted CSV field.");
    }
    if !field.is_empty() || !row.is_empty() || closed {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_handles_quoted_names_sections_and_printing_hints() {
        let deck = detect("Quantity,Name,Section,Set,Collector Number\r\n2,\"Urza, Lord High Artificer\",Main,MH1,75\r\n1,\"A \"\"quoted\"\" name\",Commander,,\r\n").unwrap();
        assert!(deck.unparsed.is_empty());
        assert_eq!(deck.entries[0].name, "Urza, Lord High Artificer");
        assert_eq!(deck.entries[0].set.as_deref(), Some("MH1"));
        assert_eq!(deck.entries[1].name, "A \"quoted\" name");
        assert_eq!(deck.entries[1].section, Section::Commander);
    }
    #[test]
    fn spreadsheet_tabs_preserve_names_and_sections() {
        let deck = detect(
            " qty\t name\tboard\r\n3\tUrza, Lord High Artificer\tmainboard\r\n1\tForest\tsideboard",
        )
        .unwrap();
        assert!(deck.unparsed.is_empty());
        assert_eq!(deck.entries[0].count, 3);
        assert_eq!(deck.entries[0].name, "Urza, Lord High Artificer");
        assert_eq!(deck.entries[1].section, Section::Sideboard);
    }

    #[test]
    fn structured_import_reports_bad_quantities_sections_and_syntax() {
        for input in [
            "count,name\n1,\"broken",
            "count,name,section\n1,Card,Unknown",
            "count,name\n4294967296,Card",
            "{broken",
            r#"[{"count":1,"name":"Card","section":"Unknown"}]"#,
        ] {
            let deck = detect(input).unwrap();
            assert!(!deck.unparsed.is_empty(), "{input}");
        }
    }
    #[test]
    fn json_preserves_sections_and_rejects_negative_counts() {
        let deck = detect(r#"{"entries":[{"count":2,"name":"Card","section":"sideboard"},{"count":-1,"name":"Bad"}]}"#).unwrap();
        assert_eq!(deck.entries.len(), 1);
        assert_eq!(deck.entries[0].section, Section::Sideboard);
        assert_eq!(deck.unparsed.len(), 1);
    }
}
