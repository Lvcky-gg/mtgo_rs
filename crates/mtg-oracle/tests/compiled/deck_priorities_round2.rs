use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn narcomoeba_returns_only_when_milled_and_can_be_declined() {
    for accept in [false, true] {
        let mut t = Table::default();
        let narco = t.card("{1}{U}", "Creature — Illusion", Some((1, 1)),
            "Flying\nWhen this card is put into your graveyard from your library, you may put it onto the battlefield.");
        let mill = t.card("{0}", "Sorcery", None, "Mill a card.");
        let mut g = Game::new(t);
        let spell = g.put(mill, P0, Zone::Hand);
        g.main();
        let card = g.put(narco, P0, Zone::Library);
        let library = g
            .engine
            .state
            .zone_order
            .get_mut(&mtg_core::ZoneRef::of(Zone::Library, P0))
            .unwrap();
        library.retain(|id| *id != card);
        library.insert(0, card);
        g.act(Action::Cast { object: spell }, &[], &[Answer::Bool(accept)]);
        assert_eq!(g.find(narco).is_some(), accept);
        assert_eq!(g.count(Zone::Graveyard, P0), if accept { 1 } else { 2 });
    }
}

#[test]
fn narcomoeba_does_not_return_when_discarded() {
    let mut t = Table::default();
    let narco = t.card("{1}{U}", "Creature — Illusion", Some((1, 1)),
        "Flying\nWhen this card is put into your graveyard from your library, you may put it onto the battlefield.");
    let discard = t.card("{0}", "Sorcery", None, "Discard a card.");
    let mut g = Game::new(t);
    let spell = g.put(discard, P0, Zone::Hand);
    let card = g.put(narco, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![card])],
    );
    assert!(g.find(narco).is_none());
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
}

#[test]
fn samut_grants_split_second_only_to_your_instant_and_sorcery_spells() {
    use mtg_core::Keyword;
    let mut t = Table::default();
    let samut = t.card("{2}{R}{G}{W}", "Legendary Creature", Some((5, 3)),
        "Double strike, vigilance, haste\nInstant and sorcery spells you control have split second.");
    let instant = t.card("{0}", "Instant", None, "You gain 1 life.");
    let creature = t.bear();
    let mut g = Game::new(t);
    g.put(samut, P0, Zone::Battlefield);
    let mine = g.put(instant, P0, Zone::Stack);
    let theirs = g.put(instant, P1, Zone::Stack);
    let permanent = g.put(creature, P0, Zone::Stack);
    assert!(g.has(mine, Keyword::SplitSecond));
    assert!(!g.has(theirs, Keyword::SplitSecond));
    assert!(!g.has(permanent, Keyword::SplitSecond));
}

#[test]
fn samuts_split_second_prevents_responses_to_your_instants_and_sorceries() {
    use mtg_engine::choice::ChoiceKind;
    for kind in ["Instant", "Sorcery", "Artifact"] {
        let mut t = Table::default();
        let samut = t.card(
            "{2}{R}{G}{W}",
            "Legendary Creature",
            Some((5, 3)),
            "Instant and sorcery spells you control have split second.",
        );
        let spell = t.card(
            "{0}",
            kind,
            None,
            if kind == "Artifact" {
                ""
            } else {
                "You gain 1 life."
            },
        );
        let response = t.card("{0}", "Instant", None, "You gain 1 life.");
        let utility = t.card("{0}", "Creature", Some((1, 1)), "{T}: You gain 1 life.");
        let mut g = Game::new(t);
        g.lands(1);
        g.put(samut, P0, Zone::Battlefield);
        let utility = g.put(utility, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        let response = g.put(response, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: spell }, &[]);
        let ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind else {
            panic!("priority")
        };
        assert_eq!(
            legal.actions.contains(&Action::Cast { object: response }),
            kind == "Artifact"
        );
        assert_eq!(
            legal.actions.contains(&activate(utility, 0)),
            kind == "Artifact"
        );
        assert!(!legal.mana_abilities.is_empty());
    }
}
