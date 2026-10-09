//! The Thriving lands: "This land enters tapped. As it enters, choose a color other than
//! green. {T}: Add {G} or one mana of the chosen color."

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const GROVE: &str = "This land enters tapped. As it enters, choose a color other than green.\n\
                     {T}: Add {G} or one mana of the chosen color.";

#[test]
fn makes_its_color_or_the_chosen_one() {
    let mut table = Table::default();
    let grove_card = table.card("", "Land", None, GROVE);
    let mut game = Game::new(table);
    let grove = game.put(grove_card, P0, Zone::Hand);
    game.main();
    let c = game.pending.take().unwrap();
    game.engine
        .answer(
            &game.table,
            c.id,
            Answer::Action(Action::PlayLand { object: grove }),
        )
        .unwrap();
    let mut offered = Vec::new();
    let on_battlefield = |game: &Game| {
        game.engine
            .state
            .battlefield()
            .into_iter()
            .find(|id| game.engine.state.objects[id].card == grove_card)
    };
    game.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseModes { available, .. } => {
                offered = available.iter().map(|l| l.to_string()).collect();
                let blue = available.iter().position(|l| &**l == "blue").unwrap();
                Some(Answer::Modes(vec![blue as u8]))
            }
            _ => None,
        },
        |game| on_battlefield(game).is_some() && game.engine.state.priority == Some(P0),
    );
    assert_eq!(
        offered,
        ["white", "blue", "black", "red"],
        "green is not a choice"
    );
    let land = on_battlefield(&game).unwrap();
    assert!(game.engine.state.objects[&land].tapped, "it enters tapped");
    game.engine.state.objects.get_mut(&land).unwrap().tapped = false;
    let can = |cost: &str| {
        let (cost, _) = mtg_oracle::manacost::parse(cost);
        mtg_engine::mana::can_pay(&game.engine.state, &game.table, P0, &cost, 0)
    };
    assert_eq!((can("{G}"), can("{U}"), can("{R}")), (true, true, false));
}
