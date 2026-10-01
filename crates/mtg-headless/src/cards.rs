//! A tiny built-in card set, so the engine can be run before card import exists.
//!
//! **Every card here is invented.** The project ships no real card data — that is
//! imported from Scryfall into the local database at first run (see `mtg-oracle`) —
//! so this is a placeholder set that exercises the engine, not a card database.
//! It goes away once import lands.

use mtg_core::{
    AbilityId, CardId, CardType, Color, ManaCost, ManaSymbol, Subtype, Supertype, Zone,
};
use mtg_engine::layers::PrintedCards;
use mtg_ir::{
    Ability, AbilityKind, AdditionalCost, CardFace, Cost, Effect, ObjectFilter, Selector, Value,
    ability::{ActivationTiming, Keyword},
    effect::ManaOutput,
    trigger::{EventPattern, Trigger, TriggerTiming},
};

pub const PLAINS: CardId = CardId(0);
pub const DUMMY: CardId = CardId(1);
pub const SENTRY: CardId = CardId(2);
pub const SERGEANT: CardId = CardId(3);
pub const ADDER: CardId = CardId(4);
pub const COLOSSUS: CardId = CardId(5);
/// Produces white *or* green — the case that makes affordability a matching problem.
pub const DUAL: CardId = CardId(6);
/// "Whenever this attacks, you gain 1 life." — so triggers show up in a demo run.
pub const BANNER: CardId = CardId(7);

pub struct DemoCards {
    faces: Vec<CardFace>,
}

impl Default for DemoCards {
    fn default() -> Self {
        Self {
            faces: vec![
                land("Training Field", ManaOutput::Colored(Color::White)),
                creature("Practice Dummy", 2, 2, mc(1, &[Color::White]), &[]),
                creature("Sentry Post", 1, 3, mc(0, &[Color::White]), &[]),
                creature(
                    "Drill Sergeant",
                    1,
                    1,
                    mc(1, &[Color::White]),
                    &[Keyword::FirstStrike],
                ),
                creature(
                    "Rust Adder",
                    1,
                    1,
                    mc(0, &[Color::Green]),
                    &[Keyword::Deathtouch],
                ),
                creature(
                    "Siege Colossus",
                    5,
                    5,
                    mc(4, &[Color::White]),
                    &[Keyword::Trample],
                ),
                land(
                    "Verdant Well",
                    ManaOutput::AnyOf(vec![Color::White, Color::Green]),
                ),
                attack_trigger_creature("Rally Banner", 2, 2, mc(1, &[Color::White])),
            ],
        }
    }
}

impl DemoCards {
    pub fn name(&self, card: CardId) -> &str {
        self.faces.get(card.0 as usize).map_or("?", |f| &f.name)
    }
}

impl PrintedCards for DemoCards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.faces.get(card.0 as usize)
    }
    fn subtype_name(&self, _subtype: Subtype) -> Option<&str> {
        None
    }
}

fn creature(
    name: &str,
    power: i32,
    toughness: i32,
    mana_cost: ManaCost,
    kws: &[Keyword],
) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost,
        card_types: vec![CardType::Creature],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        power: Some(power),
        toughness: Some(toughness),
        loyalty: None,
        abilities: kws
            .iter()
            .enumerate()
            .map(|(i, k)| Ability {
                id: AbilityId(i as u16),
                kind: AbilityKind::Keyword(*k),
                targets: Vec::new(),
                source_text: None,
            })
            .collect(),
        oracle_text: None,
        colors: None,
    }
}

fn land(name: &str, output: ManaOutput) -> CardFace {
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: vec![CardType::Land],
        subtypes: Vec::new(),
        supertypes: vec![Supertype::Basic],
        power: None,
        toughness: None,
        loyalty: None,
        abilities: vec![mana_ability(0, output)],
        oracle_text: None,
        colors: None,
    }
}

/// A creature with "whenever this attacks, you gain 1 life".
fn attack_trigger_creature(
    name: &str,
    power: i32,
    toughness: i32,
    mana_cost: ManaCost,
) -> CardFace {
    let mut c = creature(name, power, toughness, mana_cost, &[]);
    c.abilities = vec![Ability {
        id: AbilityId(0),
        kind: AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Attacks {
                    who: ObjectFilter::IsSelf,
                },
                functions_from: Zone::Battlefield,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::GainLife {
                who: Selector::You,
                amount: Value::ONE,
            },
        },
        targets: Vec::new(),
        source_text: None,
    }];
    c
}

/// A land's `{T}: add one mana` ability.
fn mana_ability(id: u16, output: ManaOutput) -> Ability {
    Ability {
        id: AbilityId(id),
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
            // CR 605: a mana ability uses no stack and cannot be responded to.
            is_mana_ability: true,
            is_loyalty_ability: false,
            timing: ActivationTiming::Instant,
        },
        targets: Vec::new(),
        source_text: None,
    }
}

/// A mana cost from a compact spec: `mc(2, &[Color::White])` is `{2}{W}`.
fn mc(generic: u8, colors: &[Color]) -> ManaCost {
    let mut symbols = Vec::new();
    if generic > 0 {
        symbols.push(ManaSymbol::Generic(generic));
    }
    symbols.extend(colors.iter().map(|c| ManaSymbol::Colored(*c)));
    ManaCost { symbols }
}
