//! Mirror-Mad Phantasm: "{1}{U}: This creature's owner shuffles it into their library. If
//! that player does, they reveal cards from the top of that library until a card named
//! Mirror-Mad Phantasm is revealed. The player puts that card onto the battlefield and all
//! other cards revealed this way into their graveyard."

use super::harness::*;
use mtg_core::Zone;

const PHANTASM: &str = "Flying\n{1}{U}: This creature's owner shuffles it into their library. \
                        If that player does, they reveal cards from the top of that library \
                        until a card named ~ is revealed. The player puts that card onto the \
                        battlefield and all other cards revealed this way into their graveyard.";

#[test]
fn shuffles_in_and_mills_down_to_itself() {
    let mut t = Table::default();
    let phantasm = t.named_card(
        "Mirror-Mad Phantasm",
        "{3}{U}{U}",
        "Creature — Spirit",
        Some((5, 1)),
        PHANTASM,
    );
    let mut g = Game::new(t);
    g.lands(2);
    let p = g.put(phantasm, P0, Zone::Battlefield);
    g.main();
    let library = g.count(Zone::Library, P0);
    g.act(activate(p, 1), &[], &[]);
    let back = g.find(phantasm).expect("back on the battlefield");
    assert_ne!(back, p, "a new object");
    let milled = g.count(Zone::Graveyard, P0);
    assert_eq!(g.count(Zone::Library, P0) + milled, library);
    assert!(
        g.engine.state.objects[&back].zone.zone == Zone::Battlefield
            && g.engine.state.objects[&back].owner == P0
    );
}
