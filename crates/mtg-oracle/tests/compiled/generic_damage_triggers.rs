use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn repercussion_uses_damage_amount_and_damaged_creatures_controller() {
    for (owner, combat_only) in [(P0, false), (P1, false), (P1, true)] {
        let mut t = Table::default();
        let enchantment = t.card("{1}{R}{R}", "Enchantment", None,
            if combat_only {
            "Whenever a creature is dealt combat damage, this enchantment deals that much damage to that creature's controller."
        } else {
            "Whenever a creature is dealt damage, this enchantment deals that much damage to that creature's controller."
        });
        let damage = t.card(
            "{R}",
            "Instant",
            None,
            "This spell deals 3 damage to target creature.",
        );
        let creature = t.card("{3}", "Creature — Bear", Some((4, 4)), "");
        let mut g = Game::new(t);
        g.lands(1);
        g.put(enchantment, P0, Zone::Battlefield);
        let creature = g.put(creature, owner, Zone::Battlefield);
        let spell = g.put(damage, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[Target::Object(creature)]);
        assert_eq!(
            g.engine.state.players[&owner].life,
            if combat_only { 20 } else { 17 }
        );
        assert_eq!(
            g.engine.state.players[&if owner == P0 { P1 } else { P0 }].life,
            20
        );
    }
}

#[test]
fn rite_of_passage_filters_controller_and_cannot_save_lethally_damaged_creature() {
    for (owner, lethal) in [(P0, false), (P1, false), (P0, true)] {
        let mut t = Table::default();
        let rite = t.card(
            "{2}{G}",
            "Enchantment",
            None,
            "Whenever a creature you control is dealt damage, put a +1/+1 counter on it.",
        );
        let damage = t.card(
            "{R}",
            "Instant",
            None,
            if lethal {
                "This spell deals 2 damage to target creature."
            } else {
                "This spell deals 1 damage to target creature."
            },
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        g.put(rite, P0, Zone::Battlefield);
        let creature = g.put(bear, owner, Zone::Battlefield);
        let spell = g.put(damage, P0, Zone::Hand);
        g.main();
        g.cast(spell, &[Target::Object(creature)]);
        if lethal {
            assert_eq!(g.count(Zone::Graveyard, owner), 2);
        } else {
            assert_eq!(g.pt(creature), if owner == P0 { (3, 3) } else { (2, 2) });
        }
    }
}

#[test]
fn wall_of_essence_gains_life_for_combat_damage_only() {
    for combat in [false, true] {
        let mut t = Table::default();
        let wall = t.card(
            "{1}{W}",
            "Creature — Wall",
            Some((0, 4)),
            "Defender\nWhenever this creature is dealt combat damage, you gain that much life.",
        );
        let damage = t.card(
            "{R}",
            "Instant",
            None,
            "This spell deals 3 damage to target creature.",
        );
        let attacker = t.card("{3}", "Creature — Bear", Some((3, 3)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let wall = g.put(wall, P1, Zone::Battlefield);
        let attacker = g.put(attacker, P0, Zone::Battlefield);
        let spell = g.put(damage, P0, Zone::Hand);
        g.main();
        if combat {
            g.combat(&[attacker], &[(wall, attacker)], &[], &[]);
        } else {
            g.cast(spell, &[Target::Object(wall)]);
        }
        assert_eq!(
            g.engine.state.players[&P1].life,
            if combat { 23 } else { 20 }
        );
        assert_eq!(g.engine.state.players[&P0].life, 20);
    }
}
