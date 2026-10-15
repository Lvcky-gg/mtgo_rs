use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn descend_counts_only_permanent_cards_in_your_graveyard_and_rechecks_live_state() {
    let mut t = Table::default();
    let capybara = t.card("{0}", "Creature", Some((3, 3)), "This creature gets +3/+0 as long as there are four or more permanent cards in your graveyard.");
    let permanent = t.card("{0}", "Artifact", None, "");
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let exile = t.card(
        "{0}",
        "Instant",
        None,
        "Exile target card from a graveyard.",
    );
    let mut g = Game::new(t);
    let capybara = g.put(capybara, P0, Zone::Battlefield);
    let dead: Vec<_> = (0..4)
        .map(|_| g.put(permanent, P0, Zone::Graveyard))
        .collect();
    g.put(permanent, P1, Zone::Graveyard);
    g.put(instant, P0, Zone::Graveyard);
    let exile = g.put(exile, P0, Zone::Hand);
    g.main();
    assert_eq!(g.pt(capybara), (6, 3));
    g.cast(exile, &[Target::Object(dead[0])]);
    assert_eq!(g.pt(capybara), (3, 3));
}

#[test]
fn regenesis_returns_two_permanent_cards_and_keeps_other_graveyard_cards() {
    let mut t = Table::default();
    let permanent = t.card("{0}", "Artifact", None, "");
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let return_all = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return up to two target permanent cards from your graveyard to your hand.",
    );
    let mut g = Game::new(t);
    let dead = [
        g.put(permanent, P0, Zone::Graveyard),
        g.put(permanent, P0, Zone::Graveyard),
    ];
    let theirs = g.put(permanent, P1, Zone::Graveyard);
    let instant = g.put(instant, P0, Zone::Graveyard);
    let spell = g.put(return_all, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(dead[0]), Target::Object(dead[1])]);
    assert_eq!(
        g.engine
            .state
            .objects
            .values()
            .filter(|o| o.card == permanent && o.zone == mtg_core::ZoneRef::of(Zone::Hand, P0))
            .count(),
        2
    );
    assert_eq!(g.engine.state.objects[&theirs].zone.zone, Zone::Graveyard);
    assert_eq!(g.engine.state.objects[&instant].zone.zone, Zone::Graveyard);
}
