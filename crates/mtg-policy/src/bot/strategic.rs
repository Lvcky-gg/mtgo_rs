//! Context-dependent priorities for static locks and combo enablers.
//!
//! This reads public objects and our own visible cards. It identifies supported
//! capabilities from compiled IR, rather than assigning a threat list by name.

use std::collections::BTreeMap;

use mtg_core::{AbilityId, ObjectId, PlayerId, Target, Zone};
use mtg_engine::PlayerView;
use mtg_ir::effect::{Modification, Restriction};
use mtg_ir::{AbilityKind, AdditionalCost, CardFace, Effect, ObjectFilter, Selector, Value};

use super::Bot;

pub(super) struct StrategicCard {
    statics: Vec<Restriction>,
    searches: bool,
    searches_from_battlefield: bool,
    draws: BTreeMap<Option<AbilityId>, Vec<Selector>>,
}

impl StrategicCard {
    pub(super) fn new(face: &CardFace) -> Self {
        let mut out = Self {
            statics: Vec::new(),
            searches: false,
            searches_from_battlefield: false,
            draws: BTreeMap::new(),
        };
        for ability in &face.abilities {
            if let AbilityKind::Static {
                modification: Modification::Restriction(restriction),
                condition: None,
                ..
            } = &ability.kind
                && matches!(
                    restriction,
                    Restriction::CantCast { .. }
                        | Restriction::CantCastAboveLandCount { .. }
                        | Restriction::GrantEscape { .. }
                        | Restriction::WinInsteadOfEmptyDraw
                        | Restriction::OpponentsCantSearch
                        | Restriction::ControlOpponentsSearches
                )
            {
                out.statics.push(restriction.clone());
            }
            let effect = match &ability.kind {
                AbilityKind::SpellEffect(effect)
                | AbilityKind::Activated { effect, .. }
                | AbilityKind::Triggered { effect, .. } => Some(effect),
                _ => None,
            };
            let mut searches = effect.is_some_and(explicit_search);
            if let Some(effect) = effect {
                let id = if matches!(ability.kind, AbilityKind::SpellEffect(_)) {
                    None
                } else {
                    Some(ability.id)
                };
                let mut draws = Vec::new();
                draw_selectors(effect, &mut draws);
                out.draws.insert(id, draws);
            }
            // Normal tutors name a chosen subset of the library in a MoveZone.
            let mut inspected = ability.clone();
            mtg_ir::walk::Visitor {
                filter: &mut |_| {},
                token: &mut |_| {},
                value: &mut |_| {},
                selector: &mut |selector| {
                    searches |= matches!(
                        selector,
                        Selector::ChosenBy {
                            zone: Zone::Library,
                            ..
                        }
                    );
                },
            }
            .ability(&mut inspected);
            out.searches |= searches;
            out.searches_from_battlefield |= searches
                && matches!(
                    ability.kind,
                    AbilityKind::Activated {
                        functions_from: Zone::Battlefield,
                        ..
                    }
                );
        }
        out
    }
}

impl Bot {
    fn strategic_card(&self, view: &PlayerView, id: ObjectId) -> Option<&StrategicCard> {
        let object = view.visible.get(&id)?;
        self.strategic_cards.get(&(object.card?, object.face))
    }

    pub(super) fn pending_empty_draw_win(&self, view: &PlayerView, source: ObjectId) -> bool {
        let Some(permanent) = view.visible.get(&source) else {
            return false;
        };
        if !view
            .players
            .get(&permanent.controller)
            .is_some_and(|p| p.library_size == 0)
            || !self.strategic_card(view, source).is_some_and(|card| {
                card.statics
                    .iter()
                    .any(|r| matches!(r, Restriction::WinInsteadOfEmptyDraw))
            })
        {
            return false;
        }
        let Some(top) = view.stack.first().and_then(|id| view.visible.get(id)) else {
            return false;
        };
        let Some(draws) = self
            .strategic_card(view, top.id)
            .and_then(|card| card.draws.get(&top.ability))
        else {
            return false;
        };
        draws.iter().any(|who| {
            affects_player(who, top.controller, permanent.controller)
                || matches!(who, Selector::Target { index } if top.targets.get(usize::from(*index)) == Some(&Target::Player(permanent.controller)))
        })
    }

    /// Static effects become urgent when the public position supplies their
    /// resources or when they constrain a visible part of our own plan.
    pub fn strategic_threat_score(&self, view: &PlayerView, id: ObjectId) -> i64 {
        let Some(object) = view.visible.get(&id) else {
            return 0;
        };
        let Some(card) = self.strategic_card(view, id) else {
            return 0;
        };
        let controller = object.controller;
        card.statics
            .iter()
            .map(|restriction| match restriction {
                Restriction::WinInsteadOfEmptyDraw => {
                    match view.players.get(&controller).map(|p| p.library_size) {
                        Some(0) => 2_000,
                        Some(1..=3) => 128,
                        _ => 12,
                    }
                }
                Restriction::GrantEscape { cards, additional } => {
                    self.escape_priority(view, controller, cards, additional)
                }
                Restriction::CantCast {
                    who,
                    spells,
                    beyond: None | Some(0),
                } if affects_player(who, controller, view.viewer) => {
                    self.blocked_cast_priority(view, controller, spells, None)
                }
                Restriction::CantCastAboveLandCount { who, spells }
                    if affects_player(who, controller, view.viewer) =>
                {
                    let lands = view
                        .visible
                        .values()
                        .filter(|o| o.controller == view.viewer && o.zone.zone == Zone::Battlefield)
                        .filter(|o| self.stats(view, o.id).is_some_and(|s| s.land))
                        .count();
                    self.blocked_cast_priority(view, controller, spells, Some(lands))
                }
                Restriction::OpponentsCantSearch | Restriction::ControlOpponentsSearches
                    if controller != view.viewer =>
                {
                    let needs_search = view
                        .visible
                        .values()
                        .filter(|o| {
                            o.controller == view.viewer
                                && matches!(
                                    o.zone.zone,
                                    Zone::Hand | Zone::Battlefield | Zone::Command
                                )
                        })
                        .any(|o| {
                            self.strategic_card(view, o.id).is_some_and(|card| {
                                if o.zone.zone == Zone::Battlefield {
                                    card.searches_from_battlefield
                                } else {
                                    card.searches
                                }
                            })
                        });
                    if needs_search { 96 } else { 0 }
                }
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    fn blocked_cast_priority(
        &self,
        view: &PlayerView,
        controller: PlayerId,
        spells: &ObjectFilter,
        above: Option<usize>,
    ) -> i64 {
        let blocked = view
            .visible
            .values()
            .filter(|o| {
                o.controller == view.viewer && matches!(o.zone.zone, Zone::Hand | Zone::Command)
            })
            .filter(|o| {
                self.stats(view, o.id).is_some_and(|stats| {
                    !stats.land
                        && above.is_none_or(|lands| u64::from(stats.mana_value) > lands as u64)
                })
            })
            .filter(|o| self.known_filter_for(view, o.id, spells, controller) == Some(true))
            .count();
        if blocked == 0 {
            0
        } else {
            64 + blocked.min(4) as i64 * 8
        }
    }

    fn escape_priority(
        &self,
        view: &PlayerView,
        controller: PlayerId,
        cards: &ObjectFilter,
        additional: &[AdditionalCost],
    ) -> i64 {
        let (needed, cost_filter) = match additional {
            [] => (0, None),
            [
                AdditionalCost::ExileFrom {
                    zone: Zone::Graveyard,
                    count: Value::Fixed(count),
                    filter,
                },
            ] if *count >= 0 => (*count as usize, Some(filter)),
            _ => return 0,
        };
        let graveyard: Vec<_> = view
            .visible
            .values()
            .filter(|o| o.zone.zone == Zone::Graveyard && o.zone.player == Some(controller))
            .collect();
        let enabled = graveyard
            .iter()
            .filter(|candidate| {
                self.known_filter_for(view, candidate.id, cards, controller) == Some(true)
                    && graveyard
                        .iter()
                        .filter(|o| o.id != candidate.id)
                        .filter(|o| {
                            cost_filter.is_none_or(|filter| {
                                self.known_filter_from(
                                    view,
                                    o.id,
                                    filter,
                                    controller,
                                    Some(candidate.id),
                                ) == Some(true)
                            })
                        })
                        .count()
                        >= needed
            })
            .count();
        if enabled == 0 {
            0
        } else {
            96 + enabled.min(4) as i64 * 8
        }
    }
}

fn draw_selectors(effect: &Effect, out: &mut Vec<Selector>) {
    match effect {
        Effect::Draw { who, count } if !matches!(count, Value::Fixed(n) if *n <= 0) => {
            out.push(who.clone())
        }
        Effect::Sequence(items) => {
            for item in items {
                draw_selectors(item, out);
            }
        }
        Effect::If {
            then, otherwise, ..
        } => {
            draw_selectors(then, out);
            draw_selectors(otherwise, out);
        }
        Effect::May { then, .. } | Effect::MayPay { then, .. } => draw_selectors(then, out),
        Effect::Let { body, .. } | Effect::ForEach { body, .. } | Effect::Repeat { body, .. } => {
            draw_selectors(body, out)
        }
        Effect::FreezeValues { effect } => draw_selectors(effect, out),
        Effect::Modal { modes, .. } => {
            for (_, effect) in modes {
                draw_selectors(effect, out);
            }
        }
        _ => {}
    }
}

fn affects_player(selector: &Selector, controller: PlayerId, player: PlayerId) -> bool {
    match selector {
        Selector::You => player == controller,
        Selector::Opponents => player != controller,
        Selector::EachPlayer => true,
        Selector::Player(who) => *who == player,
        Selector::Union(selectors) => selectors
            .iter()
            .any(|selector| affects_player(selector, controller, player)),
        _ => false,
    }
}

fn explicit_search(effect: &Effect) -> bool {
    match effect {
        Effect::SearchAndSplit { .. }
        | Effect::SearchExileMayCast { .. }
        | Effect::SearchLibraryAndGraveyard { .. }
        | Effect::SearchLibraryOrGraveyard { .. } => true,
        Effect::Sequence(items) => items.iter().any(explicit_search),
        Effect::If {
            then, otherwise, ..
        } => explicit_search(then) || explicit_search(otherwise),
        Effect::May {
            then, otherwise, ..
        } => explicit_search(then) || otherwise.as_deref().is_some_and(explicit_search),
        Effect::MayPay { then, .. } => explicit_search(then),
        Effect::Let { body, .. } | Effect::ForEach { body, .. } | Effect::Repeat { body, .. } => {
            explicit_search(body)
        }
        Effect::FreezeValues { effect } => explicit_search(effect),
        Effect::Modal { modes, .. } => modes.iter().any(|(_, effect)| explicit_search(effect)),
        _ => false,
    }
}
