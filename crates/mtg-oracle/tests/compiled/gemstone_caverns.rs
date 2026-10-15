//! Gemstone Caverns: begins the game on the battlefield only for a player who isn't the
//! starting player, with a luck counter, and then they exile a card from their hand. It
//! taps for {C}, or for any color while it has a luck counter.

use super::harness::*;
use mtg_core::Zone;
use mtg_engine::{
    Progress,
    choice::{Answer, ChoiceKind},
    mana::mana_sources,
};
use mtg_ir::effect::ManaOutput;

const CAVERNS: &str = "If this card is in your opening hand and you're not the starting \
                       player, you may begin the game with ~ on the battlefield with a luck \
                       counter on it. If you do, exile a card from your hand.\n{T}: Add {C}. \
                       If ~ has a luck counter on it, instead add one mana of any color.";

/// Run the pregame with both players keeping, answering "begin with these" with every
/// card offered and "exile a card" with `exile`. Returns who was offered what.
fn pregame(g: &mut Game, exile: mtg_core::ObjectId) -> Vec<(mtg_core::PlayerId, String)> {
    g.engine.state.pregame = Some(mtg_engine::state::Pregame::new(vec![P0, P1]));
    let mut asked = Vec::new();
    for _ in 0..50 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::KeepOrMulligan { .. } => {
                    g.engine.answer(&g.table, c.id, Answer::Bool(true)).unwrap();
                }
                ChoiceKind::ChooseObjects { from, .. } => {
                    asked.push((c.who, c.because.to_string()));
                    let answer = if c.because.contains("exile") {
                        assert!(from.contains(&exile));
                        vec![exile]
                    } else {
                        from.clone()
                    };
                    g.engine
                        .answer(&g.table, c.id, Answer::Objects(answer))
                        .unwrap();
                }
                _ => break,
            },
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!(),
        }
    }
    asked
}

fn output(g: &Game, who: mtg_core::PlayerId, caverns: mtg_core::ObjectId) -> Vec<ManaOutput> {
    mana_sources(&g.engine.state, &g.table, who)
        .into_iter()
        .find(|s| s.object == caverns)
        .expect("a mana source")
        .outputs
}

#[test]
fn the_player_on_the_draw_begins_with_it_and_exiles_a_card() {
    let mut t = Table::default();
    let caverns = t.card("", "Legendary Land", None, CAVERNS);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(caverns, P1, Zone::Hand);
    let keep = g.put(bear, P1, Zone::Hand);
    let gone = g.put(bear, P1, Zone::Hand);
    let asked = pregame(&mut g, gone);
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked.iter().all(|(who, _)| *who == P1));
    let on_field = g.find(caverns).expect("began the game on the battlefield");
    let counters = &g.engine.state.objects[&on_field].counters;
    assert_eq!(counters.values().sum::<i32>(), 1, "one luck counter");
    assert!(!g.engine.state.objects.contains_key(&gone), "exiled");
    assert!(g.engine.state.objects.contains_key(&keep));
    assert_eq!(g.count(Zone::Exile, P1) + g.count(Zone::Exile, P0), 0);
    assert_eq!(
        g.engine
            .state
            .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
            .len(),
        1
    );
    assert_eq!(
        output(&g, P1, on_field),
        vec![ManaOutput::AnyOf(vec![
            mtg_core::Color::White,
            mtg_core::Color::Blue,
            mtg_core::Color::Black,
            mtg_core::Color::Red,
            mtg_core::Color::Green,
        ])]
    );
}

#[test]
fn the_starting_player_is_not_offered_it() {
    let mut t = Table::default();
    let caverns = t.card("", "Legendary Land", None, CAVERNS);
    let bear = t.bear();
    let mut g = Game::new(t);
    g.put(caverns, P0, Zone::Hand);
    let other = g.put(bear, P0, Zone::Hand);
    let asked = pregame(&mut g, other);
    assert!(asked.is_empty(), "{asked:?}");
    assert!(g.find(caverns).is_none());
}

#[test]
fn without_a_luck_counter_it_taps_for_colorless() {
    let mut t = Table::default();
    let caverns = t.card("", "Legendary Land", None, CAVERNS);
    let mut g = Game::new(t);
    let on_field = g.put(caverns, P0, Zone::Battlefield);
    g.main();
    assert_eq!(output(&g, P0, on_field), vec![ManaOutput::Colorless]);
}
