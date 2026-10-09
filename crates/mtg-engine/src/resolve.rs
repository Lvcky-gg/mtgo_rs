//! Resolving effect trees.
//!
//! Walks an [`mtg_ir::Effect`] and applies it. Each primitive evaluates its
//! selectors *at the moment it runs* and applies its events immediately, rather
//! than the whole tree being evaluated up front and applied at the end. That is
//! not an implementation convenience — it is what the rules require. In "destroy
//! target creature, then draw a card for each creature that died this turn", the
//! second clause must see the result of the first.
//!
//! # What is here, and what is not
//!
//! The choice-free primitives are implemented. Anything needing a player decision
//! mid-resolution — a modal choice not fixed at announcement, "choose a card in
//! your hand", an announced number — returns [`ResolveError::NeedsChoice`] rather
//! than guessing. Gathering those is the resolver-driver's job in [`crate::Engine`]
//! and is the next piece of work; the split is documented here so the boundary is
//! visible rather than implied by a silent `_ => {}`.

use std::collections::BTreeMap;

#[cfg(test)]
mod stack_departure_tests {
    use super::*;
    use crate::state::CastContext;

    #[test]
    fn exile_on_leave_replaces_hand_library_and_graveyard_destinations() {
        for destination in [Zone::Hand, Zone::Library, Zone::Graveyard] {
            for exile_on_leave in [false, true] {
                let player = PlayerId(0);
                let mut state = GameState::new(&[player, PlayerId(1)], 20);
                let object = state.place(mtg_core::CardId(0), player, ZoneRef::shared(Zone::Stack));
                state.objects.get_mut(&object).unwrap().cast_context = Some(CastContext {
                    exile_on_leave,
                    ..Default::default()
                });
                let mut log = Vec::new();
                let moved = move_to_at(
                    &mut state,
                    &mut log,
                    object,
                    destination,
                    Some(0),
                    Cause::Resolution(object),
                )
                .unwrap();
                assert_eq!(
                    state.objects[&moved].zone.zone,
                    if exile_on_leave {
                        Zone::Exile
                    } else {
                        destination
                    }
                );
                assert!(
                    matches!(log.last().unwrap().event, Event::ZoneChange { to, index, .. }
                    if to.zone == if exile_on_leave { Zone::Exile } else { destination }
                    && index == if exile_on_leave { None } else { Some(0) })
                );
            }
        }
    }
}

use mtg_core::{
    AbilityId, Cause, Event, ObjectId, PlayerId, StampedEvent, Target, Timestamp, Zone, ZoneRef,
};
use mtg_ir::{
    Effect, Selector, ability::Keyword, effect::Duration, effect::ManaOutput, selector::Binding,
};

use crate::{
    apply,
    choice::{Answer, ChoiceKind},
    eval::{self, ComputedChars, Ctx, EvalError},
    layers::PrintedCards,
    state::{AffectedSet, ContinuousEffect, GameState},
};

/// Frozen target occurrences for a copy's pending retargeting decision.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "verification", derive(serde::Serialize))]
pub struct Retargeting {
    specs: Vec<mtg_ir::selector::TargetSpec>,
    groups: Vec<usize>,
    current: Vec<Target>,
}
impl Retargeting {
    pub(crate) fn accepts(&self, kind: &ChoiceKind, answer: &Answer) -> bool {
        let (ChoiceKind::ChooseTargets { slots, .. }, Answer::Targets(picked)) = (kind, answer)
        else {
            return false;
        };
        if picked.len() != slots.len() || picked.len() != self.current.len() {
            return false;
        }
        let mut chosen = Vec::with_capacity(picked.len());
        for (group, offered) in picked.iter().zip(slots) {
            if group.len() != 1 || !offered.contains(&group[0]) {
                return false;
            }
            chosen.push(group[0]);
        }
        for i in 0..chosen.len() {
            for j in 0..i {
                if chosen[i] == chosen[j]
                    && (chosen[i] != self.current[i] || chosen[j] != self.current[j])
                    && (self.groups[i] == self.groups[j]
                        || self.specs[i].distinct_from_other_targets)
                {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
pub enum ResolveError {
    Eval(EvalError),
    /// The effect needs a decision from a player before it can continue.
    ///
    /// Resolution unwinds, the caller rolls the state back, asks, and runs the whole
    /// resolution again with the answer in hand. See [`ResolveCtx::need`].
    Ask {
        who: PlayerId,
        kind: Box<crate::choice::ChoiceKind>,
        because: Box<str>,
        retargeting: Option<Retargeting>,
    },
    /// A primitive not yet implemented. Named so the gap is legible in a failure.
    Unsupported(&'static str),
}

impl From<EvalError> for ResolveError {
    fn from(e: EvalError) -> Self {
        ResolveError::Eval(e)
    }
}

/// Everything a resolution needs beyond the state.
#[derive(Clone, Debug)]
pub struct ResolveCtx {
    pub source: ObjectId,
    pub controller: PlayerId,
    pub targets: Vec<Target>,
    pub x: u32,
    pub bindings: BTreeMap<Binding, Vec<Target>>,
    /// Which colour an ambiguous mana output should produce.
    ///
    /// Set by the payment planner, which already decided this while working out that
    /// the cost was payable. Asking the player again would be asking a question
    /// whose answer is already determined.
    pub mana_choice: Option<mtg_core::Color>,

    /// Per chosen target, whether it is still legal (CR 608.2b). Empty means all legal.
    pub target_legal: Vec<bool>,

    /// Answers already gathered for this resolution, in the order they were asked.
    ///
    /// Resolution cannot suspend — it is a recursive tree walk with side effects — so
    /// it is made *restartable* instead. When a choice is needed and no answer is
    /// waiting here, resolution unwinds with [`ResolveError::Ask`], the caller rolls
    /// the game state back to where the resolution started, asks the player, appends
    /// the answer, and runs the whole resolution again.
    ///
    /// This works because resolution is deterministic given the starting state and the
    /// answers: the Nth choice on a re-run is necessarily the same Nth choice. It costs
    /// one replay per choice, and effects have only a handful. It also makes resolution
    /// restartable in general, which is what taking a choice back needs.
    pub answers: Vec<Answer>,
    /// Modes chosen when the spell or ability was announced.
    pub modes: Option<Vec<u8>>,
    /// How many answers this run has consumed. Reset on every attempt.
    consumed: usize,
    mana_restrictions: Vec<(mtg_ir::ObjectFilter, bool)>,
}

impl ResolveCtx {
    pub fn new(source: ObjectId, controller: PlayerId) -> Self {
        Self {
            source,
            controller,
            targets: Vec::new(),
            x: 0,
            bindings: BTreeMap::new(),
            mana_choice: None,
            target_legal: Vec::new(),
            answers: Vec::new(),
            modes: None,
            consumed: 0,
            mana_restrictions: Vec::new(),
        }
    }

    /// Ask for a decision, or take the answer already gathered.
    ///
    /// Answers are matched to choices *by position*, which is sound because a re-run
    /// from the same starting state reaches the same choice points in the same order.
    fn need(
        &mut self,
        who: PlayerId,
        kind: crate::choice::ChoiceKind,
        because: &str,
    ) -> Result<Answer, ResolveError> {
        if let Some(a) = self.answers.get(self.consumed) {
            self.consumed += 1;
            return Ok(a.clone());
        }
        Err(ResolveError::Ask {
            who,
            kind: Box::new(kind),
            because: because.into(),
            retargeting: None,
        })
    }
}

/// Resolve an effect, applying its events as it goes.
pub fn resolve(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    effect: &Effect,
    rc: &mut ResolveCtx,
) -> Result<(), ResolveError> {
    let result = resolve_inner(state, cards, log, effect, rc);
    if result.is_ok() {
        // A player can ascend between instructions of one resolving ability,
        // even if a later instruction removes the tenth permanent again.
        refresh_city_blessings(state, cards, log, Cause::Resolution(rc.source));
    }
    result
}

fn controlled_permanents(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
) -> Vec<ObjectId> {
    state
        .battlefield()
        .into_iter()
        .filter(|id| crate::layers::controller(state, *id) == Some(player))
        .filter(|id| crate::layers::compute(state, cards, *id).is_some_and(|ch| ch.is_permanent()))
        .collect()
}

/// Ascend on a permanent is immediate, including during another ability's resolution.
pub(crate) fn refresh_city_blessings(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    cause: Cause,
) {
    if state.battlefield().len() < 10 {
        return;
    }
    let players: Vec<_> = state
        .players
        .values()
        .filter(|p| !p.city_blessing)
        .map(|p| p.id)
        .collect();
    for player in players {
        let permanents = controlled_permanents(state, cards, player);
        if permanents.len() >= 10
            && permanents.iter().any(|id| {
                crate::layers::compute(state, cards, *id).is_some_and(|ch| {
                    ch.granted_keywords.contains(&Keyword::Ascend)
                        || state.objects.get(id).is_some_and(|o| {
                            cards.face(o.card, o.face).is_some_and(|face| {
                                face.abilities.iter().any(|a| {
                                    ch.abilities.contains(&a.id)
                                        && matches!(
                                            a.kind,
                                            mtg_ir::AbilityKind::Keyword(Keyword::Ascend)
                                        )
                                })
                            })
                        })
                })
            })
        {
            apply::apply(state, cause, Event::CityBlessingGranted { player }, log);
        }
    }
}

fn resolve_inner(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    effect: &Effect,
    rc: &mut ResolveCtx,
) -> Result<(), ResolveError> {
    let cause = Cause::Resolution(rc.source);

    match effect {
        Effect::Nothing => Ok(()),
        Effect::Ascend => {
            if !state.player(rc.controller).city_blessing
                && controlled_permanents(state, cards, rc.controller).len() >= 10
            {
                apply::apply(
                    state,
                    cause,
                    Event::CityBlessingGranted {
                        player: rc.controller,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::PreventAllCombatDamage => {
            apply::apply(
                state,
                cause,
                Event::CombatDamagePreventionChanged { active: true },
                log,
            );
            Ok(())
        }
        Effect::DamageCantBePrevented => {
            apply::apply(
                state,
                cause,
                Event::DamageUnpreventableChanged { active: true },
                log,
            );
            Ok(())
        }
        Effect::PreventDamage { to } => {
            let recipients = with_ctx(state, cards, rc, |ctx| targets_of(ctx, to))?;
            for target in recipients {
                apply::apply(
                    state,
                    cause,
                    Event::DamagePreventionChanged {
                        target,
                        active: true,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::PreventDamageShield {
            to,
            by,
            combat_only,
            amount,
        } => {
            let (recipients, sources) = with_ctx(state, cards, rc, |ctx| {
                let recipients = match to {
                    Some(to) => targets_of(ctx, to)?.into_iter().map(Some).collect(),
                    None => vec![None],
                };
                let sources = match by {
                    Some(by) => eval::objects(ctx, by)?.into_iter().map(Some).collect(),
                    None => vec![None],
                };
                Ok((recipients, sources))
            })?;
            let remaining = match amount {
                Some(v) => Some(value_asking(state, cards, rc, v)?.max(0) as u32),
                None => None,
            };
            if remaining == Some(0) {
                return Ok(());
            }
            let mut events = Vec::new();
            let mut id = state.next_shield;
            for to in &recipients {
                for by in &sources {
                    events.push(Event::DamageShieldCreated {
                        shield: mtg_core::DamageShield {
                            id,
                            to: *to,
                            by: *by,
                            combat_only: *combat_only,
                            remaining,
                        },
                    });
                    id += 1;
                }
            }
            apply::apply_simultaneous(state, cause, events, log);
            Ok(())
        }

        Effect::Sequence(items) => {
            for item in items {
                resolve(state, cards, log, item, rc)?;
            }
            Ok(())
        }

        Effect::If {
            cond,
            then,
            otherwise,
        } => {
            let taken = with_ctx(state, cards, rc, |ctx| eval::condition(ctx, cond))?;
            resolve(state, cards, log, if taken { then } else { otherwise }, rc)
        }

        Effect::Let { slot, what, body } => {
            let found = if matches!(what, Selector::ChosenBy { .. }) {
                objects_asking(state, cards, rc, what)?
                    .into_iter()
                    .map(Target::Object)
                    .collect()
            } else {
                with_ctx(state, cards, rc, |ctx| targets_of(ctx, what))?
            };
            rc.bindings.insert(*slot, found);
            resolve(state, cards, log, body, rc)
        }

        Effect::ForEach { what, body } => {
            // The set is fixed before the body runs. An effect that acts on each
            // creature does not act on creatures the body itself creates.
            let each = with_ctx(state, cards, rc, |ctx| targets_of(ctx, what))?;
            for one in each {
                rc.bindings.insert(Binding::It, vec![one]);
                resolve(state, cards, log, body, rc)?;
            }
            Ok(())
        }

        Effect::Repeat { times, body } => {
            let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, times))?;
            for _ in 0..n.max(0) {
                resolve(state, cards, log, body, rc)?;
            }
            Ok(())
        }

        Effect::SpendOnly {
            only,
            spells_only,
            effect,
        } => {
            rc.mana_restrictions.push((only.clone(), *spells_only));
            let result = resolve(state, cards, log, effect, rc);
            rc.mana_restrictions.pop();
            result
        }
        Effect::AddMana { who, produces } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                // Made concrete by the source, "one mana of each color among …" expanded.
                let made: Vec<ManaOutput> = produces
                    .iter()
                    .flat_map(|o| {
                        crate::mana::concrete_outputs(state, cards, rc.source, rc.controller, o)
                    })
                    .collect();
                for out in made {
                    // "Add {G} for each creature you control": counted now — with this
                    // resolution's targets ("for each card in target opponent's hand").
                    let out = &match out {
                        ManaOutput::Repeated { amount, output }
                            if !matches!(amount, mtg_ir::Value::Fixed(_)) =>
                        {
                            let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, &amount))?;
                            ManaOutput::Repeated {
                                amount: mtg_ir::Value::Fixed(n.max(0)),
                                output,
                            }
                        }
                        other => other,
                    };
                    let (color, amount) = resolve_output(out, rc);
                    if amount > 0 {
                        let before = state.player(p).mana.amounts;
                        apply::apply(
                            state,
                            cause,
                            Event::ManaAdded {
                                player: p,
                                color,
                                amount,
                            },
                            log,
                        );
                        if !rc.mana_restrictions.is_empty() {
                            let amounts = std::array::from_fn(|slot| {
                                state.player(p).mana.amounts[slot].saturating_sub(before[slot])
                            });
                            if amounts.iter().any(|n| *n > 0) {
                                state.restricted_mana.push(crate::state::RestrictedMana {
                                    player: p,
                                    source: rc.source,
                                    amounts,
                                    restrictions: rc.mana_restrictions.clone(),
                                });
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        Effect::GainEnergy { who, amount } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let n = value_asking(state, cards, rc, amount)?.max(0);
            for player in players {
                apply::apply(state, cause, Event::EnergyChanged { player, delta: n }, log);
            }
            Ok(())
        }

        Effect::GivePoison { who, amount } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let n = value_asking(state, cards, rc, amount)?.max(0) as u32;
            for player in players {
                if n > 0 {
                    apply::apply(state, cause, Event::Poisoned { player, amount: n }, log);
                }
            }
            Ok(())
        }

        Effect::GainLife { who, amount } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let n = value_asking(state, cards, rc, amount)?;
            for p in players {
                if n > 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::LifeChanged {
                            player: p,
                            delta: n,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::LoseLife { who, amount } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let n = value_asking(state, cards, rc, amount)?;
            for p in players {
                if n > 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::LifeChanged {
                            player: p,
                            delta: -n,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Draw { who, count } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let n = value_asking(state, cards, rc, count)?;
            for p in players {
                for _ in 0..n.max(0) {
                    draw_one(state, cards, log, rc, p, cause)?;
                }
            }
            Ok(())
        }

        Effect::DealDamage { source, to, amount } => {
            let (src, recipients) = with_ctx(state, cards, rc, |ctx| {
                let src = eval::objects(ctx, source)?
                    .first()
                    .copied()
                    .unwrap_or(ctx.source);
                Ok((src, targets_of(ctx, to)?))
            })?;
            let n = value_asking(state, cards, rc, amount)?;
            if n <= 0 {
                return Ok(());
            }
            let shares: Vec<(Target, u32)> =
                recipients.into_iter().map(|t| (t, n as u32)).collect();
            deal_damage(state, cards, log, rc, cause, src, &shares)
        }

        Effect::DealDamageDivided { source, shares, .. } => {
            let (src, shares) = with_ctx(state, cards, rc, |ctx| {
                let src = eval::objects(ctx, source)?
                    .first()
                    .copied()
                    .unwrap_or(ctx.source);
                // A target that has become illegal is dealt none of its share (CR 608.2b).
                let mut tally: Vec<(Target, u32)> = Vec::new();
                for share in shares {
                    for t in targets_of(ctx, share)? {
                        match tally.iter_mut().find(|(u, _)| *u == t) {
                            Some((_, n)) => *n += 1,
                            None => tally.push((t, 1)),
                        }
                    }
                }
                Ok((src, tally))
            })?;
            deal_damage(state, cards, log, rc, cause, src, &shares)
        }

        Effect::Tap { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                if state.objects.get(&id).is_some_and(|o| !o.tapped) {
                    apply::apply(
                        state,
                        cause,
                        Event::TapChanged {
                            object: id,
                            tapped: true,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Untap { what } => {
            let ids = objects_asking(state, cards, rc, what)?;
            for id in ids {
                if state.objects.get(&id).is_some_and(|o| o.tapped) {
                    apply::apply(
                        state,
                        cause,
                        Event::TapChanged {
                            object: id,
                            tapped: false,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::AddCounters { what, kind, amount } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let n = value_asking(state, cards, rc, amount)?;
            for id in ids {
                if n != 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::CountersChanged {
                            object: id,
                            kind: *kind,
                            delta: n,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::CopyCounters { from, to } => {
            let (from, to) = with_ctx(state, cards, rc, |ctx| {
                Ok((eval::objects(ctx, from)?, eval::objects(ctx, to)?))
            })?;
            // As it last existed, for a creature that has died (CR 608.2h).
            let counters: Vec<(mtg_core::CounterKind, i32)> = from
                .iter()
                .filter_map(|id| state.objects.get(id).or_else(|| state.last_known.get(id)))
                .flat_map(|o| o.counters.iter().map(|(k, n)| (*k, *n)))
                .filter(|(_, n)| *n > 0)
                .collect();
            for id in to {
                for (kind, delta) in &counters {
                    apply::apply(
                        state,
                        cause,
                        Event::CountersChanged {
                            object: id,
                            kind: *kind,
                            delta: *delta,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::RemoveCounters { what, kind, amount } => {
            let (ids, n) = with_ctx(state, cards, rc, |ctx| {
                Ok((eval::objects(ctx, what)?, eval::value(ctx, amount)?))
            })?;
            for id in ids {
                if n != 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::CountersChanged {
                            object: id,
                            kind: *kind,
                            delta: -n,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Destroy { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            // Remembered for "for each creature destroyed this way", by the identity each
            // had on the battlefield (its last-known information).
            let mut destroyed = Vec::new();
            let mut events = Vec::new();
            for id in ids {
                for mut e in destruction(state, cards, id) {
                    if let Event::ZoneChange { new_object, .. } = &mut e {
                        *new_object = state.new_object_id();
                        destroyed.push(Target::Object(id));
                    }
                    events.push(e);
                }
            }
            apply::apply_simultaneous(state, cause, events, log);
            rc.bindings
                .insert(mtg_ir::selector::Binding::DESTROYED, destroyed);
            Ok(())
        }

        Effect::Fight { a, b } => {
            let (a, b) = with_ctx(state, cards, rc, |ctx| {
                Ok((
                    eval::objects(ctx, a)?.first().copied(),
                    eval::objects(ctx, b)?.first().copied(),
                ))
            })?;
            let (Some(a), Some(b)) = (a, b) else {
                return Ok(());
            };
            let creature = |id| {
                state
                    .objects
                    .get(&id)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                    && crate::layers::compute(state, cards, id)
                        .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
            };
            if !creature(a) || !creature(b) {
                return Ok(());
            }
            // Both powers are fixed before either deals damage: the damage is dealt at
            // the same time.
            let power = |id| {
                crate::layers::compute(state, cards, id)
                    .and_then(|c| c.power)
                    .unwrap_or(0)
                    .max(0)
            };
            let (pa, pb) = (power(a), power(b));
            let (ka, kb) = (Binding::Named(u16::MAX - 1), Binding::Named(u16::MAX - 2));
            rc.bindings.insert(ka, vec![Target::Object(a)]);
            rc.bindings.insert(kb, vec![Target::Object(b)]);
            for (from, to, n) in [(ka, kb, pa), (kb, ka, pb)] {
                resolve(
                    state,
                    cards,
                    log,
                    &Effect::DealDamage {
                        source: Selector::Bound(from),
                        to: Selector::Bound(to),
                        amount: mtg_ir::Value::Fixed(n),
                    },
                    rc,
                )?;
            }
            Ok(())
        }

        Effect::Delayed { on, effect } => {
            let card = state
                .objects
                .get(&rc.source)
                .map(|o| o.card)
                .unwrap_or(mtg_core::CardId(u32::MAX));
            let id = state.next_delayed;
            state.next_delayed += 1;
            state.delayed.push(crate::state::DelayedTrigger {
                id,
                source: rc.source,
                card,
                controller: rc.controller,
                on: on.clone(),
                effect: (**effect).clone(),
                bindings: rc.bindings.clone(),
                targets: Vec::new(),
            });
            Ok(())
        }

        Effect::Regenerate { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                if state
                    .objects
                    .get(&id)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(state, cause, Event::ShieldGained { object: id }, log);
                }
            }
            Ok(())
        }

        Effect::Reveal { what } => {
            for object in objects_asking(state, cards, rc, what)? {
                apply::apply(state, cause, Event::Revealed { object }, log);
            }
            Ok(())
        }

        Effect::RemoveFromCombat { what } => {
            for object in with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))? {
                apply::apply(state, cause, Event::RemovedFromCombat { object }, log);
            }
            Ok(())
        }

        Effect::LookAtHand { whose } => {
            let by = rc.controller;
            for p in with_ctx(state, cards, rc, |ctx| eval::players(ctx, whose))? {
                for object in state.objects_in(ZoneRef::of(Zone::Hand, p)) {
                    apply::apply(state, cause, Event::LookedAt { object, by }, log);
                }
            }
            Ok(())
        }

        Effect::MoveZone {
            what,
            to,
            position,
            tapped,
            owner_relative_to,
            face_down,
            under_control_of,
        } => {
            // Only the owner's own zone, face up, under its owner's control is carried
            // out. Anything else is refused rather than approximated.
            if owner_relative_to.is_some()
                || *face_down
                || (under_control_of.is_some() && *to != Zone::Battlefield)
            {
                return Err(ResolveError::Unsupported("zone change with a twist"));
            }
            let new_controller = match under_control_of {
                Some(sel) => with_ctx(state, cards, rc, |ctx| eval::players(ctx, sel))?
                    .first()
                    .copied(),
                None => None,
            };
            let mut ids = objects_asking(state, cards, rc, what)?;
            // "Search your library for …" (CR 701.19): searched, found or not.
            if let Selector::ChosenBy {
                chooser,
                zone: Zone::Library,
                ..
            } = what
                && let Some(player) = with_ctx(state, cards, rc, |ctx| eval::players(ctx, chooser))?
                    .first()
                    .copied()
            {
                apply::apply(state, cause, Event::LibrarySearched { player }, log);
            }
            if *to == Zone::Library
                && matches!(
                    position,
                    mtg_ir::effect::ZonePosition::Top
                        | mtg_ir::effect::ZonePosition::Bottom
                        | mtg_ir::effect::ZonePosition::BottomRandom
                )
            {
                let mut by_owner = std::collections::BTreeMap::<PlayerId, Vec<ObjectId>>::new();
                for id in ids {
                    if let Some(object) = state.objects.get(&id) {
                        by_owner.entry(object.owner).or_default().push(id);
                    }
                }
                ids = Vec::new();
                for (owner, mut group) in by_owner {
                    // A random order is the game's, from its seeded generator.
                    if matches!(position, mtg_ir::effect::ZonePosition::BottomRandom) {
                        state.rng.shuffle(&mut group);
                        ids.extend(group);
                        continue;
                    }
                    let mut ordered =
                        ask_order(rc, owner, group, "order for your library, first is highest")?;
                    // Each top insertion goes above the previous one.
                    if matches!(position, mtg_ir::effect::ZonePosition::Top) {
                        ordered.reverse();
                    }
                    ids.extend(ordered);
                }
            }
            let mut moved = Vec::new();
            for id in ids {
                // CR 712.14b: putting an MDFC onto the battlefield uses its front,
                // and an instant/sorcery front cannot enter as a permanent.
                if *to == Zone::Battlefield
                    && state.objects.get(&id).is_some_and(|o| {
                        cards.layout(o.card) == mtg_ir::Layout::ModalDfc
                            && o.zone.zone != Zone::Stack
                            && cards.face(o.card, 0).is_some_and(|f| {
                                f.card_types.contains(&mtg_core::CardType::Instant)
                                    || f.card_types.contains(&mtg_core::CardType::Sorcery)
                            })
                    })
                {
                    continue;
                }
                let index = match position {
                    mtg_ir::effect::ZonePosition::Bottom
                    | mtg_ir::effect::ZonePosition::BottomRandom => Some(u32::MAX),
                    mtg_ir::effect::ZonePosition::FromTop(n) => Some(u32::from(*n)),
                    // "its owner's choice of the top or bottom of their library"
                    mtg_ir::effect::ZonePosition::OwnerChooses => {
                        let Some(owner) = state.objects.get(&id).map(|o| o.owner) else {
                            continue;
                        };
                        if ask_confirm(rc, owner, "put it on top of your library (or the bottom)?")?
                        {
                            None
                        } else {
                            Some(u32::MAX)
                        }
                    }
                    _ => None,
                };
                let Some(new_id) = move_to_at(state, log, id, *to, index, cause) else {
                    continue;
                };
                if *to == Zone::Battlefield {
                    if let Some(player) = new_controller {
                        apply::apply(
                            state,
                            cause,
                            Event::EnteredUnderControl {
                                object: new_id,
                                player,
                            },
                            log,
                        );
                    }
                    let pay = enter_choice(state, cards, rc, new_id)?;
                    entered(state, cards, log, new_id, cause, pay);
                    if *tapped {
                        apply::apply(state, cause, Event::EnteredTapped { object: new_id }, log);
                    }
                }
                moved.push(Target::Object(new_id));
            }
            // "Return it to the battlefield. Put a counter on it": later steps find the
            // object under its new identity.
            rc.bindings.insert(Binding::It, moved);
            Ok(())
        }

        Effect::Attach { what, to } => {
            let (ids, dest) = with_ctx(state, cards, rc, |ctx| {
                Ok((
                    eval::objects(ctx, what)?,
                    eval::objects(ctx, to)?.first().copied(),
                ))
            })?;
            for id in ids {
                apply::apply(
                    state,
                    cause,
                    Event::Attached {
                        object: id,
                        to: dest,
                    },
                    log,
                );
            }
            Ok(())
        }

        Effect::Continuous {
            what,
            modification,
            duration,
        } => {
            let affected = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            // CR 608.2h: a one-shot effect's numbers are fixed as it resolves — "gets +1/+1
            // for each creature you control until end of turn" does not keep counting. Only
            // a static ability's effect is re-evaluated.
            let mut modification = modification.clone();
            // "Protection from the color of your choice": chosen as the effect begins.
            if let mtg_ir::effect::Modification::Restriction(
                mtg_ir::effect::Restriction::Protection {
                    from,
                    chosen_color: chosen @ true,
                    ..
                },
            ) = &mut modification
            {
                use mtg_core::Color;
                const COLORS: [Color; 5] = [
                    Color::White,
                    Color::Blue,
                    Color::Black,
                    Color::Red,
                    Color::Green,
                ];
                let answer = rc.need(
                    rc.controller,
                    crate::choice::ChoiceKind::ChooseModes {
                        available: ["white", "blue", "black", "red", "green"]
                            .map(Box::<str>::from)
                            .to_vec(),
                        count: 1,
                        min: None,
                    },
                    "choose a color to gain protection from",
                )?;
                let i = match answer {
                    Answer::Modes(m) => m.first().copied().unwrap_or(0) as usize,
                    _ => 0,
                };
                *from = mtg_ir::ObjectFilter::HasColor(COLORS[i.min(4)]);
                *chosen = false;
            }
            // "Target player can't cast spells this turn": that player, fixed now.
            if let mtg_ir::effect::Modification::Restriction(
                mtg_ir::effect::Restriction::CantCast { who, .. },
            ) = &mut modification
                && matches!(who, Selector::Target { .. } | Selector::Bound(_))
            {
                let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
                *who = match players.as_slice() {
                    [one] => Selector::Player(*one),
                    several => {
                        Selector::Union(several.iter().map(|p| Selector::Player(*p)).collect())
                    }
                };
            }
            // "Becomes the chosen color/type": the source's choice, fixed now.
            if let mtg_ir::effect::Modification::BecomesChosen(choice) = &modification {
                let source = state.objects.get(&rc.source);
                modification = match choice {
                    mtg_ir::effect::EntryChoice::Color => {
                        match source.and_then(|o| o.chosen_color) {
                            Some(c) => mtg_ir::effect::Modification::SetColors(vec![c]),
                            None => return Ok(()),
                        }
                    }
                    mtg_ir::effect::EntryChoice::CreatureType => {
                        match source.and_then(|o| o.chosen_subtype) {
                            Some(s) => mtg_ir::effect::Modification::SetCreatureTypes(vec![s]),
                            None => return Ok(()),
                        }
                    }
                };
            }
            if *duration != Duration::WhileSourcePresent {
                use mtg_ir::effect::Modification as M;
                if let M::ModifyPowerToughness { power, toughness }
                | M::SetBasePowerToughness { power, toughness } = &mut modification
                {
                    let (p, t) = with_ctx(state, cards, rc, |ctx| {
                        Ok((eval::value(ctx, power)?, eval::value(ctx, toughness)?))
                    })?;
                    *power = mtg_ir::Value::Fixed(p);
                    *toughness = mtg_ir::Value::Fixed(t);
                }
            }
            let modification = &modification;
            let id = state.new_object_id();
            let timestamp = state.bump();
            state.continuous.push(ContinuousEffect {
                id,
                source: rc.source,
                // A one-shot effect freezes its set; a static ability keeps
                // re-evaluating. `Continuous` from a resolving spell is the former.
                affected: match duration {
                    Duration::WhileSourcePresent => AffectedSet::Dynamic(what.clone()),
                    _ => AffectedSet::Fixed(affected),
                },
                modification: modification.clone(),
                duration: *duration,
                timestamp,
                layer: layer_of(modification),
                ability: None,
                // Who "you" is once a resolved spell has left the stack: "you may play an
                // additional land this turn".
                controller: Some(rc.controller),
            });
            apply::apply(
                state,
                cause,
                Event::ContinuousEffectBegan {
                    effect: id,
                    source: rc.source,
                },
                log,
            );
            Ok(())
        }

        // ---- effects that ask their controller something ------------------
        Effect::May {
            prompt,
            then,
            otherwise,
        } => {
            if ask_confirm(rc, rc.controller, prompt)? {
                resolve(state, cards, log, then, rc)
            } else if let Some(otherwise) = otherwise {
                resolve(state, cards, log, otherwise, rc)
            } else {
                Ok(())
            }
        }

        Effect::MayPay { cost, then } => {
            // Mana, life and energy are understood here. Any other additional cost during
            // resolution (sacrifice a creature, discard a card) needs the cost system to
            // become restartable too, and is refused rather than waived — a cost silently
            // treated as free is worse than an unimplemented one.
            let mut life = 0;
            let mut energy = 0;
            for part in &cost.additional {
                match part {
                    mtg_ir::AdditionalCost::PayLife {
                        amount: mtg_ir::Value::Fixed(n),
                    } => life += n,
                    mtg_ir::AdditionalCost::PayEnergy {
                        amount: mtg_ir::Value::Fixed(n),
                    } => energy += n,
                    _ => {
                        return Err(ResolveError::Unsupported(
                            "additional cost during resolution",
                        ));
                    }
                }
            }
            let player = state.player(rc.controller);
            if player.life < life || (player.energy as i32) < energy {
                return Ok(());
            }
            // "You may pay {X}": X is chosen now, up to what can be paid, and is X in what
            // paying it does — fixed into that effect, so a "when you do" trigger keeps it.
            let variable = cost.mana.symbols.contains(&mtg_core::ManaSymbol::Variable);
            let mut x = if variable { 0 } else { rc.x };
            let affordable = crate::mana::can_pay(state, cards, rc.controller, &cost.mana, x);
            if !affordable {
                return Ok(());
            }
            if !ask_confirm(rc, rc.controller, "pay the cost?")? {
                return Ok(());
            }
            let mut then = then.clone();
            if variable {
                let mut max = 0;
                while max < 100
                    && crate::mana::can_pay(state, cards, rc.controller, &cost.mana, max + 1)
                {
                    max += 1;
                }
                x = match rc.need(
                    rc.controller,
                    ChoiceKind::ChooseX { min: 0, max },
                    "choose a value for X",
                )? {
                    Answer::Number(n) => n.min(max),
                    _ => 0,
                };
                mtg_ir::walk::substitute_value(
                    &mut then,
                    &mtg_ir::Value::X,
                    &mtg_ir::Value::Fixed(x as i32),
                );
            }
            if let Some(plan) = crate::mana::plan(state, cards, rc.controller, &cost.mana, x) {
                pay_plan(state, cards, log, rc.controller, &plan);
                if life > 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::LifeChanged {
                            player: rc.controller,
                            delta: -life,
                        },
                        log,
                    );
                }
                if energy > 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::EnergyChanged {
                            player: rc.controller,
                            delta: -energy,
                        },
                        log,
                    );
                }
                resolve(state, cards, log, &then, rc)
            } else {
                Ok(())
            }
        }

        Effect::UnlessPays {
            payer,
            cost,
            times,
            otherwise,
        } => {
            // "… unless that player pays {1}": the first player named; nobody to pay
            // means nothing is paid.
            let who = with_ctx(state, cards, rc, |ctx| eval::players(ctx, payer))?
                .into_iter()
                .find(|p| state.players.get(p).is_some_and(|s| !s.has_lost));
            // Mana and life ("cumulative upkeep—pay 1 life") are understood.
            let mut life_each = 0;
            for part in &cost.additional {
                match part {
                    mtg_ir::AdditionalCost::PayLife {
                        amount: mtg_ir::Value::Fixed(n),
                    } => life_each += n,
                    _ => {
                        return Err(ResolveError::Unsupported(
                            "additional cost during resolution",
                        ));
                    }
                }
            }
            let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, times))?.max(0);
            let mana = mtg_core::ManaCost {
                symbols: (0..n).flat_map(|_| cost.mana.symbols.clone()).collect(),
            };
            let life = life_each * n;
            // Life can be paid only from a life total at least that large (CR 119.4).
            if let Some(who) = who
                && state.player(who).life >= life
                && crate::mana::can_pay(state, cards, who, &mana, rc.x)
                && ask_confirm(rc, who, "pay the cost?")?
                && let Some(plan) = crate::mana::plan(state, cards, who, &mana, rc.x)
            {
                pay_plan(state, cards, log, who, &plan);
                if life > 0 {
                    apply::apply(
                        state,
                        cause,
                        Event::LifeChanged {
                            player: who,
                            delta: -life,
                        },
                        log,
                    );
                }
                return Ok(());
            }
            resolve(state, cards, log, otherwise, rc)
        }

        Effect::Modal {
            choose,
            modes,
            at_least,
        } => {
            let count = with_ctx(state, cards, rc, |ctx| eval::value(ctx, choose))?.max(0) as u8;
            let min = at_least.unwrap_or(count).min(count);
            let labels: Vec<Box<str>> = modes.iter().map(|(label, _)| label.clone()).collect();
            // Modes chosen on announcement (CR 700.2) are used once, by the modal effect
            // at the top; a modal effect with none announced asks now.
            let picked = match rc.modes.take() {
                Some(ms) => ms,
                None => ask_modes(rc, rc.controller, labels, count, min)?,
            };
            for i in picked {
                if let Some((_, effect)) = modes.get(i as usize) {
                    resolve(state, cards, log, effect, rc)?;
                }
            }
            Ok(())
        }

        Effect::Discard {
            who,
            count,
            at_random,
        } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, count))?.max(0) as u32;
                let hand = state.objects_in(ZoneRef::of(Zone::Hand, p));
                let n = n.min(hand.len() as u32);
                if n == 0 {
                    continue;
                }
                let chosen = if *at_random {
                    // A random discard is not a choice, so it must not prompt. The game's
                    // seeded generator picks, so a replay discards the same cards.
                    let mut hand = hand;
                    state.rng.shuffle(&mut hand);
                    hand.into_iter().take(n as usize).collect()
                } else {
                    ask_objects(rc, p, hand, n, n, "discard")?
                };
                for id in chosen {
                    discard(state, cards, log, id, cause);
                }
            }
            Ok(())
        }

        Effect::Madness { cost } => {
            let card = rc.source;
            if state
                .objects
                .get(&card)
                .is_some_and(|o| o.zone.zone == Zone::Exile)
            {
                let owner = state.objects[&card].owner;
                if !cast_during_resolution(state, cards, log, rc, card, owner, Some(cost))? {
                    move_to(state, log, card, Zone::Graveyard, cause);
                }
            }
            Ok(())
        }

        Effect::Sacrifice { who, what } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                let candidates = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|id| crate::layers::controller(state, *id) == Some(p))
                    .collect::<Vec<_>>();
                if candidates.is_empty() {
                    continue;
                }
                // Sacrificing is mandatory once the effect resolves; which one is the
                // choice. "Sacrifice it" names one object and asks nothing — which a mana
                // ability, resolving with no way to ask, depends on.
                let chosen = if matches!(what, Selector::SelfSource) {
                    candidates
                } else {
                    ask_objects(rc, p, candidates, 1, 1, "sacrifice")?
                };
                for id in chosen {
                    move_to(state, log, id, Zone::Graveyard, cause);
                    apply::apply(
                        state,
                        cause,
                        Event::Sacrificed {
                            player: p,
                            object: id,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::ReorderLibraryTop { who, count } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, count))?.max(0);
                let top: Vec<ObjectId> = state
                    .objects_in(ZoneRef::of(Zone::Library, p))
                    .into_iter()
                    .take(n as usize)
                    .collect();
                if top.len() < 2 {
                    continue;
                }
                for (i, id) in ask_order(
                    rc,
                    rc.controller,
                    top,
                    "order for the top, first is topmost",
                )?
                .into_iter()
                .enumerate()
                {
                    move_to_at(state, log, id, Zone::Library, Some(i as u32), cause);
                }
            }
            Ok(())
        }

        Effect::Dig {
            count,
            take,
            up_to,
            filter,
            additional_filter,
            take_to,
            tapped,
            reveal,
            rest_to,
            rest_random,
        } => {
            let p = rc.controller;
            let (n, k) = with_ctx(state, cards, rc, |ctx| {
                Ok((eval::value(ctx, count)?, eval::value(ctx, take)?))
            })?;
            let top: Vec<ObjectId> = state
                .objects_in(ZoneRef::of(Zone::Library, p))
                .into_iter()
                .take(n.max(0) as usize)
                .collect();
            if top.is_empty() {
                return Ok(());
            }
            let candidates: Vec<ObjectId> = with_ctx(state, cards, rc, |ctx| {
                Ok(top
                    .iter()
                    .copied()
                    .filter(|id| eval::matches(ctx, filter, *id).unwrap_or(false))
                    .collect())
            })?;
            let max = (k.max(0) as u32).min(candidates.len() as u32);
            let min = if *up_to { 0 } else { max };
            let mut chosen = ask_objects(rc, p, candidates, min, max, "take from among them")?;
            if let Some(filter) = additional_filter {
                let candidates = with_ctx(state, cards, rc, |ctx| {
                    Ok(top
                        .iter()
                        .copied()
                        .filter(|id| {
                            !chosen.contains(id) && eval::matches(ctx, filter, *id).unwrap_or(false)
                        })
                        .collect::<Vec<_>>())
                })?;
                let max = 1.min(candidates.len() as u32);
                chosen.extend(ask_objects(
                    rc,
                    p,
                    candidates,
                    0,
                    max,
                    "take the other quality",
                )?);
            }
            if *reveal {
                for id in &chosen {
                    apply::apply(state, cause, Event::Revealed { object: *id }, log);
                }
            }
            for id in &chosen {
                let index = (*take_to == Zone::Library).then_some(0);
                if let Some(new_id) = move_to_at(state, log, *id, *take_to, index, cause)
                    && *take_to == Zone::Battlefield
                {
                    let pay = enter_choice(state, cards, rc, new_id)?;
                    entered(state, cards, log, new_id, cause, pay);
                    if *tapped {
                        apply::apply(state, cause, Event::EnteredTapped { object: new_id }, log);
                    }
                }
            }
            let rest: Vec<ObjectId> = top.into_iter().filter(|id| !chosen.contains(id)).collect();
            if *rest_to == Zone::Library {
                let rest = if *rest_random || rest.len() < 2 {
                    rest
                } else {
                    ask_order(rc, p, rest, "order for the bottom, first is highest")?
                };
                let count = rest.len() as u32;
                for id in rest {
                    move_to_at(state, log, id, Zone::Library, Some(u32::MAX), cause);
                }
                if *rest_random && count > 1 {
                    apply::apply(
                        state,
                        cause,
                        Event::LibraryBottomShuffled { player: p, count },
                        log,
                    );
                }
            } else {
                for id in rest {
                    move_to(state, log, id, *rest_to, cause);
                }
            }
            Ok(())
        }
        Effect::LookAndSort {
            who,
            count,
            keep_zone,
            other_zone,
        } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, count))?.max(0) as u32;
                let top: Vec<ObjectId> = state
                    .objects_in(ZoneRef::of(Zone::Library, p))
                    .into_iter()
                    .take(n as usize)
                    .collect();
                if n > 0 {
                    let surveil = *other_zone == Zone::Graveyard;
                    apply::apply(state, cause, Event::Scried { player: p, surveil }, log);
                }
                if top.is_empty() {
                    continue;
                }
                // The scry/surveil shape: choose any subset to move elsewhere; the rest
                // stay where they are.
                let because = match other_zone {
                    Zone::Library => "put on the bottom of your library",
                    Zone::Graveyard => "put into your graveyard",
                    _ => "put aside",
                };
                let moved = ask_objects(rc, p, top.clone(), 0, top.len() as u32, because)?;
                let kept: Vec<ObjectId> = top
                    .iter()
                    .copied()
                    .filter(|id| !moved.contains(id))
                    .collect();

                // Scry: the cards set aside go to the *bottom* of the library in any order
                // (CR 701.22a); surveil puts them into the graveyard (CR 701.25a).
                if *other_zone == Zone::Library {
                    for id in ask_order(rc, p, moved, "order for the bottom, first is highest")? {
                        move_to_at(state, log, id, Zone::Library, Some(u32::MAX), cause);
                    }
                } else {
                    for id in &moved {
                        move_to(state, log, *id, *other_zone, cause);
                    }
                }
                // The rest go back on top in any order.
                if *keep_zone == Zone::Library && kept.len() > 1 {
                    for (i, id) in ask_order(rc, p, kept, "order for the top, first is topmost")?
                        .into_iter()
                        .enumerate()
                    {
                        move_to_at(state, log, id, Zone::Library, Some(i as u32), cause);
                    }
                }
            }
            Ok(())
        }

        Effect::CreateToken {
            token,
            count,
            controller,
        } => {
            // The spec must have been registered as a card face; an unregistered one has
            // nothing to be.
            let Some(card) = token.card else {
                return Err(ResolveError::Unsupported("token without a card face"));
            };
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, controller))?;
            let n = value_asking(state, cards, rc, count)?.max(0);
            let mut made = Vec::new();
            for p in players {
                for _ in 0..n {
                    let object = state.new_object_id();
                    apply::apply(
                        state,
                        cause,
                        Event::Created {
                            object,
                            card,
                            owner: p,
                        },
                        log,
                    );
                    entered(state, cards, log, object, cause, None);
                    made.push(Target::Object(object));
                }
            }
            // "Create a token. Put a +1/+1 counter on it."
            rc.bindings.insert(Binding::It, made);
            Ok(())
        }

        Effect::ExileLinked { what } => {
            let ids = objects_asking(state, cards, rc, what)?;
            let mut moved = Vec::new();
            for id in ids {
                if let Some(new_id) = move_to_at(state, log, id, Zone::Exile, None, cause) {
                    state.exiled_with.push((rc.source, new_id));
                    moved.push(Target::Object(new_id));
                }
            }
            rc.bindings.insert(Binding::It, moved);
            Ok(())
        }
        Effect::ReturnExiledWith { to, tapped } => {
            // A leaves-the-battlefield trigger's source is the card where it went; the exile
            // was remembered under the permanent it was. Follow the moves back.
            let mut identities = vec![rc.source];
            while let Some(earlier) = log.iter().rev().find_map(|e| match e.event {
                Event::ZoneChange {
                    object, new_object, ..
                } if new_object == *identities.last()? => Some(object),
                _ => None,
            }) {
                if identities.contains(&earlier) {
                    break;
                }
                identities.push(earlier);
            }
            let mine: Vec<ObjectId> = state
                .exiled_with
                .iter()
                .filter(|(source, _)| identities.contains(source))
                .map(|(_, card)| *card)
                .collect();
            state
                .exiled_with
                .retain(|(source, _)| !identities.contains(source));
            for card in mine {
                // Only if it's still the exiled card (CR 400.7).
                if !state
                    .objects
                    .get(&card)
                    .is_some_and(|o| o.zone.zone == Zone::Exile)
                {
                    continue;
                }
                if let Some(new_id) = move_to_at(state, log, card, *to, None, cause)
                    && *tapped
                {
                    apply::apply(
                        state,
                        cause,
                        Event::TapChanged {
                            object: new_id,
                            tapped: true,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }
        Effect::ExileUntilSourceLeaves { what } => {
            // The source is the permanent, however the effect got here (CR 610.3c).
            let here = state
                .objects
                .get(&rc.source)
                .is_some_and(|o| o.zone.zone == Zone::Battlefield);
            if !here {
                return Ok(());
            }
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let mut moved = Vec::new();
            for id in ids {
                if let Some(new_id) = move_to_at(state, log, id, Zone::Exile, None, cause) {
                    state.linked_exile.push((rc.source, new_id));
                    moved.push(Target::Object(new_id));
                }
            }
            rc.bindings.insert(Binding::It, moved);
            Ok(())
        }

        Effect::EnterAttacking { what, like } => {
            let defender = rc.bindings.get(like).and_then(|t| t.first()).copied();
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                // Only from the hand, where ninjutsu works (CR 702.49a).
                if !state
                    .objects
                    .get(&id)
                    .is_some_and(|o| o.zone.zone == Zone::Hand)
                {
                    continue;
                }
                let Some(new_id) = move_to_at(state, log, id, Zone::Battlefield, None, cause)
                else {
                    continue;
                };
                entered(state, cards, log, new_id, cause, None);
                apply::apply(state, cause, Event::EnteredTapped { object: new_id }, log);
                // Still attacking only if combat is still on.
                if let Some(defender) = defender
                    && state.step.is_combat()
                {
                    apply::apply(
                        state,
                        cause,
                        Event::EnteredAttacking {
                            attacker: new_id,
                            defender,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Connive { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for creature in ids {
                let Some(who) = crate::layers::controller(state, creature) else {
                    continue;
                };
                draw_one(state, cards, log, rc, who, cause)?;
                let hand = state.objects_in(ZoneRef::of(Zone::Hand, who));
                if hand.is_empty() {
                    continue;
                }
                let chosen = ask_objects(rc, who, hand, 1, 1, "discard")?;
                let nonland = chosen.iter().any(|id| {
                    state
                        .objects
                        .get(id)
                        .and_then(|o| cards.face(o.card, o.face))
                        .is_some_and(|f| !f.card_types.contains(&mtg_core::CardType::Land))
                });
                for id in chosen {
                    discard(state, cards, log, id, cause);
                }
                if nonland
                    && state
                        .objects
                        .get(&creature)
                        .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(
                        state,
                        cause,
                        Event::CountersChanged {
                            object: creature,
                            kind: mtg_core::CounterKind::PlusOnePlusOne,
                            delta: 1,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Explore { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for creature in ids {
                let Some(who) = crate::layers::controller(state, creature) else {
                    continue;
                };
                let library = ZoneRef::of(Zone::Library, who);
                let top = state.objects_in(library).first().copied();
                if let Some(top) = top {
                    apply::apply(state, cause, Event::Revealed { object: top }, log);
                }
                let land = top.is_some_and(|id| {
                    state
                        .objects
                        .get(&id)
                        .and_then(|o| cards.face(o.card, o.face))
                        .is_some_and(|f| f.card_types.contains(&mtg_core::CardType::Land))
                });
                if land && let Some(top) = top {
                    let new_object = state.new_object_id();
                    apply::apply(
                        state,
                        cause,
                        Event::ZoneChange {
                            object: top,
                            new_object,
                            from: library,
                            to: ZoneRef::of(Zone::Hand, who),
                            index: None,
                        },
                        log,
                    );
                    continue;
                }
                // A nonland card, or none at all: a +1/+1 counter (CR 701.44a).
                if state
                    .objects
                    .get(&creature)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(
                        state,
                        cause,
                        Event::CountersChanged {
                            object: creature,
                            kind: mtg_core::CounterKind::PlusOnePlusOne,
                            delta: 1,
                        },
                        log,
                    );
                }
                if let Some(top) = top
                    && ask_confirm(rc, who, "put the revealed card into your graveyard?")?
                {
                    let new_object = state.new_object_id();
                    apply::apply(
                        state,
                        cause,
                        Event::ZoneChange {
                            object: top,
                            new_object,
                            from: library,
                            to: ZoneRef::of(Zone::Graveyard, who),
                            index: None,
                        },
                        log,
                    );
                }
            }
            Ok(())
        }

        Effect::Transform { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for object in ids {
                let Some(o) = state.objects.get(&object) else {
                    continue;
                };
                // Only a transforming double-faced permanent transforms (CR 701.28c).
                if o.zone.zone != Zone::Battlefield
                    || o.face_down
                    || cards.layout(o.card) != mtg_ir::Layout::Transforming
                {
                    continue;
                }
                let face = 1 - o.face.min(1);
                if cards.face(o.card, face).is_none() {
                    continue;
                }
                apply::apply(state, cause, Event::Transformed { object, face }, log);
            }
            Ok(())
        }

        Effect::ExileReturnTransformed { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                let transforming = state
                    .objects
                    .get(&id)
                    .is_some_and(|o| cards.layout(o.card) == mtg_ir::Layout::Transforming);
                let Some(exiled) = move_to_at(state, log, id, Zone::Exile, None, cause) else {
                    continue;
                };
                if !transforming {
                    continue;
                }
                let Some(back) = move_to_at(state, log, exiled, Zone::Battlefield, None, cause)
                else {
                    continue;
                };
                // Back face up before anything sees it. It returns under its owner's
                // control, which for a Saga is "your" control unless it was stolen.
                apply::apply(
                    state,
                    cause,
                    Event::Transformed {
                        object: back,
                        face: 1,
                    },
                    log,
                );
                entered(state, cards, log, back, cause, None);
            }
            Ok(())
        }

        Effect::GrantPlay {
            what,
            until,
            cast_only,
        } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let player = rc.controller;
            let until_turn = match until {
                mtg_ir::effect::PlayUntil::ThisTurn => state.turn,
                // The turn number of this player's next turn.
                mtg_ir::effect::PlayUntil::EndOfYourNextTurn => {
                    let order = &state.turn_order;
                    let n = order.len().max(1) as u32;
                    let at = |p| order.iter().position(|q| *q == p).unwrap_or(0) as u32;
                    let ahead = (at(player) + n - at(state.active_player)) % n;
                    state.turn + if ahead == 0 { n } else { ahead }
                }
            };
            for object in ids {
                apply::apply(
                    state,
                    cause,
                    Event::PlayPermission {
                        object,
                        player,
                        until_turn,
                        cast_only: *cast_only,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::ExileSelfWithCounters { kind, amount } => {
            if let Some(object) = move_to_at(state, log, rc.source, Zone::Exile, None, cause)
                && *amount > 0
            {
                apply::apply(
                    state,
                    cause,
                    Event::CountersChanged {
                        object,
                        kind: *kind,
                        delta: *amount as i32,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::SearchLibraryAndGraveyard { filter } => {
            let me = rc.controller;
            let mut candidates = Vec::new();
            for zone in [Zone::Library, Zone::Graveyard] {
                for id in state.objects_in(ZoneRef::of(zone, me)) {
                    if with_ctx(state, cards, rc, |ctx| eval::matches(ctx, filter, id))? {
                        candidates.push(id);
                    }
                }
            }
            let chosen = if candidates.is_empty() {
                Vec::new()
            } else {
                ask_objects(rc, me, candidates, 0, 1, "search for")?
            };
            let from_graveyard = chosen.first().is_some_and(|id| {
                state
                    .objects
                    .get(id)
                    .is_some_and(|o| o.zone.zone == Zone::Graveyard)
            });
            for id in chosen {
                apply::apply(state, cause, Event::Revealed { object: id }, log);
                move_to(state, log, id, Zone::Hand, cause);
            }
            if !from_graveyard {
                apply::apply(state, cause, Event::Shuffled { player: me }, log);
            }
            Ok(())
        }
        Effect::RevealRandom { who } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let mut revealed = Vec::new();
            for p in players {
                let hand = state.objects_in(ZoneRef::of(Zone::Hand, p));
                if hand.is_empty() {
                    continue;
                }
                let pick = hand[(state.rng.next_u64() % hand.len() as u64) as usize];
                apply::apply(state, cause, Event::Revealed { object: pick }, log);
                revealed.push(Target::Object(pick));
            }
            rc.bindings.insert(Binding::It, revealed);
            Ok(())
        }
        Effect::AddManaAnyCombination { amount } => {
            use mtg_core::Color;
            const COLORS: [Color; 5] = [
                Color::White,
                Color::Blue,
                Color::Black,
                Color::Red,
                Color::Green,
            ];
            let n = value_asking(state, cards, rc, amount)?.max(0);
            let player = rc.controller;
            for _ in 0..n {
                let i = match rc.need(
                    player,
                    ChoiceKind::ChooseModes {
                        available: ["white", "blue", "black", "red", "green"]
                            .map(Box::<str>::from)
                            .to_vec(),
                        count: 1,
                        min: None,
                    },
                    "choose a color of mana to add",
                )? {
                    Answer::Modes(m) => m.first().copied().unwrap_or(0) as usize,
                    _ => 0,
                };
                apply::apply(
                    state,
                    cause,
                    Event::ManaAdded {
                        player,
                        color: Some(COLORS[i.min(4)]),
                        amount: 1,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::OnceEachTurn { body } => {
            if state.done_once_this_turn.contains(&rc.source) {
                return Ok(());
            }
            resolve(state, cards, log, body, rc)
        }
        Effect::MarkOnceEachTurn => {
            state.done_once_this_turn.insert(rc.source);
            Ok(())
        }
        Effect::AsPlayer { who, body } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let me = rc.controller;
            for p in players {
                rc.controller = p;
                let done = resolve(state, cards, log, body, rc);
                rc.controller = me;
                done?;
            }
            Ok(())
        }
        // Like a coin flip, rolled with the state's seeded generator.
        Effect::RollDie {
            sides,
            outcomes,
            then,
        } => {
            let result = (state.rng.next_u64() % u64::from((*sides).max(1))) as u32 + 1;
            let player = rc.controller;
            apply::apply(
                state,
                cause,
                Event::DieRolled {
                    player,
                    sides: *sides,
                    result,
                },
                log,
            );
            let rolled = mtg_ir::Value::Fixed(result as i32);
            let mut row = outcomes
                .iter()
                .find(|(lo, hi, _)| (*lo..=*hi).contains(&result))
                .map_or(Effect::Nothing, |(_, _, e)| e.clone());
            let mut after = (**then).clone();
            mtg_ir::walk::substitute_value(&mut row, &mtg_ir::Value::RollResult, &rolled);
            mtg_ir::walk::substitute_value(&mut after, &mtg_ir::Value::RollResult, &rolled);
            resolve(state, cards, log, &row, rc)?;
            resolve(state, cards, log, &after, rc)
        }
        // The game's own seeded generator decides, so a resolution run again from its
        // snapshot (to ask a question) flips the same way.
        Effect::FlipCoin { win, lose } => {
            let won = state.rng.next_u64() & 1 == 1;
            let player = rc.controller;
            apply::apply(state, cause, Event::CoinFlipped { player, won }, log);
            resolve(state, cards, log, if won { win } else { lose }, rc)
        }
        Effect::Clash { win, lose } => {
            let me = rc.controller;
            let opponents = with_ctx(state, cards, rc, |ctx| {
                eval::players(ctx, &Selector::Opponents)
            })?;
            let opponent = match opponents.as_slice() {
                [] => None,
                [only] => Some(*only),
                many => {
                    let labels = many.iter().map(|p| format!("player {}", p.0 + 1).into());
                    match rc.need(
                        me,
                        ChoiceKind::ChooseModes {
                            available: labels.collect(),
                            count: 1,
                            min: None,
                        },
                        "clash with which opponent",
                    )? {
                        Answer::Modes(m) => many.get(m.first().copied().unwrap_or(0) as usize),
                        _ => many.first(),
                    }
                    .copied()
                }
            };
            let clashing: Vec<PlayerId> = std::iter::once(me).chain(opponent).collect();
            let revealed: Vec<(PlayerId, Option<ObjectId>)> = clashing
                .iter()
                .map(|p| {
                    let top = state
                        .objects_in(ZoneRef::of(Zone::Library, *p))
                        .first()
                        .copied();
                    (*p, top)
                })
                .collect();
            for (_, top) in &revealed {
                if let Some(object) = top {
                    apply::apply(state, cause, Event::Revealed { object: *object }, log);
                }
            }
            let mana_value = |state: &GameState, id: ObjectId| {
                crate::layers::compute(state, cards, id).map(|c| c.mana_cost.mana_value())
            };
            // CR 701.23b: a player wins with a card of higher mana value than every other
            // card revealed.
            let values: Vec<Option<u32>> = revealed
                .iter()
                .map(|(_, top)| top.and_then(|id| mana_value(state, id)))
                .collect();
            let wins: Vec<bool> = (0..values.len())
                .map(|i| {
                    values[i].is_some_and(|m| {
                        values
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| *j != i)
                            .all(|(_, other)| other.is_none_or(|o| m > o))
                    })
                })
                .collect();
            let won = wins[0];
            // Each clashing player puts their card on the top or the bottom.
            for (p, top) in &revealed {
                let Some(id) = top else {
                    continue;
                };
                if !ask_confirm(rc, *p, "keep the revealed card on top of your library?")? {
                    move_to_at(state, log, *id, Zone::Library, Some(u32::MAX), cause);
                }
            }
            for ((p, _), won) in revealed.iter().zip(&wins) {
                apply::apply(
                    state,
                    cause,
                    Event::Clashed {
                        player: *p,
                        won: *won,
                    },
                    log,
                );
            }
            resolve(state, cards, log, if won { win } else { lose }, rc)
        }
        Effect::Choose {
            choice,
            except,
            then,
        } => {
            let mut options = entry_options(cards, *choice);
            options.retain(|(_, _, s)| except.is_none() || *s != *except);
            let what = match choice {
                mtg_ir::effect::EntryChoice::Color => "choose a color",
                mtg_ir::effect::EntryChoice::CreatureType => "choose a creature type",
            };
            let picked = match rc.need(
                rc.controller,
                ChoiceKind::ChooseModes {
                    available: options.iter().map(|(l, _, _)| l.clone()).collect(),
                    count: 1,
                    min: None,
                },
                what,
            )? {
                Answer::Modes(m) => m.first().copied().unwrap_or(0) as usize,
                _ => 0,
            };
            let Some((_, color, subtype)) = options.get(picked).or_else(|| options.first()) else {
                return resolve(state, cards, log, then, rc);
            };
            // Kept on the resolving object, where "the chosen color" is read; a choice the
            // permanent made as it entered is put back afterwards.
            let before = state
                .objects
                .get(&rc.source)
                .map(|o| (o.chosen_color, o.chosen_subtype));
            apply::apply(
                state,
                cause,
                Event::ChoiceMade {
                    object: rc.source,
                    color: *color,
                    subtype: *subtype,
                },
                log,
            );
            let result = resolve(state, cards, log, then, rc);
            if let Some((color, subtype)) = before
                && (color.is_some() || subtype.is_some())
            {
                apply::apply(
                    state,
                    cause,
                    Event::ChoiceMade {
                        object: rc.source,
                        color,
                        subtype,
                    },
                    log,
                );
            }
            result
        }
        Effect::Reflexive { effect, targets } => {
            let card = state
                .objects
                .get(&rc.source)
                .or_else(|| state.last_known.get(&rc.source))
                .map(|o| o.card)
                .unwrap_or(mtg_core::CardId(u32::MAX));
            let id = state.next_delayed;
            state.next_delayed += 1;
            state.delayed.push(crate::state::DelayedTrigger {
                id,
                source: rc.source,
                card,
                controller: rc.controller,
                on: mtg_ir::EventPattern::Reflexive,
                effect: (**effect).clone(),
                bindings: rc.bindings.clone(),
                targets: targets.clone(),
            });
            apply::apply(state, cause, Event::ReflexiveTriggered { id }, log);
            Ok(())
        }
        Effect::ExileIfDiesThisTurn { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let turn = state.turn;
            for object in ids {
                apply::apply(state, cause, Event::ExileIfDies { object, turn }, log);
            }
            Ok(())
        }
        Effect::PutAttacking { what } => {
            let Some(defender) = state
                .combat
                .defending_player
                .filter(|_| state.step.is_combat())
            else {
                return Ok(());
            };
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for attacker in ids {
                apply::apply(
                    state,
                    cause,
                    Event::EnteredAttacking {
                        attacker,
                        defender: Target::Player(defender),
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::ExtraTurn { who } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for player in players {
                apply::apply(state, cause, Event::ExtraTurnAdded { player }, log);
            }
            Ok(())
        }
        Effect::AdditionalCombat => {
            apply::apply(state, cause, Event::AdditionalCombatAdded, log);
            Ok(())
        }
        Effect::SkipNextTurn { who } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for player in players {
                apply::apply(state, cause, Event::TurnSkipAdded { player }, log);
            }
            Ok(())
        }
        Effect::GrantCastLater { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for object in ids {
                let Some(o) = state.objects.get(&object) else {
                    continue;
                };
                let cost = cards.face(o.card, o.face).map(|f| f.mana_cost.clone());
                let player = o.owner;
                apply::apply(
                    state,
                    cause,
                    Event::CastLater {
                        object,
                        player,
                        after_turn: state.turn,
                        cost,
                        sorcery: false,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::BecomeMonarch { who, emblem } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let Some(player) = players.first().copied() else {
                return Ok(());
            };
            if state.monarch == Some(player) {
                return Ok(());
            }
            let (object, card) = match state.monarch_emblem {
                Some(id) => match state.objects.get(&id) {
                    Some(o) => (id, o.card),
                    None => return Err(ResolveError::Unsupported("monarch emblem gone")),
                },
                None => {
                    let Some(card) = emblem.as_ref().and_then(|t| t.card) else {
                        return Err(ResolveError::Unsupported("monarch without its emblem"));
                    };
                    (state.new_object_id(), card)
                }
            };
            apply::apply(
                state,
                cause,
                Event::BecameMonarch {
                    player,
                    emblem: object,
                    card,
                },
                log,
            );
            Ok(())
        }
        Effect::GainClassLevel { level } => {
            let object = rc.source;
            if state
                .objects
                .get(&object)
                .is_some_and(|o| o.zone.zone == Zone::Battlefield)
            {
                apply::apply(
                    state,
                    cause,
                    Event::ClassLevelGained {
                        object,
                        level: *level,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::BecomeMonstrous { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for object in ids {
                if state
                    .objects
                    .get(&object)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(state, cause, Event::BecameMonstrous { object }, log);
                }
            }
            Ok(())
        }

        Effect::BecomeRenowned { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for object in ids {
                if state
                    .objects
                    .get(&object)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(state, cause, Event::BecameRenowned { object }, log);
                }
            }
            Ok(())
        }

        Effect::ExileIfLeaves { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for object in ids {
                if state
                    .objects
                    .get(&object)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                {
                    apply::apply(state, cause, Event::ExileIfLeaves { object }, log);
                }
            }
            Ok(())
        }

        Effect::CreateTokenCopy {
            of,
            count,
            controller,
        } => {
            let (originals, players) = with_ctx(state, cards, rc, |ctx| {
                Ok((eval::objects(ctx, of)?, eval::players(ctx, controller)?))
            })?;
            // The copiable values of the object as it is, or as it last existed.
            let Some((card, face)) = originals.first().and_then(|id| {
                state
                    .objects
                    .get(id)
                    .or_else(|| state.last_known.get(id))
                    .map(|o| (o.card, o.face))
            }) else {
                return Ok(());
            };
            let n = value_asking(state, cards, rc, count)?.max(0);
            let mut made = Vec::new();
            for p in players {
                for _ in 0..n {
                    let object = state.new_object_id();
                    apply::apply(
                        state,
                        cause,
                        Event::Created {
                            object,
                            card,
                            owner: p,
                        },
                        log,
                    );
                    if face != 0 {
                        apply::apply(state, cause, Event::BecameCopy { object, card, face }, log);
                    }
                    entered(state, cards, log, object, cause, None);
                    made.push(Target::Object(object));
                }
            }
            rc.bindings.insert(Binding::It, made);
            Ok(())
        }

        Effect::Shuffle { who } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for player in players {
                apply::apply(state, cause, Event::Shuffled { player }, log);
            }
            Ok(())
        }
        // Layer 2 (CR 613.1b): only "you" gain control here, which is what
        // `layers::controller` reads.
        Effect::GainControl {
            what,
            who: Selector::You,
            duration,
        } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let ids: Vec<ObjectId> = ids
                .into_iter()
                .filter(|id| {
                    state
                        .objects
                        .get(id)
                        .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                })
                .collect();
            if ids.is_empty() {
                return Ok(());
            }
            let id = state.new_object_id();
            let timestamp = state.bump();
            let modification = mtg_ir::effect::Modification::Control(Selector::You);
            state.continuous.push(ContinuousEffect {
                id,
                source: rc.source,
                affected: AffectedSet::Fixed(ids),
                layer: layer_of(&modification),
                modification,
                duration: *duration,
                timestamp,
                ability: None,
                controller: Some(rc.controller),
            });
            apply::apply(
                state,
                cause,
                Event::ContinuousEffectBegan {
                    effect: id,
                    source: rc.source,
                },
                log,
            );
            Ok(())
        }
        Effect::GainControl { .. } => Err(ResolveError::Unsupported("control change")),
        Effect::ExchangeControl { a, b } => {
            let (a, b) = with_ctx(state, cards, rc, |ctx| {
                Ok((
                    eval::objects(ctx, a)?.first().copied(),
                    eval::objects(ctx, b)?.first().copied(),
                ))
            })?;
            let on_field = |id: Option<ObjectId>| {
                id.filter(|id| {
                    state
                        .objects
                        .get(id)
                        .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                })
            };
            let (Some(a), Some(b)) = (on_field(a), on_field(b)) else {
                return Ok(());
            };
            let (Some(ca), Some(cb)) = (
                crate::layers::controller(state, a),
                crate::layers::controller(state, b),
            ) else {
                return Ok(());
            };
            // Both under one player: no exchange happens (CR 701.12b).
            if ca == cb {
                return Ok(());
            }
            for (object, to) in [(a, cb), (b, ca)] {
                let id = state.new_object_id();
                let timestamp = state.bump();
                let modification = mtg_ir::effect::Modification::Control(Selector::You);
                state.continuous.push(ContinuousEffect {
                    id,
                    source: rc.source,
                    affected: AffectedSet::Fixed(vec![object]),
                    layer: layer_of(&modification),
                    modification,
                    duration: mtg_ir::effect::Duration::Permanent,
                    timestamp,
                    ability: None,
                    controller: Some(to),
                });
                apply::apply(
                    state,
                    cause,
                    Event::ContinuousEffectBegan {
                        effect: id,
                        source: rc.source,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::CounterUnlessPays {
            what,
            mana,
            life,
            discard,
            exile,
            times,
        } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            // "{1} for each card in your graveyard": counted as it resolves.
            let mana = match times {
                Some(times) => {
                    let n = with_ctx(state, cards, rc, |ctx| eval::value(ctx, times))?.max(0);
                    &mtg_core::ManaCost {
                        symbols: (0..n).flat_map(|_| mana.symbols.clone()).collect(),
                    }
                }
                None => mana,
            };
            for id in ids {
                let Some(payer) = state
                    .objects
                    .get(&id)
                    .filter(|o| o.zone.zone == Zone::Stack)
                    .map(|o| o.controller)
                else {
                    continue;
                };
                // "Ward—discard a card": the payer may discard one to keep it.
                if *discard {
                    let hand = state.objects_in(ZoneRef::of(Zone::Hand, payer));
                    let chosen = if hand.is_empty() {
                        Vec::new()
                    } else {
                        ask_objects(rc, payer, hand, 0, 1, "discard a card to keep it")?
                    };
                    match chosen.first() {
                        Some(card) => {
                            self::discard(state, cards, log, *card, cause);
                        }
                        None => counter_object(state, cards, log, id, *exile, cause),
                    }
                    continue;
                }
                // Life can be paid only with at least that much (CR 119.4).
                if let Some(n) = life {
                    if state.player(payer).life >= *n as i32
                        && ask_confirm(rc, payer, "pay life to keep it from being countered?")?
                    {
                        apply::apply(
                            state,
                            cause,
                            Event::LifeChanged {
                                player: payer,
                                delta: -(*n as i32),
                            },
                            log,
                        );
                        continue;
                    }
                    counter_object(state, cards, log, id, *exile, cause);
                    continue;
                }
                // The payer decides; paying is only offered when it can be paid.
                if crate::mana::can_pay(state, cards, payer, mana, 0)
                    && ask_confirm(rc, payer, "pay to keep it from being countered?")?
                    && let Some(plan) = crate::mana::plan(state, cards, payer, mana, 0)
                {
                    pay_plan(state, cards, log, payer, &plan);
                    continue;
                }
                counter_object(state, cards, log, id, *exile, cause);
            }
            Ok(())
        }

        Effect::CounterSpell { what, exile } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                counter_object(state, cards, log, id, *exile, cause);
            }
            Ok(())
        }
        Effect::PhaseOut { what } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            let mut out = Vec::new();
            for id in ids {
                if state
                    .objects
                    .get(&id)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield && !o.phased_out)
                {
                    out.push(id);
                }
            }
            // Attached Auras and Equipment phase out with it, indirectly (CR 702.26g).
            let attached: Vec<ObjectId> = state
                .battlefield()
                .into_iter()
                .filter(|a| {
                    state
                        .objects
                        .get(a)
                        .and_then(|o| o.attached_to)
                        .is_some_and(|host| out.contains(&host))
                })
                .collect();
            out.extend(
                attached
                    .into_iter()
                    .filter(|a| !out.contains(a))
                    .collect::<Vec<_>>(),
            );
            let events: Vec<Event> = out
                .into_iter()
                .map(|object| Event::PhasedOut { object, out: true })
                .collect();
            if !events.is_empty() {
                apply::apply_simultaneous(state, cause, events, log);
            }
            Ok(())
        }
        Effect::LoseGame { who } | Effect::WinGame { who } => {
            let named = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            let win = matches!(effect, Effect::WinGame { .. });
            let losers: Vec<PlayerId> = state
                .apnap()
                .into_iter()
                .filter(|p| named.contains(p) != win)
                .collect();
            let events: Vec<Event> = losers
                .into_iter()
                .map(|player| Event::Lost {
                    player,
                    reason: mtg_core::LossReason::Effect,
                })
                .collect();
            if !events.is_empty() {
                apply::apply_simultaneous(state, cause, events, log);
            }
            Ok(())
        }
        // CR 115.7d — the targets stay unless the controller picks others; a target that
        // becomes the target anew is targeted again (CR 115.7).
        Effect::ChangeTargets { what, must } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for id in ids {
                let Some(obj) = state.objects.get(&id) else {
                    continue;
                };
                if obj.zone.zone != Zone::Stack {
                    continue;
                }
                let (before, owner) = match obj.cast_context.as_ref() {
                    Some(c) => (c.targets.clone(), obj.controller),
                    None => continue,
                };
                let Some(chosen) = targets_again(
                    state,
                    state,
                    cards,
                    rc,
                    id,
                    (id, owner),
                    ("choose new targets", *must),
                )?
                else {
                    continue;
                };
                if chosen == before {
                    continue;
                }
                let mut events = vec![Event::TargetsChanged {
                    object: id,
                    targets: chosen.clone(),
                }];
                events.extend(
                    chosen
                        .iter()
                        .zip(&before)
                        .filter(|(new, old)| new != old)
                        .map(|(new, _)| Event::Targeted {
                            object: id,
                            target: *new,
                        }),
                );
                for e in events {
                    apply::apply(state, cause, e, log);
                }
            }
            Ok(())
        }
        Effect::CopySpell {
            what,
            may_change_targets,
        } => {
            let ids = with_ctx(state, cards, rc, |ctx| eval::objects(ctx, what))?;
            for original in ids {
                // Spells only: copying an ability is a different rule (CR 707.10).
                let spell = state.objects.get(&original).is_some_and(|o| {
                    o.zone.zone == Zone::Stack
                        && o.cast_context.as_ref().is_none_or(|c| c.ability.is_none())
                });
                if !spell {
                    continue;
                }
                let copy = state.new_object_id();
                let targets = if *may_change_targets {
                    new_targets(state, cards, rc, original, copy)?
                } else {
                    None
                };
                apply::apply(
                    state,
                    cause,
                    Event::SpellCopied {
                        original,
                        copy,
                        controller: rc.controller,
                        targets,
                    },
                    log,
                );
            }
            Ok(())
        }
        Effect::CastWithoutPaying { what, from, haste } => {
            let ids = objects_asking(state, cards, rc, what)?;
            for id in ids {
                if state.objects.get(&id).is_some_and(|o| o.zone.zone == *from)
                    && cast_during_resolution(state, cards, log, rc, id, rc.controller, None)?
                    && *haste
                    && let Some(o) = state.objects_in(ZoneRef::shared(Zone::Stack)).first()
                    && let Some(o) = state.objects.get_mut(o)
                {
                    o.cast_context
                        .get_or_insert_with(Default::default)
                        .gains_haste = true;
                }
            }
            Ok(())
        }

        Effect::RevealHandChoose { who, filter, exile } => {
            let players = with_ctx(state, cards, rc, |ctx| eval::players(ctx, who))?;
            for p in players {
                let hand = state.objects_in(ZoneRef::of(Zone::Hand, p));
                let reveals: Vec<Event> = hand
                    .iter()
                    .map(|id| Event::Revealed { object: *id })
                    .collect();
                if !reveals.is_empty() {
                    apply::apply_simultaneous(state, cause, reveals, log);
                }
                let candidates = with_ctx(state, cards, rc, |ctx| {
                    let mut out = Vec::new();
                    for id in &hand {
                        if eval::matches(ctx, filter, *id)? {
                            out.push(*id);
                        }
                    }
                    Ok(out)
                })?;
                if candidates.is_empty() {
                    continue;
                }
                let chosen = ask_objects(rc, rc.controller, candidates, 1, 1, "choose a card")?;
                for id in chosen {
                    if *exile {
                        move_to(state, log, id, Zone::Exile, cause);
                    } else {
                        discard(state, cards, log, id, cause);
                    }
                }
            }
            Ok(())
        }

        Effect::Proliferate => {
            let who = rc.controller;
            let with_counters: Vec<ObjectId> = state
                .battlefield()
                .into_iter()
                .filter(|id| {
                    state
                        .objects
                        .get(id)
                        .is_some_and(|o| o.counters.values().any(|n| *n > 0))
                })
                .collect();
            let n = with_counters.len() as u32;
            let chosen = if n == 0 {
                Vec::new()
            } else {
                ask_objects(rc, who, with_counters, 0, n, "proliferate")?
            };
            let mut events = Vec::new();
            for id in chosen {
                if let Some(o) = state.objects.get(&id) {
                    for (kind, count) in &o.counters {
                        if *count > 0 {
                            events.push(Event::CountersChanged {
                                object: id,
                                kind: *kind,
                                delta: 1,
                            });
                        }
                    }
                }
            }
            // Players: poison is the only player counter the engine tracks.
            let poisoned: Vec<PlayerId> = state
                .players
                .values()
                .filter(|p| p.poison > 0 && !p.has_lost)
                .map(|p| p.id)
                .collect();
            for p in poisoned {
                if ask_confirm(rc, who, "give that player another poison counter?")? {
                    events.push(Event::Poisoned {
                        player: p,
                        amount: 1,
                    });
                }
            }
            if !events.is_empty() {
                apply::apply_simultaneous(state, cause, events, log);
            }
            Ok(())
        }

        Effect::Cascade => {
            let who = rc.controller;
            let value = state
                .objects
                .get(&rc.source)
                .or_else(|| state.last_known.get(&rc.source))
                .and_then(|o| cards.face(o.card, o.face))
                .map_or(0, |f| f.mana_cost.mana_value());
            let library = ZoneRef::of(Zone::Library, who);
            let mut exiled = Vec::new();
            let mut hit = None;
            while let Some(top) = state.objects_in(library).first().copied() {
                apply::apply(state, cause, Event::Revealed { object: top }, log);
                let Some(new_id) = move_to_at(state, log, top, Zone::Exile, None, cause) else {
                    break;
                };
                let found = state
                    .objects
                    .get(&new_id)
                    .and_then(|o| cards.face(o.card, o.face))
                    .is_some_and(|f| {
                        !f.card_types.contains(&mtg_core::CardType::Land)
                            && f.mana_cost.mana_value() < value
                    });
                if found {
                    hit = Some(new_id);
                    break;
                }
                exiled.push(new_id);
            }
            if let Some(card) = hit
                && !cast_during_resolution(state, cards, log, rc, card, who, None)?
            {
                exiled.push(card);
            }
            let count = exiled.len() as u32;
            for id in exiled {
                move_to_at(state, log, id, Zone::Library, Some(u32::MAX), cause);
            }
            if count > 1 {
                apply::apply(
                    state,
                    cause,
                    Event::LibraryBottomShuffled { player: who, count },
                    log,
                );
            }
            Ok(())
        }
        Effect::Native { key } => {
            let _ = key;
            Err(ResolveError::Unsupported("native ability"))
        }
    }
}

/// Cast a card while an effect resolves (CR 608.2g, 601): without paying its mana cost,
/// or for `cost` instead of it (madness).
/// The player chooses whether to; then modes, then targets, each through the ordinary
/// ask-and-replay path. `X` is 0 (CR 107.3b). A card that can't be cast — a land, one
/// with an additional cost to pay, one whose required targets can't be chosen, one of a
/// layout whose faces need their own choice — is not, and `false` says so.
fn cast_during_resolution(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    rc: &mut ResolveCtx,
    card: ObjectId,
    who: PlayerId,
    cost: Option<&mtg_core::ManaCost>,
) -> Result<bool, ResolveError> {
    let Some(obj) = state.objects.get(&card) else {
        return Ok(false);
    };
    if cards.layout(obj.card) != mtg_ir::Layout::Normal {
        return Ok(false);
    }
    let Some(face) = cards.face(obj.card, obj.face) else {
        return Ok(false);
    };
    if face.card_types.contains(&mtg_core::CardType::Land)
        || crate::cost::additional_cast_cost(face).is_some()
        || crate::cost::cast_forbidden(state, cards, card, who)
    {
        return Ok(false);
    }
    let name = face.name.clone();
    let spell = face.abilities.iter().find(|a| {
        matches!(
            a.kind,
            mtg_ir::AbilityKind::SpellEffect(_) | mtg_ir::AbilityKind::Enchant
        )
    });
    let specs = spell.map(|a| a.targets.clone()).unwrap_or_default();
    let modal = match spell.map(|a| &a.kind) {
        Some(mtg_ir::AbilityKind::SpellEffect(Effect::Modal {
            choose: mtg_ir::Value::Fixed(n),
            modes,
            at_least,
        })) => {
            let most = (*n).clamp(0, modes.len() as i32) as u8;
            Some((
                modes.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>(),
                most,
                at_least.unwrap_or(most).min(most),
            ))
        }
        _ => None,
    };
    // Every required target must be choosable before the question is even asked.
    let slots: Vec<Vec<Target>> = specs
        .iter()
        .map(|s| crate::targeting::legal_targets(state, cards, s, card, who, &[]))
        .collect();
    if specs
        .iter()
        .zip(&slots)
        .any(|(s, legal)| s.mode.is_none() && !s.up_to && legal.is_empty())
    {
        return Ok(false);
    }
    if let Some(cost) = cost
        && crate::mana::plan_spell(state, cards, who, cost, 0, card).is_none()
    {
        return Ok(false);
    }
    let question = match cost {
        Some(_) => format!("cast {name} for its madness cost?"),
        None => format!("cast {name} without paying its mana cost?"),
    };
    if !ask_confirm(rc, who, &question)? {
        return Ok(false);
    }
    let modes = match modal {
        Some((labels, count, min)) => ask_modes(rc, who, labels, count, min)?,
        None => Vec::new(),
    };
    // Only the chosen modes' slots are filled; others are placeholders (CR 700.2b).
    let wanted: Vec<bool> = specs
        .iter()
        .map(|s| s.mode.is_none_or(|m| modes.contains(&m)))
        .collect();
    let mut targets = Vec::new();
    let mut empty = Vec::new();
    if wanted.iter().any(|w| *w) {
        let asked: Vec<Vec<Target>> = slots
            .iter()
            .zip(&wanted)
            .map(|(s, w)| if *w { s.clone() } else { Vec::new() })
            .collect();
        let answer = rc.need(
            who,
            crate::choice::ChoiceKind::ChooseTargets {
                optional: specs
                    .iter()
                    .zip(&wanted)
                    .map(|(spec, wanted)| !*wanted || spec.up_to)
                    .collect(),
                slots: asked.clone(),
            },
            "choose targets",
        )?;
        let Answer::Targets(picked) = answer else {
            return Ok(false);
        };
        for (i, legal) in asked.iter().enumerate() {
            match picked
                .get(i)
                .and_then(|p| p.first())
                .filter(|t| legal.contains(t))
            {
                Some(t) => targets.push(*t),
                None if !wanted[i] || specs[i].up_to => {
                    // Filled in with the spell itself once it has an id on the stack.
                    empty.push(i as u8);
                    targets.push(Target::Object(card));
                }
                None => return Ok(false),
            }
        }
    }
    if let Some(cost) = cost {
        // Paying for a spell: mana restricted to some spells is checked against this one.
        let Some(plan) = crate::mana::plan_spell(state, cards, who, cost, 0, card) else {
            return Ok(false);
        };
        pay_plan(state, cards, log, who, &plan);
    }
    let Some(on_stack) = move_to_at(
        state,
        log,
        card,
        Zone::Stack,
        Some(0),
        Cause::PlayerAction(who),
    ) else {
        return Ok(false);
    };
    // A placeholder names the spell itself, which is never a legal target of itself and
    // so selects nothing (as in an ordinary announcement).
    for i in &empty {
        targets[*i as usize] = Target::Object(on_stack);
    }
    if let Some(o) = state.objects.get_mut(&on_stack) {
        o.controller = who;
        let cc = o.cast_context.get_or_insert_with(Default::default);
        cc.targets = targets.clone();
        cc.empty_slots = empty.clone();
        cc.modes = modes;
        cc.x = 0;
    }
    let mut targeted: Vec<Event> = Vec::new();
    for (i, t) in targets.iter().enumerate() {
        let e = Event::Targeted {
            object: on_stack,
            target: *t,
        };
        if !empty.contains(&(i as u8)) && !targeted.contains(&e) {
            targeted.push(e);
        }
    }
    if !targeted.is_empty() {
        apply::apply_simultaneous(state, Cause::PlayerAction(who), targeted, log);
    }
    count_noncreature_spell(state, cards, on_stack, who);
    apply::apply(
        state,
        Cause::PlayerAction(who),
        Event::SpellCast {
            object: on_stack,
            controller: who,
        },
        log,
    );
    Ok(true)
}

/// Count a spell being cast toward "their first noncreature spell each turn". Counted
/// just before the cast event, so a trigger on that event sees the spell included.
pub(crate) fn count_noncreature_spell(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    spell: ObjectId,
    caster: PlayerId,
) {
    if crate::layers::compute(state, cards, spell)
        .is_some_and(|c| !c.has_type(mtg_core::CardType::Creature))
    {
        *state
            .noncreature_spells_by_player
            .entry(caster)
            .or_insert(0) += 1;
    }
}

/// CR 707.10c — "you may choose new targets for the copy": the copier picks, slot by slot,
/// among what is legal now, the original's targets included. `None` keeps them.
fn new_targets(
    state: &GameState,
    cards: &dyn PrintedCards,
    rc: &mut ResolveCtx,
    original: ObjectId,
    copy: ObjectId,
) -> Result<Option<Vec<Target>>, ResolveError> {
    // Evaluate the copy's prospective identity and ownership without putting it on
    // the real stack before targets have been decided (CR 707.10c, 115.5).
    let mut preview = state.clone();
    apply::apply(
        &mut preview,
        Cause::Resolution(rc.source),
        Event::SpellCopied {
            original,
            copy,
            controller: rc.controller,
            targets: None,
        },
        &mut Vec::new(),
    );
    let controller = rc.controller;
    targets_again(
        state,
        &preview,
        cards,
        rc,
        original,
        (copy, controller),
        ("choose new targets for the copy", false),
    )
}

/// Let the resolving ability's controller choose new targets for `original`'s announced
/// target slots (each may keep its current target). Legality is judged in `judge` for
/// `subject` controlled by its controller: a copy not yet on the stack, or the spell
/// itself. `None` when it has no targets.
fn targets_again(
    state: &GameState,
    judge: &GameState,
    cards: &dyn PrintedCards,
    rc: &mut ResolveCtx,
    original: ObjectId,
    (subject, subject_controller): (ObjectId, PlayerId),
    (because, must_change): (&str, bool),
) -> Result<Option<Vec<Target>>, ResolveError> {
    let Some(cast) = state
        .objects
        .get(&original)
        .and_then(|o| o.cast_context.as_ref())
    else {
        return Ok(None);
    };
    let frozen = cast.target_specs.clone().unwrap_or_else(|| {
        let specs = crate::targeting::specs_of(state, cards, original);
        (0..cast.targets.len())
            .filter_map(|i| specs.get(i).or_else(|| specs.last()).cloned())
            .collect()
    });
    if frozen.len() != cast.targets.len() {
        return Err(ResolveError::Unsupported(
            "missing copied target specifications",
        ));
    }
    let active: Vec<_> = (0..cast.targets.len())
        .filter(|i| !cast.empty_slots.contains(&(*i as u8)))
        .collect();
    if active.is_empty() {
        return Ok(None);
    }
    let current: Vec<_> = active.iter().map(|i| cast.targets[*i]).collect();
    let specs: Vec<_> = active.iter().map(|i| frozen[*i].clone()).collect();
    let groups = active
        .iter()
        .map(|i| {
            cast.target_groups
                .as_ref()
                .and_then(|g| g.get(*i))
                .copied()
                .unwrap_or(*i)
        })
        .collect();
    let slots: Vec<Vec<Target>> = specs
        .iter()
        .zip(&current)
        .map(|(spec, keep)| {
            let mut legal = crate::targeting::legal_targets(
                judge,
                cards,
                spec,
                subject,
                subject_controller,
                &[],
            );
            if !legal.contains(keep) {
                legal.insert(0, *keep);
            }
            // CR 115.7a — "change the target": to another one, when there is one.
            if must_change && legal.len() > 1 {
                legal.retain(|t| t != keep);
            }
            legal
        })
        .collect();
    let check = Retargeting {
        specs,
        groups,
        current,
    };
    let kind = ChoiceKind::ChooseTargets {
        optional: vec![false; slots.len()],
        slots,
    };
    let answer = rc
        .need(rc.controller, kind.clone(), because)
        .map_err(|error| match error {
            ResolveError::Ask {
                who, kind, because, ..
            } => ResolveError::Ask {
                who,
                kind,
                because,
                retargeting: Some(check.clone()),
            },
            other => other,
        })?;
    if !check.accepts(&kind, &answer) {
        return Err(ResolveError::Unsupported("invalid copied target answer"));
    }
    let Answer::Targets(picked) = answer else {
        unreachable!()
    };
    let mut chosen = cast.targets.clone();
    for (index, picked) in active.iter().zip(picked) {
        chosen[*index] = picked[0];
    }
    Ok(Some(chosen))
}

/// What one mana output actually produces, given the controller's choice.
fn resolve_output(out: &ManaOutput, rc: &ResolveCtx) -> (Option<mtg_core::Color>, u16) {
    match out {
        ManaOutput::Colorless => (None, 1),
        ManaOutput::Colored(c) => (Some(*c), 1),
        // A single option needs no decision; several fall back to the first so a
        // missing choice produces *some* mana rather than silently none.
        // Replaced by the source's chosen color before this is reached; none chosen,
        // none made.
        ManaOutput::ChosenColor
        | ManaOutput::CommanderIdentity
        | ManaOutput::ExiledCardColors
        | ManaOutput::ColorsAmong(_)
        | ManaOutput::LandColors(_)
        | ManaOutput::EachColorAmong(_) => (None, 0),
        // Made concrete by its source first; unchosen, its own color.
        ManaOutput::OrChosen(c) => (Some(*c), 1),
        ManaOutput::AnyOf(cs) => {
            let picked = rc
                .mana_choice
                .filter(|c| cs.contains(c))
                .or_else(|| cs.first().copied());
            (picked, 1)
        }
        ManaOutput::Repeated { amount, output } => {
            let (c, _) = resolve_output(output, rc);
            let n = match amount {
                mtg_ir::Value::Fixed(n) => (*n).max(0) as u16,
                _ => 1,
            };
            (c, n)
        }
    }
}

/// Counter a spell or ability on the stack (CR 701.5): gone already, or impossible to
/// counter, and nothing happens.
fn counter_object(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    id: ObjectId,
    exile_instead: bool,
    cause: Cause,
) {
    // The target may already have left the stack — resolved, or countered by
    // something else that resolved first. Countering it then does nothing,
    // which is not a failure.
    let Some(obj) = state.objects.get(&id) else {
        return;
    };
    if obj.zone.zone != Zone::Stack {
        return;
    }
    if cant_be_countered(state, cards, id) {
        return;
    }

    let is_ability = obj
        .cast_context
        .as_ref()
        .is_some_and(|c| c.ability.is_some());
    let owner = obj.owner;
    let exile = exile_instead || obj.cast_context.as_ref().is_some_and(|c| c.exile_on_leave);

    apply::apply(state, cause, Event::Countered { object: id }, log);

    // CR 701.5a — a countered *spell* goes to its owner's graveyard. A
    // countered ability is not a card and simply ceases to exist; exile
    // stands in for "nowhere", as it does for a resolved ability.
    let to = if is_ability || exile {
        ZoneRef::shared(Zone::Exile)
    } else {
        ZoneRef::of(Zone::Graveyard, owner)
    };
    let new_object = state.new_object_id();
    apply::apply(
        state,
        cause,
        Event::ZoneChange {
            object: id,
            new_object,
            from: ZoneRef::shared(Zone::Stack),
            to,
            index: None,
        },
        log,
    );
}

/// Whether a continuous effect makes this object impossible to counter.
fn cant_be_countered(state: &GameState, cards: &dyn PrintedCards, id: ObjectId) -> bool {
    use mtg_ir::effect::{Modification, Restriction};
    crate::layers::effects(state, cards).iter().any(|e| {
        matches!(
            &e.modification,
            Modification::Restriction(Restriction::CantBeCountered)
        ) && crate::layers::applies(state, cards, e, id)
    })
}

/// Ask a yes/no question.
fn ask_confirm(rc: &mut ResolveCtx, who: PlayerId, because: &str) -> Result<bool, ResolveError> {
    match rc.need(who, ChoiceKind::Confirm, because)? {
        Answer::Bool(b) => Ok(b),
        // A wrongly-shaped answer declines rather than guessing yes: declining is
        // always legal for an optional effect.
        _ => Ok(false),
    }
}

/// The objects a selector picks out, asking when the selector is a choice: "search your
/// library for a basic land card" is the searching player choosing among the matching cards
/// in their library (CR 701.19). Anything else evaluates as usual.
fn objects_asking(
    state: &GameState,
    cards: &dyn PrintedCards,
    rc: &mut ResolveCtx,
    sel: &Selector,
) -> Result<Vec<ObjectId>, ResolveError> {
    let Selector::ChosenBy {
        chooser,
        zone,
        filter,
        count,
        up_to,
    } = sel
    else {
        return with_ctx(state, cards, rc, |ctx| eval::objects(ctx, sel));
    };
    let (who, candidates, n) = with_ctx(state, cards, rc, |ctx| {
        let who = eval::players(ctx, chooser)?.first().copied();
        let Some(who) = who else {
            return Ok((None, Vec::new(), 0));
        };
        let place = if zone.is_shared() {
            ZoneRef::shared(*zone)
        } else {
            ZoneRef::of(*zone, who)
        };
        let mut out = Vec::new();
        for id in ctx.state.objects_in(place) {
            if eval::matches(ctx, filter, id)? {
                out.push(id);
            }
        }
        Ok((Some(who), out, eval::value(ctx, count)?.max(0) as u32))
    })?;
    let Some(who) = who else {
        return Ok(Vec::new());
    };
    let max = n.min(candidates.len() as u32);
    if max == 0 {
        return Ok(Vec::new());
    }
    let min = if *up_to { 0 } else { max };
    ask_objects(rc, who, candidates, min, max, "choose")
}

/// Ask a player to put objects in an order, as a series of single picks: the first pick
/// comes first. The last one needs no question.
fn ask_order(
    rc: &mut ResolveCtx,
    who: PlayerId,
    mut from: Vec<ObjectId>,
    because: &str,
) -> Result<Vec<ObjectId>, ResolveError> {
    let mut out = Vec::with_capacity(from.len());
    while from.len() > 1 {
        let pick = ask_objects(rc, who, from.clone(), 1, 1, because)?;
        let Some(first) = pick.first().copied().filter(|p| from.contains(p)) else {
            // A malformed answer takes the remaining order as it stands.
            break;
        };
        from.retain(|o| *o != first);
        out.push(first);
    }
    out.extend(from);
    Ok(out)
}

/// Ask which of `from` to pick, between `min` and `max` of them.
fn ask_objects(
    rc: &mut ResolveCtx,
    who: PlayerId,
    from: Vec<ObjectId>,
    min: u32,
    max: u32,
    because: &str,
) -> Result<Vec<ObjectId>, ResolveError> {
    let kind = ChoiceKind::ChooseObjects {
        from: from.clone(),
        min,
        max,
    };
    match rc.need(who, kind, because)? {
        Answer::Objects(picked) => {
            // Only offered objects count, and never more than asked for. An answer
            // that under-delivers on a mandatory choice is topped up in order, so a
            // malformed answer cannot skip a mandatory discard.
            let mut out = Vec::new();
            for object in picked {
                if out.len() >= max as usize {
                    break;
                }
                if from.contains(&object) && !out.contains(&object) {
                    out.push(object);
                }
            }
            for candidate in &from {
                if out.len() as u32 >= min {
                    break;
                }
                if !out.contains(candidate) {
                    out.push(*candidate);
                }
            }
            Ok(out)
        }
        _ => Ok(from.into_iter().take(min as usize).collect()),
    }
}

/// Ask which modes to choose.
fn ask_modes(
    rc: &mut ResolveCtx,
    who: PlayerId,
    available: Vec<Box<str>>,
    count: u8,
    min: u8,
) -> Result<Vec<u8>, ResolveError> {
    let n = available.len();
    let kind = ChoiceKind::ChooseModes {
        available,
        count,
        min: (min != count).then_some(min),
    };
    match rc.need(who, kind, "choose a mode")? {
        Answer::Modes(picked) => {
            let mut out: Vec<u8> = picked
                .into_iter()
                .filter(|i| (*i as usize) < n)
                .take(count as usize)
                .collect();
            out.dedup();
            // Modal effects are not optional: top up in order if under the fewest allowed.
            for i in 0..n as u8 {
                if out.len() >= min as usize {
                    break;
                }
                if !out.contains(&i) {
                    out.push(i);
                }
            }
            Ok(out)
        }
        _ => Ok((0..count.min(n as u8)).collect()),
    }
}

/// Evaluate a value, asking the controller when the value *is* a question.
///
/// `Value::ChosenByController` cannot be handled inside the evaluator — evaluation is
/// pure and synchronous by design — so it is intercepted here, where asking is
/// possible.
fn value_asking(
    state: &GameState,
    cards: &dyn PrintedCards,
    rc: &mut ResolveCtx,
    v: &mtg_ir::Value,
) -> Result<i32, ResolveError> {
    if let mtg_ir::Value::ChosenByController { min, max } = v {
        let lo = with_ctx(state, cards, rc, |ctx| eval::value(ctx, min))?.max(0) as u32;
        let hi = with_ctx(state, cards, rc, |ctx| eval::value(ctx, max))?.max(0) as u32;
        let who = rc.controller;
        return Ok(ask_number(rc, who, lo, hi.max(lo))? as i32);
    }
    with_ctx(state, cards, rc, |ctx| eval::value(ctx, v))
}

/// Ask for a number within bounds.
fn ask_number(rc: &mut ResolveCtx, who: PlayerId, min: u32, max: u32) -> Result<u32, ResolveError> {
    match rc.need(who, ChoiceKind::ChooseX { min, max }, "choose a number")? {
        Answer::Number(n) => Ok(n.clamp(min, max)),
        _ => Ok(min),
    }
}

/// Pay a planned mana cost during resolution.
///
/// A trimmed version of the engine's payment path: mana abilities do not use the stack
/// (CR 605.3), so they can be resolved inline here.
fn pay_plan(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    who: PlayerId,
    plan: &crate::mana::Payment,
) {
    for (object, ability, color) in &plan.activate {
        let taps = crate::abilities::find(state, cards, *object, *ability)
            .map(|a| match &a.kind {
                mtg_ir::AbilityKind::Activated { cost, .. } => cost
                    .additional
                    .iter()
                    .any(|c| matches!(c, mtg_ir::AdditionalCost::Tap { .. })),
                _ => false,
            })
            .unwrap_or(false);
        if taps {
            apply::apply(
                state,
                Cause::CostPayment(*object),
                Event::TapChanged {
                    object: *object,
                    tapped: true,
                },
                log,
            );
        }
        // The effect is read before a sacrifice cost moves the source away.
        let effect_now =
            crate::abilities::find(state, cards, *object, *ability).and_then(|a| match &a.kind {
                mtg_ir::AbilityKind::Activated { effect, .. } => Some(effect.clone()),
                _ => None,
            });
        sacrifice_for_mana(state, cards, log, *object, *ability);
        let effect = effect_now.or_else(|| {
            crate::abilities::find(state, cards, *object, *ability).and_then(|a| match &a.kind {
                mtg_ir::AbilityKind::Activated { effect, .. } => Some(effect.clone()),
                _ => None,
            })
        });
        if let Some(effect) = effect {
            let mut inner = ResolveCtx::new(*object, who);
            inner.mana_choice = *color;
            let _ = resolve(state, cards, log, &effect, &mut inner);
        }
    }

    crate::mana::consume_restricted(state, who, plan);
    for (slot, amount) in plan.spend.iter().enumerate() {
        if *amount == 0 {
            continue;
        }
        let color = match slot {
            0 => Some(mtg_core::Color::White),
            1 => Some(mtg_core::Color::Blue),
            2 => Some(mtg_core::Color::Black),
            3 => Some(mtg_core::Color::Red),
            4 => Some(mtg_core::Color::Green),
            _ => None,
        };
        apply::apply(
            state,
            Cause::CostPayment(ObjectId(0)),
            Event::ManaSpent {
                player: who,
                color,
                amount: *amount,
            },
            log,
        );
    }

    if plan.life > 0 {
        apply::apply(
            state,
            Cause::CostPayment(ObjectId(0)),
            Event::LifeChanged {
                player: who,
                delta: -(plan.life as i32),
            },
            log,
        );
    }
}

/// The CR 613 layer a modification belongs to.
pub(crate) fn layer_of(m: &mtg_ir::effect::Modification) -> u8 {
    use crate::layers::layer;
    use mtg_ir::effect::Modification as M;
    match m {
        M::CopyOf(_) => layer::COPY,
        M::Control(_) => layer::CONTROL,
        M::ChangeText { .. } => layer::TEXT,
        M::AddTypes(_)
        | M::RemoveTypes(_)
        | M::SetTypes(_)
        | M::AddSubtypes(_)
        | M::SetCreatureTypes(_)
        | M::RemoveSupertype(_) => layer::TYPE,
        M::BecomesChosen(mtg_ir::effect::EntryChoice::CreatureType) => layer::TYPE,
        M::BecomesChosen(mtg_ir::effect::EntryChoice::Color) => layer::COLOR,
        M::NoManaCost => layer::COPY,
        M::AddColors(_) | M::SetColors(_) => layer::COLOR,
        M::GrantAbility(_) | M::LoseAllAbilities | M::LoseKeyword(_) => layer::ABILITY,
        M::SetBasePowerToughness { .. } | M::SetBasePower(_) => layer::PT_SET,
        M::ModifyPowerToughness { .. } => layer::PT_MODIFY,
        M::SwitchPowerToughness => layer::PT_SWITCH,
        // Restrictions apply outside the characteristic layers; parked at the
        // ability layer so timestamp ordering among them still works.
        M::Restriction(_) => layer::ABILITY,
    }
}

/// Run a closure against a fresh evaluation context, releasing the borrow before
/// the caller mutates state.
/// One damage record carries every consequence, so triggers and lifelink count
/// damage once even when the recipient is both a creature and a planeswalker.
pub(crate) fn object_damage_event(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    object: ObjectId,
    amount: u32,
    deathtouch: bool,
    counters: bool,
) -> Event {
    use mtg_core::{CardType, ObjectDamageKind};
    let ch = crate::layers::compute(state, cards, object);
    let is = |t| ch.as_ref().is_some_and(|c| c.has_type(t));
    let recipient = match (is(CardType::Creature), is(CardType::Planeswalker)) {
        (true, true) => ObjectDamageKind::CreaturePlaneswalker,
        (false, true) => ObjectDamageKind::Planeswalker,
        _ => ObjectDamageKind::Creature,
    };
    Event::DamageMarked {
        source,
        object,
        amount,
        recipient,
        deathtouch,
        counters,
    }
}

fn with_ctx<T>(
    state: &GameState,
    cards: &dyn PrintedCards,
    rc: &ResolveCtx,
    f: impl FnOnce(&Ctx) -> Result<T, EvalError>,
) -> Result<T, ResolveError> {
    let chars = ComputedChars(cards);
    let ctx = Ctx {
        state,
        cards,
        chars: &chars,
        source: rc.source,
        controller: rc.controller,
        targets: &rc.targets,
        target_legal: &rc.target_legal,
        x: rc.x,
        bindings: &rc.bindings,
    };
    f(&ctx).map_err(ResolveError::Eval)
}

/// Objects *and* players a selector denotes, as targets.
fn targets_of(ctx: &Ctx, sel: &Selector) -> Result<Vec<Target>, EvalError> {
    // Player evaluation deliberately interprets an object selector as its
    // controllers for player-only effects. Damage recipients must preserve the
    // selector's explicit type instead of adding those implicit controllers.
    match sel {
        Selector::Union(parts) => {
            let mut out = Vec::new();
            for part in parts {
                for target in targets_of(ctx, part)? {
                    if !out.contains(&target) {
                        out.push(target);
                    }
                }
            }
            return Ok(out);
        }
        Selector::Except(base, minus) => {
            let excluded = targets_of(ctx, minus)?;
            return Ok(targets_of(ctx, base)?
                .into_iter()
                .filter(|target| !excluded.contains(target))
                .collect());
        }
        Selector::SelfSource | Selector::All { .. } | Selector::TopOfLibrary { .. } => {
            return Ok(eval::objects(ctx, sel)?
                .into_iter()
                .map(Target::Object)
                .collect());
        }
        _ => {}
    }
    let mut out: Vec<Target> = eval::objects(ctx, sel)?
        .into_iter()
        .map(Target::Object)
        .collect();
    out.extend(eval::players(ctx, sel)?.into_iter().map(Target::Player));
    Ok(out)
}

/// One source deals damage to several recipients at once, each its own amount (CR 120.2):
/// protection and prevention apply per recipient, and lifelink gains the total.
fn deal_damage(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    rc: &ResolveCtx,
    cause: Cause,
    src: ObjectId,
    shares: &[(Target, u32)],
) -> Result<(), ResolveError> {
    let (deathtouch, lifelink, wither, infect) = with_ctx(state, cards, rc, |ctx| {
        Ok((
            ctx.has_keyword(src, Keyword::Deathtouch).unwrap_or(false),
            ctx.has_keyword(src, Keyword::Lifelink).unwrap_or(false),
            ctx.has_keyword(src, Keyword::Wither).unwrap_or(false),
            ctx.has_keyword(src, Keyword::Infect).unwrap_or(false),
        ))
    })?;
    let mut events: Vec<Event> = Vec::new();
    let mut dealt = 0i32;
    for (t, n) in shares {
        let t = *t;
        if *n == 0 {
            continue;
        }
        let amount = crate::prevention::prevent(state, cards, src, t, *n, false, &mut events);
        if amount == 0 {
            continue;
        }
        dealt += amount as i32;
        match t {
            Target::Object(o) => {
                events.push(object_damage_event(
                    state,
                    cards,
                    src,
                    o,
                    amount,
                    deathtouch,
                    wither || infect,
                ));
            }
            Target::Player(p) => {
                events.push(Event::DamageDealtToPlayer {
                    source: src,
                    player: p,
                    amount,
                    counters: infect,
                });
            }
        }
    }
    // CR 702.15b — lifelink gains the life in the same event as the damage.
    if lifelink
        && dealt > 0
        && let Some(controller) = crate::layers::controller(state, src)
    {
        events.push(Event::LifeChanged {
            player: controller,
            delta: dealt,
        });
    }
    apply::apply_simultaneous(state, cause, events, log);
    Ok(())
}

/// CR 702.52 — dredge: before a draw, the drawing player may instead mill N and return a
/// card with dredge N from their graveyard to their hand, if their library holds at least
/// N cards. Whether the draw was replaced.
fn dredge(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    rc: &mut ResolveCtx,
    player: PlayerId,
    cause: Cause,
) -> Result<bool, ResolveError> {
    let library = state.objects_in(ZoneRef::of(Zone::Library, player));
    let dredgers: Vec<(ObjectId, usize)> = state
        .objects_in(ZoneRef::of(Zone::Graveyard, player))
        .into_iter()
        .filter_map(|id| {
            let o = state.objects.get(&id)?;
            cards
                .face(o.card, o.face)?
                .abilities
                .iter()
                .find_map(|a| match a.kind {
                    mtg_ir::AbilityKind::Dredge(n) if library.len() >= usize::from(n) => {
                        Some((id, usize::from(n)))
                    }
                    _ => None,
                })
        })
        .collect();
    if dredgers.is_empty() {
        return Ok(false);
    }
    let from = dredgers.iter().map(|(id, _)| *id).collect();
    let chosen = ask_objects(rc, player, from, 0, 1, "dredge instead of drawing")?;
    let Some((card, n)) = chosen
        .first()
        .and_then(|c| dredgers.iter().find(|(id, _)| id == c))
        .copied()
    else {
        return Ok(false);
    };
    for id in library.into_iter().take(n) {
        move_to(state, log, id, Zone::Graveyard, cause);
    }
    move_to(state, log, card, Zone::Hand, cause);
    Ok(true)
}

/// Draw one card, or record the failed attempt (CR 121.3, CR 704.5b).
fn draw_one(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    rc: &mut ResolveCtx,
    player: PlayerId,
    cause: Cause,
) -> Result<(), ResolveError> {
    if dredge(state, cards, log, rc, player, cause)? {
        return Ok(());
    }
    let library = ZoneRef::of(Zone::Library, player);
    let Some(top) = state.objects_in(library).first().copied() else {
        // Drawing from an empty library does not lose the game immediately; the
        // attempt is recorded and state-based actions do the rest.
        apply::apply(
            state,
            cause,
            Event::AttemptedDrawFromEmptyLibrary { player },
            log,
        );
        return Ok(());
    };
    let new_id = state.new_object_id();
    apply::apply(
        state,
        cause,
        Event::ZoneChange {
            object: top,
            new_object: new_id,
            from: library,
            to: ZoneRef::of(Zone::Hand, player),
            index: None,
        },
        log,
    );
    apply::apply(
        state,
        cause,
        Event::Drew {
            player,
            object: new_id,
        },
        log,
    );
    Ok(())
}

/// Move a permanent to one of its owner's zones.
fn move_to(
    state: &mut GameState,
    log: &mut Vec<StampedEvent>,
    id: ObjectId,
    to: Zone,
    cause: Cause,
) -> Option<ObjectId> {
    move_to_at(state, log, id, to, None, cause)
}

/// Move an object to one of its owner's zones, at a position in an ordered zone
/// (`None` is the top). Returns the identity it has there (CR 400.7).
fn move_to_at(
    state: &mut GameState,
    log: &mut Vec<StampedEvent>,
    id: ObjectId,
    to: Zone,
    index: Option<u32>,
    cause: Cause,
) -> Option<ObjectId> {
    let obj = state.objects.get(&id)?;
    let (from, owner) = (obj.zone, obj.owner);
    // Flashback and aftermath replace every departure from the stack, including
    // effects that would return the spell to a hand or put it into a library.
    let exile = from.zone == Zone::Stack
        && to != Zone::Stack
        && obj.cast_context.as_ref().is_some_and(|c| c.exile_on_leave);
    let to = if exile { Zone::Exile } else { to };
    let index = if exile { None } else { index };
    let dest = if to.is_shared() {
        ZoneRef::shared(to)
    } else {
        ZoneRef::of(to, owner)
    };
    let new_id = state.new_object_id();
    apply::apply(
        state,
        cause,
        Event::ZoneChange {
            object: id,
            new_object: new_id,
            from,
            to: dest,
            index,
        },
        log,
    );
    Some(new_id)
}

/// What destroying a permanent actually does (CR 701.7): nothing if it is indestructible
/// (CR 702.12b); otherwise a replacement if one applies — a shield counter (CR 122.1c),
/// umbra armor (CR 702.89a), a regeneration shield (CR 701.15) — and otherwise its owner's
/// graveyard. The one path for destroy effects and for lethal damage alike. Empty means
/// nothing happens. A zone change comes back with `new_object` unset; the caller
/// allocates it.
pub(crate) fn destruction(state: &GameState, cards: &dyn PrintedCards, id: ObjectId) -> Vec<Event> {
    let Some(obj) = state.objects.get(&id) else {
        return Vec::new();
    };
    if obj.zone.zone != Zone::Battlefield {
        return Vec::new();
    }
    let has = |id: ObjectId, k: Keyword| {
        let controller = state
            .objects
            .get(&id)
            .map_or(obj.controller, |o| o.controller);
        with_ctx(state, cards, &ResolveCtx::new(id, controller), |ctx| {
            Ok(ctx.has_keyword(id, k).unwrap_or(false))
        })
        .unwrap_or(false)
    };
    let indestructible = has(id, Keyword::Indestructible)
        || crate::layers::restricted(state, cards, id, |r| {
            matches!(r, mtg_ir::effect::Restriction::Indestructible)
        });
    if indestructible {
        return Vec::new();
    }
    // CR 122.1c: a shield counter is spent instead.
    if obj
        .counters
        .get(&mtg_core::CounterKind::Shield)
        .is_some_and(|n| *n > 0)
    {
        return vec![Event::CountersChanged {
            object: id,
            kind: mtg_core::CounterKind::Shield,
            delta: -1,
        }];
    }
    // CR 702.89a: an Aura with umbra armor on it is destroyed instead, and the damage
    // goes. Destroying the Aura is itself a destruction, so an indestructible one stays.
    if let Some(aura) = state.battlefield().into_iter().find(|a| {
        state
            .objects
            .get(a)
            .is_some_and(|o| o.attached_to == Some(id))
            && has(*a, Keyword::UmbraArmor)
    }) {
        let mut events = vec![Event::DamageRemoved { object: id }];
        events.extend(destruction(state, cards, aura));
        return events;
    }
    if obj.regeneration_shields > 0
        && !crate::layers::restricted(state, cards, id, |r| {
            matches!(r, mtg_ir::effect::Restriction::CantBeRegenerated)
        })
    {
        return vec![Event::Regenerated { object: id }];
    }
    vec![Event::ZoneChange {
        object: id,
        new_object: ObjectId(0),
        from: obj.zone,
        to: ZoneRef::of(Zone::Graveyard, obj.owner),
        index: None,
    }]
}

/// A mana ability's "Sacrifice this …" cost (a Treasure), paid before it makes mana.
pub(crate) fn sacrifice_for_mana(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    object: ObjectId,
    ability: AbilityId,
) {
    let sacrifices =
        crate::abilities::find(state, cards, object, ability).is_some_and(|a| match &a.kind {
            mtg_ir::AbilityKind::Activated { cost, .. } => cost.additional.iter().any(|c| {
                matches!(
                    c,
                    mtg_ir::AdditionalCost::Sacrifice {
                        what: Selector::SelfSource,
                        ..
                    }
                )
            }),
            _ => false,
        });
    if sacrifices {
        let player = state.objects.get(&object).map(|o| o.controller);
        move_to(
            state,
            log,
            object,
            Zone::Graveyard,
            Cause::CostPayment(object),
        );
        if let Some(player) = player {
            apply::apply(
                state,
                Cause::CostPayment(object),
                Event::Sacrificed { player, object },
                log,
            );
        }
    }
}

/// Everything that happens *as* a permanent enters the battlefield: its own "enters
/// tapped" and "enters with counters" replacement effects (CR 614.1c), and a
/// planeswalker's printed loyalty (CR 306.5b).
///
/// Called at every point an object arrives on the battlefield, immediately after the
/// zone change and before anything can observe it, so the permanent is never seen
/// untapped or without its counters.
pub(crate) fn entered(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    id: ObjectId,
    cause: Cause,
    answer: Option<Answer>,
) {
    let yes = matches!(answer, Some(Answer::Bool(true)));
    use mtg_ir::effect::ReplacementKind as R;
    let Some(obj) = state.objects.get(&id) else {
        return;
    };
    if obj.zone.zone != Zone::Battlefield {
        return;
    }
    let Some(face) = cards.face(obj.card, obj.face) else {
        return;
    };
    let mut events = Vec::new();
    let mut haste = false;
    let mut devoured: Option<(u8, Vec<ObjectId>)> = None;
    // Face-down objects have no own abilities, but other permanents' replacement
    // effects still apply to them (CR 708.2 and 614.12).
    if !obj.face_down {
        // "You control enchanted creature" (layer 2), recorded on the permanent.
        if face.abilities.iter().any(|a| {
            matches!(
                &a.kind,
                mtg_ir::AbilityKind::Static {
                    what: Selector::All { filter, .. },
                    modification: mtg_ir::effect::Modification::Control(Selector::You),
                    condition: None,
                } if filter_is_attached(filter)
            )
        }) {
            events.push(Event::ControlsHost { object: id });
        }
        // CR 726.2 — a daybound permanent arriving with neither day nor night makes it day;
        // at night it enters with its night face up (CR 702.145c).
        if day_bound(cards, obj) {
            match state.day {
                None => events.push(Event::DayNight { day: true }),
                Some(false) if obj.face == 0 => events.push(Event::Transformed {
                    object: id,
                    face: 1,
                }),
                _ => {}
            }
        }
        // CR 714.3a — a Saga enters with a lore counter.
        if let Some(lore) = face.abilities.iter().find_map(|a| match a.kind {
            mtg_ir::AbilityKind::Saga { lore, .. } => Some(lore),
            _ => None,
        }) {
            events.push(Event::CountersChanged {
                object: id,
                kind: lore,
                delta: 1,
            });
        }
        if let Some(loyalty) = face.loyalty.filter(|n| *n > 0)
            && face.card_types.contains(&mtg_core::CardType::Planeswalker)
        {
            events.push(Event::CountersChanged {
                object: id,
                kind: mtg_core::CounterKind::Loyalty,
                delta: loyalty,
            });
        }
        for ability in &face.abilities {
            let mtg_ir::AbilityKind::ReplacementEffect(r) = &ability.kind else {
                continue;
            };
            if !matches!(
                &r.matches,
                mtg_ir::EventPattern::Enters {
                    who: mtg_ir::ObjectFilter::IsSelf
                }
            ) {
                continue;
            }
            match &r.kind {
                R::EntersTapped => events.push(Event::EnteredTapped { object: id }),
                R::EntersTappedUnless { condition } => {
                    // Checked as it enters, from its own point of view ("other lands").
                    let holds =
                        with_ctx(state, cards, &ResolveCtx::new(id, obj.controller), |ctx| {
                            eval::condition(ctx, condition)
                        })
                        .unwrap_or(false);
                    if !holds {
                        events.push(Event::EnteredTapped { object: id });
                    }
                }
                // Paying is only possible with that much life (CR 119.4); without an answer,
                // or unable to pay, it enters tapped.
                R::EntersTappedUnlessPaysLife { amount } => {
                    let life = state.player(obj.controller).life;
                    if yes && life >= *amount as i32 {
                        events.push(Event::LifeChanged {
                            player: obj.controller,
                            delta: -(*amount as i32),
                        });
                    } else {
                        events.push(Event::EnteredTapped { object: id });
                    }
                }
                R::Devour(n) => {
                    devoured = match &answer {
                        Some(Answer::Objects(chosen)) => Some((*n, chosen.clone())),
                        _ => None,
                    };
                }
                R::EntersWithCounterIfChosen if yes => {
                    events.push(Event::CountersChanged {
                        object: id,
                        kind: mtg_core::CounterKind::PlusOnePlusOne,
                        delta: 1,
                    });
                }
                R::EntersChoosing(_) | R::EntersChoosingColorExcept(_) => {
                    // The answer picks one of the options; none given, the first.
                    let options = entry_options_of(cards, &r.kind);
                    let picked = match &answer {
                        Some(Answer::Modes(m)) => m.first().copied().unwrap_or(0) as usize,
                        _ => 0,
                    };
                    if let Some((_, color, subtype)) =
                        options.get(picked).or_else(|| options.first())
                    {
                        events.push(Event::ChoiceMade {
                            object: id,
                            color: *color,
                            subtype: *subtype,
                        });
                    }
                }
                R::EntersWithCounterOrHaste => {
                    if yes {
                        events.push(Event::CountersChanged {
                            object: id,
                            kind: mtg_core::CounterKind::PlusOnePlusOne,
                            delta: 1,
                        });
                    } else {
                        haste = true;
                    }
                }
                R::EntersWithCounters {
                    condition: Some(c), ..
                } if !with_ctx(state, cards, &ResolveCtx::new(id, obj.controller), |ctx| {
                    eval::condition(ctx, c)
                })
                .unwrap_or(false) => {}
                R::EntersWithCounters { kind, amount, .. } => {
                    let n = match amount {
                        mtg_ir::Value::Fixed(n) => *n,
                        // "enters with X counters": the X it was cast with.
                        mtg_ir::Value::X => obj.cast_x as i32,
                        // Sunburst, converge, "for each creature you control": from the
                        // entering permanent's point of view.
                        other => {
                            let mut rc = ResolveCtx::new(id, obj.controller);
                            rc.x = obj.cast_x;
                            with_ctx(state, cards, &rc, |ctx| eval::value(ctx, other)).unwrap_or(0)
                        }
                    };
                    if n > 0 {
                        events.push(Event::CountersChanged {
                            object: id,
                            kind: *kind,
                            delta: n,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    // Other permanents' "… enter tapped" (CR 614.1c), from each one's point of view.
    if !events
        .iter()
        .any(|e| matches!(e, Event::EnteredTapped { .. }))
    {
        let others: Vec<ObjectId> = state
            .battlefield()
            .into_iter()
            .filter(|o| *o != id)
            .collect();
        let tapped = others.into_iter().any(|other| {
            let Some(o) = state.objects.get(&other).filter(|o| !o.face_down) else {
                return false;
            };
            let Some(f) = cards.face(o.card, o.face) else {
                return false;
            };
            f.abilities.iter().any(|a| match &a.kind {
                mtg_ir::AbilityKind::ReplacementEffect(mtg_ir::effect::Replacement {
                    matches: mtg_ir::EventPattern::Enters { who },
                    kind: R::EntersTapped,
                }) if *who != mtg_ir::ObjectFilter::IsSelf => {
                    with_ctx(state, cards, &ResolveCtx::new(other, o.controller), |ctx| {
                        eval::matches(ctx, who, id)
                    })
                    .unwrap_or(false)
                }
                _ => false,
            })
        });
        if tapped {
            events.push(Event::EnteredTapped { object: id });
        }
    }
    // Riot from another permanent, answered when the permanent's own abilities asked
    // nothing (see `enter_question`).
    let asked_own = !obj.face_down
        && face.abilities.iter().any(|a| {
            matches!(&a.kind, mtg_ir::AbilityKind::ReplacementEffect(r)
                if matches!(r.matches, mtg_ir::EventPattern::Enters { who: mtg_ir::ObjectFilter::IsSelf }))
        });
    if !asked_own && granted_riot(state, cards, id) {
        if yes {
            events.push(Event::CountersChanged {
                object: id,
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                delta: 1,
            });
        } else {
            haste = true;
        }
    }
    // Devour: the chosen creatures are sacrificed as it enters, and it gets its counters.
    if let Some((n, chosen)) = devoured {
        let controller = state.objects.get(&id).map(|o| o.controller);
        let mut eaten = 0;
        for c in chosen {
            let ok = c != id
                && state
                    .objects
                    .get(&c)
                    .is_some_and(|o| o.zone.zone == Zone::Battlefield)
                && crate::layers::controller(state, c) == controller
                && crate::layers::compute(state, cards, c)
                    .is_some_and(|ch| ch.has_type(mtg_core::CardType::Creature));
            let Some(player) = controller.filter(|_| ok) else {
                continue;
            };
            move_to(state, log, c, Zone::Graveyard, cause);
            apply::apply(state, cause, Event::Sacrificed { player, object: c }, log);
            eaten += 1;
        }
        if eaten > 0 {
            events.push(Event::CountersChanged {
                object: id,
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                delta: i32::from(n) * eaten,
            });
        }
    }
    for e in events {
        apply::apply(state, cause, e, log);
    }
    if haste {
        grant_haste(state, log, id, cause);
    }
}

/// Whether a permanent is daybound/nightbound (CR 702.145), by its front face.
fn day_bound(cards: &dyn PrintedCards, obj: &crate::state::GameObject) -> bool {
    cards.layout(obj.card) == mtg_ir::Layout::Transforming
        && cards.face(obj.card, 0).is_some_and(|f| {
            f.abilities.iter().any(|a| {
                matches!(
                    a.kind,
                    mtg_ir::AbilityKind::Keyword(Keyword::Daybound | Keyword::Nightbound)
                )
            })
        })
}

/// CR 702.145b/d — each daybound permanent shows its day face by day and its night face by
/// night.
pub(crate) fn follow_day_night(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    cause: Cause,
) {
    let Some(day) = state.day else {
        return;
    };
    let want = u8::from(!day);
    let events: Vec<Event> = state
        .battlefield()
        .into_iter()
        .filter_map(|id| {
            let o = state.objects.get(&id).filter(|o| !o.face_down)?;
            (day_bound(cards, o) && o.face != want).then_some(Event::Transformed {
                object: id,
                face: want,
            })
        })
        .collect();
    if !events.is_empty() {
        apply::apply_simultaneous(state, cause, events, log);
    }
}

/// A permanent gains haste for as long as it stays (riot, dash, suspend).
pub(crate) fn grant_haste(
    state: &mut GameState,
    log: &mut Vec<StampedEvent>,
    object: ObjectId,
    cause: Cause,
) {
    let id = state.new_object_id();
    let timestamp = state.bump();
    let modification = mtg_ir::effect::Modification::GrantAbility(Box::new(mtg_ir::Ability {
        id: AbilityId(0),
        kind: mtg_ir::AbilityKind::Keyword(Keyword::Haste),
        targets: Vec::new(),
        source_text: None,
    }));
    state.continuous.push(ContinuousEffect {
        id,
        source: object,
        affected: AffectedSet::Fixed(vec![object]),
        layer: layer_of(&modification),
        modification,
        duration: Duration::Permanent,
        timestamp,
        ability: None,
        controller: None,
    });
    apply::apply(
        state,
        cause,
        Event::ContinuousEffectBegan {
            effect: id,
            source: object,
        },
        log,
    );
}

/// Discard a card (CR 701.8): to its owner's graveyard — or, for a card with madness, to
/// exile, recorded so its madness trigger and "whenever you discard" both see it.
pub(crate) fn discard(
    state: &mut GameState,
    cards: &dyn PrintedCards,
    log: &mut Vec<StampedEvent>,
    id: ObjectId,
    cause: Cause,
) -> Option<ObjectId> {
    let obj = state.objects.get(&id)?;
    let (from, owner) = (obj.zone, obj.owner);
    let madness = cards.face(obj.card, obj.face).is_some_and(|f| {
        f.abilities.iter().any(|a| {
            matches!(
                &a.kind,
                mtg_ir::AbilityKind::Triggered { trigger, .. }
                    if matches!(trigger.on, mtg_ir::EventPattern::ExiledForMadness { .. })
            )
        })
    });
    let to = if madness {
        ZoneRef::shared(Zone::Exile)
    } else {
        ZoneRef::of(Zone::Graveyard, owner)
    };
    let new_object = state.new_object_id();
    apply::apply(
        state,
        cause,
        Event::ZoneChange {
            object: id,
            new_object,
            from,
            to,
            index: None,
        },
        log,
    );
    if madness {
        apply::apply(
            state,
            cause,
            Event::MadnessExiled {
                object: new_object,
                player: owner,
            },
            log,
        );
    }
    Some(new_object)
}

/// The options of an "as it enters, choose …" replacement.
fn entry_options_of(
    cards: &dyn PrintedCards,
    kind: &mtg_ir::effect::ReplacementKind,
) -> Vec<(Box<str>, Option<mtg_core::Color>, Option<mtg_core::Subtype>)> {
    use mtg_ir::effect::ReplacementKind as R;
    match kind {
        R::EntersChoosing(choice) => entry_options(cards, *choice),
        R::EntersChoosingColorExcept(except) => {
            entry_options(cards, mtg_ir::effect::EntryChoice::Color)
                .into_iter()
                .filter(|(_, c, _)| *c != Some(*except))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What a permanent may choose as it enters, with the label each choice is shown as.
fn entry_options(
    cards: &dyn PrintedCards,
    kind: mtg_ir::effect::EntryChoice,
) -> Vec<(Box<str>, Option<mtg_core::Color>, Option<mtg_core::Subtype>)> {
    use mtg_core::Color;
    match kind {
        mtg_ir::effect::EntryChoice::Color => [
            ("white", Color::White),
            ("blue", Color::Blue),
            ("black", Color::Black),
            ("red", Color::Red),
            ("green", Color::Green),
        ]
        .into_iter()
        .map(|(l, c)| (l.into(), Some(c), None))
        .collect(),
        // Every creature type the card data knows (interned ids are dense).
        mtg_ir::effect::EntryChoice::CreatureType => {
            let mut out: Vec<_> = (0..u16::MAX)
                .map(mtg_core::Subtype)
                .take_while(|s| s.0 < 8192)
                .filter_map(|s| {
                    let name = cards.subtype_name(s)?;
                    mtg_core::is_creature_type(name).then(|| (name.into(), None, Some(s)))
                })
                .collect();
            out.sort_by(|a: &(Box<str>, _, _), b| a.0.cmp(&b.0));
            out.dedup_by(|a, b| a.0 == b.0);
            out
        }
    }
}

/// The question a permanent asks as it enters (CR 614.12), if it has one: shock lands,
/// unleash, riot (yes or no), and "choose a color / creature type". With the answer to
/// take when none is given. The answer is passed to [`entered`].
pub(crate) fn enter_question(
    state: &GameState,
    cards: &dyn PrintedCards,
    id: ObjectId,
) -> Option<(crate::choice::ChoiceKind, String, Answer)> {
    use crate::choice::ChoiceKind;
    use mtg_ir::effect::ReplacementKind as R;
    let obj = state.objects.get(&id)?;
    let confirm = |q: String| Some((ChoiceKind::Confirm, q, Answer::Bool(false)));
    let riot = || {
        granted_riot(state, cards, id)
            .then(|| confirm("have it enter with a +1/+1 counter? If not, it has haste".into()))
            .flatten()
    };
    if obj.face_down {
        return riot();
    }
    let own = cards
        .face(obj.card, obj.face)?
        .abilities
        .iter()
        .find_map(|a| match &a.kind {
            mtg_ir::AbilityKind::ReplacementEffect(mtg_ir::effect::Replacement {
                kind, ..
            }) => match kind {
                R::EntersTappedUnlessPaysLife { amount } => {
                    confirm(format!("pay {amount} life? If you don't, it enters tapped"))
                }
                R::EntersWithCounterIfChosen => confirm(
                    "have it enter with a +1/+1 counter? It can't block while it has one".into(),
                ),
                R::EntersWithCounterOrHaste => {
                    confirm("have it enter with a +1/+1 counter? If not, it has haste".into())
                }
                R::Devour(n) => {
                    let from: Vec<ObjectId> = state
                        .battlefield()
                        .into_iter()
                        .filter(|o| *o != id)
                        .filter(|o| crate::layers::controller(state, *o) == Some(obj.controller))
                        .filter(|o| {
                            crate::layers::compute(state, cards, *o)
                                .is_some_and(|c| c.has_type(mtg_core::CardType::Creature))
                        })
                        .collect();
                    if from.is_empty() {
                        return None;
                    }
                    let max = from.len() as u32;
                    Some((
                        ChoiceKind::ChooseObjects { from, min: 0, max },
                        format!("devour {n}: sacrifice any number of creatures"),
                        Answer::Objects(Vec::new()),
                    ))
                }
                // Mox Diamond: which card to discard, if any; none means the graveyard.
                R::EntersIfDiscards(filter) => {
                    let from = discard_options(state, cards, id, filter);
                    if from.is_empty() {
                        return None;
                    }
                    Some((
                        ChoiceKind::ChooseObjects {
                            from,
                            min: 0,
                            max: 1,
                        },
                        "discard a card? If you don't, it goes to the graveyard instead".into(),
                        Answer::Objects(Vec::new()),
                    ))
                }
                R::EntersChoosing(_) | R::EntersChoosingColorExcept(_) => {
                    let options = entry_options_of(cards, kind);
                    if options.is_empty() {
                        return None;
                    }
                    let what = match kind {
                        R::EntersChoosing(mtg_ir::effect::EntryChoice::CreatureType) => {
                            "choose a creature type"
                        }
                        _ => "choose a color",
                    };
                    Some((
                        ChoiceKind::ChooseModes {
                            available: options.into_iter().map(|(l, _, _)| l).collect(),
                            count: 1,
                            min: None,
                        },
                        what.into(),
                        Answer::Modes(vec![0]),
                    ))
                }
                _ => None,
            },
            _ => None,
        });
    own.or_else(riot)
}

/// Whether another permanent gives this one riot as it enters ("nontoken creatures you
/// control have riot"). It asks only when the permanent's own abilities ask nothing.
fn granted_riot(state: &GameState, cards: &dyn PrintedCards, id: ObjectId) -> bool {
    use mtg_ir::effect::ReplacementKind as R;
    state.battlefield().into_iter().any(|other| {
        let Some(o) = state
            .objects
            .get(&other)
            .filter(|o| other != id && !o.face_down)
        else {
            return false;
        };
        let Some(f) = cards.face(o.card, o.face) else {
            return false;
        };
        f.abilities.iter().any(|a| match &a.kind {
            mtg_ir::AbilityKind::ReplacementEffect(mtg_ir::effect::Replacement {
                matches: mtg_ir::EventPattern::Enters { who },
                kind: R::EntersWithCounterOrHaste,
            }) if *who != mtg_ir::ObjectFilter::IsSelf => {
                with_ctx(state, cards, &ResolveCtx::new(other, o.controller), |ctx| {
                    eval::matches(ctx, who, id)
                })
                .unwrap_or(false)
            }
            _ => false,
        })
    })
}

/// The cards in the controller's hand that may be discarded for `EntersIfDiscards`.
pub(crate) fn discard_options(
    state: &GameState,
    cards: &dyn PrintedCards,
    id: ObjectId,
    filter: &mtg_ir::ObjectFilter,
) -> Vec<ObjectId> {
    let Some(controller) = state.objects.get(&id).map(|o| o.controller) else {
        return Vec::new();
    };
    let hand = state.objects_in(ZoneRef::of(Zone::Hand, controller));
    with_ctx(state, cards, &ResolveCtx::new(id, controller), |ctx| {
        let mut out = Vec::new();
        for card in &hand {
            if *card != id && eval::matches(ctx, filter, *card)? {
                out.push(*card);
            }
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// Mox Diamond's replacement, if the face has one: the filter for the discarded card.
pub(crate) fn enters_if_discards(
    cards: &dyn PrintedCards,
    face: &mtg_ir::CardFace,
) -> Option<mtg_ir::ObjectFilter> {
    let _ = cards;
    face.abilities.iter().find_map(|a| match &a.kind {
        mtg_ir::AbilityKind::ReplacementEffect(mtg_ir::effect::Replacement {
            kind: mtg_ir::effect::ReplacementKind::EntersIfDiscards(f),
            ..
        }) => Some(f.clone()),
        _ => None,
    })
}

/// Ask, during a resolution, a permanent's enter question.
pub(crate) fn enter_choice(
    state: &GameState,
    cards: &dyn PrintedCards,
    rc: &mut ResolveCtx,
    id: ObjectId,
) -> Result<Option<Answer>, ResolveError> {
    let Some((kind, question, _)) = enter_question(state, cards, id) else {
        return Ok(None);
    };
    let who = state
        .objects
        .get(&id)
        .map_or(rc.controller, |o| o.controller);
    rc.need(who, kind, &question).map(Some)
}

/// Whether a filter names the object its source is attached to ("enchanted creature").
fn filter_is_attached(f: &mtg_ir::ObjectFilter) -> bool {
    match f {
        mtg_ir::ObjectFilter::AttachedToSelf => true,
        mtg_ir::ObjectFilter::And(fs) => fs.contains(&mtg_ir::ObjectFilter::AttachedToSelf),
        _ => false,
    }
}

/// Timestamp helper kept next to its only users, for readability at the call site.
pub fn now(state: &mut GameState) -> Timestamp {
    state.bump()
}

/// Ability index helper, so callers need not construct the newtype inline.
pub const fn ability(i: u16) -> AbilityId {
    AbilityId(i)
}
