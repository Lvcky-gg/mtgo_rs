//! `mtg-cards` — populate and query the local card database.
//!
//! ```text
//!   mtg-cards import <file>     import a Scryfall bulk export from disk
//!   mtg-cards import --fetch    download it first (needs --features fetch)
//!   mtg-cards search <words>    full-text search
//!   mtg-cards deck <name> <f>   import a pasted deck list
//!   mtg-cards stats             what the database holds
//! ```
//!
//! The database lives under the user's data directory, so the game and this tool share one
//! file without either being told where it is.

use std::path::PathBuf;

use mtg_deck::Resolver;
use mtg_oracle::library::StoreResolver;
use mtg_store::{Section, Store};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");

    let result = match command {
        "import" => import(&args[1..]),
        "search" => search(&args[1..]),
        "deck" => deck(&args[1..]),
        "stats" => stats(),
        "coverage" => coverage(&args[1..]),
        "parse" => parse(&args[1..]),
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        other => Err(format!("unknown command {other:?}; try `mtg-cards help`")),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn usage() {
    println!(
        "mtg-cards — local card database\n\n\
         \x20 import <file>        import a Scryfall bulk export\n\
         \x20 import --fetch       download the export first (build with --features fetch)\n\
         \x20 search <words>       full-text search over name, type and rules text\n\
         \x20 deck <name> <file>   import a pasted deck list\n\
         \x20 stats                what the database currently holds\n\
         \x20 coverage [N]         cards playable as printed, and the N most common lines not\n\
         \x20                      yet understood\n\
         \x20 parse <type> <text>  compile one card's rules text and show the result\n\n\
         database: {}",
        db_path().display()
    );
}

/// Where the database lives. Shared with the GUI, so both open the same file.
fn db_path() -> PathBuf {
    mtg_store::default_path()
}

fn open() -> Result<Store, String> {
    let path = db_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    Store::open(&path).map_err(|e| e.to_string())
}

fn import(args: &[String]) -> Result<(), String> {
    let mut store = open()?;

    if args.first().map(String::as_str) == Some("--fetch") {
        #[cfg(feature = "fetch")]
        {
            let force = args.iter().any(|a| a == "--force");
            println!("fetching bulk data from Scryfall…");
            match mtg_oracle::fetch::fetch_and_import_with_progress(
                &mut store,
                force,
                &mut |progress| {
                    eprintln!("{}", progress.status());
                },
            )
            .map_err(|e| e.to_string())?
            {
                Some(report) => println!("{}", report.summary()),
                None => println!(
                    "already imported within the last day; pass --force to download anyway"
                ),
            }
            return Ok(());
        }
        #[cfg(not(feature = "fetch"))]
        {
            return Err(
                "this build has no downloader. Either rebuild with `--features fetch`, or \
                 download the file yourself and import it:\n  \
                 curl -L -o oracle-cards.json \"$(curl -s https://api.scryfall.com/bulk-data \
                 | grep -o 'https://[^\"]*oracle-cards[^\"]*json')\"\n  \
                 mtg-cards import oracle-cards.json"
                    .into(),
            );
        }
    }

    let path = args
        .first()
        .ok_or("usage: mtg-cards import <file> | --fetch")?;
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open {path}: {e}"))?;
    // Buffered because the importer reads it a token at a time.
    let reader = std::io::BufReader::with_capacity(1 << 20, file);

    println!("importing {path}…");
    let report =
        mtg_oracle::fetch::import_from_with_progress(&mut store, reader, &mut |progress| {
            eprintln!("{}", progress.status());
        })
        .map_err(|e| e.to_string())?;
    println!("{}", report.summary());

    if report.unparsed_costs > 0 || report.unknown_type_words > 0 {
        println!(
            "\nnote: {} cards had a mana symbol and {} had a type-line word this build does \
             not know. They are stored and searchable; the engine may not read them correctly.",
            report.unparsed_costs, report.unknown_type_words
        );
    }
    Ok(())
}

fn search(args: &[String]) -> Result<(), String> {
    let store = open()?;
    let query = args.join(" ");
    if query.is_empty() {
        return Err("usage: mtg-cards search <words>".into());
    }
    let hits = store.search(&query, 25).map_err(|e| e.to_string())?;
    if hits.is_empty() {
        println!("nothing matched {query:?}");
        return Ok(());
    }
    for (oracle, name) in hits {
        let type_line = store
            .card(oracle)
            .map_err(|e| e.to_string())?
            .and_then(|c| c.faces.first().map(|f| f.type_line.clone()))
            .unwrap_or_default();
        println!("{name}  —  {type_line}");
    }
    Ok(())
}

fn deck(args: &[String]) -> Result<(), String> {
    let (name, path) = match args {
        [name, path] => (name, path),
        _ => return Err("usage: mtg-cards deck <name> <file>".into()),
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut store = open()?;
    save_deck(&mut store, name, &text)
}

fn save_deck(store: &mut Store, name: &str, text: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the deck a name".into());
    }

    let parsed = mtg_deck::parse(text);
    if parsed
        .entries
        .iter()
        .any(|entry| entry.section == mtg_deck::Section::Companion)
    {
        return Err("companion designation is not supported yet; the deck was not saved".into());
    }
    if parsed.count_in(mtg_deck::Section::Commander) > 1 {
        return Err("multiple commanders are not supported yet; the deck was not saved".into());
    }
    let entries = {
        let resolver = StoreResolver(store);
        let resolved = mtg_deck::resolve::resolve(&parsed, &resolver);

        // Report what could not be matched rather than importing a quietly incomplete deck.
        for failure in &resolved.failed {
            let hints: Vec<String> = failure.suggestions.iter().map(|(_, n)| n.clone()).collect();
            if hints.is_empty() {
                println!("unmatched: {}", failure.entry.name);
            } else {
                println!(
                    "unmatched: {}  (did you mean: {}?)",
                    failure.entry.name,
                    hints.join(", ")
                );
            }
        }
        for line in &parsed.unparsed {
            println!("unreadable line: {line}");
        }

        let mut entries: Vec<(u32, u32, Section)> = resolved
            .main
            .iter()
            .map(|(id, n)| (id.0, *n, Section::Main))
            .collect();
        entries.extend(
            resolved
                .sideboard
                .iter()
                .map(|(id, n)| (id.0, *n, Section::Sideboard)),
        );
        entries.extend(
            resolved
                .commanders
                .iter()
                .map(|id| (id.0, 1, Section::Commander)),
        );
        entries
    };

    let total: u64 = entries.iter().map(|(_, n, _)| u64::from(*n)).sum();
    if total == 0 {
        return Err("nothing resolved — is the card database imported?".into());
    }

    store
        .put_deck(name, None, &entries)
        .map_err(|e| e.to_string())?;
    println!(
        "saved {name:?}: {total} cards across {} entries",
        entries.len()
    );
    Ok(())
}

fn stats() -> Result<(), String> {
    let store = open()?;
    println!("database   {}", db_path().display());
    println!(
        "cards      {}",
        store.card_count().map_err(|e| e.to_string())?
    );
    println!(
        "subtypes   {}",
        store.subtypes().map_err(|e| e.to_string())?.len()
    );
    println!(
        "decks      {}",
        store.deck_list().map_err(|e| e.to_string())?.len()
    );
    if let Ok(Some(when)) = store.meta("last_import_unix") {
        println!("imported   unix {when}");
    }
    let unused = StoreResolver(&store).suggest("", 0).len();
    let _ = unused;
    Ok(())
}

/// How much of the database the rules-text compiler understands, and what to teach it next.
fn coverage(args: &[String]) -> Result<(), String> {
    use std::collections::BTreeMap;
    let top: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(40);
    // `--list` prints every playable card's text instead, for reviewing what the
    // compiler accepts.
    let list = args.iter().any(|a| a == "--list");
    // `--unparsed` prints every line not understood, one per line, for offline analysis.
    let dump = args.iter().any(|a| a == "--unparsed");
    // `--sole` prints lines that are a card's only obstacle, verbatim.
    let sole_dump = args.iter().any(|a| a == "--sole");
    let store = open()?;
    let subtypes = mtg_oracle::compile::SubtypeNames(store.subtypes().map_err(|e| e.to_string())?);
    let cards = store.all_cards().map_err(|e| e.to_string())?;

    let (mut playable, mut total) = (0usize, 0usize);
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    // Lines that are the only thing standing between a card and playable: teaching one
    // of these converts cards directly.
    let mut sole: BTreeMap<String, usize> = BTreeMap::new();
    for (_, card) in &cards {
        total += 1;
        let mut all = true;
        let mut blockers: Vec<String> = Vec::new();
        for row in &card.faces {
            let types = mtg_oracle::typeline::parse(&row.type_line);
            let compiled = mtg_oracle::compile::compile(
                &mtg_oracle::compile::FaceText {
                    name: &row.name,
                    card_types: &types.card_types,
                    subtypes: &types.subtypes,
                    oracle_text: row.oracle_text.as_deref(),
                    mana_cost: row.mana_cost.as_str(),
                },
                &subtypes,
            );
            for line in &compiled.unparsed {
                if dump && mtg_oracle::compile::layout_understood(card) {
                    println!("{}", line.replace(&row.name, "~"));
                }
                *missing.entry(template(line, &row.name)).or_default() += 1;
                blockers.push(template(line, &row.name));
            }
            all &= compiled.understood()
                && (card.layout != "split"
                    || mtg_oracle::compile::split_abilities_understood(&compiled));
        }
        blockers.dedup();
        if mtg_oracle::compile::layout_understood(card) && blockers.len() == 1 && sole_dump {
            for row in &card.faces {
                let types = mtg_oracle::typeline::parse(&row.type_line);
                let compiled = mtg_oracle::compile::compile(
                    &mtg_oracle::compile::FaceText {
                        name: &row.name,
                        card_types: &types.card_types,
                        subtypes: &types.subtypes,
                        oracle_text: row.oracle_text.as_deref(),
                        mana_cost: row.mana_cost.as_str(),
                    },
                    &subtypes,
                );
                for l in compiled.unparsed {
                    println!("SOLE\t{}", l.replace(&row.name, "~"));
                }
            }
        }
        if mtg_oracle::compile::layout_understood(card) && blockers.len() == 1 {
            *sole.entry(blockers.remove(0)).or_default() += 1;
        }
        // Faces the engine cannot cast are not playable however well they compile.
        let ok = all && mtg_oracle::compile::layout_understood(card);
        playable += usize::from(ok);
        if ok && list {
            for row in &card.faces {
                if let Some(text) = row.oracle_text.as_deref().filter(|t| !t.is_empty()) {
                    println!("== {} [{}]\n{text}", row.name, row.type_line);
                }
            }
        }
    }

    println!(
        "{playable} of {total} cards playable as printed ({:.1}%)",
        100.0 * playable as f64 / total.max(1) as f64
    );
    let mut ranked: Vec<(String, usize)> = missing.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("\nmost common lines not understood:");
    for (line, n) in ranked.into_iter().take(top) {
        println!("{n:6}  {line}");
    }
    let mut sole: Vec<(String, usize)> = sole.into_iter().collect();
    sole.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("\nlines that are a card's only obstacle:");
    for (line, n) in sole.into_iter().take(top) {
        println!("{n:6}  {line}");
    }
    Ok(())
}

/// A line with its specifics blanked, so the same shape on different cards counts once.
fn template(line: &str, name: &str) -> String {
    let s = line.replace(name, "~");
    let mut out = String::with_capacity(s.len());
    let mut in_symbol = false;
    for c in s.chars() {
        match c {
            '{' => {
                in_symbol = true;
                out.push_str("{M}");
            }
            '}' => in_symbol = false,
            _ if in_symbol => {}
            c if c.is_ascii_digit() => {
                if !out.ends_with('N') {
                    out.push('N');
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Compile rules text as if printed on a card of the given type line, and show the result.
/// Each line of input text may be separated by `|`.
fn parse(args: &[String]) -> Result<(), String> {
    let [type_line, text @ ..] = args else {
        return Err("usage: mtg-cards parse <type line> <text>".into());
    };
    let store = open()?;
    let subtypes = mtg_oracle::compile::SubtypeNames(store.subtypes().map_err(|e| e.to_string())?);
    let text = text.join(" ").replace('|', "\n");
    let types = mtg_oracle::typeline::parse(type_line);
    let compiled = mtg_oracle::compile::compile(
        &mtg_oracle::compile::FaceText {
            name: "Test Card",
            card_types: &types.card_types,
            subtypes: &types.subtypes,
            oracle_text: Some(&text),
            mana_cost: "",
        },
        &subtypes,
    );
    for a in &compiled.abilities {
        println!("{:#?}", a.kind);
    }
    for line in &compiled.unparsed {
        println!("NOT UNDERSTOOD: {line}");
    }
    Ok(())
}

#[cfg(test)]
mod deck_tests {
    use super::*;

    #[test]
    fn unsupported_command_sections_preserve_the_existing_deck() {
        let mut store = Store::in_memory().unwrap();
        let id = store.put_deck("Existing", None, &[]).unwrap();
        let original = store.deck(id).unwrap();
        for (text, expected) in [
            (
                "Commander\n1 First Captain\n1 Second Captain",
                "multiple commanders",
            ),
            ("Commander\n2 First Captain", "multiple commanders"),
            ("Companion\n1 Traveling Friend", "companion designation"),
        ] {
            let error = save_deck(&mut store, "Existing", text).unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert_eq!(store.deck(id).unwrap(), original);
        }
    }

    #[test]
    fn large_import_totals_and_trimmed_names_are_preserved() {
        let mut store = Store::in_memory().unwrap();
        let card = mtg_store::StoredCard {
            oracle_uuid: "invented-import-card".into(),
            name: "Invented Scout".into(),
            layout: "normal".into(),
            faces: vec![mtg_store::FaceRow {
                name: "Invented Scout".into(),
                ..Default::default()
            }],
        };
        let oracle = store
            .put_card(&card, &[mtg_deck::resolve::normalise(&card.name)])
            .unwrap();
        save_deck(
            &mut store,
            "  Scouts  ",
            "4294967295 Invented Scout\nSideboard\n4294967295 Invented Scout",
        )
        .unwrap();
        let decks = store.deck_list().unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].1, "Scouts");
        let deck = store.deck(decks[0].0).unwrap().unwrap();
        assert_eq!(deck.entries.len(), 2);
        assert!(
            deck.entries
                .iter()
                .all(|(id, count, _)| *id == oracle && *count == u32::MAX)
        );
        assert!(
            save_deck(&mut store, "  ", "1 Invented Scout")
                .unwrap_err()
                .contains("name")
        );
        assert_eq!(store.deck_list().unwrap(), decks);
    }
}
