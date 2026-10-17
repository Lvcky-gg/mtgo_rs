//! Monocolored, "power or toughness N or less", and "exile all graveyards".

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::actions::Action;

#[test]
fn monocolored_means_exactly_one_color() {
    for (cost, legal) in [("{G}", true), ("{G}{U}", false), ("{2}", false)] {
        let mut t = Table::default();
        let spell = t.card(
            "{B}",
            "Instant",
            None,
            "Destroy target monocolored creature.",
        );
        let body = t.card(cost, "Creature — Beast", Some((2, 2)), "");
        let mut g = Game::new(t);
        g.lands(1);
        g.put(body, P1, Zone::Battlefield);
        let s = g.put(spell, P0, Zone::Hand);
        assert_eq!(
            g.main().contains(&Action::Cast { object: s }),
            legal,
            "{cost}"
        );
    }
}

#[test]
fn power_or_toughness_either_one_is_enough() {
    for (pt, legal) in [((1, 5), true), ((5, 1), true), ((2, 2), false)] {
        let mut t = Table::default();
        let spell = t.card(
            "{1}",
            "Instant",
            None,
            "Exile target creature with power or toughness 1 or less.",
        );
        let body = t.card("{3}", "Creature — Beast", Some(pt), "");
        let mut g = Game::new(t);
        g.lands(1);
        g.put(body, P1, Zone::Battlefield);
        let s = g.put(spell, P0, Zone::Hand);
        assert_eq!(
            g.main().contains(&Action::Cast { object: s }),
            legal,
            "{pt:?}"
        );
    }
}

#[test]
fn exile_all_graveyards_takes_every_players_cards() {
    let mut t = Table::default();
    let spell = t.card("{0}", "Sorcery", None, "Exile all graveyards.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(bear, P0, Zone::Graveyard);
    g.put(bear, P1, Zone::Graveyard);
    g.put(bear, P1, Zone::Graveyard);
    let s = g.put(spell, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    // The spell itself goes to the graveyard after it resolves.
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn a_wrath_spares_creatures_with_counters() {
    let mut t = Table::default();
    let verdict = t.card(
        "{0}",
        "Sorcery",
        None,
        "Destroy all creatures with no counters on them.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let plain = g.put(bear, P1, Zone::Battlefield);
    let countered = g.put(bear, P1, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&countered)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 1);
    let s = g.put(verdict, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert!(!g.engine.state.objects.contains_key(&plain));
    assert!(g.engine.state.objects.contains_key(&countered));
}

#[test]
fn ruinous_blast_spares_legends_and_lands() {
    let mut t = Table::default();
    // A plain sorcery: the legendary-sorcery rule isn't what's tested here.
    let blast = t.card(
        "{0}",
        "Sorcery",
        None,
        "Exile all nonland permanents that aren't legendary.",
    );
    let bear = t.bear();
    let legend = t.card("{2}", "Legendary Creature — Bear", Some((2, 2)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Battlefield);
    let l = g.put(legend, P1, Zone::Battlefield);
    let s = g.put(blast, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert!(!g.engine.state.objects.contains_key(&b));
    assert!(g.engine.state.objects.contains_key(&l));
    // Lands stay.
    assert!(g.engine.state.battlefield().len() >= 2);
}

#[test]
fn wave_goodbye_spares_creatures_with_a_counter() {
    let mut t = Table::default();
    let wave = t.card(
        "{0}",
        "Sorcery",
        None,
        "Return each creature without a +1/+1 counter on it to its owner's hand.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    let plain = g.put(bear, P1, Zone::Battlefield);
    let kept = g.put(bear, P1, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&kept)
        .unwrap()
        .counters
        .insert(mtg_core::CounterKind::PlusOnePlusOne, 1);
    let s = g.put(wave, P0, Zone::Hand);
    g.main();
    g.cast(s, &[]);
    assert!(!g.engine.state.objects.contains_key(&plain));
    assert!(g.engine.state.objects.contains_key(&kept));
}

#[test]
fn number_words_past_ten() {
    let mut t = Table::default();
    let sanity = t.card(
        "{0}",
        "Sorcery",
        None,
        "Each opponent mills fourteen cards.",
    );
    let mut g = Game::new(t);
    let s = g.put(sanity, P0, Zone::Hand);
    g.main();
    let before = g.count(Zone::Library, P1);
    g.cast(s, &[]);
    assert_eq!(g.count(Zone::Library, P1), before - 14);
}
