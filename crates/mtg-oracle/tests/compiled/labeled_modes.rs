use super::harness::*;
use mtg_core::{Target, Zone, ZoneRef};
use mtg_engine::{Action, choice::Answer};

#[test]
fn dawnbringer_cleric_named_modes_apply_only_the_chosen_effect() {
    for mode in 0..3 {
        let mut t = Table::default();
        let cleric = t.card("{1}{W}", "Creature — Soldier", Some((1, 3)),
            "When this creature enters, choose one —\n• Cure Wounds — You gain 2 life.\n• Dispel Magic — Destroy target enchantment.\n• Gentle Repose — Exile target card from a graveyard.");
        let enchantment = t.card("{1}", "Enchantment", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(2);
        let enchantment = g.put(enchantment, P1, Zone::Battlefield);
        let grave = g.put(bear, P1, Zone::Graveyard);
        let spell = g.put(cleric, P0, Zone::Hand);
        g.main();
        let targets = match mode {
            1 => vec![Target::Object(enchantment)],
            2 => vec![Target::Object(grave)],
            _ => vec![],
        };
        g.act(
            Action::Cast { object: spell },
            &targets,
            &[Answer::Modes(vec![mode])],
        );
        assert_eq!(
            g.engine.state.players[&P0].life,
            if mode == 0 { 22 } else { 20 }
        );
        assert_eq!(g.engine.state.objects.contains_key(&enchantment), mode != 1);
        assert_eq!(g.engine.state.objects.contains_key(&grave), mode != 2);
        let exile_count = g
            .engine
            .state
            .objects
            .values()
            .filter(|o| o.zone == ZoneRef::shared(Zone::Exile))
            .count();
        assert_eq!(exile_count, usize::from(mode == 2));
    }
}

#[test]
fn dispatch_keeps_metalcraft_condition_after_stripping_label() {
    for artifacts in [2, 3] {
        let mut t = Table::default();
        let dispatch = t.card("{W}", "Instant", None,
            "Tap target creature.\nMetalcraft — If you control three or more artifacts, exile that creature.");
        let artifact = t.card("{1}", "Artifact", None, "");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        for _ in 0..artifacts {
            g.put(artifact, P0, Zone::Battlefield);
        }
        let creature = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(dispatch, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[Target::Object(creature)]);
        if artifacts == 2 {
            assert!(g.engine.state.objects[&creature].tapped);
            assert_eq!(
                g.engine.state.objects[&creature].zone.zone,
                Zone::Battlefield
            );
        } else {
            assert!(
                g.engine
                    .state
                    .objects
                    .values()
                    .any(|o| o.card == bear && o.zone == ZoneRef::shared(Zone::Exile))
            );
        }
    }
}

#[test]
fn pair_of_goblins_named_modes_make_tokens_or_pump_existing_creatures() {
    for mode in 0..2 {
        let mut t = Table::default();
        let goblins = t.card("{2}{R}", "Instant", None,
            "Choose one —\n• Charge Them — Creatures you control get +2/+0 until end of turn.\n• Befriend Them — Create two 1/1 red Goblin creature tokens.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(3);
        let ours = g.put(bear, P0, Zone::Battlefield);
        let theirs = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(goblins, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Modes(vec![mode])],
        );
        assert_eq!(g.pt(ours), if mode == 0 { (4, 2) } else { (2, 2) });
        assert_eq!(g.pt(theirs), (2, 2));
        let tokens: Vec<_> = g
            .engine
            .state
            .objects
            .values()
            .filter(|o| o.zone == ZoneRef::shared(Zone::Battlefield) && o.is_token)
            .map(|o| o.id)
            .collect();
        assert_eq!(tokens.len(), if mode == 1 { 2 } else { 0 });
        for token in tokens {
            assert_eq!(g.pt(token), (1, 1));
            assert_eq!(g.engine.state.objects[&token].controller, P0);
        }
    }
}

#[test]
fn guard_approach_modes_use_their_own_target_restrictions() {
    use mtg_core::Keyword;
    for mode in 0..2 {
        let mut t = Table::default();
        let guard = t.card("{U}", "Instant", None,
            "Choose one —\n• Distract the Guard — Tap target creature.\n• Hide — Target creature you control gains hexproof until end of turn.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let ours = g.put(bear, P0, Zone::Battlefield);
        let theirs = g.put(bear, P1, Zone::Battlefield);
        let spell = g.put(guard, P0, Zone::Hand);
        g.main();
        let target = if mode == 0 { theirs } else { ours };
        g.act(
            Action::Cast { object: spell },
            &[Target::Object(target)],
            &[Answer::Modes(vec![mode])],
        );
        assert_eq!(g.engine.state.objects[&theirs].tapped, mode == 0);
        assert_eq!(g.has(ours, Keyword::Hexproof), mode == 1);
        assert!(!g.has(theirs, Keyword::Hexproof));
    }
}
