use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_ir::PrintedCards;

#[test]
fn color_count_protection_prevents_targeting_blocking_and_damage() {
    for (quality, protected_count) in [("colorless", 0), ("monocolored", 1), ("multicolored", 2)] {
        for (colors, cost) in [(0, "{0}"), (1, "{R}"), (2, "{R}{G}")] {
            let mut table = Table::default();
            let protected = table.card(
                "{W}",
                "Creature",
                Some((2, 6)),
                &format!("Protection from {quality}"),
            );
            let blocker = table.card(cost, "Creature", Some((2, 6)), "");
            let targeted = table.card(
                cost,
                "Instant",
                None,
                "~ deals 2 damage to target creature.",
            );
            let sweep = table.card(cost, "Sorcery", None, "~ deals 2 damage to each creature.");
            let mut game = Game::new(table);
            game.lands(2);
            let protected = game.put(protected, P0, Zone::Battlefield);
            let blocker = game.put(blocker, P1, Zone::Battlefield);
            let targeted = game.put(targeted, P0, Zone::Hand);
            let sweep = game.put(sweep, P0, Zone::Hand);
            game.engine.state.step = Step::PrecombatMain;
            game.main();
            let card = game.engine.state.objects[&targeted].card;
            let spec = &game.table.face(card, 0).unwrap().abilities[0].targets[0];
            let targets = mtg_engine::targeting::legal_targets(
                &game.engine.state,
                &game.table,
                spec,
                targeted,
                P0,
                &[],
            );
            let permitted = colors != protected_count;
            assert_eq!(
                targets.contains(&Target::Object(protected)),
                permitted,
                "targeting: {quality} from {cost}"
            );
            assert_eq!(
                mtg_engine::combat::can_block(&game.engine.state, &game.table, blocker, protected),
                permitted,
                "blocking: {quality} from {cost}"
            );
            game.cast(sweep, &[]);
            assert_eq!(
                game.engine.state.objects[&protected].damage,
                if permitted { 2 } else { 0 },
                "damage: {quality} from {cost}"
            );
        }
    }
}

#[test]
fn protection_combines_colors_and_card_types() {
    let mut table = Table::default();
    let protected = table.card(
        "{W}",
        "Creature",
        Some((2, 2)),
        "Protection from red and from artifacts",
    );
    let red = table.card("{R}", "Creature", Some((1, 1)), "");
    let artifact = table.card("{U}", "Artifact Creature", Some((1, 1)), "");
    let blue = table.card("{U}", "Creature", Some((1, 1)), "");
    let mut game = Game::new(table);
    let protected = game.put(protected, P0, Zone::Battlefield);
    for (card, allowed) in [(red, false), (artifact, false), (blue, true)] {
        let blocker = game.put(card, P1, Zone::Battlefield);
        assert_eq!(
            mtg_engine::combat::can_block(&game.engine.state, &game.table, blocker, protected),
            allowed
        );
    }
}
