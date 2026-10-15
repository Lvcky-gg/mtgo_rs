//! Casting from the graveyard by a granted permission: Snapcaster Mage's flashback,
//! Yawgmoth's Will, and Vohar, Vodalian Desecrator.

use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const SNAPCASTER: &str = "Flash\nWhen this creature enters, target instant or sorcery card in \
                          your graveyard gains flashback until end of turn. The flashback cost \
                          is equal to its mana cost.";
const WILL: &str = "Until end of turn, you may play lands and cast spells from your \
                    graveyard.\nIf a card would be put into your graveyard from anywhere this \
                    turn, exile that card instead.";
const VOHAR: &str = "{T}: Draw a card, then discard a card. If you discarded an instant or \
                     sorcery card this way, each opponent loses 1 life and you gain 1 life.\n{2}, \
                     Sacrifice ~: You may cast target instant or sorcery card from your \
                     graveyard this turn. If that spell would be put into your graveyard, exile \
                     it instead. Activate only as a sorcery.";

fn castable(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

fn exiled(g: &Game) -> usize {
    g.engine
        .state
        .objects_in(ZoneRef::shared(Zone::Exile))
        .len()
}

#[test]
fn snapcaster_gives_flashback_for_its_mana_cost_then_it_is_exiled() {
    let mut t = Table::default();
    let snap = t.card(
        "{1}{U}",
        "Creature — Human Wizard",
        Some((2, 1)),
        SNAPCASTER,
    );
    let gift = t.card("{1}{W}", "Sorcery", None, "You gain 3 life.");
    let other = t.card("{R}", "Instant", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(4);
    let snap = g.put(snap, P0, Zone::Hand);
    let gift = g.put(gift, P0, Zone::Graveyard);
    let other = g.put(other, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(!castable(&actions, gift), "no flashback yet");
    g.cast(snap, &[Target::Object(gift)]);
    let actions = g.main();
    assert!(castable(&actions, gift));
    assert!(!castable(&actions, other), "only the target");
    g.cast(gift, &[]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(exiled(&g), 1, "flashback exiles it");
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn yawgmoths_will_replays_the_graveyard_and_exiles_what_would_return() {
    let mut t = Table::default();
    let will = t.card("{0}", "Sorcery", None, WILL);
    let gift = t.card("{0}", "Sorcery", None, "You gain 3 life.");
    let land = t.card("", "Land", None, "");
    let mut g = Game::new(t);
    let will = g.put(will, P0, Zone::Hand);
    let gift = g.put(gift, P0, Zone::Graveyard);
    let land = g.put(land, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(!castable(&actions, gift));
    g.cast(will, &[]);
    assert_eq!(exiled(&g), 1, "the Will exiles itself");
    let actions = g.main();
    assert!(castable(&actions, gift));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::PlayLand { object } if *object == land))
    );
    g.cast(gift, &[]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(exiled(&g), 2, "it would go back to the graveyard");
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "only the land");
    // Over at end of turn.
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |g| g.engine.state.active_player == P1);
    assert!(g.engine.state.graveyard_exiled.is_empty());
}

#[test]
fn vohar_loots_and_drains_only_for_instants_and_sorceries() {
    for (discard_instant, drained) in [(true, true), (false, false)] {
        let mut t = Table::default();
        let vohar = t.card(
            "{U}{B}",
            "Legendary Creature — Phyrexian Merfolk Wizard",
            Some((1, 2)),
            VOHAR,
        );
        let instant = t.card("{U}", "Instant", None, "You gain 1 life.");
        let bear = t.bear();
        let mut g = Game::new(t);
        let v = g.put(vohar, P0, Zone::Battlefield);
        let instant = g.put(instant, P0, Zone::Hand);
        let creature = g.put(bear, P0, Zone::Hand);
        g.main();
        let toss = if discard_instant { instant } else { creature };
        g.act(activate(v, 0), &[], &[Answer::Objects(vec![toss])]);
        let expected = if drained { (21, 19) } else { (20, 20) };
        assert_eq!((g.life(P0), g.life(P1)), expected, "{discard_instant}");
    }
}

#[test]
fn vohar_sacrificed_casts_a_graveyard_spell_that_is_then_exiled() {
    let mut t = Table::default();
    let vohar = t.card(
        "{U}{B}",
        "Legendary Creature — Phyrexian Merfolk Wizard",
        Some((1, 2)),
        VOHAR,
    );
    let gift = t.card("{1}", "Sorcery", None, "You gain 3 life.");
    let mut g = Game::new(t);
    g.lands(3);
    let v = g.put(vohar, P0, Zone::Battlefield);
    let gift = g.put(gift, P0, Zone::Graveyard);
    let actions = g.main();
    let sac = actions
        .iter()
        .find(|a| matches!(a, Action::ActivateAbility { source, ability } if *source == v && ability.0 != 0))
        .cloned()
        .expect("offered");
    g.act(sac, &[Target::Object(gift)], &[]);
    let actions = g.main();
    assert!(castable(&actions, gift));
    g.cast(gift, &[]);
    assert_eq!(g.life(P0), 23);
    assert_eq!(exiled(&g), 1);
}
