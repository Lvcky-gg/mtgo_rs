use super::harness::*;
use mtg_core::{Event, Target, Zone, ZoneRef};
use mtg_engine::actions::Action;

#[test]
fn targeted_permanent_is_shuffled_into_its_owners_library() {
    for explicit_owner in [false, true] {
        let mut t = Table::default();
        let spell_card = t.card("{1}{G}", "Instant", None,
            if explicit_owner {
                "The owner of target artifact or enchantment an opponent controls shuffles it into their library."
            } else {
                "Choose target artifact or enchantment. Its owner shuffles it into their library."
            });
        let artifact_card = t.card("{1}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let owner = if explicit_owner { P0 } else { P1 };
        let controller = if explicit_owner { P1 } else { P0 };
        let artifact = g.put(artifact_card, owner, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&artifact)
            .unwrap()
            .controller = controller;
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let before = [g.count(Zone::Library, P0), g.count(Zone::Library, P1)];
        let log_start = g.engine.log.len();
        g.cast(spell, &[Target::Object(artifact)]);
        assert!(!g.engine.state.objects.contains_key(&artifact));
        for (i, player) in [P0, P1].into_iter().enumerate() {
            assert_eq!(
                g.count(Zone::Library, player),
                before[i] + usize::from(player == owner)
            );
            let shuffles = g.engine.log[log_start..]
                .iter()
                .filter(|e| matches!(&e.event, Event::Shuffled { player: p } if *p == player))
                .count();
            assert_eq!(shuffles, usize::from(player == owner));
        }
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Library, owner))
                .iter()
                .any(|id| g.engine.state.objects[id].card == artifact_card)
        );
    }
}

#[test]
fn owner_shuffle_and_optional_graveyard_shuffle_use_separate_libraries() {
    use mtg_engine::choice::Answer;
    let mut t = Table::default();
    let spell_card = t.card("{1}{G}", "Instant", None,
        "The owner of target artifact or enchantment an opponent controls shuffles it into their library. You may shuffle up to four target cards from your graveyard into your library.");
    let artifact_card = t.card("{1}", "Artifact", None, "");
    let land_card = t.mountain();
    let mut g = Game::new(t);
    g.lands(2);
    let artifact = g.put(artifact_card, P1, Zone::Battlefield);
    let land = g.put(land_card, P0, Zone::Graveyard);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let before = [g.count(Zone::Library, P0), g.count(Zone::Library, P1)];
    let log_start = g.engine.log.len();
    g.act(
        Action::Cast { object: spell },
        &[Target::Object(artifact), Target::Object(land)],
        &[Answer::Bool(true)],
    );
    for (i, player) in [P0, P1].into_iter().enumerate() {
        assert_eq!(g.count(Zone::Library, player), before[i] + 1);
        assert_eq!(
            g.engine.log[log_start..]
                .iter()
                .filter(|e| matches!(&e.event, Event::Shuffled { player: p } if *p == player))
                .count(),
            1
        );
    }
    assert!(!g.engine.state.objects.contains_key(&artifact));
    assert!(!g.engine.state.objects.contains_key(&land));
}

#[test]
fn an_illegal_target_does_not_shuffle_any_library() {
    let mut t = Table::default();
    let shuffle_card = t.card(
        "{G}",
        "Instant",
        None,
        "Choose target artifact or enchantment. Its owner shuffles it into their library.",
    );
    let destroy_card = t.card("{G}", "Instant", None, "Destroy target artifact.");
    let artifact_card = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let artifact = g.put(artifact_card, P1, Zone::Battlefield);
    let shuffle = g.put(shuffle_card, P0, Zone::Hand);
    let destroy = g.put(destroy_card, P0, Zone::Hand);
    g.main();
    let before = [g.count(Zone::Library, P0), g.count(Zone::Library, P1)];
    let log_start = g.engine.log.len();
    g.act_holding(
        Action::Cast { object: shuffle },
        &[Target::Object(artifact)],
    );
    g.cast(destroy, &[Target::Object(artifact)]);
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
    assert_eq!(
        [g.count(Zone::Library, P0), g.count(Zone::Library, P1)],
        before
    );
    assert!(
        !g.engine.log[log_start..]
            .iter()
            .any(|e| matches!(e.event, Event::Shuffled { .. }))
    );
}
