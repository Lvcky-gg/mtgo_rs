//! The demo opponent, playing the demo game for real: it has to put a board on the table.

use mtg_core::{CardId, PlayerId, Zone, ZoneRef};
use mtg_engine::{Engine, Progress};
use mtg_headless::{DemoCards, opening_game};
use mtg_policy::bot::Bot;

fn creatures_and_lands(engine: &Engine, cards: &DemoCards, who: PlayerId) -> (usize, usize) {
    let mut out = (0, 0);
    for id in engine.state.objects_in(ZoneRef::shared(Zone::Battlefield)) {
        if mtg_engine::layers::controller(&engine.state, id) != Some(who) {
            continue;
        }
        let Some(ch) = mtg_engine::layers::compute(&engine.state, cards, id) else {
            continue;
        };
        if ch.has_type(mtg_core::CardType::Creature) {
            out.0 += 1;
        }
        if ch.has_type(mtg_core::CardType::Land) {
            out.1 += 1;
        }
    }
    out
}

#[test]
fn two_bots_build_boards_and_fight() {
    let cards = DemoCards::default();
    let bot = Bot::new(&cards, (0..8).map(CardId));
    let mut engine = Engine::new(opening_game());

    let mut rejected = 0;
    let mut ordering_questions = 0;
    for _ in 0..200_000 {
        match engine.advance(&cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => break,
            Progress::NeedsChoice(c) => {
                if engine.state.turn > 12 {
                    break;
                }
                let view = mtg_engine::view::project(&engine.state, c.who);
                if matches!(c.kind, mtg_engine::ChoiceKind::OrderTriggers { .. }) {
                    ordering_questions += 1;
                }
                let answer = bot.decide(&c, &view);
                if let Err(e) = engine.answer(&cards, c.id, answer.clone()) {
                    eprintln!(
                        "REJECTED {e:?}: {:?} -> {answer:?} (default {:?})",
                        c.kind, c.default
                    );
                    rejected += 1;
                    engine
                        .answer(&cards, c.id, mtg_policy::well_formed(&c, &view))
                        .unwrap();
                }
            }
        }
    }

    assert_eq!(rejected, 0, "every answer the bot gave was legal");
    // Several banners attack together over twelve turns; their life gains commute, so their
    // triggers go on the stack without anyone being asked to order them.
    assert_eq!(
        ordering_questions, 0,
        "no trigger ordering question in the demo game"
    );
    for p in [PlayerId(0), PlayerId(1)] {
        let (creatures, lands) = creatures_and_lands(&engine, &cards, p);
        println!(
            "{p:?}: {creatures} creatures, {lands} lands, {} life",
            engine.state.player(p).life
        );
        assert!(lands >= 3, "{p:?} played lands: {lands}");
        assert!(creatures > 0, "{p:?} cast creatures: {creatures}");
    }
    // Life totals are no guide here — the demo's banner gains life on each attack — so count
    // attacks in the log instead.
    for p in [PlayerId(0), PlayerId(1)] {
        let attacks = engine
            .log
            .iter()
            .filter(|e| {
                matches!(&e.event, mtg_core::Event::Attacked { attacker, .. }
                if mtg_engine::layers::controller(&engine.state, *attacker) == Some(p))
            })
            .count();
        assert!(attacks > 0, "{p:?} attacked at least once in twelve turns");
    }
}
