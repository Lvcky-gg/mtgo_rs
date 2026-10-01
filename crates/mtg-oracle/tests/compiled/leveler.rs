//! Level up (CR 702.87, 711): brackets that switch on as level counters pile up.
use super::harness::*;
use mtg_core::{Keyword, Zone};

#[test]
fn leveling_up_climbs_through_the_brackets() {
    let mut t = Table::default();
    let student = t.card(
        "{W}",
        "Creature — Human Soldier",
        Some((1, 1)),
        "Level up {W} ({W}: Put a level counter on this. Level up only as a sorcery.)\n\
         LEVEL 2-3\n3/3\nFirst strike\nLEVEL 4+\n4/4\nDouble strike",
    );
    let mut g = Game::new(t);
    g.lands(4);
    let s = g.put(student, P0, Zone::Battlefield);
    g.main();
    assert_eq!(g.pt(s), (1, 1));
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (1, 1), "level 1: no bracket yet");
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (3, 3), "level 2");
    assert!(g.has(s, Keyword::FirstStrike));
    assert!(!g.has(s, Keyword::DoubleStrike));
    g.act(activate(s, 0), &[], &[]);
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.pt(s), (4, 4), "level 4");
    assert!(g.has(s, Keyword::DoubleStrike));
    assert!(!g.has(s, Keyword::FirstStrike), "the old bracket is gone");
}
