//! Trigger detection (CR 603.2).
//!
//! The piece that makes triggered abilities actually fire. Everything else about
//! triggers was already here — they go on the stack in APNAP order, their controller
//! orders them when the order can matter ([`crate::triggers`]) — but nothing was
//! *noticing* events. This scans newly logged events against every triggered ability
//! that could see them.
//!
//! # Detection is a log scan, not a hook
//!
//! It would be simpler to call into detection from inside [`crate::apply`], the moment
//! each event is applied. It is deliberately not done that way: `apply` would then
//! need card data, and every caller would need to pass it. More importantly, a scan
//! over the log means **a replayed log detects exactly the same triggers** — detection
//! is a pure function of the event stream, like everything else in the engine.
//!
//! `GameState::scanned_upto` marks the boundary. The settle phase scans, then runs
//! state-based actions, then places whatever fired — and because settle loops, events
//! caused by state-based actions get scanned on the next pass round.
//!
//! # Ordering, and why it is not violated by scanning late
//!
//! CR 603.3 says a triggered ability waits to go on the stack until a player would
//! next receive priority. So the gap between an event happening and detection noticing
//! is not observable: nothing can happen in between. What *is* observable is the
//! trigger's controller and the values it captures, and those are fixed at detection
//! time from the event itself, not read later.
//!
//! # Last-known information
//!
//! A "dies" trigger has a problem: by the time anything can look, the creature is
//! gone. CR 603.10 answers it by checking the condition against the state *before* the
//! event, so [`crate::state::GameState::last_known`] keeps the object as it last was
//! on the battlefield, and filters consult it when the live object has gone.

use std::collections::BTreeMap;

use mtg_core::{AbilityId, Characteristics, Event, ObjectId, PlayerId, StampedEvent, Target, Zone};
use mtg_ir::{
    AbilityKind, ObjectFilter, Selector,
    selector::Binding,
    trigger::{DamageRecipient, EventPattern, Trigger, TriggerLimit, TriggerTiming},
};

use crate::{
    layers::PrintedCards,
    state::{GameObject, GameState},
    triggers::PendingTrigger,
};

/// Scan every event not yet scanned, and return the abilities that triggered.
///
/// Does not mutate: the caller pushes the results onto the queue and advances
/// `scanned_upto`, which keeps this testable against a fixed log.
pub fn detect(
    state: &GameState,
    cards: &dyn PrintedCards,
    log: &[StampedEvent],
) -> Vec<PendingTrigger> {
    let mut out = Vec::new();

    let fresh = &log[state.scanned_upto.min(log.len())..];
    let mut fired_delayed: Vec<u32> = Vec::new();
    for (i, stamped) in fresh.iter().enumerate() {
        // Delayed triggers (CR 603.7): each fires once, the first time its event happens.
        for d in &state.delayed {
            if fired_delayed.contains(&d.id) {
                continue;
            }
            let event_bindings = if d.on == EventPattern::Reflexive {
                // Only its own moment.
                match stamped.event {
                    Event::ReflexiveTriggered { id } if id == d.id => BTreeMap::new(),
                    _ => continue,
                }
            } else {
                let Some(b) =
                    pattern_matches(state, cards, &d.on, &stamped.event, d.controller, d.source)
                else {
                    continue;
                };
                b
            };
            fired_delayed.push(d.id);
            // What the creating effect meant by "it" wins over the event's.
            let mut bindings = event_bindings;
            for (k, v) in &d.bindings {
                bindings.insert(*k, v.clone());
            }
            let resolver = crate::triggers::StateResolver {
                state,
                source: d.source,
                controller: d.controller,
                targets: &[],
            };
            out.push(PendingTrigger {
                source: d.source,
                card: d.card,
                face: lookup(state, d.source).map_or(0, |o| o.face),
                // Distinct per delayed trigger, so two are never mistaken for copies.
                ability: AbilityId(u16::MAX - (d.id % 30000) as u16),
                controller: d.controller,
                cause: stamped.clone(),
                fired_at: stamped.at,
                footprint: mtg_ir::footprint::analyse(&d.effect, &resolver),
                bindings,
                delayed: Some((d.id, d.effect.clone())),
                // What it targets travels as its text, the way a granted ability's does.
                granted: (!d.targets.is_empty()).then(|| {
                    Box::new(mtg_ir::Ability {
                        id: AbilityId(u16::MAX - (d.id % 30000) as u16),
                        kind: AbilityKind::Triggered {
                            trigger: Trigger {
                                on: EventPattern::Reflexive,
                                functions_from: Zone::Battlefield,
                                intervening_if: None,
                                optional: false,
                                limit: None,
                                timing: TriggerTiming::Normal,
                            },
                            effect: d.effect.clone(),
                        },
                        targets: d.targets.clone(),
                        source_text: None,
                    })
                }),
            });
        }
        // CR 509.3c: "becomes blocked" triggers once per declaration however many
        // creatures block. Blockers are declared in one simultaneous batch, one event per
        // blocker; only the first event for each attacker counts.
        let repeat_block = match &stamped.event {
            Event::Blocked { attacker, .. } => fresh[..i].iter().any(|e| {
                e.at == stamped.at
                    && matches!(&e.event, Event::Blocked { attacker: a, .. } if a == attacker)
            }),
            _ => false,
        };
        for (source, face, ability_id, trigger) in candidate_abilities(state, cards, &stamped.event)
        {
            if repeat_block && matches!(trigger.on, EventPattern::BecomesBlocked { .. }) {
                continue;
            }
            // A state trigger is not looking at events at all (CR 603.8), and the
            // other special timings are handled by their own mechanisms.
            if !matches!(
                trigger.timing,
                TriggerTiming::Normal | TriggerTiming::LeavesBattlefield
            ) {
                continue;
            }

            let controller = crate::layers::controller(state, source)
                .or_else(|| lookup(state, source).map(|o| o.controller))
                .unwrap_or(state.active_player);

            if !functions_here(state, cards, source, &trigger, stamped) {
                continue;
            }

            // "Deals combat damage": damage from anything but a combat damage step is not
            // combat damage (CR 510.2), whoever dealt it.
            if matches!(
                trigger.on,
                EventPattern::DealsDamage {
                    combat_only: true,
                    ..
                } | EventPattern::TakesDamage {
                    combat_only: true,
                    ..
                }
            ) && stamped.cause != mtg_core::Cause::Combat
            {
                continue;
            }

            let Some(bindings) = pattern_matches(
                state,
                cards,
                &trigger.on,
                &stamped.event,
                controller,
                source,
            ) else {
                continue;
            };

            if limit_reached(state, source, ability_id, trigger.limit) {
                continue;
            }
            // "Your second card each turn": which draw this was, counting back from the
            // turn's total past the draws logged after it.
            if let (EventPattern::NthDraw { n, .. }, Event::Drew { player, .. }) =
                (&trigger.on, &stamped.event)
            {
                let later = fresh[i + 1..]
                    .iter()
                    .filter(|e| matches!(&e.event, Event::Drew { player: p, .. } if p == player))
                    .count() as u32;
                let total = state.draws_this_turn.get(player).copied().unwrap_or(0);
                if total.checked_sub(later) != Some(*n) {
                    continue;
                }
            }
            // "One or more": already triggered for this batch.
            if trigger.limit == Some(TriggerLimit::OncePerBatch)
                && out.iter().any(|t: &PendingTrigger| {
                    t.source == source && t.ability == ability_id && same_batch(&t.cause, stamped)
                })
            {
                continue;
            }

            out.push(build(
                state,
                cards,
                (source, face),
                ability_id,
                controller,
                stamped,
                bindings,
            ));
        }
        out.extend(looking_back(state, cards, fresh, i));
    }

    out
}

/// Leaves-the-battlefield abilities look back in time (CR 603.10a): an Aura's "when
/// enchanted creature dies" triggers even though the Aura went to the graveyard with the
/// creature, or in the state-based actions just after. For a permanent leaving, this finds
/// such abilities of permanents that left with it or later in the same scan, by their
/// last-known information.
fn looking_back(
    state: &GameState,
    cards: &dyn PrintedCards,
    fresh: &[StampedEvent],
    i: usize,
) -> Vec<PendingTrigger> {
    let stamped = &fresh[i];
    let Event::ZoneChange {
        object: leaving,
        from,
        ..
    } = &stamped.event
    else {
        return Vec::new();
    };
    if from.zone != Zone::Battlefield {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (j, later) in fresh.iter().enumerate() {
        let Event::ZoneChange { object, from, .. } = &later.event else {
            continue;
        };
        if j == i || object == leaving || from.zone != Zone::Battlefield {
            continue;
        }
        // It was still on the battlefield when this one left.
        if j < i && later.at != stamped.at {
            continue;
        }
        let Some(old) = state.last_known.get(object).filter(|o| !o.face_down) else {
            continue;
        };
        let Some(face) = cards.face(old.card, old.face) else {
            continue;
        };
        for ability in &face.abilities {
            let AbilityKind::Triggered { trigger, .. } = &ability.kind else {
                continue;
            };
            if trigger.functions_from != Zone::Battlefield
                || trigger.timing != TriggerTiming::LeavesBattlefield
                || limit_reached(state, *object, ability.id, trigger.limit)
            {
                continue;
            }
            let Some(bindings) = pattern_matches(
                state,
                cards,
                &trigger.on,
                &stamped.event,
                old.controller,
                *object,
            ) else {
                continue;
            };
            out.push(build(
                state,
                cards,
                (*object, old.face),
                ability.id,
                old.controller,
                stamped,
                bindings,
            ));
        }
    }
    out
}

/// State triggers whose condition is currently true (CR 603.8).
///
/// These do not watch events at all — they watch a *condition*, and fire as soon as the
/// game state satisfies it. So they cannot be found by scanning the log; they have to be
/// polled, which is why they are the one [`TriggerTiming`] that [`detect`] skips.
///
/// Polling happens at the same moment state-based actions are checked, which is what
/// keeps "as soon as" honest: both are the engine noticing that the world has changed
/// underneath it rather than reacting to something that happened.
///
/// The caller is responsible for not firing one that is already waiting or on the stack.
/// Without that check a condition which stays true would re-trigger on every pass and
/// the game would never reach priority — see [`Engine::poll_state_triggers`].
///
/// [`Engine::poll_state_triggers`]: crate::Engine
pub fn state_triggers(state: &GameState, cards: &dyn PrintedCards) -> Vec<PendingTrigger> {
    let mut out = Vec::new();

    for obj in state.objects.values() {
        let Some(face) = cards.face(obj.card, obj.face) else {
            continue;
        };
        let granted = crate::layers::compute(state, cards, obj.id).map(|c| c.abilities);

        for ability in &face.abilities {
            let AbilityKind::Triggered { trigger, .. } = &ability.kind else {
                continue;
            };
            if trigger.timing != TriggerTiming::StateTrigger {
                continue;
            }
            if let Some(list) = &granted
                && !list.contains(&ability.id)
            {
                continue;
            }
            if obj.zone.zone != trigger.functions_from {
                continue;
            }
            let EventPattern::StateIs(cond) = &trigger.on else {
                continue;
            };

            let controller = crate::layers::controller(state, obj.id).unwrap_or(obj.controller);
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source: obj.id,
                controller,
                targets: &[],
                target_legal: &[],
                x: 0,
                bindings: crate::empty_bindings(),
            };
            if !crate::eval::condition(&ctx, cond).unwrap_or(false) {
                continue;
            }
            if limit_reached(state, obj.id, ability.id, trigger.limit) {
                continue;
            }

            // A state trigger has no causing event, so it carries a synthetic one whose
            // cause is the state-based check that noticed the condition.
            let cause = StampedEvent {
                id: state.next_event,
                at: state.generation,
                cause: mtg_core::Cause::StateBasedAction,
                event: Event::PriorityReceived { player: controller },
            };
            out.push(build(
                state,
                cards,
                (obj.id, obj.face),
                ability.id,
                controller,
                &cause,
                BTreeMap::new(),
            ));
        }
    }

    out
}

/// Every triggered ability that could be looking, with its source.
fn candidate_abilities(
    state: &GameState,
    cards: &dyn PrintedCards,
    event: &Event,
) -> Vec<(ObjectId, u8, AbilityId, Trigger)> {
    let mut out = Vec::new();

    // Every observer can trigger on another object's zone change (CR 603.2).
    // Checking only the arriving/departing card loses landfall, evolve, and deaths
    // witnessed by permanents that remain on the battlefield.
    for obj in state.objects.values() {
        let obj_id = obj.id;
        // Look back to the battlefield face for leaves/dies triggers (CR 603.10).
        // Keep the new identity as source so self-return effects retain their existing
        // zone-change semantics, but snapshot the face supplying the ability.
        let departed = match event {
            Event::ZoneChange {
                object,
                new_object,
                from,
                ..
            } if from.zone == Zone::Battlefield && *new_object == obj_id => {
                state.last_known.get(object)
            }
            _ => None,
        };
        // A face-down permanent had no abilities as it left (CR 708.2).
        if let Some(old) = departed.filter(|o| !o.face_down)
            && let Some(face) = cards.face(old.card, old.face)
        {
            for ability in &face.abilities {
                if let AbilityKind::Triggered { trigger, .. } = &ability.kind
                    && trigger.functions_from == Zone::Battlefield
                {
                    out.push((obj.id, old.face, ability.id, trigger.clone()));
                }
            }
        }
        let Some(face) = cards.face(obj.card, obj.face) else {
            continue;
        };
        let ch = crate::layers::compute(state, cards, obj.id);
        let grants_any = ch.as_ref().is_some_and(|c| !c.granted_abilities.is_empty());
        let granted = ch.map(|c| c.abilities);
        for ability in &face.abilities {
            let AbilityKind::Triggered { trigger, .. } = &ability.kind else {
                continue;
            };
            if departed.is_some() && trigger.functions_from == Zone::Battlefield {
                continue;
            }
            if let Some(list) = &granted
                && !list.contains(&ability.id)
            {
                continue;
            }
            out.push((obj.id, obj.face, ability.id, trigger.clone()));
        }
        // A face-down disguised permanent has ward {2} (CR 702.168a).
        if obj.zone.zone == Zone::Battlefield
            && obj.face_down
            && cards.face(obj.card, obj.face).is_some_and(|f| {
                f.abilities
                    .iter()
                    .any(|a| matches!(a.kind, AbilityKind::Morph { disguise: true, .. }))
            })
            && let AbilityKind::Triggered { trigger, .. } = disguise_ward().kind
        {
            out.push((obj.id, obj.face, AbilityId::DISGUISE_WARD, trigger));
        }
        // Granted triggered abilities, of a permanent (see `crate::abilities`).
        if obj.zone.zone == Zone::Battlefield && grants_any {
            for ability in crate::abilities::current(state, cards, obj.id) {
                if ability.id.granted_index().is_some()
                    && let AbilityKind::Triggered { trigger, .. } = &ability.kind
                    && trigger.functions_from == Zone::Battlefield
                {
                    out.push((obj.id, obj.face, ability.id, trigger.clone()));
                }
            }
        }
    }
    out
}

/// CR 603.6 — whether the ability functions from the zone its source is in.
///
/// The exception that matters: a "dies" or "leaves the battlefield" trigger functions
/// from the battlefield, but by the time it fires its source is in the graveyard. So
/// when the event *is* the source leaving the battlefield, the battlefield requirement
/// is treated as met.
fn functions_here(
    state: &GameState,
    _cards: &dyn PrintedCards,
    source: ObjectId,
    trigger: &Trigger,
    stamped: &StampedEvent,
) -> bool {
    let Some(obj) = lookup(state, source) else {
        return false;
    };

    if obj.zone.zone == trigger.functions_from {
        return true;
    }

    // The source is the thing that just left the battlefield.
    if trigger.functions_from == Zone::Battlefield
        && let Event::ZoneChange {
            new_object, from, ..
        } = &stamped.event
        && from.zone == Zone::Battlefield
        && *new_object == source
    {
        return true;
    }

    false
}

/// Live object, or the last-known version if it has left the battlefield.
fn lookup(state: &GameState, id: ObjectId) -> Option<&GameObject> {
    state.objects.get(&id).or_else(|| state.last_known.get(&id))
}

/// Whether two events happened as one batch (CR 603.2c): applied simultaneously, or as
/// parts of one resolving spell or ability.
fn same_batch(a: &StampedEvent, b: &StampedEvent) -> bool {
    a.at == b.at
        || matches!(
            (a.cause, b.cause),
            (mtg_core::Cause::Resolution(x), mtg_core::Cause::Resolution(y)) if x == y
        )
}

fn limit_reached(
    state: &GameState,
    source: ObjectId,
    ability: AbilityId,
    limit: Option<TriggerLimit>,
) -> bool {
    let Some(limit) = limit else { return false };
    let count = state
        .triggered_this_turn
        .get(&(source, ability))
        .copied()
        .unwrap_or(0);
    match limit {
        TriggerLimit::OncePerTurn
        | TriggerLimit::OncePerTurnPerSource
        | TriggerLimit::FirstEachTurn => count >= 1,
        TriggerLimit::OncePerBatch => false,
    }
}

fn build(
    state: &GameState,
    cards: &dyn PrintedCards,
    source_face: (ObjectId, u8),
    ability: AbilityId,
    controller: PlayerId,
    stamped: &StampedEvent,
    bindings: BTreeMap<Binding, Vec<Target>>,
) -> PendingTrigger {
    let (source, face) = source_face;
    // The footprint is computed now, while the event's context is in hand, so the
    // ordering analysis has something concrete to work with.
    let card = lookup(state, source)
        .map(|o| o.card)
        .unwrap_or(mtg_core::CardId(0));
    let granted = if ability == AbilityId::DISGUISE_WARD {
        Some(Box::new(disguise_ward()))
    } else {
        ability
            .granted_index()
            .and_then(|_| crate::abilities::find(state, cards, source, ability))
            .map(|a| Box::new(a.into_owned()))
    };
    let effect = match &granted {
        Some(a) => Some(&**a),
        None => cards
            .face(card, face)
            .and_then(|f| f.abilities.iter().find(|a| a.id == ability)),
    }
    .and_then(|a| match &a.kind {
        AbilityKind::Triggered { effect, .. } => Some(effect.clone()),
        _ => None,
    });

    let footprint = match effect {
        Some(effect) => {
            let resolver = crate::triggers::StateResolver {
                state,
                source,
                controller,
                targets: &[],
            };
            mtg_ir::footprint::analyse(&effect, &resolver)
        }
        // No effect found means something is wrong with the card data; treat it as
        // opaque so it can never be auto-ordered.
        None => mtg_ir::footprint::Footprint::unanalysable(),
    };

    PendingTrigger {
        source,
        card,
        face,
        ability,
        controller,
        cause: stamped.clone(),
        fired_at: stamped.at,
        footprint,
        bindings,
        delayed: None,
        granted,
    }
}

// ---- pattern matching ---------------------------------------------------

type Bindings = BTreeMap<Binding, Vec<Target>>;

fn bind(subject: Option<Target>, other: Option<Target>) -> Bindings {
    let mut b = BTreeMap::new();
    if let Some(s) = subject {
        b.insert(Binding::EventSubject, vec![s]);
    }
    if let Some(o) = other {
        b.insert(Binding::EventOther, vec![o]);
    }
    b
}

/// Whether a pattern matches an event. Returns the bindings the effect will need.
fn pattern_matches(
    state: &GameState,
    cards: &dyn PrintedCards,
    pattern: &EventPattern,
    event: &Event,
    controller: PlayerId,
    source: ObjectId,
) -> Option<Bindings> {
    // "This" across a zone change: a creature that dies is a new object in the graveyard
    // (CR 400.7), and that new object is the one carrying the trigger — but the event
    // names the old one. For this event, the old identity counts as the source too.
    let was = match event {
        Event::ZoneChange {
            object, new_object, ..
        } if *new_object == source => Some(*object),
        _ => None,
    };
    let selves = [Some(source), was];
    let subject_ok = |filter: &ObjectFilter, id: ObjectId| {
        subject_matches(state, cards, filter, id, controller, &selves)
    };
    let player_ok = |sel: &Selector, p: PlayerId| match sel {
        Selector::EnchantedPlayer => {
            lookup(state, source).and_then(|o| o.attached_player) == Some(p)
        }
        // "the upkeep of enchanted creature's controller"
        Selector::ControllerOf(inner)
            if matches!(
                &**inner,
                Selector::All {
                    filter: ObjectFilter::AttachedToSelf,
                    ..
                }
            ) =>
        {
            lookup(state, source)
                .and_then(|o| o.attached_to)
                .and_then(|host| crate::layers::controller(state, host))
                == Some(p)
        }
        _ => selector_covers_player(state, sel, p, controller),
    };

    match (pattern, event) {
        // ---- zone changes ----------------------------------------------
        (EventPattern::Enters { who }, Event::ZoneChange { new_object, to, .. })
            if to.zone == Zone::Battlefield =>
        {
            subject_ok(who, *new_object).then(|| bind(Some(Target::Object(*new_object)), None))
        }
        // A token entering is created, not moved (CR 111.1, 603.6a).
        (EventPattern::Enters { who }, Event::Created { object, .. }) => {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None))
        }

        (
            EventPattern::Leaves { who },
            Event::ZoneChange {
                object,
                new_object,
                from,
                ..
            },
        ) if from.zone == Zone::Battlefield => {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*new_object)), None))
        }

        (
            EventPattern::Dies { who },
            Event::ZoneChange {
                object,
                new_object,
                from,
                to,
                ..
            },
        ) if from.zone == Zone::Battlefield && to.zone == Zone::Graveyard => {
            // The other party is the permanent as it was, for last-known information
            // ("if it had no +1/+1 counters on it").
            subject_ok(who, *object).then(|| {
                bind(
                    Some(Target::Object(*new_object)),
                    Some(Target::Object(*object)),
                )
            })
        }

        (
            EventPattern::ZoneChange {
                who,
                from: want_from,
                to: want_to,
            },
            Event::ZoneChange {
                object,
                new_object,
                from,
                to,
                ..
            },
        ) => {
            let from_ok = want_from.is_none_or(|z| z == from.zone);
            let to_ok = want_to.is_none_or(|z| z == to.zone);
            // The pre-move identity is the one a filter should describe.
            (from_ok && to_ok && subject_ok(who, *object))
                .then(|| bind(Some(Target::Object(*new_object)), None))
        }

        // ---- turn structure --------------------------------------------
        (
            EventPattern::StepBegins { step, whose },
            Event::StepBegan {
                active, step: s, ..
            },
        ) => (step == s && player_ok(whose, *active))
            .then(|| bind(Some(Target::Player(*active)), None)),
        (
            EventPattern::StepEnds { step, whose },
            Event::StepEnded {
                active, step: s, ..
            },
        ) => (step == s && player_ok(whose, *active))
            .then(|| bind(Some(Target::Player(*active)), None)),

        // ---- the stack --------------------------------------------------
        (
            EventPattern::Cast { who, by },
            Event::SpellCast {
                object,
                controller: caster,
            },
        ) => (subject_ok(who, *object) && player_ok(by, *caster))
            .then(|| bind(Some(Target::Object(*object)), Some(Target::Player(*caster)))),
        (
            EventPattern::CastTargeting { by, target },
            Event::SpellCast {
                object,
                controller: caster,
            },
        ) => (player_ok(by, *caster)
            && state
                .objects
                .get(object)
                .and_then(|o| o.cast_context.as_ref())
                .is_some_and(|c| {
                    c.targets
                        .iter()
                        .any(|t| matches!(t, Target::Object(id) if subject_ok(target, *id)))
                }))
        .then(|| bind(Some(Target::Object(*object)), Some(Target::Player(*caster)))),
        (
            EventPattern::Copied { who, by },
            Event::SpellCopied {
                copy, controller, ..
            },
        ) => (subject_ok(who, *copy) && player_ok(by, *controller)).then(|| {
            bind(
                Some(Target::Object(*copy)),
                Some(Target::Player(*controller)),
            )
        }),
        (
            EventPattern::AbilityActivated { by },
            Event::AbilityPutOnStack {
                controller: c,
                source: s,
                ..
            },
        ) => player_ok(by, *c).then(|| bind(Some(Target::Object(*s)), Some(Target::Player(*c)))),
        (EventPattern::BecomesTarget { who, by }, Event::Targeted { object, target }) => {
            let hit = match target {
                Target::Object(o) => subject_ok(who, *o),
                Target::Player(_) => false,
            };
            // "by" is who controls the spell or ability doing the targeting.
            let targeter = crate::layers::controller(state, *object)
                .or_else(|| lookup(state, *object).map(|o| o.controller));
            (hit && targeter.is_some_and(|t| player_ok(by, t)))
                .then(|| bind(Some(*target), Some(Target::Object(*object))))
        }

        // ---- combat ------------------------------------------------------
        (
            EventPattern::Attacks { who },
            Event::Attacked {
                attacker, defender, ..
            },
        ) => subject_ok(who, *attacker)
            .then(|| bind(Some(Target::Object(*attacker)), Some(*defender))),
        (
            EventPattern::AttacksMostLife { who },
            Event::Attacked {
                attacker,
                defender: Target::Player(p),
                ..
            },
        ) => {
            let most = state.players.values().map(|s| s.life).max().unwrap_or(0);
            (subject_ok(who, *attacker) && state.player(*p).life >= most)
                .then(|| bind(Some(Target::Object(*attacker)), Some(Target::Player(*p))))
        }
        (
            EventPattern::BlockedBy {
                attacker: a,
                blocker: b,
            },
            Event::Blocked { blocker, attacker },
        ) if subject_ok(a, *attacker) && subject_ok(b, *blocker) => Some(bind(
            Some(Target::Object(*blocker)),
            Some(Target::Object(*attacker)),
        )),
        (EventPattern::Blocks { who }, Event::Blocked { blocker, attacker }) => {
            subject_ok(who, *blocker).then(|| {
                bind(
                    Some(Target::Object(*blocker)),
                    Some(Target::Object(*attacker)),
                )
            })
        }
        (
            EventPattern::Scried { whose, surveil },
            Event::Scried {
                player,
                surveil: did,
            },
        ) if surveil == did => {
            player_ok(whose, *player).then(|| bind(Some(Target::Player(*player)), None))
        }
        (EventPattern::Sacrificed { who, by }, Event::Sacrificed { player, object }) => {
            (subject_ok(who, *object) && player_ok(by, *player))
                .then(|| bind(Some(Target::Object(*object)), Some(Target::Player(*player))))
        }
        (EventPattern::Cycled { who, by }, Event::Cycled { player, object }) => {
            (subject_ok(who, *object) && player_ok(by, *player))
                .then(|| bind(Some(Target::Object(*object)), Some(Target::Player(*player))))
        }
        (EventPattern::AttacksUnblocked { who }, Event::BecameUnblocked { attacker }) => {
            subject_ok(who, *attacker).then(|| bind(Some(Target::Object(*attacker)), None))
        }
        (EventPattern::BecomesBlocked { who }, Event::Blocked { blocker, attacker }) => {
            subject_ok(who, *attacker).then(|| {
                bind(
                    Some(Target::Object(*attacker)),
                    Some(Target::Object(*blocker)),
                )
            })
        }

        // ---- damage ------------------------------------------------------
        (
            EventPattern::DealsDamage {
                source: src_filter,
                to,
                combat_only,
            },
            Event::DamageMarked {
                source: dealer,
                object: victim,
                ..
            },
        ) => {
            let recipient_ok = match to {
                DamageRecipient::Any => true,
                DamageRecipient::Object(f) => subject_ok(f, *victim),
                DamageRecipient::Player(_) => false,
            };
            // Combat-only patterns need the cause, which the caller has; damage from
            // a resolving spell is not combat damage.
            let _ = combat_only;
            (recipient_ok && subject_ok(src_filter, *dealer))
                .then(|| bind(Some(Target::Object(*dealer)), Some(Target::Object(*victim))))
        }
        (
            EventPattern::DealsDamage {
                source: src_filter,
                to,
                ..
            },
            Event::DamageDealtToPlayer {
                source: dealer,
                player,
                ..
            },
        ) => {
            let recipient_ok = match to {
                DamageRecipient::Any => true,
                DamageRecipient::Player(sel) => player_ok(sel, *player),
                DamageRecipient::Object(_) => false,
            };
            (recipient_ok && subject_ok(src_filter, *dealer))
                .then(|| bind(Some(Target::Object(*dealer)), Some(Target::Player(*player))))
        }
        (
            EventPattern::TakesDamage { who, .. },
            Event::DamageMarked {
                source: dealer,
                object: victim,
                ..
            },
        ) => subject_ok(who, *victim)
            .then(|| bind(Some(Target::Object(*victim)), Some(Target::Object(*dealer)))),

        // ---- players -----------------------------------------------------
        (EventPattern::LifeGained { whose }, Event::LifeChanged { player, delta })
            if *delta > 0 =>
        {
            player_ok(whose, *player).then(|| bind(Some(Target::Player(*player)), None))
        }
        (EventPattern::LifeLost { whose }, Event::LifeChanged { player, delta }) if *delta < 0 => {
            player_ok(whose, *player).then(|| bind(Some(Target::Player(*player)), None))
        }
        // Damage to a player is loss of life (CR 120.3a); infect's poison is not.
        (
            EventPattern::LifeLost { whose },
            Event::DamageDealtToPlayer {
                player,
                amount,
                counters: false,
                ..
            },
        ) if *amount > 0 => {
            player_ok(whose, *player).then(|| bind(Some(Target::Player(*player)), None))
        }
        (
            EventPattern::Draws { whose } | EventPattern::NthDraw { whose, .. },
            Event::Drew { player, object },
        ) => player_ok(whose, *player)
            .then(|| bind(Some(Target::Player(*player)), Some(Target::Object(*object)))),
        (
            EventPattern::Discards { whose },
            Event::ZoneChange {
                new_object,
                from,
                to,
                ..
            },
        ) if from.zone == Zone::Hand && to.zone == Zone::Graveyard => from
            .player
            .filter(|p| player_ok(whose, *p))
            .map(|p| bind(Some(Target::Player(p)), Some(Target::Object(*new_object)))),
        // A madness card is discarded too, though it went to exile.
        (EventPattern::Discards { whose }, Event::MadnessExiled { object, player }) => {
            player_ok(whose, *player)
                .then(|| bind(Some(Target::Player(*player)), Some(Target::Object(*object))))
        }
        (
            EventPattern::BecomesClassLevel { level },
            Event::ClassLevelGained { object, level: l },
        ) => (*l == *level && subject_ok(&ObjectFilter::IsSelf, *object))
            .then(|| bind(Some(Target::Object(*object)), None)),
        (
            EventPattern::NthSpellCast { n },
            Event::SpellCast {
                object,
                controller: by,
            },
        ) if *by == controller => (state.spells_by_player.get(by).copied().unwrap_or(0) == *n)
            .then(|| bind(Some(Target::Object(*object)), None)),
        (
            EventPattern::TurnedFaceUp { who },
            Event::FaceDownChanged {
                object,
                face_down: false,
            },
        ) => subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None)),
        (EventPattern::BecomesMonstrous { who }, Event::BecameMonstrous { object }) => {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None))
        }
        (EventPattern::ExiledForMadness { who }, Event::MadnessExiled { object, .. }) => {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None))
        }

        // ---- permanents --------------------------------------------------
        (EventPattern::BecomesTapped { who }, Event::TapChanged { object, tapped }) if *tapped => {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None))
        }
        (EventPattern::BecomesUntapped { who }, Event::TapChanged { object, tapped })
            if !*tapped =>
        {
            subject_ok(who, *object).then(|| bind(Some(Target::Object(*object)), None))
        }
        (
            EventPattern::CounterPlaced { on, kind },
            Event::CountersChanged {
                object,
                kind: k,
                delta,
            },
        ) if *delta > 0 && kind == k => {
            subject_ok(on, *object).then(|| bind(Some(Target::Object(*object)), None))
        }

        (
            EventPattern::ChapterReached { lore, chapter },
            Event::CountersChanged {
                object,
                kind,
                delta,
            },
        ) if *delta > 0 && kind == lore && *object == source => {
            let after = state
                .objects
                .get(object)
                .and_then(|o| o.counters.get(kind).copied())
                .unwrap_or(0);
            let before = after - delta;
            let n = i32::from(*chapter);
            (before < n && n <= after).then(|| bind(Some(Target::Object(*object)), None))
        }
        (
            EventPattern::LastCounterRemoved { from, kind },
            Event::CountersChanged {
                object,
                kind: k,
                delta,
            },
        ) if *delta < 0
            && kind == k
            && state
                .objects
                .get(object)
                .is_some_and(|o| o.counters.get(kind).copied().unwrap_or(0) <= 0) =>
        {
            subject_ok(from, *object).then(|| bind(Some(Target::Object(*object)), None))
        }

        // ---- composition -------------------------------------------------
        //
        // Fires once per event even when several arms match, which is what
        // "whenever X or Y" means.
        (EventPattern::AnyOf(arms), _) => arms
            .iter()
            .find_map(|arm| pattern_matches(state, cards, arm, event, controller, source)),

        // A state trigger is not an event pattern; it is polled separately.
        (EventPattern::StateIs(_), _) => None,

        _ => None,
    }
}

/// Whether a selector denoting players covers a particular player.
///
/// Only the selectors that appear in trigger patterns are handled — a pattern saying
/// "whenever a player chosen by an effect draws" is not a thing.
fn selector_covers_player(
    state: &GameState,
    sel: &Selector,
    who: PlayerId,
    controller: PlayerId,
) -> bool {
    match sel {
        Selector::You => who == controller,
        Selector::Opponents => who != controller,
        Selector::EachPlayer => true,
        Selector::ActivePlayer => who == state.active_player,
        Selector::DefendingPlayer => state.combat.defending_player == Some(who),
        // Needs the source; `pattern_matches` answers it before getting here.
        Selector::EnchantedPlayer => false,
        Selector::Union(parts) => parts
            .iter()
            .any(|p| selector_covers_player(state, p, who, controller)),
        Selector::Except(base, minus) => {
            selector_covers_player(state, base, who, controller)
                && !selector_covers_player(state, minus, who, controller)
        }
        _ => false,
    }
}

/// Test a filter against an object that may no longer exist.
///
/// Deliberately separate from [`crate::eval::matches`], which needs a live object and
/// the full layer system. Here the subject may have left the battlefield, in which case
/// its last-known state answers instead (CR 603.10) and characteristics come from the
/// printed card plus the counters it had. Continuous effects that were applying to it
/// are *not* reconstructed — a known limit, and the reason this is a separate function
/// rather than a flag on the other one.
fn subject_matches(
    state: &GameState,
    cards: &dyn PrintedCards,
    filter: &ObjectFilter,
    id: ObjectId,
    controller: PlayerId,
    selves: &[Option<ObjectId>],
) -> bool {
    let Some(obj) = lookup(state, id) else {
        return false;
    };
    let live = state.objects.contains_key(&id);

    let chars = if live {
        crate::layers::compute(state, cards, id)
    } else if obj.face_down {
        Some(Characteristics::face_down())
    } else {
        cards
            .face(obj.card, obj.face)
            .map(|f| f.printed_characteristics())
    };
    let Some(chars) = chars else { return false };

    eval_filter(state, cards, filter, obj, &chars, id, controller, selves)
}

#[allow(clippy::too_many_arguments)]
fn eval_filter(
    state: &GameState,
    cards: &dyn PrintedCards,
    filter: &ObjectFilter,
    obj: &GameObject,
    chars: &Characteristics,
    id: ObjectId,
    controller: PlayerId,
    selves: &[Option<ObjectId>],
) -> bool {
    let recur = |f: &ObjectFilter| eval_filter(state, cards, f, obj, chars, id, controller, selves);

    match filter {
        ObjectFilter::Any => true,
        // The event's object is the trigger's own source, under either of the identities
        // it has in this event (see `pattern_matches`).
        ObjectFilter::IsSelf => selves.contains(&Some(id)),
        ObjectFilter::HasType(t) => chars.has_type(*t),
        ObjectFilter::HasSubtype(s) => chars.has_subtype(*s, |s| cards.subtype_name(s)),
        ObjectFilter::HasSupertype(s) => chars.supertypes.contains(s),
        ObjectFilter::HasColor(c) => chars.colors.contains(*c),
        ObjectFilter::Colorless => chars.colors.is_colorless(),
        ObjectFilter::Multicolored => chars.colors.count() >= 2,
        ObjectFilter::Token => obj.is_token,
        ObjectFilter::AttachedToSource => {
            selves.iter().flatten().any(|s| obj.attached_to == Some(*s))
        }
        ObjectFilter::NamedLikeSource => {
            let name = |o: &GameObject| cards.face(o.card, o.face).map(|f| f.name.clone());
            selves
                .iter()
                .flatten()
                .filter_map(|s| lookup(state, *s))
                .any(|s| name(s).is_some() && name(s) == name(obj))
        }
        ObjectFilter::Tapped(want) => obj.tapped == *want,
        ObjectFilter::AttackingOrBlocking => {
            state.combat.is_attacking(id) || state.combat.is_blocking(id)
        }
        ObjectFilter::Attacking => state.combat.is_attacking(id),
        ObjectFilter::Blocking => state.combat.is_blocking(id),
        ObjectFilter::BlockingSource => selves.iter().flatten().any(|s| {
            state
                .combat
                .blocks
                .get(s)
                .is_some_and(|bs| bs.contains(&id))
        }),
        // Attachment is a relation to the trigger's source, which this path does not
        // carry; no printed trigger condition asks it.
        // "Enchanted creature", "equipped creature": what the source is (or was, as it
        // left) attached to.
        ObjectFilter::AttachedToSelf => selves.iter().flatten().any(|s| {
            lookup(state, *s)
                .is_some_and(|o| o.attached_to == Some(id) || o.was_attached_to == Some(id))
        }),
        ObjectFilter::HasKeyword(k) => {
            chars.granted_keywords.contains(k)
                || cards.face(obj.card, obj.face).is_some_and(|f| {
                    f.abilities.iter().any(|a| {
                        chars.abilities.contains(&a.id)
                            && matches!(a.kind, mtg_ir::AbilityKind::Keyword(x) if x == *k)
                    })
                })
        }
        ObjectFilter::ToughnessAtMost(v) => chars.toughness.unwrap_or(0) <= static_value(v),
        ObjectFilter::ManaValueAtLeast(v) => chars.mana_cost.mana_value() as i32 >= static_value(v),
        ObjectFilter::ControlledBy(sel) => {
            selector_covers_player(state, sel, obj.controller, controller)
        }
        ObjectFilter::OwnedBy(sel) => selector_covers_player(state, sel, obj.owner, controller),
        ObjectFilter::PowerAtLeast(v) => chars.power.unwrap_or(0) >= static_value(v),
        ObjectFilter::PowerAtMost(v) => chars.power.unwrap_or(0) <= static_value(v),
        ObjectFilter::ManaValueAtMost(v) => chars.mana_cost.mana_value() as i32 <= static_value(v),
        ObjectFilter::HasCounter(kind) => obj.counters.get(kind).copied().unwrap_or(0) > 0,
        ObjectFilter::EnteredThisTurn => obj.summoning_sick,
        // Chosen as the trigger's source entered.
        ObjectFilter::HasChosenSubtype => selves
            .first()
            .copied()
            .flatten()
            .and_then(|s| lookup(state, s))
            .and_then(|s| s.chosen_subtype)
            .is_some_and(|s| chars.has_subtype(s, |s| cards.subtype_name(s))),
        ObjectFilter::HasChosenColor => selves
            .first()
            .copied()
            .flatten()
            .and_then(|s| lookup(state, s))
            .and_then(|s| s.chosen_color)
            .is_some_and(|c| chars.colors.contains(c)),
        // Targeting legality is not a question a trigger condition asks.
        ObjectFilter::Targetable => true,
        ObjectFilter::IsSpell => {
            obj.zone.zone == Zone::Stack
                && obj
                    .cast_context
                    .as_ref()
                    .is_none_or(|c| c.ability.is_none())
        }
        ObjectFilter::IsAbility => {
            obj.zone.zone == Zone::Stack
                && obj
                    .cast_context
                    .as_ref()
                    .is_some_and(|c| c.ability.is_some())
        }
        ObjectFilter::Not(inner) => !recur(inner),
        ObjectFilter::And(fs) => fs.iter().all(recur),
        ObjectFilter::Or(fs) => fs.iter().any(recur),
    }
}

/// Values inside a trigger's filter must be static: evaluating a dynamic one would
/// need a live object and the full evaluator, which is exactly what this path cannot
/// assume. A dynamic value reads as 0, which fails closed.
fn static_value(v: &mtg_ir::Value) -> i32 {
    match v {
        mtg_ir::Value::Fixed(n) => *n,
        _ => 0,
    }
}

/// How much happened in an event, for a triggered ability's "that much" (CR 603.3a): the
/// damage dealt, or the life gained or lost.
pub fn event_amount(event: &Event) -> u32 {
    match event {
        Event::DamageDealtToPlayer { amount, .. } | Event::DamageMarked { amount, .. } => *amount,
        Event::LifeChanged { delta, .. } => delta.unsigned_abs(),
        _ => 0,
    }
}

/// Ward {2}, as a face-down disguised permanent has it: "Whenever this permanent becomes the
/// target of a spell or ability an opponent controls, counter it unless that player pays
/// {2}."
fn disguise_ward() -> mtg_ir::Ability {
    mtg_ir::Ability {
        id: AbilityId::DISGUISE_WARD,
        kind: AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::BecomesTarget {
                    who: ObjectFilter::IsSelf,
                    by: Selector::Opponents,
                },
                functions_from: Zone::Battlefield,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: mtg_ir::Effect::CounterUnlessPays {
                what: Selector::Bound(Binding::EventOther),
                mana: mtg_core::ManaCost {
                    symbols: vec![mtg_core::ManaSymbol::Generic(2)],
                },
                life: None,
                discard: false,
                exile: false,
            },
        },
        targets: Vec::new(),
        source_text: Some("Ward {2}".into()),
    }
}
