//! "If one or more tokens would be created under your control, twice that many …"

use super::harness::*;
use mtg_core::{PlayerId, Zone};

const PROCESSION: &str = "If an effect would create one or more tokens under your control, it creates twice that many of those tokens instead.";
const OJER: &str = "If one or more creature tokens would be created under your control, three times that many of those tokens are created instead.";

/// How many tokens you gain by casting `text`, with the given doublers on each side.
fn tokens(text: &str, mine: &[&str], theirs: &[&str]) -> usize {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, text);
    let mine: Vec<_> = mine
        .iter()
        .map(|d| t.card("{0}", "Enchantment", None, d))
        .collect();
    let theirs: Vec<_> = theirs
        .iter()
        .map(|d| t.card("{0}", "Enchantment", None, d))
        .collect();
    let mut g = Game::new(t);
    for d in mine {
        g.put(d, P0, Zone::Battlefield);
    }
    for d in theirs {
        g.put(d, P1, Zone::Battlefield);
    }
    let before = count_tokens(&g, P0);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    count_tokens(&g, P0) - before
}

fn count_tokens(g: &Game, who: PlayerId) -> usize {
    g.engine
        .state
        .battlefield()
        .iter()
        .filter(|id| {
            let o = &g.engine.state.objects[id];
            o.card.0 >= mtg_ir::walk::TOKEN_CARD_BASE && o.controller == who
        })
        .count()
}

const SOLDIERS: &str = "Create two 1/1 white Soldier creature tokens.";
const TREASURE: &str = "Create a Treasure token.";

#[test]
fn one_doubler_doubles_and_two_quadruple() {
    assert_eq!(tokens(SOLDIERS, &[], &[]), 2);
    assert_eq!(tokens(SOLDIERS, &[PROCESSION], &[]), 4);
    assert_eq!(tokens(SOLDIERS, &[PROCESSION, PROCESSION], &[]), 8);
}

#[test]
fn a_creature_token_tripler_ignores_other_tokens() {
    assert_eq!(tokens(SOLDIERS, &[OJER], &[]), 6);
    assert_eq!(tokens(TREASURE, &[OJER], &[]), 1);
    assert_eq!(tokens(TREASURE, &[PROCESSION, OJER], &[]), 2);
}

#[test]
fn an_opponents_doubler_does_not_double_yours() {
    assert_eq!(tokens(SOLDIERS, &[], &[PROCESSION]), 2);
}
