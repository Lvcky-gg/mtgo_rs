//! Shared test scaffolding: an in-memory card database and a driver loop.
//!
//! Every card here is invented. The real project ships no card data at all (it is
//! imported from Scryfall at first run), so tests cannot depend on any particular
//! card existing.
//!
//! Each test binary uses a different subset of these fixtures, hence the blanket
//! `dead_code` allowance: this module is compiled once per test file.

#![allow(dead_code)]

use mtg_core::{CardId, CardType, Color, ManaCost, ManaSymbol, PlayerId, Subtype, Supertype, Zone};
use mtg_engine::{
    Choice, Engine, Progress,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
};
use mtg_ir::{
    Ability, AbilityKind, AdditionalCost, CardFace, Cost, Effect, ObjectFilter, Selector, Value,
    ability::{ActivationTiming, Keyword},
    effect::ManaOutput,
    selector::{Binding, TargetSpec},
    trigger::{Condition, EventPattern, Trigger, TriggerLimit, TriggerTiming},
};

pub const DUMMY: CardId = CardId(0);
pub const FIELD: CardId = CardId(1);
pub const FIRST_STRIKER: CardId = CardId(2);
pub const DEATHTOUCHER: CardId = CardId(3);
pub const BIG: CardId = CardId(4);
pub const LEGEND: CardId = CardId(5);
/// Asymmetric power/toughness, so layer-ordering tests can tell orders apart.
pub const SENTRY: CardId = CardId(6);
/// A land that taps for white.
pub const WHITE_SOURCE: CardId = CardId(7);
/// A land that taps for white *or* green.
pub const DUAL_SOURCE: CardId = CardId(8);
/// Costs `{1}{W}`, so casting it needs two lands.
pub const COSTED: CardId = CardId(9);
/// Costs `{W}{G}`, so it needs two differently-coloured sources.
pub const TWO_COLOR: CardId = CardId(10);
pub const FLYER: CardId = CardId(11);
pub const REACHER: CardId = CardId(12);
pub const MENACER: CardId = CardId(13);
pub const VIGILANT: CardId = CardId(14);
pub const WALL: CardId = CardId(15);
pub const TRAMPLER: CardId = CardId(16);
pub const DOUBLE: CardId = CardId(17);
/// Two instants that gain different amounts of life, so resolution *order* is
/// observable in the log rather than only the final total.
pub const SMALL_GAIN: CardId = CardId(18);
pub const BIG_GAIN: CardId = CardId(19);

// --- cards with triggered abilities ---
/// "Whenever this attacks, you gain 1 life."
pub const ON_ATTACK: CardId = CardId(20);
/// "When this dies, you gain 2 life."
pub const ON_DEATH: CardId = CardId(21);
/// "When this enters, you gain 3 life."
pub const ON_ENTER: CardId = CardId(22);
/// "At the beginning of your upkeep, you gain 1 life."
pub const ON_UPKEEP: CardId = CardId(23);
/// "Whenever a creature you control enters, you gain 1 life. Only once each turn."
pub const ONCE_PER_TURN: CardId = CardId(24);
/// "At the beginning of your upkeep, if you control three or more creatures, gain 5 life."
pub const GATED: CardId = CardId(25);
/// "Whenever a creature you control enters, tap it." — exercises `Binding::EventSubject`.
pub const TAP_THE_NEWCOMER: CardId = CardId(26);
/// "Whenever a creature an opponent controls enters, you gain 1 life."
pub const ON_OPPONENT_ENTER: CardId = CardId(27);

// --- cards whose resolution asks a question ---
/// "When this enters, you may gain 3 life."
pub const MAY_GAIN: CardId = CardId(28);
/// "When this enters, choose one — gain 2 life; or gain 7 life."
pub const MODAL: CardId = CardId(29);
/// "When this enters, discard a card."
pub const FORCED_DISCARD: CardId = CardId(30);
/// "When this enters, discard a card at random." — must not prompt.
pub const RANDOM_DISCARD: CardId = CardId(31);
/// "When this enters, sacrifice a creature."
pub const SAC_ETB: CardId = CardId(32);
/// "When this enters, you gain X life, where you choose X between 0 and 3."
pub const CHOSEN_X: CardId = CardId(33);
/// "When this enters, you gain 1 life, then you may gain 10 life."
///
/// The regression fixture for restartable resolution: if the re-run after the
/// question double-applied the first clause, the life total would be one too high.
pub const SEQ_MAY: CardId = CardId(34);

// --- cards that target ---
/// "Deal 2 damage to target creature."
pub const BOLT: CardId = CardId(35);
/// "Deal 2 damage to target creature. You gain 2 life."
///
/// Fixture for CR 608.2b: if the creature is gone the *whole* spell fails, so no life
/// is gained either.
pub const BOLT_AND_GAIN: CardId = CardId(36);
/// "Target player gains 3 life."
pub const HEAL_PLAYER: CardId = CardId(37);
/// "Deal 1 damage to each of two target creatures." Slots must be distinct.
pub const TWO_BOLTS: CardId = CardId(38);
pub const HEXPROOF_CREATURE: CardId = CardId(39);
pub const SHROUD_CREATURE: CardId = CardId(40);
/// "Counter target spell."
pub const COUNTER: CardId = CardId(41);
/// "When this enters, deal 2 damage to target creature."
pub const ETB_BOLT: CardId = CardId(42);
/// "When this enters, deal 2 damage to target creature an opponent controls."
///
/// Needed to test CR 603.3d: a trigger that targets *any* creature always has at least
/// one legal choice — the creature whose arrival triggered it — so "no legal target" is
/// only reachable with a filter that excludes your own side.
pub const ETB_BOLT_OPPONENT: CardId = CardId(43);

// --- state triggers (CR 603.8) ---
/// "When you have 10 or less life, you gain 5 life." Self-correcting: one firing takes
/// the condition back to false.
pub const LOW_LIFE_BIG: CardId = CardId(44);
/// "When you have 10 or less life, you gain 1 life." Has to fire repeatedly to clear the
/// condition, which is how "does it re-trigger, and does it terminate?" gets tested.
pub const LOW_LIFE_SMALL: CardId = CardId(45);
/// A 3/3 with lifelink.
pub const LIFELINKER: CardId = CardId(46);

pub const AURA_SUBTYPE: Subtype = Subtype(100);

/// Every card constant paired with the name it must resolve to.
///
/// The fixture set is a positional `Vec` addressed by separate `const` indices, which
/// means inserting a card anywhere but the end silently renumbers everything after it —
/// and the symptom is a dozen unrelated tests failing at once, nowhere near the cause.
/// `tests/fixtures.rs` asserts this table, so a misplaced insertion fails loudly in one
/// obvious place instead.
pub const CARD_NAMES: &[(CardId, &str)] = &[
    (DUMMY, "Practice Dummy"),
    (FIELD, "Training Field"),
    (FIRST_STRIKER, "Drill Sergeant"),
    (DEATHTOUCHER, "Rust Adder"),
    (BIG, "Siege Colossus"),
    (LEGEND, "Captain Vell"),
    (SENTRY, "Sentry Post"),
    (WHITE_SOURCE, "White Well"),
    (DUAL_SOURCE, "Mixed Well"),
    (COSTED, "Field Recruit"),
    (TWO_COLOR, "Border Envoy"),
    (FLYER, "Cloud Skiff"),
    (REACHER, "Web Spinner"),
    (MENACER, "Gate Crasher"),
    (VIGILANT, "Watch Captain"),
    (WALL, "Stone Rampart"),
    (TRAMPLER, "Rubble Giant"),
    (DOUBLE, "Twin Blade"),
    (SMALL_GAIN, "Minor Respite"),
    (BIG_GAIN, "Major Respite"),
    (ON_ATTACK, "Rally Banner"),
    (ON_DEATH, "Ash Warden"),
    (ON_ENTER, "Hearth Keeper"),
    (ON_UPKEEP, "Dawn Bell"),
    (ONCE_PER_TURN, "Single Chime"),
    (GATED, "Vigil Horn"),
    (TAP_THE_NEWCOMER, "Binding Sigil"),
    (ON_OPPONENT_ENTER, "Watchful Cairn"),
    (MAY_GAIN, "Kindly Lantern"),
    (MODAL, "Split Decision"),
    (FORCED_DISCARD, "Toll Collector"),
    (RANDOM_DISCARD, "Blind Toll"),
    (SAC_ETB, "Grim Bargain"),
    (CHOSEN_X, "Measured Gift"),
    (SEQ_MAY, "Twofold Boon"),
    (BOLT, "Spark Jolt"),
    (BOLT_AND_GAIN, "Jolt and Mend"),
    (HEAL_PLAYER, "Kind Word"),
    (TWO_BOLTS, "Twin Sparks"),
    (HEXPROOF_CREATURE, "Veiled Scout"),
    (SHROUD_CREATURE, "Silent Monolith"),
    (COUNTER, "Deny"),
    (ETB_BOLT, "Hearth Sentinel"),
    (ETB_BOLT_OPPONENT, "Border Sentinel"),
    (LOW_LIFE_BIG, "Last Bulwark"),
    (LOW_LIFE_SMALL, "Slow Mend"),
    (LIFELINKER, "Mercy Knight"),
];

pub struct TestCards {
    faces: Vec<CardFace>,
}

impl Default for TestCards {
    fn default() -> Self {
        Self {
            faces: vec![
                creature("Practice Dummy", 2, 2, &[]),
                land("Training Field"),
                creature("Drill Sergeant", 1, 1, &[Keyword::FirstStrike]),
                creature("Rust Adder", 1, 1, &[Keyword::Deathtouch]),
                creature("Siege Colossus", 6, 6, &[]),
                legendary_creature("Captain Vell", 3, 3),
                creature("Sentry Post", 1, 3, &[]),
                mana_land("White Well", ManaOutput::Colored(Color::White)),
                mana_land(
                    "Mixed Well",
                    ManaOutput::AnyOf(vec![Color::White, Color::Green]),
                ),
                costed_creature("Field Recruit", 2, 2, mc(1, &[Color::White])),
                costed_creature("Border Envoy", 2, 2, mc(0, &[Color::White, Color::Green])),
                creature("Cloud Skiff", 2, 2, &[Keyword::Flying]),
                creature("Web Spinner", 1, 3, &[Keyword::Reach]),
                creature("Gate Crasher", 2, 2, &[Keyword::Menace]),
                creature("Watch Captain", 2, 2, &[Keyword::Vigilance]),
                defender_creature("Stone Rampart", 0, 4),
                creature("Rubble Giant", 4, 4, &[Keyword::Trample]),
                creature("Twin Blade", 2, 2, &[Keyword::DoubleStrike]),
                instant("Minor Respite", mc(0, &[Color::White]), 1),
                instant("Major Respite", mc(0, &[Color::White]), 10),
                triggered(
                    "Rally Banner",
                    2,
                    2,
                    on(EventPattern::Attacks {
                        who: ObjectFilter::IsSelf,
                    }),
                    gain(1),
                ),
                triggered(
                    "Ash Warden",
                    2,
                    2,
                    on(EventPattern::Dies {
                        who: ObjectFilter::IsSelf,
                    }),
                    gain(2),
                ),
                triggered(
                    "Hearth Keeper",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    gain(3),
                ),
                triggered(
                    "Dawn Bell",
                    0,
                    3,
                    on(EventPattern::StepBegins {
                        step: mtg_core::Step::Upkeep,
                        whose: Selector::You,
                    }),
                    gain(1),
                ),
                triggered(
                    "Single Chime",
                    1,
                    1,
                    Trigger {
                        limit: Some(TriggerLimit::OncePerTurn),
                        ..on(EventPattern::Enters {
                            who: ObjectFilter::And(vec![
                                ObjectFilter::HasType(CardType::Creature),
                                ObjectFilter::ControlledBy(Box::new(Selector::You)),
                            ]),
                        })
                    },
                    gain(1),
                ),
                triggered(
                    "Vigil Horn",
                    0,
                    2,
                    Trigger {
                        intervening_if: Some(Condition::CountAtLeast {
                            what: Selector::All {
                                zone: Zone::Battlefield,
                                filter: ObjectFilter::And(vec![
                                    ObjectFilter::HasType(CardType::Creature),
                                    ObjectFilter::ControlledBy(Box::new(Selector::You)),
                                ]),
                            },
                            at_least: Value::Fixed(3),
                        }),
                        ..on(EventPattern::StepBegins {
                            step: mtg_core::Step::Upkeep,
                            whose: Selector::You,
                        })
                    },
                    gain(5),
                ),
                triggered(
                    "Binding Sigil",
                    0,
                    2,
                    on(EventPattern::Enters {
                        who: ObjectFilter::And(vec![
                            ObjectFilter::HasType(CardType::Creature),
                            ObjectFilter::ControlledBy(Box::new(Selector::You)),
                        ]),
                    }),
                    // "Tap it" — the newcomer, not the source.
                    Effect::Tap {
                        what: Selector::Bound(Binding::EventSubject),
                    },
                ),
                triggered(
                    "Watchful Cairn",
                    0,
                    2,
                    on(EventPattern::Enters {
                        who: ObjectFilter::And(vec![
                            ObjectFilter::HasType(CardType::Creature),
                            ObjectFilter::ControlledBy(Box::new(Selector::Opponents)),
                        ]),
                    }),
                    gain(1),
                ),
                triggered(
                    "Kindly Lantern",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::May {
                        prompt: "gain 3 life?".into(),
                        then: Box::new(gain(3)),
                        otherwise: None,
                    },
                ),
                triggered(
                    "Split Decision",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::Modal {
                        at_least: None,
                        choose: Value::ONE,
                        modes: vec![
                            ("gain 2 life".into(), gain(2)),
                            ("gain 7 life".into(), gain(7)),
                        ],
                    },
                ),
                triggered(
                    "Toll Collector",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::Discard {
                        who: Selector::You,
                        count: Value::ONE,
                        at_random: false,
                    },
                ),
                triggered(
                    "Blind Toll",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::Discard {
                        who: Selector::You,
                        count: Value::ONE,
                        at_random: true,
                    },
                ),
                triggered(
                    "Grim Bargain",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::Sacrifice {
                        who: Selector::You,
                        what: Selector::All {
                            zone: Zone::Battlefield,
                            filter: ObjectFilter::And(vec![
                                ObjectFilter::HasType(CardType::Creature),
                                ObjectFilter::ControlledBy(Box::new(Selector::You)),
                            ]),
                        },
                    },
                ),
                triggered(
                    "Measured Gift",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::GainLife {
                        who: Selector::You,
                        amount: Value::ChosenByController {
                            min: Box::new(Value::ZERO),
                            max: Box::new(Value::Fixed(3)),
                        },
                    },
                ),
                triggered(
                    "Twofold Boon",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    Effect::Sequence(vec![
                        gain(1),
                        Effect::May {
                            prompt: "gain 10 more?".into(),
                            then: Box::new(gain(10)),
                            otherwise: None,
                        },
                    ]),
                ),
                targeted_instant(
                    "Spark Jolt",
                    vec![creature_target()],
                    Effect::DealDamage {
                        source: Selector::SelfSource,
                        to: Selector::Target { index: 0 },
                        amount: Value::Fixed(2),
                    },
                ),
                targeted_instant(
                    "Jolt and Mend",
                    vec![creature_target()],
                    Effect::Sequence(vec![
                        Effect::DealDamage {
                            source: Selector::SelfSource,
                            to: Selector::Target { index: 0 },
                            amount: Value::Fixed(2),
                        },
                        gain(2),
                    ]),
                ),
                targeted_instant(
                    "Kind Word",
                    vec![TargetSpec {
                        zone: Zone::Battlefield,
                        filter: ObjectFilter::Any,
                        allows_players: true,
                        players: None,
                        mode: None,
                        count: Value::ONE,
                        up_to: false,
                        distinct_from_other_targets: false,
                    }],
                    Effect::GainLife {
                        who: Selector::Target { index: 0 },
                        amount: Value::Fixed(3),
                    },
                ),
                targeted_instant(
                    "Twin Sparks",
                    vec![distinct_creature_target(), distinct_creature_target()],
                    Effect::Sequence(vec![
                        Effect::DealDamage {
                            source: Selector::SelfSource,
                            to: Selector::Target { index: 0 },
                            amount: Value::ONE,
                        },
                        Effect::DealDamage {
                            source: Selector::SelfSource,
                            to: Selector::Target { index: 1 },
                            amount: Value::ONE,
                        },
                    ]),
                ),
                creature("Veiled Scout", 2, 2, &[Keyword::Hexproof]),
                creature("Silent Monolith", 2, 2, &[Keyword::Shroud]),
                targeted_instant(
                    "Deny",
                    vec![TargetSpec {
                        zone: Zone::Stack,
                        filter: ObjectFilter::IsSpell,
                        allows_players: false,
                        players: None,
                        mode: None,
                        count: Value::ONE,
                        up_to: false,
                        distinct_from_other_targets: false,
                    }],
                    Effect::CounterSpell {
                        what: Selector::Target { index: 0 },
                        exile: false,
                    },
                ),
                triggered_targeting(
                    "Hearth Sentinel",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    vec![creature_target()],
                    Effect::DealDamage {
                        source: Selector::SelfSource,
                        to: Selector::Target { index: 0 },
                        amount: Value::Fixed(2),
                    },
                ),
                triggered_targeting(
                    "Border Sentinel",
                    1,
                    1,
                    on(EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    }),
                    vec![TargetSpec {
                        zone: Zone::Battlefield,
                        filter: ObjectFilter::And(vec![
                            ObjectFilter::HasType(CardType::Creature),
                            ObjectFilter::ControlledBy(Box::new(Selector::Opponents)),
                        ]),
                        allows_players: false,
                        players: None,
                        mode: None,
                        count: Value::ONE,
                        up_to: false,
                        distinct_from_other_targets: false,
                    }],
                    Effect::DealDamage {
                        source: Selector::SelfSource,
                        to: Selector::Target { index: 0 },
                        amount: Value::Fixed(2),
                    },
                ),
                state_triggered("Last Bulwark", 0, 3, low_life(), gain(5)),
                state_triggered("Slow Mend", 0, 3, low_life(), gain(1)),
                creature("Mercy Knight", 3, 3, &[Keyword::Lifelink]),
            ],
        }
    }
}

impl PrintedCards for TestCards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.faces.get(card.0 as usize)
    }
    fn subtype_name(&self, subtype: Subtype) -> Option<&str> {
        (subtype == AURA_SUBTYPE).then_some("Aura")
    }
}

fn keyword_abilities(kws: &[Keyword]) -> Vec<Ability> {
    kws.iter()
        .enumerate()
        .map(|(i, k)| Ability {
            id: mtg_core::AbilityId(i as u16),
            kind: AbilityKind::Keyword(*k),
            targets: Vec::new(),
            source_text: None,
        })
        .collect()
}

pub fn creature(name: &str, power: i32, toughness: i32, kws: &[Keyword]) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Creature],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: Some(power),
        toughness: Some(toughness),
        loyalty: None,
        abilities: keyword_abilities(kws),
        oracle_text: None,
        colors: None,
    }
}

fn defender_creature(name: &str, power: i32, toughness: i32) -> CardFace {
    creature(name, power, toughness, &[Keyword::Defender])
}

fn legendary_creature(name: &str, power: i32, toughness: i32) -> CardFace {
    let mut c = creature(name, power, toughness, &[]);
    c.supertypes = vec![Supertype::Legendary];
    c
}

/// "Target creature" — one creature on the battlefield.
pub fn creature_target() -> TargetSpec {
    TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(CardType::Creature),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    }
}

/// Like [`creature_target`], but the slot may not repeat another slot's choice.
pub fn distinct_creature_target() -> TargetSpec {
    TargetSpec {
        distinct_from_other_targets: true,
        ..creature_target()
    }
}

/// An instant with target slots.
pub fn targeted_instant(name: &str, targets: Vec<TargetSpec>, effect: Effect) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Instant],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: None,
        toughness: None,
        loyalty: None,
        abilities: vec![Ability {
            id: mtg_core::AbilityId(0),
            kind: AbilityKind::SpellEffect(effect),
            targets,
            source_text: None,
        }],
        oracle_text: None,
        colors: None,
    }
}

/// A plain "whenever" trigger that functions from the battlefield.
pub fn on(pattern: EventPattern) -> Trigger {
    Trigger {
        on: pattern,
        functions_from: Zone::Battlefield,
        intervening_if: None,
        optional: false,
        limit: None,
        timing: TriggerTiming::Normal,
    }
}

/// "You gain N life."
pub fn gain(n: i32) -> Effect {
    Effect::GainLife {
        who: Selector::You,
        amount: Value::Fixed(n),
    }
}

/// "You have 10 or less life."
///
/// Spelled as "not (life >= 11)" because the condition vocabulary has `ValueAtLeast`
/// rather than a less-than form.
pub fn low_life() -> Condition {
    Condition::Not(Box::new(Condition::ValueAtLeast {
        lhs: Value::LifeTotal(Box::new(Selector::You)),
        rhs: Value::Fixed(11),
    }))
}

/// A creature with a state-triggered ability (CR 603.8): it watches a condition rather
/// than an event, and fires as soon as the condition holds.
pub fn state_triggered(
    name: &str,
    power: i32,
    toughness: i32,
    cond: Condition,
    effect: Effect,
) -> CardFace {
    let trigger = Trigger {
        on: EventPattern::StateIs(cond),
        functions_from: Zone::Battlefield,
        intervening_if: None,
        optional: false,
        limit: None,
        timing: TriggerTiming::StateTrigger,
    };
    triggered(name, power, toughness, trigger, effect)
}

/// A creature whose triggered ability chooses targets as it goes on the stack
/// (CR 603.3d).
pub fn triggered_targeting(
    name: &str,
    power: i32,
    toughness: i32,
    trigger: Trigger,
    targets: Vec<TargetSpec>,
    effect: Effect,
) -> CardFace {
    let mut c = triggered(name, power, toughness, trigger, effect);
    c.abilities[0].targets = targets;
    c
}

/// A creature whose only ability is one triggered ability.
pub fn triggered(
    name: &str,
    power: i32,
    toughness: i32,
    trigger: Trigger,
    effect: Effect,
) -> CardFace {
    let mut c = creature(name, power, toughness, &[]);
    c.abilities = vec![Ability {
        id: mtg_core::AbilityId(0),
        kind: AbilityKind::Triggered { trigger, effect },
        targets: Vec::new(),
        source_text: None,
    }];
    c
}

/// An instant that gains its controller `life` life.
pub fn instant(name: &str, mana_cost: ManaCost, life: i32) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost,
        card_types: vec![CardType::Instant],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: None,
        toughness: None,
        loyalty: None,
        abilities: vec![Ability {
            id: mtg_core::AbilityId(0),
            kind: AbilityKind::SpellEffect(Effect::GainLife {
                who: Selector::You,
                amount: mtg_ir::Value::Fixed(life),
            }),
            targets: Vec::new(),
            source_text: None,
        }],
        oracle_text: None,
        colors: None,
    }
}

/// A creature with a real mana cost.
pub fn costed_creature(name: &str, power: i32, toughness: i32, mana_cost: ManaCost) -> CardFace {
    let mut c = creature(name, power, toughness, &[]);
    c.mana_cost = mana_cost;
    c
}

/// A land with a `{T}`: add mana ability.
pub fn mana_land(name: &str, output: ManaOutput) -> CardFace {
    let mut l = land(name);
    l.abilities = vec![Ability {
        id: mtg_core::AbilityId(0),
        kind: AbilityKind::Activated {
            cost: Cost {
                additional: vec![AdditionalCost::Tap {
                    what: Selector::SelfSource,
                }],
                ..Cost::free()
            },
            effect: Effect::AddMana {
                who: Selector::You,
                produces: vec![output],
            },
            functions_from: Zone::Battlefield,
            is_mana_ability: true,
            is_loyalty_ability: false,
            timing: ActivationTiming::Instant,
        },
        targets: Vec::new(),
        source_text: None,
    }];
    l
}

/// A mana cost from a compact spec: `mc(1, &[Color::White])` is `{1}{W}`.
pub fn mc(generic: u8, colors: &[Color]) -> ManaCost {
    let mut symbols = Vec::new();
    if generic > 0 {
        symbols.push(ManaSymbol::Generic(generic));
    }
    symbols.extend(colors.iter().map(|c| ManaSymbol::Colored(*c)));
    ManaCost { symbols }
}

fn land(name: &str) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Land],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: None,
        toughness: None,
        loyalty: None,
        abilities: Vec::new(),
        oracle_text: None,
        colors: None,
    }
}

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

/// Drive the engine, passing priority automatically, until `stop` says to halt or
/// the budget runs out.
///
/// Any choice that is *not* a priority prompt is collected and returned. That is
/// what most tests assert on: the interesting question is usually "was the player
/// interrupted, and with what?", and an empty result is a meaningful answer.
pub fn run(
    engine: &mut Engine,
    cards: &TestCards,
    budget: usize,
    mut stop: impl FnMut(&Engine) -> bool,
) -> Vec<Choice> {
    let mut interruptions = Vec::new();

    for _ in 0..budget {
        if stop(engine) {
            return interruptions;
        }
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { .. } => return interruptions,
            Progress::NeedsChoice(c) => match &c.kind {
                ChoiceKind::Priority { .. } => {
                    engine
                        .answer(cards, c.id, Answer::Pass)
                        .expect("pass is always legal");
                }
                // Anything else is an interruption worth recording. Answered with
                // the engine's own default so the game can keep moving.
                _ => {
                    interruptions.push(c.clone());
                    let fallback = default_answer(&c);
                    engine
                        .answer(cards, c.id, fallback)
                        .expect("fallback answer");
                }
            },
        }
    }
    interruptions
}

fn default_answer(c: &Choice) -> Answer {
    if let Some(d) = &c.default {
        return d.clone();
    }
    match &c.kind {
        ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order(triggers.clone()),
        ChoiceKind::ChooseObjects { from, .. } => {
            Answer::Objects(from.first().copied().into_iter().collect())
        }
        ChoiceKind::KeepOneLegend { candidates, .. } => {
            Answer::Objects(candidates.first().copied().into_iter().collect())
        }
        _ => Answer::Pass,
    }
}
