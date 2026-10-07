use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn vela_triggers_for_self_and_friendly_departures_only() {
    for kind in 0..4 {
        let mut t = Table::default();
        let vela = t.card("{4}{U}{B}", "Creature — Wizard", Some((4, 4)),
            "Intimidate\nOther creatures you control have intimidate.\nWhenever this creature or another creature you control leaves the battlefield, each opponent loses 1 life.");
        let bounce = t.card(
            "{U}",
            "Instant",
            None,
            "Return target creature to its owner's hand.",
        );
        let destroy = t.card("{B}", "Instant", None, "Destroy target creature.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let source = g.put(vela, P0, Zone::Battlefield);
        let friendly = g.put(bear, P0, Zone::Battlefield);
        let opposing = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(if kind == 3 { destroy } else { bounce }, P0, Zone::Hand);
        g.main();
        let target = match kind {
            0 => source,
            2 => opposing,
            _ => friendly,
        };
        g.cast(spell, &[Target::Object(target)]);
        assert_eq!(
            g.engine.state.players[&P1].life,
            if kind == 2 { 20 } else { 19 }
        );
        assert_eq!(g.engine.state.players[&P0].life, 20);
    }
}

#[test]
fn simultaneous_source_and_other_deaths_use_last_known_information() {
    let mut t = Table::default();
    let source = t.card("{2}", "Creature — Wizard", Some((2, 2)),
        "Whenever this creature or another creature you control leaves the battlefield, each opponent loses 1 life.");
    let wipe = t.card("{1}", "Sorcery", None, "Destroy all creatures.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    g.put(source, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Battlefield);
    g.put(bear, P1, Zone::Battlefield);
    let spell = g.put(wipe, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert_eq!(g.engine.state.players[&P1].life, 18);
}
