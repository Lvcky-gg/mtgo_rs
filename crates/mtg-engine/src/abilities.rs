//! An object's abilities, printed and granted.
//!
//! A printed ability is found by its id in the object's face. A granted one (layer 6:
//! "enchanted land has '{T}: Add {G}{G}'") has no place in that list; its id is
//! [`AbilityId::granted`] of an index into the object's computed
//! [`Characteristics::granted_abilities`], which says where the text lives. Everything that
//! asks "what does this ability do?" goes through [`find`], so the two kinds look the same
//! to the rest of the engine.
//!
//! An ability on the stack is an object of its own and outlives what granted it: the
//! Aura can leave play and the ability still resolves (CR 113.7a). So a granted ability
//! going on the stack copies its text into [`crate::state::CastContext::granted`], and
//! [`find`] prefers that copy.
//!
//! [`Characteristics::granted_abilities`]: mtg_core::Characteristics::granted_abilities

use std::borrow::Cow;

use mtg_core::{AbilityId, GrantedAbility, ObjectId};
use mtg_ir::{Ability, AbilityKind, PrintedCards, effect::Modification};

use crate::state::GameState;

/// One ability of an object, by id.
pub fn find<'a>(
    state: &'a GameState,
    cards: &'a dyn PrintedCards,
    object: ObjectId,
    id: AbilityId,
) -> Option<Cow<'a, Ability>> {
    let obj = state
        .objects
        .get(&object)
        .or_else(|| state.last_known.get(&object))?;
    if let Some(ctx) = &obj.cast_context
        && ctx.ability == Some(id)
        && let Some(copy) = &ctx.granted
    {
        return Some(Cow::Borrowed(copy));
    }
    match id.granted_index() {
        Some(i) => {
            let ch = crate::layers::compute(state, cards, object)?;
            let text = granted_text(state, cards, ch.granted_abilities.get(i)?)?;
            let mut a = text.clone();
            a.id = id;
            Some(Cow::Owned(a))
        }
        None => cards
            .face(obj.card, obj.face)?
            .abilities
            .iter()
            .find(|a| a.id == id)
            .map(Cow::Borrowed),
    }
}

/// Every ability an object has right now: the printed ones it has not lost, then the
/// granted ones, each carrying the id it is known by.
pub fn current<'a>(
    state: &'a GameState,
    cards: &'a dyn PrintedCards,
    object: ObjectId,
) -> Vec<Cow<'a, Ability>> {
    let Some(obj) = state.objects.get(&object) else {
        return Vec::new();
    };
    let Some(ch) = crate::layers::compute(state, cards, object) else {
        return Vec::new();
    };
    let printed = cards
        .face(obj.card, obj.face)
        .map(|f| f.abilities.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|a| ch.abilities.contains(&a.id))
        .map(Cow::Borrowed);
    let granted = ch
        .granted_abilities
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            let mut a = granted_text(state, cards, g)?.clone();
            a.id = AbilityId::granted(i);
            Some(Cow::Owned(a))
        });
    printed.chain(granted.collect::<Vec<_>>()).collect()
}

/// The text of a granted ability, from the effect that grants it.
fn granted_text<'a>(
    state: &'a GameState,
    cards: &'a dyn PrintedCards,
    g: &GrantedAbility,
) -> Option<&'a Ability> {
    let modification = match g.static_ability {
        Some(aid) => {
            let src = state.objects.get(&g.effect)?;
            let ability = cards
                .face(src.card, src.face)?
                .abilities
                .iter()
                .find(|a| a.id == aid)?;
            match &ability.kind {
                AbilityKind::Static { modification, .. } => modification,
                _ => return None,
            }
        }
        None => {
            &state
                .continuous
                .iter()
                .find(|e| e.id == g.effect)?
                .modification
        }
    };
    match modification {
        Modification::GrantAbility(a) => Some(a),
        _ => None,
    }
}
