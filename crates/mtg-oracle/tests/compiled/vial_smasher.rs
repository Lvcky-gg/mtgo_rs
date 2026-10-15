use super::harness::*;
use mtg_core::{PlayerId, Step, Zone};
use mtg_engine::{actions::Action, choice::Answer, state::PlayerState};

const VIAL: &str = "Whenever you cast your first spell each turn, choose an opponent at random. Vial Smasher deals damage equal to that spell's mana value to that player or a planeswalker that player controls.\nPartner";

#[test]
fn vial_smasher_triggers_on_each_turns_first_spell_and_does_not_target() {
    let mut t = Table::default();
    let vial = t.named_card(
        "Vial Smasher the Fierce",
        "{1}{B}{R}",
        "Legendary Creature",
        Some((2, 3)),
        VIAL,
    );
    let spell = t.card("{3}", "Instant", None, "You gain 1 life.");
    let shield = t.card("{0}", "Enchantment", None, "You have hexproof.");
    let mut g = Game::new(t);
    g.lands(9);
    g.put(vial, P0, Zone::Battlefield);
    g.put(shield, P1, Zone::Battlefield);
    let spells: Vec<_> = (0..3).map(|_| g.put(spell, P0, Zone::Hand)).collect();
    g.main();
    g.cast(spells[0], &[]);
    assert_eq!(g.life(P1), 17);
    g.cast(spells[1], &[]);
    assert_eq!(g.life(P1), 17, "second spell does not trigger");
    g.until(P1, Step::Upkeep);
    let prompt = g.pending.take().unwrap();
    g.engine.answer(&g.table, prompt.id, Answer::Pass).unwrap();
    g.drive(|_, _| None, |g| g.engine.state.priority == Some(P0));
    g.cast(spells[2], &[]);
    assert_eq!(
        g.life(P1),
        14,
        "first spell on an opponent's turn triggers too"
    );
}

#[test]
fn vial_smasher_can_hit_the_chosen_opponents_planeswalker_through_hexproof() {
    for choose_walker in [false, true] {
        let mut t = Table::default();
        let vial = t.named_card(
            "Vial Smasher the Fierce",
            "{1}{B}{R}",
            "Legendary Creature",
            Some((2, 3)),
            VIAL,
        );
        let spell = t.card("{3}", "Instant", None, "You gain 1 life.");
        let walker = t.loyalty("{1}", "Planeswalker", 7, "Hexproof");
        let mut g = Game::new(t);
        g.lands(3);
        g.put(vial, P0, Zone::Battlefield);
        let walker = g.put(walker, P1, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&walker)
            .unwrap()
            .counters
            .insert(mtg_core::CounterKind::Loyalty, 7);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Modes(vec![u8::from(choose_walker)])],
        );
        assert_eq!(g.life(P1), if choose_walker { 20 } else { 17 });
        assert_eq!(
            g.engine.state.objects[&walker].counters[&mtg_core::CounterKind::Loyalty],
            if choose_walker { 4 } else { 7 }
        );
    }
}

#[test]
fn vial_smasher_chooses_a_single_opponent_using_the_replayable_rng() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in [1, 2, 3, 4] {
        let mut t = Table::default();
        let vial = t.named_card(
            "Vial Smasher the Fierce",
            "{1}{B}{R}",
            "Legendary Creature",
            Some((2, 3)),
            VIAL,
        );
        let spell = t.card("{3}", "Instant", None, "You gain 1 life.");
        let walker = t.loyalty("{0}", "Planeswalker", 5, "Hexproof");
        let mut g = Game::new(t);
        let p2 = PlayerId(2);
        g.engine.state.players.insert(p2, PlayerState::new(p2, 20));
        g.engine.state.turn_order.push(p2);
        g.engine.state.rng = mtg_engine::state::Rng::from_seed(&[seed; 32]);
        g.lands(3);
        g.put(vial, P0, Zone::Battlefield);
        let spell = g.put(spell, P0, Zone::Hand);
        for player in [P1, p2] {
            let id = g.put(walker, player, Zone::Battlefield);
            g.engine
                .state
                .objects
                .get_mut(&id)
                .unwrap()
                .counters
                .insert(mtg_core::CounterKind::Loyalty, 5);
        }
        g.main();
        let before = g.engine.state.clone();
        g.cast(spell, &[]);
        let lives = [g.life(P1), g.life(p2)];
        assert!(lives == [17, 20] || lives == [20, 17]);
        seen.insert(lives);
        g.engine = mtg_engine::Engine::new(before);
        g.pending = None;
        g.main();
        g.cast(spell, &[]);
        assert_eq!(
            [g.life(P1), g.life(p2)],
            lives,
            "replaying a seed reproduces the random opponent after a recipient prompt"
        );
    }
    assert_eq!(seen.len(), 2, "different seeds can select either opponent");
}

#[test]
fn vial_smasher_counts_the_announced_x_and_the_mana_cost_not_mana_paid() {
    let mut t = Table::default();
    let vial = t.named_card(
        "Vial Smasher the Fierce",
        "{1}{B}{R}",
        "Legendary Creature",
        Some((2, 3)),
        VIAL,
    );
    let spell = t.card("{X}{X}{1}", "Instant", None, "You gain X life.");
    let mut g = Game::new(t);
    g.lands(7);
    g.put(vial, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    g.act(Action::Cast { object: spell }, &[], &[Answer::Number(3)]);
    assert_eq!(g.life(P1), 13, "two X symbols at 3, plus 1");
    assert_eq!(g.life(P0), 23);
}

#[test]
fn countering_the_spell_preserves_its_mana_value_for_vial_smasher() {
    use mtg_core::Target;
    let mut t = Table::default();
    let vial = t.named_card(
        "Vial Smasher the Fierce",
        "{1}{B}{R}",
        "Legendary Creature",
        Some((2, 3)),
        VIAL,
    );
    let spell = t.card("{X}{1}", "Instant", None, "You gain X life.");
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    g.lands(5);
    g.put(vial, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    // Announce X explicitly and stop at our first priority with the cast trigger stacked.
    g.main();
    let prompt = g.pending.take().unwrap();
    g.engine
        .answer(
            &g.table,
            prompt.id,
            Answer::Action(Action::Cast { object: spell }),
        )
        .unwrap();
    g.drive(
        |_, c| {
            if matches!(c.kind, mtg_engine::choice::ChoiceKind::ChooseX { .. }) {
                Some(Answer::Number(4))
            } else {
                None
            }
        },
        |g| g.engine.state.priority == Some(P0) && g.stack().len() == 2,
    );
    let original = *g.stack().last().unwrap();
    g.cast(counter, &[Target::Object(original)]);
    assert_eq!(
        g.life(P1),
        15,
        "last-known X=4 plus 1 after the spell is countered"
    );
    assert_eq!(g.life(P0), 20);
}
