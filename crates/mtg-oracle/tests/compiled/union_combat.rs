use super::harness::*;
use mtg_core::{Target, Zone};

#[test]
fn self_or_other_combat_trigger_filters_type_recipient_and_damage_kind() {
    for mode in 0..5 {
        let mut t = Table::default();
        let source = t.card("{3}", "Creature — Dragon", Some((2, 4)),
            "Whenever this creature or another Dragon you control deals combat damage to a player, you gain 2 life.");
        let dragon = t.card("{3}", "Creature — Dragon", Some((2, 4)), "");
        let bear = t.bear();
        let damage = t.card(
            "{R}",
            "Instant",
            None,
            "Target creature you control deals 1 damage to target player.",
        );
        let mut g = Game::new(t);
        g.lands(1);
        let source = g.put(source, P0, Zone::Battlefield);
        let other = g.put(dragon, P0, Zone::Battlefield);
        let wrong_type = g.put(bear, P0, Zone::Battlefield);
        let blocker = g.put(dragon, P1, Zone::Battlefield);
        let spell = g.put(damage, P0, Zone::Hand);
        g.main();
        match mode {
            0 => g.combat(&[source], &[], &[], &[]),
            1 => g.combat(&[other], &[], &[], &[]),
            2 => g.combat(&[wrong_type], &[], &[], &[]),
            3 => g.combat(&[other], &[(blocker, other)], &[], &[]),
            _ => g.cast(spell, &[Target::Object(other), Target::Player(P1)]),
        }
        assert_eq!(
            g.engine.state.players[&P0].life,
            if mode < 2 { 22 } else { 20 }
        );
    }
}

#[test]
fn printed_tyranid_harridan_creates_flying_gargoyle_for_each_damage_source() {
    use mtg_core::{CardType, Keyword, ZoneRef};
    let mut t = Table::default();
    let harridan = t.card("{4}{G}{U}", "Creature — Tyranid", Some((4, 4)),
        "Flying, ward {4}\nShrieking Gargoyles — Whenever this creature or another Tyranid you control deals combat damage to a player, create a 1/1 blue Tyranid Gargoyle creature token with flying.");
    let tyranid = t.card("{2}", "Creature — Tyranid", Some((2, 2)), "");
    let mut g = Game::new(t);
    let source = g.put(harridan, P0, Zone::Battlefield);
    let other = g.put(tyranid, P0, Zone::Battlefield);
    g.main();
    g.combat(&[source, other], &[], &[], &[]);
    let tokens: Vec<_> = g
        .engine
        .state
        .objects
        .values()
        .filter(|o| o.zone == ZoneRef::shared(Zone::Battlefield) && o.is_token)
        .map(|o| o.id)
        .collect();
    assert_eq!(tokens.len(), 2);
    for token in tokens {
        let chars = mtg_engine::layers::compute(&g.engine.state, &g.table, token).unwrap();
        assert!(chars.has_type(CardType::Creature));
        assert!(g.has(token, Keyword::Flying));
        assert_eq!(g.pt(token), (1, 1));
        assert_eq!(g.engine.state.objects[&token].controller, P0);
    }
}
