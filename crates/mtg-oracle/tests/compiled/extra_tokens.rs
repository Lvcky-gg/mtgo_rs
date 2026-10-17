//! "… those tokens plus that many 1/1 green Squirrel creature tokens are created instead."
//! (Chatterfang), "… plus an additional Treasure token" (Xorn).

use super::harness::*;
use mtg_core::Zone;

const CHATTERFANG: &str = "If one or more tokens would be created under your control, those tokens plus that many 1/1 green Squirrel creature tokens are created instead.";
const XORN: &str = "If you would create one or more Treasure tokens, instead create those tokens plus an additional Treasure token.";

fn tokens_after(static_text: &str, spell_text: &str) -> usize {
    let mut t = Table::default();
    let s = t.card("{0}", "Enchantment", None, static_text);
    let spell = t.card("{0}", "Sorcery", None, spell_text);
    let mut g = Game::new(t);
    g.put(s, P0, Zone::Battlefield);
    let c = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(c, &[]);
    g.engine
        .state
        .battlefield()
        .iter()
        .filter(|id| g.engine.state.objects[id].card.0 >= mtg_ir::walk::TOKEN_CARD_BASE)
        .count()
}

#[test]
fn chatterfang_adds_a_squirrel_per_token() {
    assert_eq!(tokens_after(CHATTERFANG, "Create two Treasure tokens."), 4);
}

#[test]
fn xorn_adds_one_treasure_only_to_treasures() {
    assert_eq!(tokens_after(XORN, "Create two Treasure tokens."), 3);
    assert_eq!(
        tokens_after(XORN, "Create a 1/1 white Soldier creature token."),
        1
    );
}
