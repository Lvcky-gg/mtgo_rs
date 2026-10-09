//! "You may draw a card unless that player pays {1}" (Rhystic Study, Mystic Remora, Esper
//! Sentinel), "that player may pay {2}. If the player doesn't, …" (Smothering Tithe) and
//! "a player casts their second spell each turn" (Lotho): another player decides whether
//! to pay, pays from their own mana, and the effect happens only if they don't.

use super::harness::*;
use mtg_core::{ObjectId, PlayerId, Step, Zone};
use mtg_engine::{
    Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const RHYSTIC: &str =
    "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}.";
const REMORA: &str = "Whenever an opponent casts a noncreature spell, you may draw a card unless that player pays {4}.";
const SENTINEL: &str = "Whenever an opponent casts their first noncreature spell each turn, draw a \
                        card unless that player pays {X}, where X is this creature's power.";
const LOTHO: &str = "Whenever a player casts their second spell each turn, you lose 1 life and create a Treasure token.";
const TITHE: &str = "Whenever an opponent draws a card, that player may pay {2}. If the player \
                     doesn't, you create a Treasure token.";

/// P1's lands, each tapping for any color.
fn p1_lands(game: &mut Game, n: usize) {
    let land = game
        .table
        .card("", "Land", None, "{T}: Add one mana of any color.");
    for _ in 0..n {
        game.put(land, P1, Zone::Battlefield);
    }
}

fn untapped(game: &Game, who: PlayerId) -> usize {
    game.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &game.engine.state.objects[id];
            o.controller == who && !o.tapped
        })
        .count()
}

/// Play out P1's turn up to its first main phase, cast `spells` there in order (each
/// resolving before the next), answering yes/no questions from `answers` — each tagged
/// with who must be the one asked — and anything else by default. Every answer must be
/// used.
fn p1_casts(game: &mut Game, spells: &[ObjectId], answers: &[(PlayerId, bool)]) {
    let mut spells = spells.iter().copied();
    let mut answers = answers.iter().copied();
    let mut casting = spells.next();
    for _ in 0..10_000 {
        match game.engine.advance(&game.table) {
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
            Progress::NeedsChoice(c) => {
                let answer = match &c.kind {
                    ChoiceKind::Confirm => {
                        let (who, yes) = answers
                            .next()
                            .unwrap_or_else(|| panic!("unexpected question for {:?}", c.who));
                        assert_eq!(c.who, who, "asked the wrong player: {}", c.because);
                        Answer::Bool(yes)
                    }
                    ChoiceKind::Priority { .. }
                        if c.who == P1
                            && game.engine.state.active_player == P1
                            && game.engine.state.step == Step::PrecombatMain
                            && game.stack().is_empty() =>
                    {
                        match casting.take() {
                            Some(object) => {
                                casting = spells.next();
                                Answer::Action(Action::Cast { object })
                            }
                            None => {
                                assert!(answers.next().is_none(), "unused answers");
                                return;
                            }
                        }
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                game.engine
                    .answer(&game.table, c.id, answer)
                    .unwrap_or_else(|e| panic!("{e:?}"));
            }
        }
    }
    panic!("P1 never finished casting");
}

struct Setup {
    game: Game,
    spells: Vec<ObjectId>,
}

/// P0 controls a permanent with `text`; P1 has `lands` lands and one-mana spells in hand:
/// one noncreature spell per `noncreature` entry that is true, a creature for each false.
fn setup(text: &str, pt: Option<(i32, i32)>, lands: usize, kinds: &[bool]) -> Setup {
    let mut table = Table::default();
    let permanent = match pt {
        Some(pt) => table.card("", "Creature — Human", Some(pt), text),
        None => table.card("", "Enchantment", None, text),
    };
    let sorcery = table.card("{1}", "Sorcery", None, "You gain 1 life.");
    let creature = table.card("{1}", "Creature — Bear", Some((2, 2)), "");
    let mut game = Game::new(table);
    game.put(permanent, P0, Zone::Battlefield);
    p1_lands(&mut game, lands);
    let spells = kinds
        .iter()
        .map(|noncreature| {
            let card = if *noncreature { sorcery } else { creature };
            game.put(card, P1, Zone::Hand)
        })
        .collect();
    game.main();
    Setup { game, spells }
}

#[test]
fn rhystic_study_opponent_declines_and_you_draw() {
    let Setup { mut game, spells } = setup(RHYSTIC, None, 2, &[true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P1, false), (P0, true)]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn rhystic_study_opponent_pays_with_their_own_mana() {
    let Setup { mut game, spells } = setup(RHYSTIC, None, 2, &[true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P1, true)]);
    assert_eq!(game.count(Zone::Hand, P0), hand, "paid: no card");
    assert_eq!(
        untapped(&game, P1),
        0,
        "one land for the spell, one for the tax"
    );
}

#[test]
fn rhystic_study_opponent_who_cannot_pay_is_not_asked() {
    let Setup { mut game, spells } = setup(RHYSTIC, None, 1, &[true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P0, true)]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn rhystic_study_you_may_still_decline_the_card() {
    let Setup { mut game, spells } = setup(RHYSTIC, None, 1, &[true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P0, false)]);
    assert_eq!(game.count(Zone::Hand, P0), hand);
}

#[test]
fn mystic_remora_ignores_creature_spells() {
    let Setup { mut game, spells } = setup(REMORA, None, 2, &[false, true]);
    let hand = game.count(Zone::Hand, P0);
    // The creature spell asks nothing; the sorcery triggers, and P1 can't pay {4}.
    p1_casts(&mut game, &spells, &[(P0, true)]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1);
}

#[test]
fn esper_sentinel_only_the_first_noncreature_spell_and_x_is_its_power() {
    // Power 2, so the tax is {2}: P1, with three lands, can pay it after one spell but
    // declines. A creature first doesn't count; the second noncreature spell doesn't
    // trigger.
    let Setup { mut game, spells } = setup(SENTINEL, Some((2, 1)), 4, &[false, true, true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P1, false)]);
    assert_eq!(game.count(Zone::Hand, P0), hand + 1, "exactly one card");
}

#[test]
fn esper_sentinel_paid_tax_is_its_power() {
    let Setup { mut game, spells } = setup(SENTINEL, Some((2, 1)), 3, &[true]);
    let hand = game.count(Zone::Hand, P0);
    p1_casts(&mut game, &spells, &[(P1, true)]);
    assert_eq!(game.count(Zone::Hand, P0), hand);
    assert_eq!(
        untapped(&game, P1),
        0,
        "{{1}} for the spell and {{2}} for the tax"
    );
}

#[test]
fn lotho_triggers_on_any_players_second_spell() {
    let Setup { mut game, spells } = setup(LOTHO, Some((2, 2)), 3, &[true, false, true]);
    p1_casts(&mut game, &spells, &[]);
    assert_eq!(game.life(P0), 19, "only the second spell");
    let treasures = game
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &game.engine.state.objects[id];
            o.is_token && o.controller == P0
        })
        .count();
    assert_eq!(treasures, 1);
}

fn tithe_draw(lands: usize, answers: &[(PlayerId, bool)]) -> usize {
    let Setup {
        mut game,
        spells: _,
    } = setup(TITHE, None, lands, &[]);
    // P1's draw step draws a card; then P1 casts nothing.
    p1_casts(&mut game, &[], answers);
    game.engine
        .state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let o = &game.engine.state.objects[id];
            o.is_token && o.controller == P0
        })
        .count()
}

#[test]
fn smothering_tithe_opponent_declines_and_you_get_a_treasure() {
    assert_eq!(tithe_draw(2, &[(P1, false)]), 1);
}

#[test]
fn smothering_tithe_opponent_pays_two() {
    assert_eq!(tithe_draw(2, &[(P1, true)]), 0);
}

#[test]
fn smothering_tithe_opponent_who_cannot_pay_is_not_asked() {
    assert_eq!(tithe_draw(1, &[]), 1);
}
