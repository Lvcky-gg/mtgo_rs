use super::harness::*;
use mtg_core::{Event, Target, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

#[test]
fn life_buyback_is_optional_and_is_paid_once() {
    for (life, pay, bought_back) in [(20, true, true), (20, false, false), (3, true, false)] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{R}",
            "Instant",
            None,
            "Buyback—Pay 4 life.\nThis spell deals 1 damage to target player.",
        );
        let mut g = Game::new(t);
        g.lands(1);
        g.engine.state.players.get_mut(&P0).unwrap().life = life;
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        let log_start = g.engine.log.len();
        g.act(
            Action::Cast { object: spell },
            &[Target::Player(P1)],
            &[Answer::Bool(pay)],
        );
        assert_eq!(g.life(P0), life - if bought_back { 4 } else { 0 });
        assert_eq!(g.life(P1), 19);
        let in_hand = g
            .engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == spell_card);
        assert_eq!(in_hand, bought_back);
        assert_eq!(g.count(Zone::Graveyard, P0), usize::from(!bought_back));
        let payments = g.engine.log[log_start..]
            .iter()
            .filter(
                |e| matches!(&e.event, Event::LifeChanged { player, delta: -4 } if *player == P0),
            )
            .count();
        assert_eq!(payments, usize::from(bought_back));
    }
}

#[test]
fn life_kicker_marks_the_permanent_as_kicked() {
    for pay in [false, true] {
        let mut t = Table::default();
        let creature_card = t.card("{G}", "Creature — Bear", Some((2, 2)),
            "Kicker—Pay 3 life.\nIf this creature was kicked, it enters with two +1/+1 counters on it.");
        let mut g = Game::new(t);
        g.lands(1);
        g.engine.state.players.get_mut(&P0).unwrap().life = 5;
        let spell = g.put(creature_card, P0, Zone::Hand);
        g.main();
        g.act(Action::Cast { object: spell }, &[], &[Answer::Bool(pay)]);
        let creature = g.find(creature_card).unwrap();
        assert_eq!(g.pt(creature), if pay { (4, 4) } else { (2, 2) });
        assert_eq!(g.life(P0), if pay { 2 } else { 5 });
    }
}

#[test]
fn printed_slaughter_compiles_with_life_buyback() {
    let mut t = Table::default();
    t.card(
        "{1}{B}",
        "Instant",
        None,
        "Buyback—Pay 4 life.\nDestroy target nonblack creature. It can't be regenerated.",
    );
}

#[test]
fn paying_all_remaining_life_is_legal_and_loses_before_resolution() {
    let mut t = Table::default();
    let spell_card = t.card(
        "{R}",
        "Instant",
        None,
        "Buyback—Pay 4 life.\nThis spell deals 1 damage to target player.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    g.engine.state.players.get_mut(&P0).unwrap().life = 4;
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[Target::Player(P1)],
        &[Answer::Bool(true)],
    );
    assert_eq!(g.life(P0), 0);
    assert!(g.engine.state.players[&P0].has_lost);
    assert_eq!(g.life(P1), 20);
}

#[test]
fn buyback_reserves_life_for_the_other_additional_cost() {
    for (life, bought_back) in [(5, false), (7, true)] {
        let mut t = Table::default();
        let spell_card = t.card("{R}", "Instant", None,
            "As an additional cost to cast this spell, pay 2 life.\nBuyback—Pay 4 life.\nThis spell deals 1 damage to target player.");
        let mut g = Game::new(t);
        g.lands(1);
        g.engine.state.players.get_mut(&P0).unwrap().life = life;
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[Target::Player(P1)],
            &[Answer::Bool(true)],
        );
        assert_eq!(g.life(P0), life - 2 - if bought_back { 4 } else { 0 });
        assert_eq!(g.life(P1), 19);
        assert_eq!(g.count(Zone::Graveyard, P0), usize::from(!bought_back));
    }
}

#[test]
fn buyback_reserves_life_before_paying_phyrexian_mana() {
    for (life, bought_back) in [(5, false), (7, true)] {
        let mut t = Table::default();
        let spell_card = t.card(
            "{B/P}",
            "Instant",
            None,
            "Buyback—Pay 4 life.\nThis spell deals 1 damage to target player.",
        );
        let mut g = Game::new(t);
        g.engine.state.players.get_mut(&P0).unwrap().life = life;
        let spell = g.put(spell_card, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[Target::Player(P1)],
            &[Answer::Bool(true)],
        );
        assert_eq!(g.life(P0), life - 2 - if bought_back { 4 } else { 0 });
        assert_eq!(g.life(P1), 19);
        assert_eq!(g.count(Zone::Graveyard, P0), usize::from(!bought_back));
    }
}
