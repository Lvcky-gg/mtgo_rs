use super::harness::*;
use mtg_core::{Keyword, Target, Zone};
use std::collections::BTreeMap;

#[test]
fn pump_or_keyword_and_unblockable_affects_only_the_target() {
    for keyword in [false, true] {
        let mut t = Table::default();
        let spell = t.card(
            "{U}",
            "Instant",
            None,
            if keyword {
                "Target creature gains haste until end of turn and can't be blocked this turn."
            } else {
                "Target creature gets +1/+0 until end of turn and can't be blocked this turn."
            },
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let target = g.put(bear, P0, Zone::Battlefield);
        let other = g.put(bear, P0, Zone::Battlefield);
        let blocker = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[Target::Object(target)]);
        assert_eq!(g.pt(target), if keyword { (2, 2) } else { (3, 2) });
        assert_eq!(g.has(target, Keyword::Haste), keyword);
        assert_eq!(g.pt(other), (2, 2));
        g.engine
            .state
            .combat
            .attackers
            .insert(target, Target::Player(P1));
        g.engine
            .state
            .combat
            .attackers
            .insert(other, Target::Player(P1));
        let blocked_target = BTreeMap::from([(target, vec![blocker])]);
        let blocked_other = BTreeMap::from([(other, vec![blocker])]);
        assert!(
            mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P1, &blocked_target)
                .is_err()
        );
        assert!(
            mtg_engine::combat::validate_blocks(&g.engine.state, &g.table, P1, &blocked_other)
                .is_ok()
        );
    }
}
