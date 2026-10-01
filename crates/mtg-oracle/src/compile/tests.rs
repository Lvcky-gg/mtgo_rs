use super::*;
use mtg_core::Keyword;

#[test]
fn global_combat_prevention_is_exact_and_composes_with_other_effects() {
    let compiled = instant("Prevent all combat damage that would be dealt this turn. Draw a card.");
    assert!(compiled.understood());
    let AbilityKind::SpellEffect(Effect::Sequence(items)) = &compiled.abilities[0].kind else {
        panic!("sequence")
    };
    assert!(matches!(items[0], Effect::PreventAllCombatDamage));
    for text in [
        "Prevent all combat damage that would be dealt this turn by creatures you control.",
        "Prevent all combat damage that would be dealt this turn except by Dragons.",
    ] {
        assert!(!instant(text).understood(), "{text}");
    }
}

#[test]
fn targeted_prevention_accepts_recipients_but_rejects_other_scopes() {
    for text in [
        "Prevent all damage that would be dealt to target creature this turn.",
        "Prevent all damage that would be dealt to target player this turn.",
        "Prevent all damage that would be dealt to any target this turn.",
    ] {
        let c = instant(text);
        assert!(c.understood(), "{text}");
        assert_eq!(c.abilities[0].targets.len(), 1);
        assert!(matches!(
            c.abilities[0].kind,
            AbilityKind::SpellEffect(Effect::PreventDamage { .. })
        ));
    }
    for text in [
        "Prevent all damage that would be dealt to all creatures this turn.",
        "Prevent all combat damage that would be dealt to you and creatures you control this turn.",
        "Prevent all damage that would be dealt to target creature this turn by red sources.",
        "Prevent all damage that would be dealt to target creature next turn.",
        "Prevent all damage that would be dealt by creatures this turn.",
    ] {
        assert!(!instant(text).understood(), "{text}");
    }
}

#[test]
fn prevention_shields_compile_with_their_scope() {
    for (text, want_to, want_by, want_combat, want_amount) in [
        (
            "Prevent all damage that would be dealt this turn.",
            false,
            false,
            false,
            None,
        ),
        (
            "Prevent the next 3 damage that would be dealt to any target this turn.",
            true,
            false,
            false,
            Some(3),
        ),
        (
            "Prevent the next 3 damage that would be dealt to target creature this turn.",
            true,
            false,
            false,
            Some(3),
        ),
        (
            "Prevent all combat damage that would be dealt to you this turn.",
            true,
            false,
            true,
            None,
        ),
        (
            "Prevent all combat damage that would be dealt by target attacking creature this turn.",
            false,
            true,
            true,
            None,
        ),
    ] {
        let c = instant(text);
        assert!(c.understood(), "{text}");
        let AbilityKind::SpellEffect(Effect::PreventDamageShield {
            to,
            by,
            combat_only,
            amount,
        }) = &c.abilities[0].kind
        else {
            panic!("{text}: {:?}", c.abilities[0].kind)
        };
        assert_eq!(to.is_some(), want_to, "{text}");
        assert_eq!(by.is_some(), want_by, "{text}");
        assert_eq!(*combat_only, want_combat, "{text}");
        assert_eq!(*amount, want_amount.map(Value::Fixed), "{text}");
    }
}

#[test]
fn static_prevention_needs_a_permanent_subject() {
    for text in [
        "Prevent all combat damage that would be dealt to this creature.",
        "Prevent all damage that would be dealt to this creature by creatures.",
        "Prevent all damage that would be dealt by enchanted creature.",
        "Prevent all combat damage that would be dealt to and dealt by enchanted creature.",
    ] {
        let c = creature(text);
        assert!(c.understood(), "{text}");
        assert!(matches!(
            c.abilities[0].kind,
            AbilityKind::Static {
                modification: Modification::Restriction(Restriction::PreventDamage { .. }),
                ..
            }
        ));
    }
    for text in [
        "Prevent the next 2 damage that would be dealt to this creature.",
        "Prevent all damage that would be dealt to you.",
        "Prevent all damage that would be dealt to creatures you control.",
        "Prevent all damage that would be dealt by this creature by creatures.",
    ] {
        assert!(!creature(text).understood(), "{text}");
    }
}

/// A few subtypes, as the store would have them.
pub(super) struct Names;
impl Subtypes for Names {
    fn intern(&self, name: &str) -> Option<u16> {
        [
            "Elf",
            "Goblin",
            "Aura",
            "Equipment",
            "Forest",
            "Wolf",
            "Human",
            "Soldier",
        ]
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u16)
    }
}

fn face<'a>(name: &'a str, types: &'a [CardType], text: &'a str) -> FaceText<'a> {
    FaceText {
        name,
        card_types: types,
        subtypes: &[],
        oracle_text: Some(text),
        mana_cost: "",
    }
}

fn creature(text: &str) -> Compiled {
    compile(&face("Test Beast", &[CardType::Creature], text), &Names)
}

fn instant(text: &str) -> Compiled {
    compile(&face("Test Spell", &[CardType::Instant], text), &Names)
}

#[track_caller]
fn ok(c: &Compiled) -> &[Ability] {
    assert!(c.understood(), "not understood: {:?}", c.unparsed);
    &c.abilities
}

#[test]
fn an_etb_draw_is_a_self_enter_trigger() {
    let c = creature("When this creature enters, draw a card.");
    let a = ok(&c);
    let AbilityKind::Triggered { trigger, effect } = &a[0].kind else {
        panic!("{:?}", a[0].kind);
    };
    assert_eq!(
        trigger.on,
        EventPattern::Enters {
            who: ObjectFilter::IsSelf
        }
    );
    assert_eq!(
        *effect,
        Effect::Draw {
            who: Selector::You,
            count: Value::ONE
        }
    );
    assert_eq!(
        a[0].source_text.as_deref(),
        Some("When this creature enters, draw a card.")
    );
}

#[test]
fn a_burn_spell_targets_anything_that_takes_damage() {
    let c = instant("Test Spell deals 3 damage to any target.");
    let a = ok(&c);
    let AbilityKind::SpellEffect(Effect::DealDamage { to, amount, .. }) = &a[0].kind else {
        panic!("{:?}", a[0].kind);
    };
    assert_eq!(*to, Selector::Target { index: 0 });
    assert_eq!(*amount, Value::Fixed(3));
    assert!(a[0].targets[0].allows_players);
}

#[test]
fn several_sentences_and_targets_in_order() {
    let c = instant("Destroy target creature. Its controller loses 2 life.");
    ok(&c);
    assert!(!instant("Destroy target creature. Its controller fatesealed.").understood());

    let c = instant("Tap target creature. Draw a card.");
    let a = ok(&c);
    assert!(matches!(&a[0].kind, AbilityKind::SpellEffect(Effect::Sequence(v)) if v.len() == 2));
    assert_eq!(a[0].targets.len(), 1);
}

#[test]
fn a_pump_until_end_of_turn() {
    let c = instant("Target creature gets +3/+3 and gains trample until end of turn.");
    let a = ok(&c);
    let AbilityKind::SpellEffect(Effect::Sequence(parts)) = &a[0].kind else {
        panic!("{:?}", a[0].kind);
    };
    assert_eq!(parts.len(), 2, "a P/T change and a keyword grant");
}

#[test]
fn anthems_and_aura_statics() {
    let c = compile(
        &face(
            "Anthem",
            &[CardType::Enchantment],
            "Creatures you control get +1/+1.",
        ),
        &Names,
    );
    assert!(matches!(
        &ok(&c)[0].kind,
        AbilityKind::Static {
            what: Selector::All { .. },
            modification: Modification::ModifyPowerToughness { .. },
            ..
        }
    ));

    let c = compile(
        &face(
            "Wings",
            &[CardType::Enchantment],
            "Enchant creature\nEnchanted creature gets +1/+1 and has flying.",
        ),
        &Names,
    );
    let a = ok(&c);
    assert!(matches!(a[0].kind, AbilityKind::Enchant));
    assert_eq!(a.len(), 3, "enchant, the pump, the flying grant");
}

#[test]
fn tribal_plurals_resolve_to_subtypes() {
    let c = creature("Other Elves you control get +1/+1.");
    ok(&c);
    let c = creature("Other Wolves you control get +1/+1.");
    ok(&c);
}

#[test]
fn equip_is_a_sorcery_speed_attach() {
    let c = compile(
        &face(
            "Blade",
            &[CardType::Artifact],
            "Equipped creature gets +2/+0.\nEquip {2}",
        ),
        &Names,
    );
    let a = ok(&c);
    let AbilityKind::Activated { timing, effect, .. } = &a[1].kind else {
        panic!();
    };
    assert_eq!(*timing, ActivationTiming::SorcerySpeed);
    assert!(matches!(effect, Effect::Attach { .. }));
}

#[test]
fn activated_abilities_with_costs() {
    let c = creature("{1}{R}: This creature gets +1/+0 until end of turn.");
    ok(&c);
    let c = creature("{T}, Sacrifice this creature: You gain 3 life.");
    let a = ok(&c);
    let AbilityKind::Activated { cost, .. } = &a[0].kind else {
        panic!();
    };
    assert_eq!(cost.additional.len(), 2);
    ok(&creature(
        "{X}: This creature gets +X/+0 until end of turn.",
    ));
    assert!(
        !creature("This creature gets +X/+0.").understood(),
        "X means nothing here"
    );
    ok(&creature("{2}, Discard a card: Draw a card."));
    assert!(!creature("{2}, Discard a card at random: Draw a card.").understood());
}

#[test]
fn enters_tapped_lands() {
    let c = compile(
        &FaceText {
            name: "Dual",
            card_types: &[CardType::Land],
            subtypes: &[],
            oracle_text: Some("This land enters tapped.\n{T}: Add {W} or {U}."),
            mana_cost: "",
        },
        &Names,
    );
    let a = ok(&c);
    assert!(matches!(
        &a[0].kind,
        AbilityKind::ReplacementEffect(Replacement {
            kind: ReplacementKind::EntersTapped,
            ..
        })
    ));
}

#[test]
fn unknown_words_reject_the_line_not_the_card() {
    let c = creature("Flying\nWhen this creature enters, fateseal 2.");
    assert!(!c.understood());
    assert_eq!(c.abilities.len(), 1, "flying still compiles");
    assert_eq!(
        c.unparsed,
        vec!["When this creature enters, fateseal 2.".to_string()]
    );
}

#[test]
fn a_spell_is_all_or_nothing() {
    let c = instant("Draw a card.\nFateseal 2.");
    assert!(!c.understood());
    assert!(c.abilities.is_empty(), "no half-spell");
}

#[test]
fn triggers_on_other_creatures_bind_the_subject() {
    let c =
        creature("Whenever another creature you control enters, it gains haste until end of turn.");
    let a = ok(&c);
    let AbilityKind::Triggered { effect, .. } = &a[0].kind else {
        panic!()
    };
    assert!(matches!(
        effect,
        Effect::Continuous {
            what: Selector::Bound(mtg_ir::selector::Binding::EventSubject),
            ..
        }
    ));
}

#[test]
fn player_targets_and_verbs() {
    ok(&instant("Target player draws two cards."));
    ok(&instant(
        "Target opponent loses 3 life and you gain 3 life.",
    ));
    ok(&instant("Each opponent discards a card."));
    ok(&instant("Counter target noncreature spell."));
    ok(&instant("Return target creature to its owner's hand."));
    ok(&instant("Destroy target artifact or enchantment."));
    ok(&instant("Exile target creature with power 3 or greater."));
    ok(&instant(
        "Return target creature card from your graveyard to your hand.",
    ));
    assert!(
        !instant("Counter target creature.").understood(),
        "not a spell"
    );
}

// ---- carried over from `derive` (ADR-011), which this module replaces -------------

fn land(subtypes: &[&str], text: Option<&str>) -> Compiled {
    let subtypes: Vec<String> = subtypes.iter().map(|s| s.to_string()).collect();
    compile(
        &FaceText {
            name: "Test Land",
            card_types: &[CardType::Land],
            subtypes: &subtypes,
            oracle_text: text,
            mana_cost: "",
        },
        &Names,
    )
}

fn keywords(c: &Compiled) -> Vec<Keyword> {
    c.abilities
        .iter()
        .filter_map(|a| match a.kind {
            AbilityKind::Keyword(k) => Some(k),
            _ => None,
        })
        .collect()
}

fn produces(a: &Ability) -> Vec<ManaOutput> {
    match &a.kind {
        AbilityKind::Activated {
            effect: Effect::AddMana { produces, .. },
            is_mana_ability: true,
            ..
        } => produces.clone(),
        other => panic!("not a mana ability: {other:?}"),
    }
}

#[test]
fn a_basic_land_type_taps_for_its_colour() {
    let c = land(&["Forest"], Some("({T}: Add {G}.)"));
    assert_eq!(ok(&c).len(), 1);
    assert_eq!(
        produces(&c.abilities[0]),
        vec![ManaOutput::Colored(Color::Green)]
    );
}

#[test]
fn a_dual_typed_land_taps_for_either() {
    assert_eq!(land(&["Forest", "Plains"], None).abilities.len(), 2);
}

#[test]
fn keyword_lines_with_reminder_text() {
    let c = creature("Deathtouch (Any amount of damage this deals to a creature is enough.)");
    assert_eq!(keywords(&c), vec![Keyword::Deathtouch]);
    let c = creature("Flying, vigilance");
    ok(&c);
    assert_eq!(keywords(&c), vec![Keyword::Flying, Keyword::Vigilance]);
}

#[test]
fn unenforced_or_parameterised_keywords_are_not_compiled() {
    ok(&creature("Flying, protection from red"));
    ok(&creature("Flying, ward {2}"));
    // Ward with a life payment is enforced too.
    ok(&creature("Ward—Pay 3 life."));
    let text = "Protection from the color of your choice";
    let c = creature(text);
    assert!(!c.understood(), "{text}");
    assert!(keywords(&c).is_empty(), "all or nothing per line: {text}");
}

#[test]
fn tap_for_mana_lines_become_mana_abilities() {
    let c = land(&[], Some("{T}: Add {C}{C}."));
    assert!(matches!(
        &produces(&ok(&c)[0])[0],
        ManaOutput::Repeated {
            amount: Value::Fixed(2),
            ..
        }
    ));
    let c = land(&[], Some("{T}: Add {R}, {G}, or {W}."));
    assert_eq!(
        produces(&ok(&c)[0]),
        vec![ManaOutput::AnyOf(vec![
            Color::Red,
            Color::Green,
            Color::White
        ])]
    );
    let c = land(&[], Some("{T}: Add one mana of any color."));
    assert!(matches!(&produces(&ok(&c)[0])[0], ManaOutput::AnyOf(c) if c.len() == 5));
}

#[test]
fn mana_lines_with_anything_more_are_left_alone() {
    for text in [
        "{T}: Add {G} for each Elf you control.",
        "{T}: Add one mana of any color a land an opponent controls could produce.",
    ] {
        let c = land(&[], Some(text));
        assert!(c.abilities.is_empty(), "{text}");
        assert!(!c.understood(), "{text}");
    }
}

#[test]
fn a_vanilla_card_is_understood() {
    assert!(land(&[], None).understood());
    assert!(creature("").understood());
}

#[test]
fn paid_and_mixed_color_mana_lines_are_understood() {
    for text in [
        "{T}, Pay 1 life: Add {B}.",
        "{1}, {T}: Add {C}{C}.",
        "{T}: Add {W}{U}.",
    ] {
        let c = land(&[], Some(text));
        assert!(c.understood(), "{text}");
        assert!(matches!(
            ok(&c)[0].kind,
            AbilityKind::Activated {
                is_mana_ability: true,
                ..
            }
        ));
    }
}

#[test]
fn loyalty_symbols_compile_to_signed_costs() {
    let c = compile(
        &face(
            "Test Walker",
            &[CardType::Planeswalker],
            "+2: Draw a card.\n−3: You gain 2 life.\n0: Draw a card.",
        ),
        &Names,
    );
    for (a, delta) in ok(&c).iter().zip([2, -3, 0]) {
        let AbilityKind::Activated {
            cost,
            is_loyalty_ability,
            is_mana_ability,
            timing,
            ..
        } = &a.kind
        else {
            panic!("activated")
        };
        assert!(*is_loyalty_ability);
        assert!(!*is_mana_ability);
        assert!(timing.sorcery_only());
        assert_eq!(cost.additional, vec![AdditionalCost::Loyalty { delta }]);
    }
}

#[test]
fn unsupported_loyalty_costs_or_effects_are_not_compiled() {
    for text in [
        "-X: Draw a card.",
        "+1: Invent a rule.",
        "1: Draw a card.",
        "+: Draw a card.",
        "-2147483648: Draw a card.",
    ] {
        let c = compile(
            &face("Test Walker", &[CardType::Planeswalker], text),
            &Names,
        );
        assert!(!c.understood(), "{text}");
        assert!(c.abilities.is_empty(), "{text}");
    }
}

#[test]
fn quoted_grants_compile_activated_and_triggered_abilities() {
    let attack = "Creatures you control have \"Whenever this creature attacks, draw a card.\"";
    assert!(creature(attack).understood());
    for text in [
        "Creatures you control have \"{T}: Add one mana of any color.\"",
        "Other creatures you control get +1/+1 and have \"{1}: This creature gets +1/+0 until end of turn.\"",
        "Creatures you control have haste and \"{T}: You gain 1 life.\"",
    ] {
        let c = creature(text);
        assert!(c.understood(), "{text}: {:?}", c.unparsed);
        assert!(c.abilities.iter().any(|a| matches!(
            &a.kind,
            AbilityKind::Static {
                modification: Modification::GrantAbility(g),
                ..
            } if matches!(g.kind, AbilityKind::Activated { .. })
        )));
    }
    for text in [
        "Creatures you control have \"When this creature dies, draw a card.\"",
        "Creatures you control have \"This creature can't block.\"",
        // The card's own name inside the quotes: "~" would mean the wrong object.
        "Creatures you control have \"{T}: Test Beast deals 1 damage to any target.\"",
    ] {
        assert!(!creature(text).understood(), "{text}");
    }
}
