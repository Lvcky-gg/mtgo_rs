//! Scoped independent CR615.12/122.1c decision table, with no competing ordinary
//! prevention effects. Ordering choices are outside this table's scope.
mod support;
use mtg_core::{AbilityId, Color, CounterKind, DamageShield, Event, Target, Zone, ZoneRef};
use mtg_engine::prevention;
use mtg_ir::{
    Ability, AbilityKind, ObjectFilter, Selector,
    effect::{Modification, Restriction},
};
use proptest::prelude::*;
use support::{CREATURE, Cards, P0, P1, main_state};

fn property_config() -> ProptestConfig {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        ProptestConfig::default()
    } else {
        ProptestConfig::with_cases(10_000)
    }
}

proptest! {
    #![proptest_config(property_config())]
    #[test]
    fn prevention_agrees_with_scoped_rules_table(
        amount in 0u32..32, capacity in 0u32..32,
        kind in 0u8..5, unpreventable in any::<bool>(), combat in any::<bool>(),
    ) {
        // 0=no prevention, 1=protection from red, 2=shield counter,
        // 3=Phantom counter-removal prevention, 4=finite next-N shield.
        let mut cards = Cards::creature(100, false);
        cards.0.colors = Some(vec![Color::Red]);
        let ability = match kind {
            1 => Some(AbilityKind::Protection { from: ObjectFilter::HasColor(Color::Red) }),
            3 => Some(AbilityKind::Static { what: Selector::SelfSource,
                modification: Modification::Restriction(Restriction::PreventDamageRemoveCounter),
                condition: None }),
            _ => None,
        };
        if let Some(kind) = ability {
            cards.0.abilities.push(Ability { id: AbilityId(0), kind, targets: vec![], source_text: None });
        }
        let mut state = main_state();
        let source = state.place(CREATURE, P0, ZoneRef::shared(Zone::Battlefield));
        let target = state.place(CREATURE, P1, ZoneRef::shared(Zone::Battlefield));
        state.damage_unpreventable = unpreventable;
        let counter = match kind { 2 => Some(CounterKind::Shield), 3 => Some(CounterKind::PlusOnePlusOne), _ => None };
        if let Some(counter) = counter {
            state.objects.get_mut(&target).unwrap().counters.insert(counter, 2);
        }
        if kind == 4 {
            state.damage_shields.push(DamageShield { id: 1, to: Some(Target::Object(target)),
                by: Some(source), remaining: Some(capacity), combat_only: combat });
        }
        let mut events = vec![];
        let dealt = prevention::prevent(&state, &cards, source, Target::Object(target), amount, combat, &mut events);
        let expected = match (unpreventable, kind) {
            (true, _) | (false, 0) => amount,
            (false, 4) => amount.saturating_sub(capacity),
            _ => 0,
        };
        prop_assert_eq!(dealt, expected);
        let spent: u32 = events.iter().filter_map(|event| match event {
            Event::DamageShieldUsed { shield: 1, amount } => Some(*amount), _ => None,
        }).sum();
        prop_assert_eq!(spent, if kind == 4 && !unpreventable { amount - expected } else { 0 });
        let removed: Vec<_> = events.iter().filter_map(|event| match event {
            Event::CountersChanged { object, kind, delta: -1 } if *object == target => Some(*kind), _ => None,
        }).collect();
        let expected_removal: Vec<_> = counter.filter(|_| amount > 0).into_iter().collect();
        prop_assert_eq!(removed, expected_removal);
        // A simultaneous batch removes a counter once, even with several sources.
        let second = prevention::prevent(&state, &cards, source, Target::Object(target), amount, combat, &mut events);
        if kind == 4 && !unpreventable {
            prop_assert_eq!(dealt + second, (amount * 2).saturating_sub(capacity));
            let total_spent: u32 = events.iter().filter_map(|event| match event {
                Event::DamageShieldUsed { shield: 1, amount } => Some(*amount), _ => None,
            }).sum();
            prop_assert!(total_spent <= capacity);
        }
        let removals = events.iter().filter(|event| matches!(event,
            Event::CountersChanged { object, delta: -1, .. } if *object == target)).count();
        prop_assert_eq!(removals, usize::from(counter.is_some() && amount > 0));
    }
}
