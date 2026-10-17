//! Public-information threat assessment and conservative response matching.
//!
//! Scores are priorities, not predictions of a card's win probability. Unfamiliar
//! effects and target restrictions stay with the normal heuristic/search path.

use std::collections::{BTreeMap, BTreeSet};

use mtg_core::{AbilityId, CardType, CounterKind, Keyword, ObjectId, Step, Target, Zone};
use mtg_engine::{PlayerView, actions::Action};
use mtg_ir::{
    Ability, AbilityKind, CardFace, Effect, EventPattern, ObjectFilter, Selector, TargetSpec, Value,
};

use super::Bot;

pub(super) fn preference(effect: &Effect, targets: &[TargetSpec]) -> super::TargetPreference {
    if matches!(response(effect), Some(Response::Exile | Response::Bounce))
        && targets
            .first()
            .is_some_and(|spec| spec.zone == Zone::Battlefield)
    {
        super::TargetPreference::Hostile
    } else {
        super::target_preference(effect)
    }
}

#[derive(Clone, Copy)]
enum Response {
    Damage(i32),
    Destroy,
    Exile,
    Bounce,
}

enum ResponseGoal {
    Combat,
    Engine,
    EmptyDrawWin,
}

pub(super) struct CardTactics {
    types: Vec<CardType>,
    keywords: BTreeSet<Keyword>,
    protection: bool,
    engine: i64,
    responses: BTreeMap<Option<AbilityId>, (Response, TargetSpec)>,
    response_costs: BTreeMap<Option<AbilityId>, u32>,
}

impl CardTactics {
    pub(super) fn new(face: &CardFace) -> Self {
        let mut out = Self {
            types: face.card_types.clone(),
            keywords: BTreeSet::new(),
            protection: false,
            engine: 0,
            responses: BTreeMap::new(),
            response_costs: BTreeMap::new(),
        };
        for ability in &face.abilities {
            out.engine += recurring_ability_value(ability);
            let (id, effect, resource_cost) = match &ability.kind {
                // A spell spends a card as well as its mana.
                AbilityKind::SpellEffect(effect) => {
                    (None, effect, face.mana_cost.mana_value().saturating_add(1))
                }
                AbilityKind::Activated { cost, effect, .. } => {
                    let extra = cost.additional.iter().any(|c| {
                        !matches!(
                            c,
                            mtg_ir::AdditionalCost::Tap {
                                what: Selector::SelfSource
                            }
                        )
                    });
                    (
                        Some(ability.id),
                        effect,
                        cost.mana
                            .mana_value()
                            .saturating_add(if extra { 8 } else { 0 }),
                    )
                }
                AbilityKind::Triggered { effect, .. } => (Some(ability.id), effect, 0),
                AbilityKind::Keyword(keyword) => {
                    out.keywords.insert(*keyword);
                    continue;
                }
                AbilityKind::Protection { .. } => {
                    out.protection = true;
                    continue;
                }
                _ => continue,
            };
            if let [spec] = ability.targets.as_slice()
                && spec.mode.is_none()
                && spec.count == Value::Fixed(1)
                && let Some(response) = response(effect)
            {
                out.responses.insert(id, (response, spec.clone()));
                out.response_costs.insert(id, resource_cost);
            }
        }
        out
    }
}

fn response(effect: &Effect) -> Option<Response> {
    let targeted = |s: &Selector| matches!(s, Selector::Target { index: 0 });
    match effect {
        Effect::DealDamage {
            source: Selector::SelfSource,
            to,
            amount: Value::Fixed(n),
        } if targeted(to) && *n > 0 => Some(Response::Damage(*n)),
        Effect::Destroy { what } if targeted(what) => Some(Response::Destroy),
        Effect::MoveZone {
            what,
            to: Zone::Exile,
            ..
        } if targeted(what) => Some(Response::Exile),
        Effect::MoveZone {
            what,
            to: Zone::Hand,
            ..
        } if targeted(what) => Some(Response::Bounce),
        _ => None,
    }
}

// Recognize reusable resources by effect structure, not card names or mana cost.
fn engine_value(effect: &Effect) -> i64 {
    match effect {
        Effect::Draw {
            who: Selector::You,
            count,
        } => 32 * fixed_count(count),
        Effect::CreateToken {
            controller: Selector::You,
            count,
            ..
        } => 8 * fixed_count(count),
        Effect::AddMana {
            who: Selector::You, ..
        } => 8,
        Effect::WinGame { who: Selector::You } => 100,
        Effect::LoseLife {
            who: Selector::Opponents,
            ..
        } => 12,
        Effect::Sequence(items) => items.iter().map(engine_value).sum(),
        Effect::If {
            then, otherwise, ..
        } => engine_value(then).max(engine_value(otherwise)),
        Effect::May { then, .. } | Effect::MayPay { then, .. } => engine_value(then),
        Effect::UnlessPays { otherwise, .. } => engine_value(otherwise),
        Effect::Let { body, .. } => engine_value(body),
        Effect::FreezeValues { effect } => engine_value(effect),
        Effect::Modal { modes, .. } => modes
            .iter()
            .map(|(_, e)| engine_value(e))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

fn fixed_count(value: &Value) -> i64 {
    match value {
        Value::Fixed(n) => i64::from((*n).clamp(0, 4)),
        _ => 1,
    }
}

pub(crate) fn recurring_ability_value(ability: &Ability) -> i64 {
    match &ability.kind {
        AbilityKind::Activated {
            effect,
            functions_from: Zone::Battlefield,
            ..
        } => engine_value(effect),
        AbilityKind::Triggered { effect, trigger }
            if trigger.functions_from == Zone::Battlefield =>
        {
            let one_shot = match &trigger.on {
                EventPattern::Enters { who }
                | EventPattern::Leaves { who }
                | EventPattern::Dies { who } => contains_self(who),
                _ => false,
            };
            if one_shot { 0 } else { engine_value(effect) }
        }
        _ => 0,
    }
}

fn contains_self(filter: &ObjectFilter) -> bool {
    match filter {
        ObjectFilter::IsSelf => true,
        ObjectFilter::And(filters) => filters.iter().any(contains_self),
        _ => false,
    }
}

impl Bot {
    fn card_tactics(&self, view: &PlayerView, id: ObjectId) -> Option<&CardTactics> {
        let object = view.visible.get(&id)?;
        self.tactics.get(&(object.card?, object.face))
    }

    /// Relative urgency of answering an opposing permanent using visible information.
    pub fn threat_score(&self, view: &PlayerView, id: ObjectId) -> i64 {
        let Some(object) = view.visible.get(&id) else {
            return 0;
        };
        let Some(stats) = self.stats(view, id) else {
            return 0;
        };
        let engine = self.card_tactics(view, id).map_or(0, |card| card.engine);
        let damage = stats.combat_damage();
        let mut score = engine
            + self.strategic_threat_score(view, id)
            + i64::from(stats.mana_value)
            + damage * 4;
        if stats.creature {
            score += i64::from(stats.toughness.max(0));
            if stats.flying || stats.trample {
                score += damage * 2;
            }
            if stats.deathtouch {
                score += 8;
            }
        }
        if let Some(loyalty) = object.counters.get(&CounterKind::Loyalty) {
            score += 12 + i64::from(*loyalty) * 2;
        }
        if object.attacking
            && !view.prevent_combat_damage
            && !object
                .attacking_target
                .is_some_and(|target| view.prevent_damage_to.contains(&target))
            && object.attacking_target.is_some_and(|target| match target {
                Target::Player(player) => player == view.viewer,
                Target::Object(id) => view
                    .visible
                    .get(&id)
                    .is_some_and(|o| o.controller == view.viewer),
            })
        {
            score += 100 + damage * 8;
            if object.attacking_target == Some(Target::Player(view.viewer)) {
                let poison_damage = |s: &super::Stats| {
                    let damage = s.combat_damage();
                    if damage == 0 {
                        return 0;
                    }
                    (if s.infect { damage } else { 0 })
                        + i64::from(s.toxic) * if s.double_strike { 2 } else { 1 }
                };
                let (life_damage, poison): (i64, i64) = view
                    .visible
                    .values()
                    .filter(|o| {
                        o.attacking && o.attacking_target == Some(Target::Player(view.viewer))
                    })
                    .filter_map(|o| self.stats(view, o.id))
                    .fold((0, 0), |(life, poison), s| {
                        (
                            life + if s.infect { 0 } else { s.combat_damage() },
                            poison + poison_damage(&s),
                        )
                    });
                let player = view.players.get(&view.viewer);
                let life = player.map_or(i64::MAX, |p| i64::from(p.life));
                let poison_left = player.map_or(10, |p| 10u32.saturating_sub(p.poison));
                // Only attackers contributing to the lethal resource get its
                // urgency bonus. A large normal attacker can be survivable while
                // a small infect/toxic attacker is the immediate loss condition.
                score += if !stats.infect && damage > 0 && life_damage >= life {
                    1_000
                } else {
                    200
                };
                if poison >= i64::from(poison_left) && poison_damage(&stats) > 0 {
                    score += 4_000;
                }
            }
        }
        score
    }

    pub(super) fn response_value(
        &self,
        view: &PlayerView,
        source: ObjectId,
        ability: Option<AbilityId>,
        target: Target,
    ) -> i64 {
        // A counter selected to answer the current stack threat must keep that
        // target during announcement, rather than switching to a high-power
        // creature spell underneath a lethal instant.
        if ability.is_none()
            && view
                .visible
                .get(&source)
                .and_then(|o| o.card.map(|card| (card, o.face)))
                .is_some_and(|card| self.counterspells.contains(&card))
            && let Target::Object(id) = target
            && view.stack.iter().copied().find(|id| *id != source) == Some(id)
        {
            return 1_000_000;
        }
        let response = self
            .card_tactics(view, source)
            .and_then(|c| c.responses.get(&ability))
            .map(|(r, _)| *r);
        self.response_score(view, source, response, target)
    }

    fn response_score(
        &self,
        view: &PlayerView,
        source: ObjectId,
        response: Option<Response>,
        target: Target,
    ) -> i64 {
        let Target::Object(id) = target else {
            return match (response, target) {
                (Some(Response::Damage(n)), Target::Player(player))
                    if !view.prevent_damage_to.contains(&target)
                        && view
                            .players
                            .get(&player)
                            .is_some_and(|p| p.life > 0 && p.life <= n) =>
                {
                    1_000_000
                }
                _ => 0,
            };
        };
        let Some(object) = view.visible.get(&id) else {
            return 0;
        };
        let card = self.card_tactics(view, id);
        let indestructible = card.is_some_and(|c| c.keywords.contains(&Keyword::Indestructible));
        let effective = match response {
            Some(Response::Damage(n)) => {
                if view.prevent_damage_to.contains(&target) {
                    false
                } else if let Some(loyalty) = object.counters.get(&CounterKind::Loyalty) {
                    *loyalty <= n
                } else if indestructible {
                    false
                } else {
                    self.stats(view, id).is_some_and(|s| {
                        s.creature
                            && s.toughness > 0
                            && (s.toughness <= n
                                || self
                                    .stats(view, source)
                                    .is_some_and(|source| source.deathtouch))
                    })
                }
            }
            Some(Response::Destroy) => !indestructible,
            Some(Response::Exile | Response::Bounce) => true,
            None => return self.threat_score(view, id),
        };
        if effective {
            10_000 + self.threat_score(view, id)
        } else {
            -10_000
        }
    }

    /// Preserve creature-only burn and removal when there is no useful opposing target.
    pub(super) fn productive_response(
        &self,
        view: &PlayerView,
        source: ObjectId,
        face: u8,
    ) -> bool {
        let Some(object) = view.visible.get(&source) else {
            return true;
        };
        let Some((response, spec)) = object
            .card
            .and_then(|card| self.tactics.get(&(card, face)))
            .and_then(|card| card.responses.get(&None))
        else {
            return true;
        };
        if spec.zone != Zone::Battlefield {
            return true;
        }
        if view.visible.values().any(|target| {
            target.controller != view.viewer
                && target.zone.zone == spec.zone
                && self.obvious_target(view, target.id, &spec.filter)
                && self.response_score(view, source, Some(*response), Target::Object(target.id)) > 0
        }) {
            return true;
        }
        // Nonlethal damage to an opponent is still useful pressure when the
        // target slot allows it. Creature-only burn waits for a kill.
        matches!(response, Response::Damage(_))
            && spec.allows_players
            && !matches!(spec.players, Some(Selector::You))
            && view.players.keys().any(|player| {
                *player != view.viewer && !view.prevent_damage_to.contains(&Target::Player(*player))
            })
    }

    /// Use simple instant removal during an attack instead of passing until our turn.
    pub(super) fn combat_response(&self, actions: &[Action], view: &PlayerView) -> Option<Action> {
        if view.prevent_combat_damage
            || !view.stack.is_empty()
            || !matches!(
                view.step,
                Step::DeclareAttackers | Step::DeclareBlockers | Step::FirstStrikeCombatDamage
            )
        {
            return None;
        }
        self.permanent_response(actions, view, ResponseGoal::Combat)
    }

    /// Remove a repeatable draw/win engine promptly, even outside combat and on
    /// the opponent's turn. Removing its source is not a substitute for answering
    /// an ability already on the stack, so require an empty stack.
    pub(super) fn engine_response(&self, actions: &[Action], view: &PlayerView) -> Option<Action> {
        if !view.stack.is_empty() {
            return None;
        }
        self.permanent_response(actions, view, ResponseGoal::Engine)
    }

    pub(super) fn replacement_win_response(
        &self,
        actions: &[Action],
        view: &PlayerView,
    ) -> Option<Action> {
        if view.stack.is_empty() {
            return None;
        }
        self.permanent_response(actions, view, ResponseGoal::EmptyDrawWin)
    }

    fn permanent_response(
        &self,
        actions: &[Action],
        view: &PlayerView,
        goal: ResponseGoal,
    ) -> Option<Action> {
        actions
            .iter()
            .filter_map(|action| {
                let (source, face, ability) = match action {
                    Action::ActivateAbility { source, ability } => {
                        (*source, view.visible.get(source)?.face, Some(*ability))
                    }
                    _ => {
                        let (source, face, land) = action.play()?;
                        if land {
                            return None;
                        }
                        (source, face, None)
                    }
                };
                let object = view.visible.get(&source)?;
                let card = self.tactics.get(&(object.card?, face))?;
                let (response, spec) = card.responses.get(&ability)?;
                if spec.zone != Zone::Battlefield {
                    return None;
                }
                let score = view
                    .visible
                    .values()
                    .filter(|target| {
                        target.zone.zone == Zone::Battlefield
                            && target.controller != view.viewer
                            && match goal {
                                ResponseGoal::Combat => {
                                    target.attacking
                                        && target.attacking_target.is_some_and(|t| match t {
                                            Target::Player(p) => p == view.viewer,
                                            Target::Object(id) => view
                                                .visible
                                                .get(&id)
                                                .is_some_and(|o| o.controller == view.viewer),
                                        })
                                }
                                ResponseGoal::Engine => {
                                    self.card_tactics(view, target.id)
                                        .is_some_and(|card| card.engine >= 32)
                                        || self.strategic_threat_score(view, target.id) >= 32
                                }
                                ResponseGoal::EmptyDrawWin => {
                                    self.pending_empty_draw_win(view, target.id)
                                }
                            }
                            && self.obvious_target(view, target.id, &spec.filter)
                    })
                    .map(|target| {
                        self.response_score(
                            view,
                            source,
                            Some(*response),
                            Target::Object(target.id),
                        )
                    })
                    .max()?;
                (score >= 10_000).then_some((
                    score,
                    std::cmp::Reverse(*card.response_costs.get(&ability)?),
                    std::cmp::Reverse(source),
                    action,
                ))
            })
            .max_by_key(|(score, cost, source, _)| (*score, *cost, *source))
            .map(|(_, _, _, action)| action.clone())
    }

    fn obvious_target(&self, view: &PlayerView, id: ObjectId, filter: &ObjectFilter) -> bool {
        let Some(card) = self.card_tactics(view, id) else {
            return false;
        };
        if card.protection
            || card.keywords.contains(&Keyword::Hexproof)
            || card.keywords.contains(&Keyword::Shroud)
        {
            return false;
        }
        self.known_filter(view, id, filter) == Some(true)
    }

    // A conservative subset for selecting a play, not an engine legality oracle.
    // In particular, never invert an unrecognized predicate into a match.
    pub(super) fn known_filter(
        &self,
        view: &PlayerView,
        id: ObjectId,
        filter: &ObjectFilter,
    ) -> Option<bool> {
        self.known_filter_for(view, id, filter, view.viewer)
    }

    pub(super) fn known_filter_for(
        &self,
        view: &PlayerView,
        id: ObjectId,
        filter: &ObjectFilter,
        controller: mtg_core::PlayerId,
    ) -> Option<bool> {
        self.known_filter_from(view, id, filter, controller, None)
    }

    pub(super) fn known_filter_from(
        &self,
        view: &PlayerView,
        id: ObjectId,
        filter: &ObjectFilter,
        controller: mtg_core::PlayerId,
        source: Option<ObjectId>,
    ) -> Option<bool> {
        let object = view.visible.get(&id)?;
        let card = self.card_tactics(view, id)?;
        Some(match filter {
            ObjectFilter::Any => true,
            ObjectFilter::IsSelf => id == source?,
            ObjectFilter::HasType(kind) => card.types.contains(kind),
            ObjectFilter::HasColor(color) => self.has_color(view, Target::Object(id), *color),
            ObjectFilter::Tapped(value) => object.tapped == *value,
            ObjectFilter::Attacking => object.attacking,
            ObjectFilter::InZone(zone) => object.zone.zone == *zone,
            ObjectFilter::IsSpell => object.zone.zone == Zone::Stack && !object.is_ability,
            ObjectFilter::IsAbility => object.is_ability,
            ObjectFilter::ControlledBy(who) => match &**who {
                Selector::You => object.controller == controller,
                Selector::Opponents => object.controller != controller,
                _ => return None,
            },
            ObjectFilter::OwnedBy(who) => {
                let owner = object.zone.player.or_else(|| {
                    (!matches!(object.zone.zone, Zone::Battlefield | Zone::Stack))
                        .then_some(object.controller)
                })?;
                match &**who {
                    Selector::You => owner == controller,
                    Selector::Opponents => owner != controller,
                    Selector::Player(player) => owner == *player,
                    _ => return None,
                }
            }
            ObjectFilter::Not(inner) => {
                !self.known_filter_from(view, id, inner, controller, source)?
            }
            ObjectFilter::And(filters) => {
                let values: Option<Vec<_>> = filters
                    .iter()
                    .map(|f| self.known_filter_from(view, id, f, controller, source))
                    .collect();
                values?.into_iter().all(|v| v)
            }
            ObjectFilter::Or(filters) => {
                let values: Option<Vec<_>> = filters
                    .iter()
                    .map(|f| self.known_filter_from(view, id, f, controller, source))
                    .collect();
                values?.into_iter().any(|v| v)
            }
            _ => return None,
        })
    }
}
