//! Veil of Summer: "Draw a card if an opponent has cast a blue or black spell this turn.
//! Spells you control can't be countered this turn. You and permanents you control gain
//! hexproof from blue and from black until end of turn."

use super::harness::*;
use mtg_core::{Step, Target, Zone};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const VEIL: &str = "Draw a card if an opponent has cast a blue or black spell this turn. \
                    Spells you control can't be countered this turn. You and permanents you \
                    control gain hexproof from blue and from black until end of turn.";

fn any_land(t: &mut Table) -> mtg_core::CardId {
    t.card("", "Land", None, "{T}: Add one mana of any color.")
}

/// P0 casts `first` in their main phase; P1 answers with `response` (targeting what the
/// engine offers first), and P0 then casts `veil` with P1's spell on the stack.
fn opponent_casts_then_veil(response_cost: &str) -> (Game, usize) {
    let mut t = Table::default();
    let veil = t.card("{G}", "Instant", None, VEIL);
    let gift = t.card("{0}", "Sorcery", None, "You gain 3 life.");
    let response = t.card(
        response_cost,
        "Instant",
        None,
        "Target player draws a card.",
    );
    let land = any_land(&mut t);
    let mut g = Game::new(t);
    g.lands(1);
    g.put(land, P1, Zone::Battlefield);
    let gift = g.put(gift, P0, Zone::Hand);
    let response = g.put(response, P1, Zone::Hand);
    let veil = g.put(veil, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0) - 2;
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: gift }),
        )
        .unwrap();
    let (mut responded, veiled) = (false, std::cell::Cell::new(false));
    g.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. } if c.who == P1 && !responded => {
                responded = true;
                Some(Answer::Action(Action::Cast { object: response }))
            }
            ChoiceKind::ChooseTargets { .. } if c.who == P1 => {
                Some(Answer::Targets(vec![vec![Target::Player(P1)]]))
            }
            ChoiceKind::Priority { .. }
                if c.who == P0 && responded && !veiled.get() && game.stack().len() == 2 =>
            {
                veiled.set(true);
                Some(Answer::Action(Action::Cast { object: veil }))
            }
            _ => None,
        },
        |game| {
            veiled.get() && game.stack().is_empty() && game.engine.state.step == Step::PrecombatMain
        },
    );
    (g, hand)
}

#[test]
fn draws_after_an_opponent_casts_a_blue_spell() {
    let (g, hand) = opponent_casts_then_veil("{U}");
    assert_eq!(g.count(Zone::Hand, P0), hand + 1);
    assert_eq!(g.life(P0), 23);
}

#[test]
fn draws_nothing_after_an_opponent_casts_a_red_spell() {
    let (g, hand) = opponent_casts_then_veil("{R}");
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn your_own_blue_spell_does_not_count() {
    let mut t = Table::default();
    let veil = t.card("{G}", "Instant", None, VEIL);
    let blue = t.card("{U}", "Sorcery", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(2);
    let blue = g.put(blue, P0, Zone::Hand);
    let veil = g.put(veil, P0, Zone::Hand);
    g.main();
    g.cast(blue, &[]);
    let hand = g.count(Zone::Hand, P0) - 1;
    g.main();
    g.cast(veil, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand);
}

#[test]
fn spells_cast_after_it_resolves_cannot_be_countered() {
    let mut t = Table::default();
    let veil = t.card("{G}", "Instant", None, VEIL);
    let gift = t.card("{0}", "Sorcery", None, "You gain 3 life.");
    let counter = t.card("{R}", "Instant", None, "Counter target spell.");
    let land = any_land(&mut t);
    let mut g = Game::new(t);
    g.lands(1);
    g.put(land, P1, Zone::Battlefield);
    let veil = g.put(veil, P0, Zone::Hand);
    let gift = g.put(gift, P0, Zone::Hand);
    let counter = g.put(counter, P1, Zone::Hand);
    g.main();
    g.cast(veil, &[]);
    g.main();
    let c = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            c.id,
            Answer::Action(Action::Cast { object: gift }),
        )
        .unwrap();
    let countered = std::cell::Cell::new(false);
    g.drive(
        |game, c| match &c.kind {
            ChoiceKind::Priority { .. }
                if c.who == P1 && !countered.get() && !game.stack().is_empty() =>
            {
                countered.set(true);
                Some(Answer::Action(Action::Cast { object: counter }))
            }
            ChoiceKind::ChooseTargets { slots, .. } if c.who == P1 => {
                Some(Answer::Targets(vec![vec![slots[0][0]]]))
            }
            _ => None,
        },
        |game| countered.get() && game.stack().is_empty(),
    );
    assert_eq!(g.life(P0), 23, "the countered spell still resolved");
}

#[test]
fn blue_and_black_sources_of_opponents_cannot_target_you_or_your_permanents() {
    let mut t = Table::default();
    let veil = t.card("{G}", "Instant", None, VEIL);
    let bolt = |t: &mut Table, cost: &str| {
        t.card(
            cost,
            "Instant",
            None,
            "This spell deals 1 damage to any target.",
        )
    };
    let (blue, black, red) = (
        bolt(&mut t, "{U}"),
        bolt(&mut t, "{B}"),
        bolt(&mut t, "{R}"),
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let mine = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let veil = g.put(veil, P0, Zone::Hand);
    let spells = [blue, black, red].map(|c| (g.put(c, P1, Zone::Hand), c));
    let own_blue = g.put(blue, P0, Zone::Hand);
    g.main();
    g.cast(veil, &[]);
    let legal = |g: &Game, spell: mtg_core::ObjectId, card, who| {
        let face = mtg_ir::PrintedCards::face(&g.table, card, 0)
            .unwrap()
            .clone();
        mtg_engine::targeting::legal_targets(
            &g.engine.state,
            &g.table,
            &face.abilities[0].targets[0],
            spell,
            who,
            &[],
        )
    };
    for (i, (spell, card)) in spells.into_iter().enumerate() {
        let targets = legal(&g, spell, card, P1);
        let shielded = i < 2;
        assert_eq!(
            !targets.contains(&Target::Player(P0)),
            shielded,
            "spell {i}"
        );
        assert_eq!(
            !targets.contains(&Target::Object(mine)),
            shielded,
            "spell {i}"
        );
        assert!(targets.contains(&Target::Object(theirs)));
        assert!(targets.contains(&Target::Player(P1)));
    }
    let targets = legal(&g, own_blue, blue, P0);
    assert!(
        targets.contains(&Target::Player(P0)),
        "your own spells still can"
    );
    assert!(targets.contains(&Target::Object(mine)));
    // It wears off at end of turn.
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |g| g.engine.state.active_player == P1);
    let targets = legal(&g, spells[0].0, blue, P1);
    assert!(targets.contains(&Target::Player(P0)));
    assert!(targets.contains(&Target::Object(mine)));
}
