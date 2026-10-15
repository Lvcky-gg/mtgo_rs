//! Direct phasing and attachments that phase indirectly with their host.

use crate::{layers, state::GameState};
use mtg_core::{Event, ObjectId, PlayerId, Zone};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn out_events(
    state: &GameState,
    roots: impl IntoIterator<Item = ObjectId>,
) -> Vec<Event> {
    let mut out: BTreeMap<ObjectId, Option<ObjectId>> = roots
        .into_iter()
        .filter(|id| {
            state
                .objects
                .get(id)
                .is_some_and(|o| o.zone.zone == Zone::Battlefield && !o.phased_out)
        })
        .map(|id| (id, None))
        .collect();
    loop {
        let before = out.len();
        for id in state.battlefield() {
            if let Some(host) = state.objects[&id].attached_to
                && host != id
                && out.contains_key(&host)
            {
                // When it would phase out both directly and indirectly, indirect wins.
                out.insert(id, Some(host));
            }
        }
        if out.len() == before {
            break;
        }
    }
    out.into_iter()
        .map(|(object, with)| Event::PhasedOut {
            object,
            out: true,
            with,
            under: layers::controller(state, object),
        })
        .collect()
}

pub(crate) fn in_events(state: &GameState, active: PlayerId) -> Vec<Event> {
    let mut incoming: BTreeSet<ObjectId> = state
        .battlefield_with_phased_out()
        .into_iter()
        .filter(|id| {
            let object = &state.objects[id];
            object.phased_out
                && object.phased_with.is_none()
                && object.phased_under.unwrap_or(object.controller) == active
        })
        .collect();
    loop {
        let before = incoming.len();
        for id in state.battlefield_with_phased_out() {
            let object = &state.objects[&id];
            if object.phased_out
                && object
                    .phased_with
                    .is_some_and(|host| incoming.contains(&host))
            {
                incoming.insert(id);
            }
        }
        if incoming.len() == before {
            break;
        }
    }
    incoming
        .into_iter()
        .map(|object| Event::PhasedOut {
            object,
            out: false,
            with: None,
            under: None,
        })
        .collect()
}
