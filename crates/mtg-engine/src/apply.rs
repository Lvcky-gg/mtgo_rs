//! Applying events to state.
//!
//! **This is the only module that mutates [`GameState`].** Everything else
//! proposes events; this applies them and appends them to the log. Funnelling all
//! mutation through one place is what makes the guarantees in
//! [`crate::Engine`] true rather than aspirational: if state could change anywhere
//! else, the log would not determine the state and replay would drift.
//!
//! Each application is deliberately literal. `apply` does not decide what *should*
//! follow from an event — it does not kill a creature whose toughness just dropped,
//! or make a player lose when their life hits zero. Those are state-based actions
//! ([`crate::sba`]) and triggered abilities, and keeping them out of here is what
//! stops the engine growing a second, implicit rules path.

use mtg_core::{Cause, Event, ObjectId, PlayerId, StampedEvent, Zone, ZoneRef};

use crate::state::GameState;

/// Append an event to the log and apply it.
pub fn apply(state: &mut GameState, cause: Cause, event: Event, log: &mut Vec<StampedEvent>) {
    let shuffles = shuffled_instead(state, &event);
    let event = replace(state, event);
    let at = state.bump();
    let id = state.next_event.advance();
    perform(state, &event);
    record_commander_damage(state, cause, &event);
    log.push(StampedEvent {
        id,
        at,
        cause,
        event,
    });
    if let Some(player) = shuffles {
        apply(state, cause, Event::Shuffled { player }, log);
    }
}

/// The owner whose library a card is shuffled into instead of going to a graveyard
/// ("reveal ~ and shuffle it into its owner's library instead"), if this is that event.
fn shuffled_instead(state: &GameState, event: &Event) -> Option<PlayerId> {
    let Event::ZoneChange { object, to, .. } = event else {
        return None;
    };
    let o = state.objects.get(object)?;
    (to.zone == Zone::Graveyard && state.shuffled_instead_of_graveyard.contains(&o.card))
        .then_some(o.owner)
}

/// Apply a whole batch as one simultaneous happening.
///
/// Used for state-based actions, which CR 704.3 requires to happen at once, and for
/// combat damage, which CR 510.2 requires to be dealt simultaneously. They share
/// one timestamp so that no ordering between them is observable.
pub fn apply_simultaneous(
    state: &mut GameState,
    cause: Cause,
    events: Vec<Event>,
    log: &mut Vec<StampedEvent>,
) {
    let at = state.bump();
    let mut shuffles = Vec::new();
    for event in events {
        shuffles.extend(shuffled_instead(state, &event));
        let event = replace(state, event);
        let id = state.next_event.advance();
        perform(state, &event);
        record_commander_damage(state, cause, &event);
        log.push(StampedEvent {
            id,
            at,
            cause,
            event,
        });
    }
    shuffles.dedup();
    for player in shuffles {
        apply(state, cause, Event::Shuffled { player }, log);
    }
}

/// Replacement effects that are facts about the object an event happens to (CR 614), applied
/// as the event happens so the log records what actually happened: a permanent that must be
/// exiled if it would leave the battlefield goes to exile, and never "dies".
fn replace(state: &GameState, event: Event) -> Event {
    // "If that creature would die this turn, exile it instead."
    if let Event::ZoneChange {
        object,
        new_object,
        from,
        to,
        index,
    } = event
        && from.zone == Zone::Battlefield
        && to.zone == Zone::Graveyard
        && (state
            .objects
            .get(&object)
            .is_some_and(|o| o.exile_if_dies == Some(state.turn))
            || state.exile_if_dies_now.contains(&object))
    {
        let _ = index;
        return Event::ZoneChange {
            object,
            new_object,
            from,
            to: ZoneRef::shared(Zone::Exile),
            index: None,
        };
    }
    // "Players can't gain life": the gain doesn't happen (CR 119.10).
    if let Event::LifeChanged { player, delta } = event
        && delta > 0
        && state.no_life_gain.contains(&player)
    {
        return Event::LifeChanged { player, delta: 0 };
    }
    // "You gain that much life plus 1 instead", "twice that much" (CR 614.1a). With both,
    // the player orders them (CR 616.1): adding first, then doubling, gains the most.
    if let Event::LifeChanged { player, delta } = event
        && delta > 0
        && let Some((plus, doublings)) = state.life_gain_boost.get(&player)
    {
        let gained = (delta + plus).saturating_mul(1 << (*doublings).min(16));
        return Event::LifeChanged {
            player,
            delta: gained,
        };
    }
    // CR 122.1d — stunned: instead of untapping, a stun counter comes off.
    if let Event::TapChanged {
        object,
        tapped: false,
    } = event
        && state.objects.get(&object).is_some_and(|o| {
            o.tapped
                && o.counters
                    .get(&mtg_core::CounterKind::Stun)
                    .is_some_and(|n| *n > 0)
        })
    {
        return Event::CountersChanged {
            object,
            kind: mtg_core::CounterKind::Stun,
            delta: -1,
        };
    }
    match event {
        Event::ZoneChange {
            object,
            new_object,
            from,
            to,
            ..
        } if from.zone == Zone::Battlefield
            && to.zone != Zone::Exile
            && state
                .objects
                .get(&object)
                .is_some_and(|o| o.exile_if_leaves) =>
        {
            Event::ZoneChange {
                object,
                new_object,
                from,
                to: ZoneRef::shared(Zone::Exile),
                index: None,
            }
        }
        // "If ~ would be put into a graveyard from anywhere, exile it instead."
        Event::ZoneChange {
            object,
            new_object,
            from,
            to,
            ..
        } if to.zone == Zone::Graveyard
            && state
                .objects
                .get(&object)
                .is_some_and(|o| state.exiled_instead_of_graveyard.contains(&o.card)) =>
        {
            Event::ZoneChange {
                object,
                new_object,
                from,
                to: ZoneRef::shared(Zone::Exile),
                index: None,
            }
        }
        // "… reveal ~ and shuffle it into its owner's library instead."
        Event::ZoneChange {
            object,
            new_object,
            from,
            to,
            ..
        } if to.zone == Zone::Graveyard
            && state
                .objects
                .get(&object)
                .is_some_and(|o| state.shuffled_instead_of_graveyard.contains(&o.card)) =>
        {
            let owner = state.objects[&object].owner;
            Event::ZoneChange {
                object,
                new_object,
                from,
                to: ZoneRef::of(Zone::Library, owner),
                index: None,
            }
        }
        other => other,
    }
}

/// CR 903.10a: combat damage a commander deals a player is tallied, for the 21-damage rule.
/// Only combat damage counts, which is why this needs the cause as well as the event.
fn record_commander_damage(state: &mut GameState, cause: Cause, event: &Event) {
    let (
        Cause::Combat,
        Event::DamageDealtToPlayer {
            source,
            player,
            amount,
            ..
        },
    ) = (cause, event)
    else {
        return;
    };
    // The source may already have left the battlefield (first strike, then an SBA), in
    // which case last-known information says what it was.
    let Some(obj) = state
        .objects
        .get(source)
        .or_else(|| state.last_known.get(source))
    else {
        return;
    };
    let (owner, card) = (obj.owner, obj.card);
    if state.commander.is_commander(owner, card) {
        *state.commander.damage.entry((*player, owner)).or_insert(0) += amount;
    }
}

fn perform(state: &mut GameState, event: &Event) {
    match event {
        Event::ZoneChange {
            object,
            new_object,
            from,
            to,
            index,
        } => {
            zone_change(state, *object, *new_object, *from, *to, *index);
        }

        Event::Created {
            object,
            card,
            owner,
        } => {
            let ts = state.bump();
            let mut obj = crate::state::GameObject::new(
                *object,
                *card,
                *owner,
                ZoneRef::shared(Zone::Battlefield),
            );
            obj.is_token = true;
            obj.timestamp = ts;
            // A creature token is as summoning sick as any other new creature.
            obj.summoning_sick = true;
            obj.entered_turn = Some(state.turn);
            state.objects.insert(*object, obj);
        }

        Event::Poisoned { player, amount } => {
            if let Some(p) = state.players.get_mut(player) {
                p.poison += amount;
            }
        }
        Event::ShieldGained { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.regeneration_shields += 1;
            }
        }
        Event::Regenerated { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.regeneration_shields = o.regeneration_shields.saturating_sub(1);
                o.tapped = true;
                o.damage = 0;
                o.dealt_deathtouch_damage = false;
            }
            state.combat.attackers.remove(object);
            state.combat.blocks.remove(object);
            for bs in state.combat.blocks.values_mut() {
                bs.retain(|b| b != object);
            }
        }

        // A record for triggers; the discard was its own zone change.
        Event::Cycled { .. }
        | Event::Sacrificed { .. }
        | Event::Scried { .. }
        | Event::CoinFlipped { .. }
        | Event::DieRolled { .. }
        | Event::Clashed { .. } => {}

        Event::DamageRemoved { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.damage = 0;
                o.dealt_deathtouch_damage = false;
            }
        }

        Event::CeasedToExist { object } => {
            if let Some(o) = state.objects.remove(object)
                && let Some(order) = state.zone_order.get_mut(&o.zone)
            {
                order.retain(|x| x != object);
            }
        }

        Event::Shuffled { player } => {
            // The order comes from the game's own seeded generator (see `state::Rng`), so
            // replaying the log from the same seed shuffles the same way.
            let library = ZoneRef::of(Zone::Library, *player);
            let mut order = state.zone_order.remove(&library).unwrap_or_default();
            state.rng.shuffle(&mut order);
            state.zone_order.insert(library, order);
        }

        Event::LifeChanged { player, delta } => {
            if let Some(p) = state.players.get_mut(player) {
                p.life += delta;
            }
            if *delta < 0 {
                state.lost_life_this_turn.insert(*player);
            }
            if *delta > 0 {
                state.gained_life_this_turn.insert(*player);
            }
        }

        Event::Drew { player, .. } => {
            // A draw is a library-to-hand zone change plus this record of intent.
            // Kept separate so "whenever a player draws a card" can match without
            // having to recognise a particular zone change.
            *state.draws_this_turn.entry(*player).or_insert(0) += 1;
        }

        Event::MillLike { .. } => {}

        Event::AttemptedDrawFromEmptyLibrary { player } => {
            if let Some(p) = state.players.get_mut(player) {
                p.attempted_draw_from_empty = true;
            }
        }

        Event::ManaAdded {
            player,
            color,
            amount,
        } => {
            if let Some(p) = state.players.get_mut(player) {
                p.mana.add(*color, *amount);
            }
        }

        Event::ManaSpent {
            player,
            color,
            amount,
        } => {
            if let Some(p) = state.players.get_mut(player) {
                let slot = color.map_or(mtg_core::ManaPool::COLORLESS_SLOT, |c| c as usize);
                p.mana.amounts[slot] = p.mana.amounts[slot].saturating_sub(*amount);
            }
        }

        Event::Lost { player, .. } => {
            if let Some(p) = state.players.get_mut(player) {
                p.has_lost = true;
            }
        }

        Event::TapChanged { object, tapped } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.tapped = *tapped;
            }
        }
        Event::EnteredTapped { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.tapped = true;
            }
        }

        Event::DamageMarked {
            source,
            object,
            amount,
            deathtouch,
            counters,
            recipient,
        } => {
            if *amount > 0 {
                state.damaged_by_this_turn.insert((*source, *object));
            }
            if let Some(o) = state.objects.get_mut(object) {
                if !matches!(recipient, mtg_core::ObjectDamageKind::Creature) {
                    *o.counters
                        .entry(mtg_core::CounterKind::Loyalty)
                        .or_insert(0) -= *amount as i32;
                }
                if matches!(recipient, mtg_core::ObjectDamageKind::Planeswalker) {
                    return;
                }
                if *counters {
                    *o.counters
                        .entry(mtg_core::CounterKind::MinusOneMinusOne)
                        .or_insert(0) += *amount as i32;
                } else {
                    o.damage = o.damage.saturating_add(*amount);
                }
                o.dealt_deathtouch_damage |= *deathtouch;
            }
        }

        Event::DamageDealtToPlayer {
            player,
            amount,
            counters,
            ..
        } => {
            if let Some(p) = state.players.get_mut(player) {
                if *counters {
                    p.poison += amount;
                } else {
                    p.life -= *amount as i32;
                }
            }
            if *amount > 0 {
                state.damaged_this_turn.insert(*player);
                if !*counters {
                    state.lost_life_this_turn.insert(*player);
                }
            }
        }

        Event::CountersChanged {
            object,
            kind,
            delta,
        } => {
            if let Some(o) = state.objects.get_mut(object) {
                let e = o.counters.entry(*kind).or_insert(0);
                *e += delta;
                // CR 121.3: +1/+1 and -1/-1 counters annihilate, but that is a
                // state-based action, not part of adding a counter. Only the
                // bookkeeping of never storing a negative count belongs here.
                if *e <= 0 {
                    o.counters.remove(kind);
                }
            }
        }

        Event::Attached { object, to } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.attached_to = *to;
                // CR 702.103f — an unattached bestowed Aura stops being bestowed.
                if to.is_none() && o.bestowed() {
                    o.cast_for = None;
                }
            }
        }

        Event::PhasedOut { object, out } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.phased_out = *out;
            }
        }

        Event::AdventureExiled { object, player } => {
            if let Some(o) = state.objects.get_mut(object)
                && o.zone.zone == Zone::Exile
            {
                o.adventure_player = Some(*player);
            }
        }
        Event::FaceSelected { object, face } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.face = *face;
            }
        }
        Event::FaceDownChanged { object, face_down } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.face_down = *face_down;
            }
        }

        Event::AbilityActivated {
            source,
            ability,
            loyalty,
        } => {
            *state
                .activated_this_turn
                .entry((*source, *ability))
                .or_insert(0) += 1;
            if *loyalty {
                state.loyalty_activated_this_turn.insert(*source);
            }
        }

        Event::SpellCast { object, controller } => {
            let before = state.spells_cast_this_turn;
            state.spells_cast_this_turn += 1;
            *state.spells_by_player.entry(*controller).or_insert(0) += 1;
            if let Some(o) = state.objects.get_mut(object) {
                o.cast_context
                    .get_or_insert_with(Default::default)
                    .cast_index = before;
            }
        }

        Event::SpellCopied {
            original,
            copy,
            controller,
            targets,
        } => {
            if let Some(mut obj) = state.objects.get(original).cloned() {
                obj.id = *copy;
                obj.controller = *controller;
                obj.is_spell_copy = true;
                obj.timestamp = state.bump();
                if let (Some(t), Some(cc)) = (targets, obj.cast_context.as_mut()) {
                    cc.targets = t.clone();
                }
                state.objects.insert(*copy, obj);
                state
                    .zone_order
                    .entry(ZoneRef::shared(Zone::Stack))
                    .or_default()
                    .insert(0, *copy);
            }
        }

        Event::AbilityPutOnStack { .. } | Event::Targeted { .. } => {
            // The stack object itself is created by a zone change or by the
            // ability-activation command; these events record the announcement.
        }

        Event::Countered { object } | Event::Resolved { object } => {
            // Leaving the stack is a zone change, emitted alongside this.
            let _ = object;
        }

        Event::StepBegan { turn, active, step } => {
            state.turn = *turn;
            state.active_player = *active;
            state.step = *step;
            if *step == mtg_core::Step::Upkeep {
                let now = state.generation;
                let marks = state.upkeeps.entry(*active).or_default();
                *marks = (marks.1, now);
            }
            state.consecutive_passes = 0;
            for p in state.players.values_mut() {
                p.passed = false;
            }
        }

        Event::StepEnded { .. } => {
            // CR 500.4: mana pools empty as a step or phase ends.
            for p in state.players.values_mut() {
                p.mana.clear();
            }
        }

        Event::PriorityReceived { player } => {
            state.priority = Some(*player);
        }

        Event::Attacked {
            attacker,
            defender,
            defending_player,
        } => {
            state.combat.defending_player = defending_player.or_else(|| match defender {
                mtg_core::Target::Player(p) => Some(*p),
                mtg_core::Target::Object(o) => state.objects.get(o).map(|o| o.controller),
            });
            state.combat.attackers.insert(*attacker, *defender);
            if let Some(o) = state.objects.get(attacker) {
                let who = o.controller;
                state.attacked_this_turn.insert(who);
            }
            state.attacked_creatures.insert(*attacker);
        }

        Event::Blocked { blocker, attacker } => {
            state
                .combat
                .blocks
                .entry(*attacker)
                .or_default()
                .push(*blocker);
            state.combat.was_blocked.insert(*attacker);
        }

        Event::BecameUnblocked { attacker } => {
            state.combat.blocks.remove(attacker);
        }

        Event::ContinuousEffectBegan { .. } => {
            // The effect itself is pushed onto `state.continuous` by the resolver,
            // which is the only thing holding the modification to install.
        }

        Event::ContinuousEffectEnded { effect } => {
            state.continuous.retain(|e| e.id != *effect);
        }
        Event::CombatDamagePreventionChanged { active } => {
            state.prevent_combat_damage = *active;
        }
        Event::DamageUnpreventableChanged { active } => {
            state.damage_unpreventable = *active;
        }
        Event::DamagePreventionChanged { target, active } => {
            if *active {
                if !state.prevent_damage_to.contains(target) {
                    state.prevent_damage_to.push(*target);
                }
            } else {
                state.prevent_damage_to.retain(|t| t != target);
            }
        }
        Event::EnteredAttacking { attacker, defender } => {
            state.combat.attackers.insert(*attacker, *defender);
        }
        Event::DayNight { day } => {
            state.day = Some(*day);
        }
        Event::Transformed { object, face } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.face = *face;
            }
        }
        Event::EnergyChanged { player, delta } => {
            if let Some(p) = state.players.get_mut(player) {
                p.energy = p.energy.saturating_add_signed(*delta);
            }
        }
        Event::CityBlessingGranted { player } => {
            if let Some(p) = state.players.get_mut(player) {
                p.city_blessing = true;
            }
        }
        Event::ChoiceMade {
            object,
            color,
            subtype,
        } => {
            if let Some(o) = state.objects.get_mut(object) {
                if color.is_some() {
                    o.chosen_color = *color;
                }
                if subtype.is_some() {
                    o.chosen_subtype = *subtype;
                }
            }
        }
        Event::BecameMonstrous { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.monstrous = true;
            }
        }
        Event::CastLater {
            object,
            player,
            after_turn,
            cost,
            sorcery,
        } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.cast_later = Some((*player, *after_turn, cost.clone(), *sorcery));
            }
        }
        Event::ExtraTurnAdded { player } => state.extra_turns.push(*player),
        Event::TurnSkipAdded { player } => state.skipped_turns.push(*player),
        Event::AdditionalCombatAdded => state.extra_combats += 1,
        Event::ReflexiveTriggered { .. } => {}
        Event::SpentToCast { object, mana } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.cast_context
                    .get_or_insert_with(Default::default)
                    .mana_spent = mana.clone();
                o.mana_spent = mana.clone();
            }
        }
        Event::SpeedChanged { player, speed } => {
            let turn = state.turn;
            if let Some(p) = state.players.get_mut(player) {
                if p.speed.is_some() {
                    p.speed_raised_turn = turn;
                }
                p.speed = Some(*speed);
            }
        }
        Event::AttachedToPlayer { object, player } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.attached_player = Some(*player);
            }
        }
        Event::ExileIfDies { object, turn } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.exile_if_dies = Some(*turn);
            }
        }
        Event::EnteredUnderControl { object, player } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.controller = *player;
            }
        }
        Event::PlayPermission {
            object,
            player,
            until_turn,
            cast_only,
        } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.may_play = Some((*player, *until_turn, *cast_only));
            }
        }
        Event::BecameMonarch {
            player,
            emblem,
            card,
        } => {
            state.monarch = Some(*player);
            let to = ZoneRef::of(Zone::Command, *player);
            match state.objects.get_mut(emblem) {
                // The designation moves: its object goes with it.
                Some(o) => {
                    let from = o.zone;
                    o.controller = *player;
                    o.owner = *player;
                    o.zone = to;
                    if let Some(order) = state.zone_order.get_mut(&from) {
                        order.retain(|id| id != emblem);
                    }
                }
                None => {
                    let mut obj = crate::state::GameObject::new(*emblem, *card, *player, to);
                    obj.timestamp = state.bump();
                    state.objects.insert(*emblem, obj);
                    state.monarch_emblem = Some(*emblem);
                }
            }
            state.zone_order.entry(to).or_default().push(*emblem);
        }
        Event::ClassLevelGained { object, level } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.class_level = *level;
            }
        }
        Event::BecameRenowned { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.renowned = true;
            }
        }
        Event::ControlsHost { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.controls_host = true;
            }
        }
        Event::MadnessExiled { .. } => {
            // The zone change beside it did the moving; this records why.
        }
        Event::LibraryBottomShuffled { player, count } => {
            let library = ZoneRef::of(Zone::Library, *player);
            if let Some(order) = state.zone_order.get_mut(&library) {
                let from = order.len().saturating_sub(*count as usize);
                let mut bottom = order.split_off(from);
                state.rng.shuffle(&mut bottom);
                order.extend(bottom);
            }
        }
        Event::Revealed { object } => {
            if let Some(card) = state.objects.get(object) {
                state.revealed_cards.push(crate::view::RevealedCard {
                    owner: card.owner,
                    card: card.card,
                    face: card.face,
                });
            }
        }
        Event::RemovedFromCombat { object } => {
            state.combat.attackers.remove(object);
            state.combat.blocks.remove(object);
            for bs in state.combat.blocks.values_mut() {
                bs.retain(|b| b != object);
            }
        }
        Event::LookedAt { object, by } => {
            if let Some(card) = state.objects.get(object) {
                let seen = crate::view::RevealedCard {
                    owner: card.owner,
                    card: card.card,
                    face: card.face,
                };
                state.looked_at.push((*by, seen));
            }
        }
        Event::ExileIfLeaves { object } => {
            if let Some(o) = state.objects.get_mut(object) {
                o.exile_if_leaves = true;
            }
        }
        Event::BecameCopy { object, card, face } => {
            if let Some(o) = state.objects.get_mut(object) {
                if o.original.is_none() {
                    o.original = Some((o.card, o.face));
                }
                o.card = *card;
                o.face = *face;
            }
        }
        Event::DamageShieldCreated { shield } => {
            state.next_shield = state.next_shield.max(shield.id + 1);
            state.damage_shields.push(shield.clone());
        }
        Event::DamageShieldUsed { shield, amount } => {
            if let Some(s) = state.damage_shields.iter_mut().find(|s| s.id == *shield)
                && let Some(left) = s.remaining.as_mut()
            {
                *left = left.saturating_sub(*amount);
            }
            state
                .damage_shields
                .retain(|s| s.id != *shield || s.remaining != Some(0));
        }
        Event::DamageShieldEnded { shield } => {
            state.damage_shields.retain(|s| s.id != *shield);
        }
    }
}

/// Move an object between zones, giving it a new identity (CR 400.7).
fn zone_change(
    state: &mut GameState,
    object: ObjectId,
    new_object: ObjectId,
    from: ZoneRef,
    to: ZoneRef,
    index: Option<u32>,
) {
    // Remove from the old zone's order, if it kept one.
    if let Some(order) = state.zone_order.get_mut(&from) {
        order.retain(|o| *o != object);
    }

    let Some(mut obj) = state.objects.remove(&object) else {
        return;
    };
    // Prevention follows the selected object identity, not its card in a new zone.
    state
        .prevent_damage_to
        .retain(|t| *t != mtg_core::Target::Object(object));
    state
        .damage_shields
        .retain(|s| s.to != Some(mtg_core::Target::Object(object)) && s.by != Some(object));

    // CR 603.10 / last-known information: remember what it was while it was on the
    // battlefield, keyed by the identity it had there. Recorded here because this is
    // the only moment both the old identity and its state exist together.
    if from.zone == Zone::Battlefield {
        state.last_known.insert(object, obj.clone());
        if to.zone == Zone::Graveyard {
            state.died_this_turn.push(object);
        }
    }
    // A spell leaving the stack is remembered too: it may have just dealt damage, and
    // "whenever you're dealt damage" looks at its source after it has resolved.
    if from.zone == Zone::Stack {
        state.last_known.insert(object, obj.clone());
    }

    // A new object in the new zone: a fresh id, a fresh timestamp for CR 613.7, and
    // none of the state that belonged to the old object. Damage, counters and
    // attachments do not survive a zone change, and forgetting that is a classic
    // source of ghost state.
    obj.id = new_object;
    obj.zone = to;
    obj.timestamp = state.bump();
    obj.exile_if_leaves = false;
    obj.controls_host = false;
    obj.renowned = false;
    obj.monstrous = false;
    obj.class_level = 0;
    obj.chosen_color = None;
    obj.chosen_subtype = None;
    // A copy of a permanent spell becomes a token as it resolves (CR 707.10a, 111.12).
    if obj.is_spell_copy && to.zone == Zone::Battlefield {
        obj.is_spell_copy = false;
        obj.is_token = true;
    }
    // A copy stops being one as it leaves (CR 707.2): the card is itself again.
    if let Some((card, face)) = obj.original.take() {
        obj.card = card;
        obj.face = face;
    }
    obj.damage = 0;
    obj.dealt_deathtouch_damage = false;
    obj.counters.clear();
    obj.attached_to = None;
    obj.was_attached_to = None;
    obj.cast_from = None;
    obj.regeneration_shields = 0;
    obj.cast_x = 0;
    obj.kicked = false;
    obj.kicks = 0;
    obj.cast_for = None;
    obj.tapped = false;
    // A face-down spell resolves into a face-down permanent (CR 708.4); any other move
    // turns it face up.
    obj.face_down = obj.face_down && from.zone == Zone::Stack && to.zone == Zone::Battlefield;
    // CR 712.8a, 712.13: only a resolving spell retains its chosen face.
    if !(from.zone == Zone::Stack && to.zone == Zone::Battlefield) {
        obj.face = 0;
    }
    obj.cast_context = None;
    obj.adventure_player = None;
    obj.may_play = None;
    obj.cast_later = None;
    obj.exile_if_dies = None;
    obj.attached_player = None;
    // A spell's record of what was spent goes with it to the battlefield, and no further.
    if !(from.zone == Zone::Stack && to.zone == Zone::Battlefield) {
        obj.mana_spent.clear();
    }
    // Only a creature entering the battlefield is summoning sick.
    obj.summoning_sick = to.zone == Zone::Battlefield;
    obj.entered_turn = (to.zone == Zone::Battlefield).then_some(state.turn);

    if to.zone.is_ordered() {
        let order = state.zone_order.entry(to).or_default();
        let at = match index {
            Some(i) => (i as usize).min(order.len()),
            // Graveyards and libraries default to the top, which for a graveyard
            // means most-recently-added.
            None => 0,
        };
        order.insert(at, new_object);
    }

    // A permanent leaving the battlefield takes anything attached to it with it,
    // in the sense that those attachments become unattached.
    if from.zone == Zone::Battlefield {
        for o in state.objects.values_mut() {
            if o.attached_to == Some(object) {
                o.attached_to = None;
                o.was_attached_to = Some(object);
            }
        }
        state.combat.attackers.remove(&object);
        state.combat.blocks.remove(&object);
        for bs in state.combat.blocks.values_mut() {
            bs.retain(|b| *b != object);
        }
        // Continuous effects that exist only while their source is on the
        // battlefield end now.
        state.continuous.retain(|e| {
            e.source != object
                || !matches!(
                    e.duration,
                    mtg_ir::effect::Duration::WhileSourcePresent
                        | mtg_ir::effect::Duration::UntilSourceLeaves
                )
        });
    }

    state.objects.insert(new_object, obj);
}
