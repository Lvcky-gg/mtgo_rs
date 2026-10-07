//! Inline library counts evaluate an existing value expression at resolution.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn machinate_looks_at_exactly_the_number_of_artifacts_controlled() {
    for count in [1, 3] {
        let mut t = Table::default();
        let spell = t.card("{1}{U}{U}", "Instant", None,
            "Look at the top X cards of your library, where X is the number of artifacts you control. Put one of those cards into your hand and the rest on the bottom of your library in any order.");
        let artifact = t.card("{2}", "Artifact", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(3);
        for _ in 0..count {
            g.put(artifact, P0, Zone::Battlefield);
        }
        g.put(bear, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[artifact, bear, artifact, bear]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..count],
            1,
            1,
            vec![ids[count - 1]],
        );
        let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        assert_eq!(
            library[0], ids[count],
            "the first card beyond X stays on top"
        );
    }
}

#[test]
fn zero_artifacts_leaves_the_entire_library_untouched() {
    let mut t = Table::default();
    let spell = t.card("{1}{U}{U}", "Instant", None,
        "Look at the top X cards of your library, where X is the number of artifacts you control. Put one of those cards into your hand and the rest on the bottom of your library in any order.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[bear, bear, bear]);
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[]);
    assert_eq!(
        &g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[..3],
        &ids
    );
    assert_eq!(g.count(Zone::Hand, P0), hand - 1);
}

#[test]
fn stirring_honormancer_counts_the_entering_creature_in_its_trigger() {
    let mut t = Table::default();
    let honormancer = t.card("{3}{U}", "Creature — Bear", Some((3, 2)),
        "When this creature enters, look at the top X cards of your library, where X is the number of creatures you control. Put one of those cards into your hand and the rest into your graveyard.");
    let bear = t.bear();
    let artifact = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(4);
    g.put(bear, P0, Zone::Battlefield);
    g.put(artifact, P0, Zone::Battlefield);
    let spell = g.put(honormancer, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[artifact, bear, artifact]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..2],
        1,
        1,
        vec![ids[0]],
    );
    assert!(g.find(honormancer).is_some());
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
    assert_eq!(
        g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0],
        ids[2]
    );
}

#[test]
fn undefined_x_unknown_count_and_later_x_references_remain_rejected() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    for text in [
        "Look at the top X cards of your library. Put one of those cards into your hand and the rest into your graveyard.",
        "Look at the top X cards of your library, where X is the number of imaginary things you control. Put one of those cards into your hand and the rest into your graveyard.",
        "Look at the top X cards of your library, where X is the number of creatures you control. Put up to X creature cards from among them into your hand and the rest into your graveyard.",
    ] {
        let face = FaceText {
            name: "Unsupported",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "{U}",
        };
        assert!(!compile(&face, &SubtypeNames(vec![])).understood());
    }
}
