//! Match counters to the opposing spell, including public counter protection and taxes.

use std::collections::BTreeMap;

use mtg_core::{AbilityId, CardType, ManaSymbol, ObjectId, PlayerId, Zone};
use mtg_engine::PlayerView;
use mtg_ir::effect::{ManaOutput, Modification, Restriction};
use mtg_ir::{
    AbilityKind, AdditionalCost, CardFace, Effect, EventPattern, ObjectFilter, Selector,
    TargetSpec, Value,
};

use super::Bot;

enum Counter {
    Hard,
    GenericTax(u32),
    UnknownTax,
}

pub(super) struct StackCard {
    counter: Option<(Counter, TargetSpec)>,
    uncounterable: bool,
    shields: Vec<ObjectFilter>,
    simple_mana: u32,
    effects: BTreeMap<Option<AbilityId>, Effect>,
    permanent: bool,
    life_payoff: bool,
}

impl StackCard {
    pub(super) fn new(face: &CardFace) -> Self {
        let mut out = Self {
            counter: None,
            uncounterable: false,
            shields: Vec::new(),
            simple_mana: 0,
            effects: BTreeMap::new(),
            permanent: !face
                .card_types
                .iter()
                .any(|kind| matches!(kind, CardType::Instant | CardType::Sorcery)),
            life_payoff: false,
        };
        for ability in &face.abilities {
            let effect = match &ability.kind {
                AbilityKind::SpellEffect(effect) => Some((None, effect)),
                AbilityKind::Activated { effect, .. } | AbilityKind::Triggered { effect, .. } => {
                    Some((Some(ability.id), effect))
                }
                _ => None,
            };
            if let Some((id, effect)) = effect {
                out.effects.insert(id, effect.clone());
                out.life_payoff |= life_sensitive_effect(effect);
            }
            if let AbilityKind::Triggered { trigger, .. } = &ability.kind {
                out.life_payoff |= life_gain_event(&trigger.on);
                out.life_payoff |= trigger.intervening_if.as_ref().is_some_and(life_condition);
            }
            if let AbilityKind::Static {
                condition: Some(condition),
                ..
            } = &ability.kind
            {
                out.life_payoff |= life_condition(condition);
            }
            if let AbilityKind::Activated { cost, .. } = &ability.kind {
                out.life_payoff |= cost
                    .additional
                    .iter()
                    .any(|cost| matches!(cost, AdditionalCost::PayLife { .. }));
            }
            let mut reads_life = false;
            let mut inspected = ability.clone();
            mtg_ir::walk::Visitor {
                filter: &mut |_| {},
                token: &mut |_| {},
                selector: &mut |_| {},
                value: &mut |value| {
                    reads_life |= matches!(
                        value,
                        Value::LifeTotal(_)
                            | Value::LeastLifeTotal(_)
                            | Value::GreatestLifeTotal(_)
                    );
                },
            }
            .ability(&mut inspected);
            out.life_payoff |= reads_life;
            match &ability.kind {
                AbilityKind::SpellEffect(effect) => {
                    if let [spec] = ability.targets.as_slice()
                        && spec.zone == Zone::Stack
                        && spec.mode.is_none()
                    {
                        let counter = match effect {
                            Effect::CounterSpell {
                                what: Selector::Target { index: 0 },
                                ..
                            } => Some(Counter::Hard),
                            Effect::CounterUnlessPays {
                                what: Selector::Target { index: 0 },
                                mana,
                                life,
                                discard,
                                times,
                                ..
                            } => {
                                let multiplier = match times {
                                    None => Some(1),
                                    Some(Value::Fixed(n)) => u32::try_from(*n).ok(),
                                    _ => None,
                                };
                                let generic = mana
                                    .symbols
                                    .iter()
                                    .all(|s| matches!(s, ManaSymbol::Generic(_)));
                                Some(
                                    if generic && life.is_none() && !discard && multiplier.is_some()
                                    {
                                        Counter::GenericTax(
                                            mana.mana_value()
                                                .saturating_mul(multiplier.unwrap_or(1)),
                                        )
                                    } else {
                                        Counter::UnknownTax
                                    },
                                )
                            }
                            _ => None,
                        };
                        if let Some(counter) = counter {
                            out.counter = Some((counter, spec.clone()));
                        }
                    }
                }
                AbilityKind::Static {
                    what,
                    modification: Modification::Restriction(Restriction::CantBeCountered),
                    condition: None,
                } => match what {
                    Selector::SelfSource => out.uncounterable = true,
                    Selector::All {
                        zone: Zone::Stack,
                        filter,
                    } => out.shields.push(filter.clone()),
                    _ => {}
                },
                AbilityKind::Activated {
                    cost,
                    effect:
                        Effect::AddMana {
                            who: Selector::You,
                            produces,
                        },
                    is_mana_ability: true,
                    functions_from: Zone::Battlefield,
                    ..
                } if cost.mana.symbols.is_empty()
                    && cost.timing.is_empty()
                    && matches!(
                        cost.additional.as_slice(),
                        [AdditionalCost::Tap {
                            what: Selector::SelfSource
                        }]
                    ) =>
                {
                    // Multiple tapping abilities are alternatives, not separate sources.
                    let mana = produces
                        .iter()
                        .map(simple_output)
                        .fold(0, u32::saturating_add);
                    out.simple_mana = out.simple_mana.max(mana);
                }
                _ => {}
            }
        }
        out
    }
}

fn simple_output(output: &ManaOutput) -> u32 {
    match output {
        ManaOutput::Colored(_) | ManaOutput::Colorless => 1,
        ManaOutput::AnyOf(colors) if !colors.is_empty() => 1,
        ManaOutput::Repeated {
            amount: Value::Fixed(n),
            output,
        } => u32::try_from(*n)
            .unwrap_or(0)
            .saturating_mul(simple_output(output)),
        _ => 0,
    }
}

impl Bot {
    fn stack_card(&self, view: &PlayerView, id: ObjectId) -> Option<&StackCard> {
        let object = view.visible.get(&id)?;
        self.stack_cards.get(&(object.card?, object.face))
    }

    /// A narrowly recognized low-impact effect may resolve so interaction remains
    /// available for a later threat. Unknown or mixed effects remain answerable.
    pub(super) fn low_impact_stack_object(&self, view: &PlayerView, id: ObjectId) -> bool {
        let Some(object) = view.visible.get(&id) else {
            return false;
        };
        let Some(card) = self.stack_card(view, id) else {
            return false;
        };
        if !object.is_ability && card.permanent {
            return false;
        }
        let Some(gain) = card
            .effects
            .get(&object.ability)
            .and_then(small_self_life_gain)
        else {
            return false;
        };
        if gain > 3 {
            return false;
        }
        if gain == 0 {
            return true;
        }
        let Some(player) = view.players.get(&object.controller) else {
            return false;
        };
        if player.life < 25 {
            return false;
        }
        // Life is a resource for some engines and alternate wins. Do not dismiss
        // life gain in the presence of a visible payoff or an unidentified card.
        if view
            .visible
            .values()
            .filter(|o| o.zone.zone == Zone::Battlefield)
            .any(|o| {
                self.stack_card(view, o.id)
                    .is_none_or(|card| card.life_payoff)
            })
        {
            return false;
        }
        // Even a few points can move an opponent out of our potential attack's
        // lethal range. This is conservative and does not assume blocks.
        let damage: i64 = view
            .visible
            .values()
            .filter(|o| {
                o.controller == view.viewer && o.zone.zone == Zone::Battlefield && !o.tapped
            })
            .filter_map(|o| self.stats(view, o.id))
            .filter(|s| s.creature)
            .map(|s| s.combat_damage())
            .sum();
        let life = i64::from(player.life);
        !(damage >= life && damage < life + i64::from(gain))
    }

    pub(super) fn visibly_uncounterable(&self, view: &PlayerView, spell: ObjectId) -> bool {
        if !view.visible.get(&spell).is_some_and(|o| o.is_ability)
            && self
                .stack_card(view, spell)
                .is_some_and(|card| card.uncounterable)
        {
            return true;
        }
        view.visible
            .values()
            .filter(|o| o.zone.zone == Zone::Battlefield)
            .any(|source| {
                self.stack_card(view, source.id).is_some_and(|card| {
                    card.shields.iter().any(|filter| {
                        self.known_filter_for(view, spell, filter, source.controller) == Some(true)
                    })
                })
            })
    }

    /// Lower is better. Unknown conditional counters remain a fallback; known
    /// payable generic taxes are held instead of being mistaken for a hard answer.
    pub(super) fn counter_reliability(
        &self,
        view: &PlayerView,
        source: ObjectId,
        face: u8,
        spell: ObjectId,
    ) -> Option<u8> {
        let object = view.visible.get(&source)?;
        let Some((counter, spec)) = object
            .card
            .and_then(|card| self.stack_cards.get(&(card, face)))
            .and_then(|card| card.counter.as_ref())
        else {
            return (!view.visible.get(&spell)?.is_ability).then_some(1);
        };
        let matches = self.known_filter(view, spell, &spec.filter);
        if matches == Some(false) || (matches.is_none() && view.visible.get(&spell)?.is_ability) {
            return None;
        }
        let unknown_filter = matches.is_none();
        let reliability = match counter {
            Counter::Hard => 0,
            Counter::GenericTax(tax) => {
                let controller = view.visible.get(&spell)?.controller;
                if self.obvious_generic_mana(view, controller) >= *tax {
                    return None;
                }
                // Hidden or conditional sources can still let the opponent pay.
                1
            }
            Counter::UnknownTax => 2,
        };
        Some(reliability.max(u8::from(unknown_filter)))
    }

    fn obvious_generic_mana(&self, view: &PlayerView, player: PlayerId) -> u32 {
        let floating: u32 = view
            .players
            .get(&player)
            .map_or(0, |p| p.mana.iter().map(|n| u32::from(*n)).sum());
        view.visible
            .values()
            .filter(|o| o.controller == player && o.zone.zone == Zone::Battlefield && !o.tapped)
            .filter_map(|o| {
                let card = self.stack_card(view, o.id)?;
                // The view does not expose summoning sickness. Count only
                // noncreature sources with a simple, unconditional tap ability.
                let creature = self.stats(view, o.id)?.creature;
                (!creature).then_some(card.simple_mana)
            })
            .fold(floating, u32::saturating_add)
    }
}

fn small_self_life_gain(effect: &Effect) -> Option<u32> {
    match effect {
        Effect::Nothing => Some(0),
        Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(n),
        } => u32::try_from(*n).ok(),
        Effect::Sequence(items) => items.iter().try_fold(0u32, |total, effect| {
            total.checked_add(small_self_life_gain(effect)?)
        }),
        Effect::FreezeValues { effect } => small_self_life_gain(effect),
        _ => None,
    }
}

fn life_sensitive_effect(effect: &Effect) -> bool {
    match effect {
        Effect::WinGame { .. } => true,
        Effect::Sequence(items) => items.iter().any(life_sensitive_effect),
        Effect::If {
            cond,
            then,
            otherwise,
        } => {
            life_condition(cond) || life_sensitive_effect(then) || life_sensitive_effect(otherwise)
        }
        Effect::May {
            then, otherwise, ..
        } => life_sensitive_effect(then) || otherwise.as_deref().is_some_and(life_sensitive_effect),
        Effect::MayPay { cost, then } => {
            cost.additional
                .iter()
                .any(|c| matches!(c, AdditionalCost::PayLife { .. }))
                || life_sensitive_effect(then)
        }
        Effect::Let { body, .. } => life_sensitive_effect(body),
        Effect::FreezeValues { effect } => life_sensitive_effect(effect),
        Effect::Modal { modes, .. } => modes
            .iter()
            .any(|(_, effect)| life_sensitive_effect(effect)),
        _ => false,
    }
}

fn life_condition(condition: &mtg_ir::trigger::Condition) -> bool {
    use mtg_ir::trigger::Condition;
    match condition {
        Condition::YouGainedLifeThisTurn
        | Condition::YouLostLifeThisTurn
        | Condition::OpponentLostLifeThisTurn => true,
        Condition::Not(inner) | Condition::WhenTriggered(inner) => life_condition(inner),
        Condition::And(conditions) | Condition::Or(conditions) => {
            conditions.iter().any(life_condition)
        }
        _ => false,
    }
}

fn life_gain_event(pattern: &EventPattern) -> bool {
    match pattern {
        EventPattern::LifeGained { .. } => true,
        EventPattern::AnyOf(patterns) => patterns.iter().any(life_gain_event),
        _ => false,
    }
}
