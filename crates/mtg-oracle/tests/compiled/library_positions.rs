use super::harness::*;
use mtg_core::{Step, Target, Zone, ZoneRef};

#[test]
fn targeted_library_placement_uses_the_owner_and_exact_depth() {
    for (ordinal, depth) in [("second", 1), ("third", 2)] {
        let mut table = Table::default();
        let spell = table.card(
            "{U}",
            "Instant",
            None,
            &format!("Put target creature into its owner's library {ordinal} from the top."),
        );
        let creature = table.bear();
        let mut game = Game::new(table);
        game.lands(1);
        let target = game.put(creature, P1, Zone::Battlefield);
        let spell = game.put(spell, P0, Zone::Hand);
        let library = ZoneRef::of(Zone::Library, P1);
        let original = game.engine.state.objects_in(library);
        game.engine.state.step = Step::PrecombatMain;
        game.main();
        game.cast(spell, &[Target::Object(target)]);
        let mut order = game.engine.state.objects_in(library);
        let moved = order.remove(depth);
        assert_eq!(game.engine.state.objects[&moved].card, creature);
        assert_eq!(order, original);
        assert!(!game.engine.state.objects.contains_key(&target));
    }
}

#[test]
fn graveyard_library_ability_inserts_at_depth_or_at_the_end_of_a_short_library() {
    for (ordinal, depth) in [("second", 1), ("third", 2)] {
        for short in [None, Some(0), Some(1)] {
            let mut table = Table::default();
            let creature = table.card("{3}{G}", "Creature", Some((3, 3)),
                &format!("{{G}}: Put this card from your graveyard into your library {ordinal} from the top."));
            let mut game = Game::new(table);
            game.lands(1);
            let source = game.put(creature, P0, Zone::Graveyard);
            let library = ZoneRef::of(Zone::Library, P0);
            if let Some(keep) = short {
                let ids = game.engine.state.objects_in(library);
                for id in &ids[keep..] {
                    game.engine.state.objects.remove(id);
                }
                game.engine
                    .state
                    .zone_order
                    .insert(library, ids[..keep].to_vec());
            }
            let original = game.engine.state.objects_in(library);
            game.engine.state.step = Step::PrecombatMain;
            assert!(game.main().contains(&activate(source, 0)));
            game.act(activate(source, 0), &[], &[]);
            let mut order = game.engine.state.objects_in(library);
            let moved = order.remove(depth.min(original.len()));
            assert_eq!(game.engine.state.objects[&moved].card, creature);
            assert_eq!(order, original);
            assert_eq!(game.count(Zone::Graveyard, P0), 0);
        }
    }
}
