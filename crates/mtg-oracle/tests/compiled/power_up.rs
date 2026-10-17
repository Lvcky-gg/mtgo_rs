use super::harness::*;
use mtg_core::{Keyword, Step, Target, Zone};

#[test]
fn hercules_can_power_up_at_the_reduced_cost_on_his_entry_turn_only_once() {
    let mut t = Table::default();
    let hero = t.named_card("Hercules, Prince of Power", "{2}{G}", "Legendary Creature — Demigod Warrior Hero", Some((3, 3)), "Power-up — {4}{G}: Put a +1/+1 counter on Hercules. He gains vigilance, indestructible, and haste until end of turn. (Activate each power-up ability only once. Reduce the cost by his mana cost if he entered this turn.)");
    let mut g = Game::new(t);
    g.lands(5);
    let hand = g.put(hero, P0, Zone::Hand);
    g.main();
    g.cast(hand, &[]);
    let hero = g.find(hero).unwrap();
    assert!(offers(&g.main(), hero));
    g.act(activate(hero, 0), &[], &[]);
    assert_eq!(g.pt(hero), (4, 4));
    for k in [Keyword::Vigilance, Keyword::Indestructible, Keyword::Haste] {
        assert!(g.has(hero, k));
    }
    assert!(!offers(&g.main(), hero));
    g.until(P1, Step::Upkeep);
    assert!(!g.has(hero, Keyword::Indestructible));
    assert!(!offers(&g.main(), hero));
}

#[test]
fn power_up_cost_reduction_expires_and_does_not_discount_a_permanent_already_in_play() {
    for entered in [false, true] {
        let mut t = Table::default();
        let hero = t.card(
            "{1}{G}",
            "Creature — Hero",
            Some((2, 3)),
            "Power-up — {3}{G}: Put a +1/+1 counter on this creature.",
        );
        let mut g = Game::new(t);
        g.lands(2);
        let hero = g.put(hero, P0, Zone::Battlefield);
        if entered {
            g.engine.state.objects.get_mut(&hero).unwrap().entered_turn = Some(g.engine.state.turn);
        }
        assert_eq!(offers(&g.main(), hero), entered);
        g.until(P1, Step::Upkeep);
        assert!(!offers(&g.main(), hero));
    }
}

#[test]
fn a_countered_power_up_stays_used_but_blinking_creates_a_new_allowance() {
    let mut t = Table::default();
    let hero = t.card(
        "{0}",
        "Creature — Hero",
        Some((2, 3)),
        "Power-up — {0}: Put a +1/+1 counter on this creature.",
    );
    let stifle = t.card("{0}", "Instant", None, "Counter target activated ability.");
    let blink = t.card("{0}", "Instant", None, "Exile target creature you control, then return that card to the battlefield under its owner's control.");
    let mut g = Game::new(t);
    let original = g.put(hero, P0, Zone::Battlefield);
    let stifle = g.put(stifle, P0, Zone::Hand);
    let blink = g.put(blink, P0, Zone::Hand);
    g.main();
    g.act_holding(activate(original, 0), &[]);
    let ability = g.stack()[0];
    g.cast(stifle, &[Target::Object(ability)]);
    assert_eq!(g.pt(original), (2, 3));
    assert!(!offers(&g.main(), original));
    g.cast(blink, &[Target::Object(original)]);
    let returned = g.find(hero).unwrap();
    assert_ne!(returned, original);
    assert!(offers(&g.main(), returned));
    g.act(activate(returned, 0), &[], &[]);
    assert_eq!(g.pt(returned), (3, 4));
}

#[test]
fn power_up_color_and_colorless_reductions_use_excess_for_generic_mana() {
    use mtg_core::AbilityId;
    for (printed, activation, expected) in [
        ("{2}{G}", "{4}{G}", 2),
        ("{2}{G}{G}", "{4}{G}", 1),
        ("{2}{C}", "{4}{C}", 2),
        ("{2}{C}", "{4}{W}", 2),
        ("{X}{G}", "{1}{G}", 1),
        ("{5}{G}", "{1}{G}", 0),
    ] {
        let mut t = Table::default();
        let source = t.card(
            printed,
            "Creature — Hero",
            Some((2, 3)),
            &format!("Power-up — {activation}: Put a +1/+1 counter on this creature."),
        );
        let mut g = Game::new(t);
        let source = g.put(source, P0, Zone::Battlefield);
        g.engine
            .state
            .objects
            .get_mut(&source)
            .unwrap()
            .entered_turn = Some(g.engine.state.turn);
        let cost = mtg_engine::cost::ability_cost(&g.engine.state, &g.table, source, AbilityId(0))
            .unwrap();
        assert_eq!(cost.mana.mana_value(), expected, "{printed} / {activation}");
        if activation == "{4}{W}" {
            assert!(
                cost.mana
                    .symbols
                    .contains(&mtg_core::ManaSymbol::Colored(mtg_core::Color::White))
            );
        }
    }
}

#[test]
fn power_up_reduction_applies_after_activation_cost_increases() {
    let mut t = Table::default();
    let source = t.card(
        "{5}{G}",
        "Creature — Hero",
        Some((2, 3)),
        "Power-up — {1}{G}: Put a +1/+1 counter on this creature.",
    );
    let tax = t.card(
        "{0}",
        "Enchantment",
        None,
        "Activated abilities of creatures cost {2} more to activate.",
    );
    let mut g = Game::new(t);
    let source = g.put(source, P0, Zone::Battlefield);
    g.put(tax, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&source)
        .unwrap()
        .entered_turn = Some(g.engine.state.turn);
    assert!(offers(&g.main(), source));
    g.act(activate(source, 0), &[], &[]);
    assert_eq!(g.pt(source), (3, 4));
}

#[test]
fn hybrid_power_up_costs_are_blocked_instead_of_losing_the_keyword_rules() {
    struct Names;
    impl mtg_oracle::convert::Subtypes for Names {
        fn intern(&self, _: &str) -> Option<u16> {
            None
        }
    }
    for (mana_cost, text) in [
        (
            "{1}{G/U}",
            "Power-up — {4}{G/U}: Put a +1/+1 counter on this creature.",
        ),
        (
            "{1}{G}",
            "Power-up — {4}{G/U}: Put a +1/+1 counter on this creature.",
        ),
    ] {
        let compiled = mtg_oracle::compile::compile(
            &mtg_oracle::compile::FaceText {
                name: "Test Hero",
                card_types: &[mtg_core::CardType::Creature],
                subtypes: &[],
                oracle_text: Some(text),
                mana_cost,
            },
            &Names,
        );
        assert!(!compiled.understood());
        assert_eq!(compiled.unparsed.len(), 1);
    }
}
