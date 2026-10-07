//! Variable scry/surveil counts use their explicit value at resolution.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn ugins_insight_scries_the_greatest_mana_value_then_draws_three() {
    let mut t = Table::default();
    let spell=t.card("{3}{U}{U}","Sorcery",None,
        "Scry X, where X is the greatest mana value among permanents you control, then draw three cards.");
    let rock = t.card("{3}", "Artifact", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(5);
    g.put(rock, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    let ids = stack_top(&mut g, &[bear, bear, bear, bear]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..3],
        0,
        3,
        vec![],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
    assert_eq!(
        g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0],
        ids[3]
    );
}

#[test]
fn surveil_definition_counts_only_matching_tapped_creatures() {
    let mut t = Table::default();
    let spell = t.card(
        "{U}",
        "Sorcery",
        None,
        "Surveil X, where X is the number of tapped Wizards you control.",
    );
    let wizard = t.card("{U}", "Creature — Wizard", Some((1, 1)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let tapped: Vec<_> = (0..2)
        .map(|_| g.put(wizard, P0, Zone::Battlefield))
        .collect();
    g.put(wizard, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    for id in tapped.into_iter().chain([other]) {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    let ids = stack_top(&mut g, &[bear, bear, bear]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..2],
        0,
        2,
        ids[..2].to_vec(),
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 3);
    assert_eq!(
        g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0],
        ids[2]
    );
    assert!(g.engine.state.revealed_cards.is_empty());
}

#[test]
fn mana_x_is_accepted_but_undefined_x_and_unsupported_continuations_are_rejected() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    for (text, cost, expected) in [
        ("Scry X.", "{X}{U}", true),
        ("Surveil X.", "{X}{U}", true),
        ("Scry X.", "{U}", false),
        ("Surveil X.", "{U}", false),
        (
            "Scry X, where X is the number of creatures you control, then draw X cards.",
            "{U}",
            false,
        ),
    ] {
        let face = FaceText {
            name: "Variable selection",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: cost,
        };
        assert_eq!(
            compile(&face, &SubtypeNames(vec![])).understood(),
            expected,
            "{text}"
        );
    }
}
