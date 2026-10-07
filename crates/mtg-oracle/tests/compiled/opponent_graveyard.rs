use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_ir::PrintedCards;

#[test]
fn opposing_graveyard_targets_exclude_own_cards_and_other_zones() {
    for text in [
        "Exile target card from an opponent's graveyard.",
        "Exile target creature card in an opponent's graveyard.",
    ] {
        let mut t = Table::default();
        let spell_card = t.card("{0}", "Instant", None, text);
        let bear = t.bear();
        let other = t.card("{0}", "Artifact", None, "");
        let mut g = Game::new(t);
        let own = g.put(bear, P0, Zone::Graveyard);
        let opponent = g.put(bear, P1, Zone::Graveyard);
        let artifact = g.put(other, P1, Zone::Graveyard);
        let hand = g.put(bear, P1, Zone::Hand);
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let spec = &g.table.face(spell_card, 0).unwrap().abilities[0].targets[0];
        let legal =
            mtg_engine::targeting::legal_targets(&g.engine.state, &g.table, spec, spell, P0, &[]);
        assert!(legal.contains(&Target::Object(opponent)));
        assert_eq!(
            legal.contains(&Target::Object(artifact)),
            !text.contains("creature")
        );
        assert!(!legal.contains(&Target::Object(own)));
        assert!(!legal.contains(&Target::Object(hand)));
        g.cast(spell, &[Target::Object(opponent)]);
        assert_eq!(
            g.engine
                .state
                .objects_in(ZoneRef::shared(Zone::Exile))
                .len(),
            1
        );
        assert_eq!(g.count(Zone::Graveyard, P1), 1);
        assert_eq!(
            g.engine.state.objects[&own].zone,
            ZoneRef::of(Zone::Graveyard, P0)
        );
    }
}

#[test]
fn reanimation_from_opponents_graveyard_preserves_ownership() {
    let mut t = Table::default();
    let spell_card = t.card("{0}", "Sorcery", None,
        "Put target creature card from an opponent's graveyard onto the battlefield under your control.");
    let bear = t.bear();
    let mut g = Game::new(t);
    let creature = g.put(bear, P1, Zone::Graveyard);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[Target::Object(creature)]);
    let returned = g.find(bear).unwrap();
    assert_eq!(g.engine.state.objects[&returned].owner, P1);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, returned),
        Some(P0)
    );
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
}
