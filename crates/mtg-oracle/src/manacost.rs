//! Parsing a printed mana cost.
//!
//! Costs arrive as brace-delimited symbols: `{2}{W}{U}`, `{X}{R}`, `{W/U}`, `{2/W}`,
//! `{W/P}`, `{C}`, `{S}`. The grammar is small and closed, so this is a scanner rather
//! than a parser — but the hybrid forms are where a naive split on `/` goes wrong, since
//! `{2/W}` and `{W/U}` and `{W/P}` all contain a slash and mean three different things.

use mtg_core::{Color, ManaCost, ManaSymbol};

/// What a cost string could not be understood as.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unparsed {
    /// Symbols that were not recognised, e.g. `"{Q}"`. Reported rather than skipped so
    /// an import can flag a card it did not fully read.
    pub symbols: Vec<String>,
}

/// Parse a mana cost, returning it alongside anything unrecognised.
pub fn parse(cost: &str) -> (ManaCost, Unparsed) {
    let mut symbols = Vec::new();
    let mut unparsed = Vec::new();

    for token in tokens(cost) {
        match symbol(&token) {
            Some(s) => symbols.push(s),
            None => unparsed.push(format!("{{{token}}}")),
        }
    }

    (ManaCost { symbols }, Unparsed { symbols: unparsed })
}

/// The contents of each `{...}` group, in order.
fn tokens(cost: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;

    for c in cost.chars() {
        match c {
            '{' => current = Some(String::new()),
            '}' => {
                if let Some(t) = current.take() {
                    out.push(t);
                }
            }
            _ => {
                if let Some(t) = current.as_mut() {
                    t.push(c);
                }
            }
        }
    }

    out
}

fn symbol(token: &str) -> Option<ManaSymbol> {
    // Generic: a bare number.
    if let Ok(n) = token.parse::<u8>() {
        return Some(ManaSymbol::Generic(n));
    }

    // Hybrids, split on the slash. Three distinct shapes share the punctuation.
    if let Some((a, b)) = token.split_once('/') {
        // Phyrexian: `{W/P}` — payable with mana or life.
        if b.eq_ignore_ascii_case("P") {
            return color(a).map(ManaSymbol::Phyrexian);
        }
        // Mono-hybrid: `{2/W}` — payable as that much generic, or one colored.
        if let Ok(n) = a.parse::<u8>() {
            return color(b).map(|c| ManaSymbol::MonoHybrid(n, c));
        }
        // Ordinary hybrid: `{W/U}` — either half.
        return match (color(a), color(b)) {
            (Some(x), Some(y)) => Some(ManaSymbol::Hybrid(x, y)),
            _ => None,
        };
    }

    match token.to_ascii_uppercase().as_str() {
        "X" | "Y" | "Z" => Some(ManaSymbol::Variable),
        "C" => Some(ManaSymbol::Colorless),
        "S" => Some(ManaSymbol::Snow),
        other => color(other).map(ManaSymbol::Colored),
    }
}

fn color(s: &str) -> Option<Color> {
    Some(match s.to_ascii_uppercase().as_str() {
        "W" => Color::White,
        "U" => Color::Blue,
        "B" => Color::Black,
        "R" => Color::Red,
        "G" => Color::Green,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Color::{Black as B, Blue as U, Green as G, Red as R, White as W};

    fn ok(cost: &str) -> Vec<ManaSymbol> {
        let (c, u) = parse(cost);
        assert!(
            u.symbols.is_empty(),
            "{cost} had unparsed symbols: {:?}",
            u.symbols
        );
        c.symbols
    }

    #[test]
    fn an_empty_cost_is_free() {
        assert!(ok("").is_empty());
    }

    #[test]
    fn generic_and_colored() {
        assert_eq!(
            ok("{2}{W}{U}"),
            vec![
                ManaSymbol::Generic(2),
                ManaSymbol::Colored(W),
                ManaSymbol::Colored(U)
            ]
        );
    }

    #[test]
    fn all_five_colors() {
        assert_eq!(
            ok("{W}{U}{B}{R}{G}"),
            vec![
                ManaSymbol::Colored(W),
                ManaSymbol::Colored(U),
                ManaSymbol::Colored(B),
                ManaSymbol::Colored(R),
                ManaSymbol::Colored(G),
            ]
        );
    }

    #[test]
    fn zero_is_a_generic_cost_not_an_absent_one() {
        assert_eq!(ok("{0}"), vec![ManaSymbol::Generic(0)]);
    }

    #[test]
    fn multi_digit_generic() {
        assert_eq!(ok("{15}"), vec![ManaSymbol::Generic(15)]);
    }

    #[test]
    fn variable_costs() {
        assert_eq!(
            ok("{X}{R}"),
            vec![ManaSymbol::Variable, ManaSymbol::Colored(R)]
        );
        // Cards with two variables use Y and Z for the later ones.
        assert_eq!(
            ok("{X}{Y}"),
            vec![ManaSymbol::Variable, ManaSymbol::Variable]
        );
    }

    #[test]
    fn colorless_is_not_generic() {
        // {C} requires colorless mana specifically; {1} accepts anything.
        assert_eq!(ok("{C}"), vec![ManaSymbol::Colorless]);
        assert_ne!(ok("{C}"), ok("{1}"));
    }

    #[test]
    fn snow() {
        assert_eq!(ok("{S}"), vec![ManaSymbol::Snow]);
    }

    // The three slash-bearing shapes, which is where a naive split goes wrong.

    #[test]
    fn ordinary_hybrid() {
        assert_eq!(ok("{W/U}"), vec![ManaSymbol::Hybrid(W, U)]);
    }

    #[test]
    fn mono_hybrid_is_generic_or_colored() {
        assert_eq!(ok("{2/W}"), vec![ManaSymbol::MonoHybrid(2, W)]);
    }

    #[test]
    fn phyrexian_is_mana_or_life() {
        assert_eq!(ok("{W/P}"), vec![ManaSymbol::Phyrexian(W)]);
    }

    #[test]
    fn the_three_slash_forms_are_distinguished() {
        // All three contain a slash and mean different things.
        assert_eq!(ok("{W/U}{2/W}{W/P}").len(), 3);
        assert_eq!(
            ok("{W/U}{2/W}{W/P}"),
            vec![
                ManaSymbol::Hybrid(W, U),
                ManaSymbol::MonoHybrid(2, W),
                ManaSymbol::Phyrexian(W),
            ]
        );
    }

    #[test]
    fn an_unknown_symbol_is_reported_and_the_rest_still_parses() {
        let (cost, unparsed) = parse("{W}{Q}{U}");
        assert_eq!(
            cost.symbols,
            vec![ManaSymbol::Colored(W), ManaSymbol::Colored(U)]
        );
        assert_eq!(unparsed.symbols, vec!["{Q}"]);
    }

    #[test]
    fn text_outside_braces_is_ignored() {
        assert_eq!(
            ok("  {W} {U} "),
            vec![ManaSymbol::Colored(W), ManaSymbol::Colored(U)]
        );
    }

    #[test]
    fn mana_value_comes_out_right() {
        // Sanity-check the round trip into the engine's own accessor.
        let (cost, _) = parse("{2}{W}{U}");
        assert_eq!(cost.mana_value(), 4);
        let (x, _) = parse("{X}{R}");
        assert_eq!(x.mana_value(), 1, "X counts as zero outside the stack");
        let (hybrid, _) = parse("{2/W}{W/U}");
        assert_eq!(
            hybrid.mana_value(),
            3,
            "mono-hybrid counts its generic half"
        );
    }
}
