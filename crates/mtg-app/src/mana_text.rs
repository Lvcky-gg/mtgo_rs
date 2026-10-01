//! Text with mana symbols in it: `{2}{W}`, `{T}: Add {G}.`
//!
//! Costs, rules text, action labels and the mana pool all write symbols the way they are printed
//! in Scryfall's data — in braces. This splits such a string into text and symbols and flows the
//! pieces into lines, so the renderer can draw each symbol as an image at the right place.
//!
//! Pure, and separate from drawing, for the same reason as `board` and `format`: layout is where a
//! renderer goes wrong, and it can be tested without a window when it takes a measuring function
//! instead of a font.

/// One piece of a string: plain text, or a symbol's code without its braces (`"W"`, `"2/W"`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece<'a> {
    Text(&'a str),
    Symbol(&'a str),
}

/// Split a string into text and `{...}` symbols.
///
/// A brace that does not open a well-formed symbol is left as text, so a stray `{` in rules text
/// prints as itself rather than swallowing the rest of the line.
pub fn parse(s: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    // Text runs from `text_start` up to the next real symbol; a brace that turns out not to be
    // one is simply skipped over, so it stays inside the run.
    let (mut text_start, mut i) = (0, 0);
    while let Some(offset) = s[i..].find('{') {
        let open = i + offset;
        match s[open + 1..].find('}') {
            Some(len) if is_symbol_code(&s[open + 1..open + 1 + len]) => {
                if open > text_start {
                    out.push(Piece::Text(&s[text_start..open]));
                }
                out.push(Piece::Symbol(&s[open + 1..open + 1 + len]));
                i = open + len + 2;
                text_start = i;
            }
            _ => i = open + 1,
        }
    }
    if text_start < s.len() {
        out.push(Piece::Text(&s[text_start..]));
    }
    out
}

/// Whether `code` is a symbol Scryfall draws: letters, digits, `/`, and the odd `½` or `∞`.
fn is_symbol_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 8
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '/' | '½' | '∞'))
}

/// The file name Scryfall uses for a symbol: the code with its slashes removed.
/// `W/U` → `WU`, `2/W` → `2W`, `W/P` → `WP`. `None` for a code that could not be a symbol, so the
/// name is always a safe single path component.
pub fn symbol_file(code: &str) -> Option<String> {
    if !is_symbol_code(code) || !code.is_ascii() {
        return None;
    }
    Some(code.replace('/', ""))
}

/// The colours a symbol is drawn in when its image is not available: one per coloured half.
pub fn symbol_colors(code: &str) -> Vec<char> {
    code.split('/')
        .filter_map(|part| match part {
            "W" | "U" | "B" | "R" | "G" => part.chars().next(),
            _ => None,
        })
        .collect()
}

/// At most `max` characters of `text`, never ending inside a symbol, with `…` if anything was
/// cut. For the short rules line on a board panel, where a half symbol would print as `{T`.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut end = text.char_indices().nth(max).map_or(text.len(), |(i, _)| i);
    // Back up to before an opening brace that is not closed within the kept part.
    if let Some(open) = text[..end].rfind('{')
        && !text[open..end].contains('}')
    {
        end = open;
    }
    format!("{}…", text[..end].trim_end())
}

/// Something positioned by [`flow`], relative to the top-left of the laid-out block.
#[derive(Clone, PartialEq, Debug)]
pub struct Placed<'a> {
    pub piece: Piece<'a>,
    pub x: f32,
    pub line: usize,
}

/// Lay pieces out left to right, wrapping at `max_width` between words and symbols.
///
/// `measure` gives the width of a run of text. Symbols are `symbol` wide. Text is broken at
/// spaces, so a wrapped line never starts halfway through a word; a single word wider than the
/// line is placed anyway rather than lost. Newlines in the text start a new line.
pub fn flow<'a>(
    pieces: &[Piece<'a>],
    measure: impl Fn(&str) -> f32,
    symbol: f32,
    max_width: f32,
) -> (Vec<Placed<'a>>, usize) {
    let mut cursor = Cursor {
        placed: Vec::new(),
        x: 0.0,
        line: 0,
        max_width,
    };

    for piece in pieces {
        match *piece {
            Piece::Symbol(_) => cursor.place(*piece, symbol),
            Piece::Text(text) => {
                for (i, paragraph) in text.split('\n').enumerate() {
                    if i > 0 {
                        cursor.newline();
                    }
                    for word in paragraph.split_inclusive(' ') {
                        // A wrapped line should not begin with the space that caused the wrap.
                        let word = if cursor.x == 0.0 {
                            word.trim_start()
                        } else {
                            word
                        };
                        if !word.is_empty() {
                            cursor.place(Piece::Text(word), measure(word));
                        }
                    }
                }
            }
        }
    }

    let lines = if cursor.placed.is_empty() {
        0
    } else {
        cursor.line + 1
    };
    (cursor.placed, lines)
}

struct Cursor<'a> {
    placed: Vec<Placed<'a>>,
    x: f32,
    line: usize,
    max_width: f32,
}

impl<'a> Cursor<'a> {
    fn place(&mut self, piece: Piece<'a>, width: f32) {
        if self.x > 0.0 && self.x + width > self.max_width {
            self.newline();
        }
        self.placed.push(Placed {
            piece,
            x: self.x,
            line: self.line,
        });
        self.x += width;
    }

    fn newline(&mut self) {
        self.line += 1;
        self.x = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cost_is_all_symbols() {
        assert_eq!(
            parse("{2}{W}{U}"),
            vec![Piece::Symbol("2"), Piece::Symbol("W"), Piece::Symbol("U")]
        );
    }

    #[test]
    fn rules_text_mixes_symbols_and_words() {
        assert_eq!(
            parse("{T}: Add {G}."),
            vec![
                Piece::Symbol("T"),
                Piece::Text(": Add "),
                Piece::Symbol("G"),
                Piece::Text(".")
            ]
        );
    }

    #[test]
    fn hybrid_and_phyrexian_codes_keep_their_slash() {
        assert_eq!(
            parse("{W/U}{2/G}{B/P}"),
            vec![
                Piece::Symbol("W/U"),
                Piece::Symbol("2/G"),
                Piece::Symbol("B/P")
            ]
        );
    }

    #[test]
    fn a_brace_that_is_not_a_symbol_stays_text() {
        let pieces = parse("a {not a symbol} and {R}");
        assert!(pieces.contains(&Piece::Symbol("R")));
        let text: String = pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Text(t) => Some(*t),
                _ => None,
            })
            .collect();
        assert_eq!(text, "a {not a symbol} and ");
        assert_eq!(parse("unclosed {W"), vec![Piece::Text("unclosed {W")]);
    }

    #[test]
    fn file_names_follow_scryfall_and_are_safe() {
        assert_eq!(symbol_file("W").as_deref(), Some("W"));
        assert_eq!(symbol_file("W/U").as_deref(), Some("WU"));
        assert_eq!(symbol_file("2/W").as_deref(), Some("2W"));
        assert_eq!(symbol_file("10").as_deref(), Some("10"));
        assert_eq!(symbol_file("../x"), None);
        assert_eq!(symbol_file("w"), None, "lowercase is not a symbol");
    }

    #[test]
    fn fallback_colours_come_from_each_half() {
        assert_eq!(symbol_colors("W"), vec!['W']);
        assert_eq!(symbol_colors("R/G"), vec!['R', 'G']);
        assert_eq!(symbol_colors("2/B"), vec!['B']);
        assert!(symbol_colors("3").is_empty());
    }

    #[test]
    fn truncation_never_splits_a_symbol() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(
            truncate("abc {T}: Add {G}.", 6),
            "abc…",
            "cut before the half-kept {{T}}"
        );
        assert_eq!(truncate("abc {T}: Add {G}.", 8), "abc {T}:…");
    }

    /// Every character is 1 wide, so widths are easy to reason about.
    fn chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    #[test]
    fn a_short_line_does_not_wrap() {
        let (placed, lines) = flow(&parse("{T}: Add {G}."), chars, 2.0, 100.0);
        assert_eq!(lines, 1);
        let xs: Vec<f32> = placed.iter().map(|p| p.x).collect();
        // {T} is 2 wide, ": " 2, "Add " 4, {G} 2, then ".".
        assert_eq!(xs, vec![0.0, 2.0, 4.0, 8.0, 10.0]);
    }

    #[test]
    fn long_text_wraps_between_words_and_never_starts_a_line_with_a_space() {
        let (placed, lines) = flow(&parse("aaa bbb ccc"), chars, 2.0, 8.0);
        assert_eq!(lines, 2);
        let second: Vec<_> = placed.iter().filter(|p| p.line == 1).collect();
        assert_eq!(second[0].x, 0.0);
        assert_eq!(second[0].piece, Piece::Text("ccc"));
    }

    #[test]
    fn a_symbol_wraps_like_a_word() {
        let (placed, lines) = flow(&parse("abcd{W}"), chars, 2.0, 5.0);
        assert_eq!(lines, 2);
        assert_eq!(
            placed[1],
            Placed {
                piece: Piece::Symbol("W"),
                x: 0.0,
                line: 1
            }
        );
    }

    #[test]
    fn newlines_start_a_new_line() {
        let (placed, lines) = flow(&parse("one\ntwo"), chars, 2.0, 100.0);
        assert_eq!(lines, 2);
        assert_eq!(
            placed[1],
            Placed {
                piece: Piece::Text("two"),
                x: 0.0,
                line: 1
            }
        );
    }

    #[test]
    fn an_empty_string_is_no_lines() {
        assert_eq!(flow(&parse(""), chars, 2.0, 10.0).1, 0);
    }
}
