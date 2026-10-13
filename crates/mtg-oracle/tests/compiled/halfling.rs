use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::actions::Action;
use mtg_ir::PrintedCards;

const HALFLING: &str = "{T}: Add {C}.\n{T}: Add one mana of any color. Spend this mana only to cast a legendary spell, and that spell can't be countered.";

fn countered(cost: &str, legendary: bool, floating: bool, extra_mana: bool) -> Game {
    let mut t = Table::default();
    let halfling = t.card("{G}", "Creature", Some((1, 2)), HALFLING);
    let ty = if legendary {
        "Legendary Enchantment"
    } else {
        "Enchantment"
    };
    let spell = t.card(cost, ty, None, "When ~ enters, you gain 3 life.");
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    let source = g.put(halfling, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    if extra_mana {
        g.engine
            .state
            .players
            .get_mut(&P0)
            .unwrap()
            .mana
            .add(Some(Color::Blue), 1);
    }
    if floating {
        g.act(
            Action::ActivateManaAbility {
                source,
                ability: mtg_core::AbilityId(1),
                color: Some(Color::Blue),
            },
            &[],
            &[],
        );
    }
    g.act_holding(Action::Cast { object: spell }, &[]);
    let on_stack = g.stack()[0];
    g.cast(counter, &[Target::Object(on_stack)]);
    g
}

#[test]
fn halfling_mana_protects_legendary_spells_with_automatic_or_floating_payment() {
    for floating in [false, true] {
        let g = countered("{U}", true, floating, false);
        assert_eq!(g.life(P0), 23);
        assert!(g.engine.state.restricted_mana.is_empty());
    }
}

#[test]
fn colorless_halfling_mana_does_not_protect_a_nonlegendary_spell() {
    let g = countered("{1}", false, false, false);
    assert_eq!(g.life(P0), 20);
}

#[test]
fn protected_floating_mana_is_used_before_ordinary_mana_of_the_same_color() {
    let g = countered("{U}", true, true, true);
    assert_eq!(g.life(P0), 23);
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Blue as usize],
        1
    );
    assert!(g.engine.state.restricted_mana.is_empty());
}

#[test]
fn ineligible_halfling_mana_remains_unspent_and_does_not_protect_another_spell() {
    let g = countered("{U}", false, true, true);
    assert_eq!(g.life(P0), 20);
    assert_eq!(g.engine.state.restricted_mana.len(), 1);
    assert_eq!(
        g.engine.state.player(P0).mana.amounts[Color::Blue as usize],
        1
    );
}

#[test]
fn a_copy_of_a_protected_spell_can_be_countered() {
    let mut t = Table::default();
    let halfling = t.card("{G}", "Creature", Some((1, 2)), HALFLING);
    let original = t.card(
        "{U}",
        "Legendary Enchantment",
        None,
        "When ~ enters, you gain 3 life.",
    );
    let copy = t.card("{0}", "Instant", None, "Copy target spell.");
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    g.put(halfling, P0, Zone::Battlefield);
    let original = g.put(original, P0, Zone::Hand);
    let copy = g.put(copy, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: original }, &[]);
    let original = g.stack()[0];
    g.act_holding(Action::Cast { object: copy }, &[Target::Object(original)]);
    g.drive(
        |_, _| None,
        |g| {
            g.engine
                .state
                .objects
                .values()
                .any(|o| o.is_spell_copy && o.zone.zone == Zone::Stack)
        },
    );
    let copied = g
        .stack()
        .into_iter()
        .find(|id| g.engine.state.objects[id].is_spell_copy)
        .unwrap();
    assert!(
        !g.engine.state.objects[&copied]
            .cast_context
            .as_ref()
            .unwrap()
            .uncounterable
    );
    g.cast(counter, &[Target::Object(copied)]);
    assert_eq!(g.life(P0), 23);
}

#[test]
fn cavern_mana_remembers_the_chosen_type_after_the_land_leaves() {
    let mut t = Table::default();
    let cavern = t.card("", "Land", None, "As this land enters, choose a creature type.\n{T}: Add {C}.\n{T}: Add one mana of any color. Spend this mana only to cast a creature spell of the chosen type, and that spell can't be countered.");
    let bear = t.card(
        "{U}",
        "Creature — Bear",
        Some((2, 2)),
        "When ~ enters, you gain 3 life.",
    );
    let wizard = t.card("{U}", "Creature — Wizard", Some((2, 2)), "");
    let destroy = t.card("{0}", "Instant", None, "Destroy target land.");
    let counter = t.card("{0}", "Instant", None, "Counter target spell.");
    let mut g = Game::new(t);
    let source = g.put(cavern, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&source)
        .unwrap()
        .chosen_subtype = g.table.subtype_named("Bear");
    let bear = g.put(bear, P0, Zone::Hand);
    let wizard = g.put(wizard, P0, Zone::Hand);
    let destroy = g.put(destroy, P0, Zone::Hand);
    let counter = g.put(counter, P0, Zone::Hand);
    g.main();
    g.act(
        Action::ActivateManaAbility {
            source,
            ability: mtg_core::AbilityId(2),
            color: Some(Color::Blue),
        },
        &[],
        &[],
    );
    g.cast(destroy, &[Target::Object(source)]);
    let mtg_engine::choice::ChoiceKind::Priority { legal } = &g.pending.as_ref().unwrap().kind
    else {
        panic!("priority");
    };
    assert!(legal.actions.contains(&Action::Cast { object: bear }));
    assert!(!legal.actions.contains(&Action::Cast { object: wizard }));
    g.act_holding(Action::Cast { object: bear }, &[]);
    let spell = g.stack()[0];
    g.cast(counter, &[Target::Object(spell)]);
    assert_eq!(g.life(P0), 23);
}
