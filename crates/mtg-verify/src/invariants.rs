use mtg_core::Zone;
use mtg_engine::state::GameState;
use mtg_ir::PrintedCards;
use std::collections::BTreeSet;

/// Structural invariants checked at every semantic-fuzzer/replay choice boundary.
/// These do not stand in for CR propositions or independent differential models.
pub fn verify(state: &GameState, cards: &dyn PrintedCards) -> Result<(), String> {
    let mut ordered = BTreeSet::new();
    for (zone, ids) in &state.zone_order {
        for id in ids {
            if !ordered.insert(*id) {
                return Err("zone_uniqueness: duplicate ordered object".into());
            }
            if state.objects.get(id).is_none_or(|o| o.zone != *zone) {
                return Err("zone_uniqueness: stale zone member".into());
            }
        }
    }
    for (id, object) in &state.objects {
        if *id != object.id {
            return Err("identity: object key differs from identity".into());
        }
        if !state.players.contains_key(&object.owner)
            || !state.players.contains_key(&object.controller)
        {
            return Err("ownership: unknown player".into());
        }
        if cards.face(object.card, object.face).is_none() {
            return Err("card: missing printed characteristics".into());
        }
        if object.zone.zone.is_ordered() && !ordered.contains(id) {
            return Err("zone_uniqueness: missing ordered-zone member".into());
        }
        if object.counters.values().any(|n| *n < 0) {
            return Err("counter: negative count".into());
        }
        if object
            .attached_to
            .is_some_and(|host| !state.objects.contains_key(&host))
        {
            return Err("attachment: absent host".into());
        }
    }
    if state
        .priority
        .is_some_and(|p| state.players.get(&p).is_none_or(|p| p.has_lost))
    {
        return Err("priority: inactive or unknown holder".into());
    }
    for id in state.objects_in(mtg_core::ZoneRef::shared(Zone::Stack)) {
        if state.objects[&id].cast_context.is_none() {
            return Err("stack: missing cast context".into());
        }
    }
    Ok(())
}
