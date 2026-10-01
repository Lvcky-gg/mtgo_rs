//! Classes (CR 716): levels gained as a sorcery, each adding its abilities.
use super::harness::*;
use mtg_core::{Keyword, Zone};
use mtg_engine::actions::Action;

#[test]
fn a_class_gains_its_levels_in_order() {
    let mut t = Table::default();
    let class = t.card(
        "{W}",
        "Enchantment — Class",
        None,
        "(Gain the next level as a sorcery to add its ability.)\n\
         Creatures you control get +1/+0.\n\
         {1}{W}: Level 2\n\
         When this Class becomes level 2, you gain 3 life.\n\
         {2}: Level 3\n\
         Creatures you control have flying.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    let c = g.put(class, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    let actions = g.main();
    assert_eq!(g.pt(b), (3, 2), "level 1");
    assert!(!g.has(b, Keyword::Flying));
    // Level 3 is not offered before level 2.
    assert!(actions.contains(&activate(c, 1)), "level 2");
    assert!(!actions.contains(&activate(c, 3)), "not level 3 yet");
    g.act(activate(c, 1), &[], &[]);
    assert_eq!(g.life(P0), 23, "becomes level 2");
    let actions: Vec<Action> = g.main();
    assert!(!actions.contains(&activate(c, 1)), "level 2 only once");
    g.act(activate(c, 3), &[], &[]);
    assert!(g.has(b, Keyword::Flying), "level 3");
    assert_eq!(g.pt(b), (3, 2));
}
