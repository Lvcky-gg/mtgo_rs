use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn collective_counters_distinguish_spells_abilities_and_controllers() {
    for (text, expected) in [
        ("Counter all abilities.", [26, 21]),
        ("Counter all abilities your opponents control.", [27, 21]),
        ("Counter all spells your opponents control.", [27, 22]),
        (
            "Exile all other spells and counter all abilities.",
            [21, 21],
        ),
    ] {
        let mut t = Table::default();
        let watcher = t.card(
            "{0}",
            "Enchantment",
            None,
            "Whenever a player casts a spell, you gain 1 life.",
        );
        let spell = t.card("{0}", "Instant", None, "You gain 5 life.");
        let counter = t.card("{0}", "Instant", None, text);
        let mut g = Game::new(t);
        g.put(watcher, P0, Zone::Battlefield);
        g.put(watcher, P1, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        let counter = g.put(counter, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: spell }, &[]);
        assert_eq!(g.stack().len(), 3);
        g.cast(counter, &[]);
        assert_eq!([g.life(P0), g.life(P1)], expected, "{text}");
        assert!(g.stack().is_empty());
    }
}
