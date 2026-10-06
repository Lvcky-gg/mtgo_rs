//! "Tap an untapped creature you control" and other costs that tap chosen permanents.
use super::harness::*;
use mtg_core::{Step, Zone};
use mtg_engine::choice::Answer;

fn tapped(g: &Game, id: mtg_core::ObjectId) -> bool {
    g.engine.state.objects[&id].tapped
}

#[test]
fn tapping_a_chosen_creature_pays_the_cost() {
    let mut t = Table::default();
    let rider = t.card(
        "{1}{W}",
        "Creature — Soldier",
        Some((2, 2)),
        "Tap an untapped creature you control: This creature gets +1/+1 until end of turn.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let r = g.put(rider, P0, Zone::Battlefield);
    let b = g.put(bear, P0, Zone::Battlefield);
    // Summoning sickness only stops {T} costs of the creature's own abilities (CR 302.6).
    g.engine.state.objects.get_mut(&b).unwrap().summoning_sick = true;
    let actions = g.main();
    assert!(offers(&actions, r));
    g.act(activate(r, 0), &[], &[Answer::Objects(vec![b])]);
    assert!(tapped(&g, b), "the chosen bear paid");
    assert!(!tapped(&g, r));
    assert_eq!(g.pt(r), (3, 3));
    // The rider itself is still untapped and can pay for its own ability.
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(offers(&actions, r));
    g.act(activate(r, 0), &[], &[Answer::Objects(vec![r])]);
    assert!(tapped(&g, r));
    assert_eq!(g.pt(r), (4, 4));
    let actions = g.until(P0, Step::PrecombatMain);
    assert!(!offers(&actions, r), "nothing untapped is left to tap");
}

#[test]
fn the_cost_counts_only_matching_untapped_permanents() {
    for goblins in [1, 2] {
        let mut t = Table::default();
        let caller = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 1)),
            "Tap two untapped Goblins you control: Draw a card.",
        );
        let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
        let mut g = Game::new(t);
        let c = g.put(caller, P0, Zone::Battlefield);
        let gs: Vec<_> = (0..goblins)
            .map(|_| g.put(goblin, P0, Zone::Battlefield))
            .collect();
        let actions = g.main();
        assert_eq!(
            offers(&actions, c),
            goblins == 2,
            "the Elf itself doesn't count"
        );
        if goblins == 2 {
            let hand = g.count(Zone::Hand, P0);
            g.act(activate(c, 0), &[], &[Answer::Objects(gs.clone())]);
            assert!(gs.iter().all(|id| tapped(&g, *id)));
            assert_eq!(g.count(Zone::Hand, P0), hand + 1);
        }
    }
}

#[test]
fn with_its_own_tap_symbol_the_source_cant_be_the_creature_tapped() {
    for helper in [false, true] {
        let mut t = Table::default();
        let elder = t.card(
            "{1}{G}",
            "Creature — Elf",
            Some((1, 1)),
            "{T}, Tap an untapped creature you control: Draw a card.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        let e = g.put(elder, P0, Zone::Battlefield);
        let b = helper.then(|| g.put(bear, P0, Zone::Battlefield));
        let actions = g.main();
        assert_eq!(
            offers(&actions, e),
            helper,
            "the elder alone can't pay twice"
        );
        if let Some(b) = b {
            g.act(activate(e, 0), &[], &[Answer::Objects(vec![b])]);
            assert!(tapped(&g, e) && tapped(&g, b));
        }
    }
}

#[test]
fn a_spell_whose_additional_cost_taps_a_creature() {
    let mut t = Table::default();
    let spell = t.card(
        "{R}",
        "Instant",
        None,
        "As an additional cost to cast this spell, tap an untapped creature you control.\n\
         This spell deals 3 damage to any target.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(spell, P0, Zone::Hand);
    let b = g.put(bear, P0, Zone::Battlefield);
    g.main();
    g.act(
        mtg_engine::actions::Action::Cast { object: s },
        &[mtg_core::Target::Player(P1)],
        &[Answer::Objects(vec![b])],
    );
    assert!(tapped(&g, b));
    assert_eq!(g.life(P1), 17);
}
