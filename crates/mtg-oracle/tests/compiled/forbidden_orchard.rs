use super::harness::*;
use mtg_core::{AbilityId, Color, Target, Zone};
use mtg_engine::actions::Action;

const ORCHARD: &str = "{T}: Add one mana of any color.\nWhenever you tap this land for mana, target opponent creates a 1/1 colorless Spirit creature token.";

#[test]
fn orchard_triggers_once_for_manual_and_automatic_mana_activation() {
    for automatic in [false, true] {
        let mut t = Table::default();
        let land = t.card("", "Land", None, ORCHARD);
        let spell = t.card("{U}", "Instant", None, "You gain 1 life.");
        let mut g = Game::new(t);
        let land = g.put(land, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        if automatic {
            g.cast(spell, &[Target::Player(P1)]);
        } else {
            g.act(
                Action::ActivateManaAbility {
                    source: land,
                    ability: AbilityId(0),
                    color: Some(Color::Blue),
                },
                &[Target::Player(P1)],
                &[],
            );
            assert_eq!(
                g.engine.state.player(P0).mana.amounts[Color::Blue as usize],
                1
            );
        }
        let tokens: Vec<_> = g
            .engine
            .state
            .objects
            .values()
            .filter(|o| o.is_token)
            .map(|o| o.id)
            .collect();
        assert_eq!(tokens.len(), 1);
        assert_eq!(g.engine.state.objects[&tokens[0]].controller, P1);
        assert_eq!(g.engine.state.objects[&tokens[0]].owner, P1);
        assert_eq!(g.pt(tokens[0]), (1, 1));
        assert!(g.engine.state.objects[&land].tapped);
    }
}

#[test]
fn tapping_orchard_without_activating_its_mana_ability_does_not_trigger() {
    for nonmana_ability in [false, true] {
        let mut t = Table::default();
        let land = t.card(
            "",
            "Land",
            None,
            &format!("{ORCHARD}\n{{T}}: You gain 1 life."),
        );
        let tap = t.card("{0}", "Instant", None, "Tap target land.");
        let mut g = Game::new(t);
        let land = g.put(land, P0, Zone::Battlefield);
        let tap = g.put(tap, P0, Zone::Hand);
        g.main();
        if nonmana_ability {
            g.act(activate(land, 2), &[], &[]);
        } else {
            g.cast(tap, &[Target::Object(land)]);
        }
        assert!(g.engine.state.objects[&land].tapped);
        assert!(!g.engine.state.objects.values().any(|o| o.is_token));
    }
}
