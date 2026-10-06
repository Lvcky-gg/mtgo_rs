//! Combat legality and damage assignment (CR 506–511).
//!
//! Pure functions over state: who *may* attack, who *may* block whom, and how much
//! damage goes where. The engine's combat flow ([`crate::engine`]) drives these and
//! owns the suspending choices; nothing here mutates anything.
//!
//! # Why the eligible-blockers list is not enough on its own
//!
//! Most blocking restrictions are pairwise — flying can only be blocked by flying or
//! reach — so they can be answered one blocker against one attacker. Menace cannot:
//! "can't be blocked except by two or more creatures" is a property of the *whole*
//! declaration, and a blocker that would be legal alongside a second one is illegal
//! by itself. So there are two layers: [`can_block`] builds the candidate list a UI
//! offers, and [`validate_blocks`] checks a complete declaration. A UI that only
//! consulted the first would let a player make an illegal block.

use std::collections::BTreeMap;

use mtg_core::{CardType, ObjectId, PlayerId, Target};
use mtg_ir::{
    ability::Keyword,
    effect::{Modification, Restriction},
};

use crate::{
    eval::{ComputedChars, Ctx},
    layers::PrintedCards,
    state::GameState,
};

/// Build an evaluation context for one object, for keyword lookups.
fn ctx_for<'a>(
    state: &'a GameState,
    cards: &'a dyn PrintedCards,
    chars: &'a ComputedChars<'a>,
    id: ObjectId,
) -> Ctx<'a> {
    let controller = state
        .objects
        .get(&id)
        .map(|o| o.controller)
        .unwrap_or(state.active_player);
    Ctx {
        state,
        cards,
        chars,
        source: id,
        controller,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    }
}

fn has(state: &GameState, cards: &dyn PrintedCards, id: ObjectId, kw: Keyword) -> bool {
    let chars = ComputedChars(cards);
    ctx_for(state, cards, &chars, id)
        .has_keyword(id, kw)
        .unwrap_or(false)
}

/// Whether this permanent's activated abilities can't be activated (Arrest).
pub fn abilities_blocked(state: &GameState, cards: &dyn PrintedCards, id: ObjectId) -> bool {
    restricted(state, cards, id, |r| {
        matches!(r, Restriction::CantActivateAbilities)
    })
}

/// Whether any continuous effect imposes a restriction on this object.
fn restricted(
    state: &GameState,
    cards: &dyn PrintedCards,
    id: ObjectId,
    want: fn(&Restriction) -> bool,
) -> bool {
    crate::layers::effects(state, cards).iter().any(|e| {
        matches!(&e.modification, Modification::Restriction(r) if want(r))
            && crate::layers::applies(state, cards, e, id)
    })
}

fn is_creature(state: &GameState, cards: &dyn PrintedCards, id: ObjectId) -> bool {
    crate::layers::compute(state, cards, id).is_some_and(|c| c.has_type(CardType::Creature))
}

/// Creatures that could be declared as attackers (CR 508.1a).
pub fn eligible_attackers(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
) -> Vec<ObjectId> {
    state
        .battlefield()
        .into_iter()
        .filter(|id| {
            let Some(obj) = state.objects.get(id) else {
                return false;
            };
            if crate::layers::controller(state, *id) != Some(player) {
                return false;
            }
            if !is_creature(state, cards, *id) {
                return false;
            }
            // Tapped creatures cannot attack (CR 508.1a).
            if obj.tapped {
                return false;
            }
            // Summoning sickness, unless it has haste (CR 302.6).
            if crate::layers::summoning_sick(state, *id) && !has(state, cards, *id, Keyword::Haste)
            {
                return false;
            }
            // Defender (CR 702.3b).
            if has(state, cards, *id, Keyword::Defender) {
                return false;
            }
            if restricted(state, cards, *id, |r| matches!(r, Restriction::CantAttack)) {
                return false;
            }
            if !defender_condition_met(state, cards, *id, player) {
                return false;
            }
            true
        })
        .collect()
}

/// "Can't attack unless defending player controls an Island": each such restriction on
/// the creature is checked against the player it would attack.
fn defender_condition_met(
    state: &GameState,
    cards: &dyn PrintedCards,
    id: ObjectId,
    attacker: PlayerId,
) -> bool {
    let Some(defender) = crate::turn::living(state)
        .into_iter()
        .find(|p| *p != attacker)
    else {
        return true;
    };
    crate::layers::effects(state, cards).iter().all(|e| {
        let mtg_ir::effect::Modification::Restriction(
            Restriction::CantAttackUnlessDefenderControls(filter),
        ) = &e.modification
        else {
            return true;
        };
        if !crate::layers::applies(state, cards, e, id) {
            return true;
        }
        let chars = crate::eval::ComputedChars(cards);
        let ctx = crate::eval::Ctx {
            state,
            cards,
            chars: &chars,
            source: id,
            controller: attacker,
            targets: &[],
            target_legal: &[],
            x: 0,
            bindings: crate::empty_bindings(),
        };
        state.battlefield().into_iter().any(|p| {
            crate::layers::controller(state, p) == Some(defender)
                && crate::eval::matches(&ctx, filter, p).unwrap_or(false)
        })
    })
}

/// Destinations in the current two-seat combat flow: the defending player and
/// planeswalkers they control. Players are listed first for legacy declarations.
pub fn attack_destinations(state: &GameState, cards: &dyn PrintedCards) -> Vec<Target> {
    let Some(defender) = crate::turn::living(state)
        .into_iter()
        .find(|p| *p != state.active_player)
    else {
        return Vec::new();
    };
    let mut destinations = vec![Target::Player(defender)];
    destinations.extend(
        state
            .battlefield()
            .into_iter()
            .filter(|id| {
                crate::layers::controller(state, *id) == Some(defender)
                    && crate::layers::compute(state, cards, *id)
                        .is_some_and(|c| c.has_type(CardType::Planeswalker))
            })
            .map(Target::Object),
    );
    destinations
}

/// Creatures that could block, each with the attackers it could block.
///
/// Pairwise only — see the module docs. A blocker appears here against an attacker it
/// could block *given help*, so menace-protected attackers still list their possible
/// blockers; [`validate_blocks`] rejects a declaration that under-commits.
pub fn eligible_blockers(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
) -> Vec<(ObjectId, Vec<ObjectId>)> {
    let attackers: Vec<ObjectId> = state.combat.attackers.keys().copied().collect();

    state
        .battlefield()
        .into_iter()
        .filter_map(|id| {
            let obj = state.objects.get(&id)?;
            if crate::layers::controller(state, id) != Some(defender) {
                return None;
            }
            if !is_creature(state, cards, id) {
                return None;
            }
            // A tapped creature cannot block (CR 509.1a). Summoning sickness does
            // *not* prevent blocking — only attacking and `{T}` costs.
            if obj.tapped {
                return None;
            }
            if restricted(state, cards, id, |r| matches!(r, Restriction::CantBlock)) {
                return None;
            }

            let can: Vec<ObjectId> = attackers
                .iter()
                .copied()
                .filter(|a| can_block(state, cards, id, *a))
                .collect();
            (!can.is_empty()).then_some((id, can))
        })
        .collect()
}

/// Whether `blocker` could block `attacker`, ignoring restrictions that depend on
/// how many other creatures also block (menace).
pub fn can_block(
    state: &GameState,
    cards: &dyn PrintedCards,
    blocker: ObjectId,
    attacker: ObjectId,
) -> bool {
    let a = |k| has(state, cards, attacker, k);
    let b = |k| has(state, cards, blocker, k);
    let chars = |id| crate::layers::compute(state, cards, id);

    // Flying: only blockable by flying or reach (CR 702.9b).
    if a(Keyword::Flying) && !b(Keyword::Flying) && !b(Keyword::Reach) {
        return false;
    }
    // Protection: can't be blocked by creatures with the quality (CR 702.16f).
    if crate::eval::protected_from(state, cards, attacker, blocker) {
        return false;
    }
    // Shadow works both ways (CR 702.28b).
    if a(Keyword::Shadow) != b(Keyword::Shadow) {
        return false;
    }
    if a(Keyword::Horsemanship) && !b(Keyword::Horsemanship) {
        return false;
    }
    let (Some(ac), Some(bc)) = (chars(attacker), chars(blocker)) else {
        return false;
    };
    let artifact = bc.has_type(CardType::Artifact);
    if a(Keyword::Fear) && !artifact && !bc.colors.contains(mtg_core::Color::Black) {
        return false;
    }
    if a(Keyword::Intimidate) && !artifact && ac.colors.intersect(bc.colors).is_colorless() {
        return false;
    }
    if a(Keyword::Skulk) && bc.power.unwrap_or(0) > ac.power.unwrap_or(0) {
        return false;
    }
    // Landwalk: the defending player is the one controlling the blocker (CR 702.14c).
    for (walk, land) in [
        (Keyword::Plainswalk, "Plains"),
        (Keyword::Islandwalk, "Island"),
        (Keyword::Swampwalk, "Swamp"),
        (Keyword::Mountainwalk, "Mountain"),
        (Keyword::Forestwalk, "Forest"),
    ] {
        if a(walk)
            && let Some(defender) = crate::layers::controller(state, blocker)
            && controls_land_type(state, cards, defender, land)
        {
            return false;
        }
    }
    // "Can't be blocked except by …" on the attacker, "can block only …" on the blocker,
    // each evaluated from its own creature's point of view.
    for e in crate::layers::effects(state, cards) {
        let Modification::Restriction(r) = &e.modification else {
            continue;
        };
        let (subject, filter, other) = match r {
            Restriction::CantBeBlockedExceptBy(f) => (attacker, f, blocker),
            Restriction::CanBlockOnly(f) => (blocker, f, attacker),
            _ => continue,
        };
        if !crate::layers::applies(state, cards, &e, subject) {
            continue;
        }
        let chars = ComputedChars(cards);
        let ctx = ctx_for(state, cards, &chars, subject);
        if !crate::eval::matches(&ctx, filter, other).unwrap_or(false) {
            return false;
        }
    }
    true
}

/// Whether a player controls a land with a given subtype.
fn controls_land_type(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    subtype: &str,
) -> bool {
    state.battlefield().into_iter().any(|id| {
        crate::layers::controller(state, id) == Some(player)
            && crate::layers::compute(state, cards, id).is_some_and(|c| {
                c.has_type(CardType::Land)
                    && c.subtypes
                        .iter()
                        .any(|s| cards.subtype_name(*s) == Some(subtype))
            })
    })
}

/// Creatures that must attack this combat if able (CR 508.1d): of those that can, the
/// ones a "attacks each combat if able" requirement applies to.
///
/// A lone required attacker that can't attack alone is not required: obeying it would
/// need a second creature the rules don't oblige to attack, so this errs towards the
/// declaration that is always legal.
pub fn must_attack(state: &GameState, cards: &dyn PrintedCards, player: PlayerId) -> Vec<ObjectId> {
    let must: Vec<ObjectId> = eligible_attackers(state, cards, player)
        .into_iter()
        .filter(|id| {
            restricted(state, cards, *id, |r| {
                matches!(r, Restriction::MustAttackIfAble | Restriction::Goaded)
            })
        })
        .collect();
    if attacking_alone_illegal(state, cards, &must) {
        return Vec::new();
    }
    must
}

/// Whether a declaration of exactly these attackers breaks "can't attack alone"
/// (CR 506.5): one creature attacking by itself when it may not.
pub fn attacking_alone_illegal(
    state: &GameState,
    cards: &dyn PrintedCards,
    attackers: &[ObjectId],
) -> bool {
    matches!(attackers, [only] if restricted(state, cards, *only, |r| {
        matches!(r, Restriction::CantAttackAlone)
    }))
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BlockError {
    /// A creature was declared as a blocker but cannot block that attacker.
    IllegalPair {
        blocker: ObjectId,
        attacker: ObjectId,
    },
    /// Menace: blocked by exactly one creature (CR 702.110b).
    NeedsMoreBlockers {
        attacker: ObjectId,
        have: usize,
        need: usize,
    },
    /// The creature is not on the battlefield, or is not the defender's.
    NotAvailable { blocker: ObjectId },
    /// One creature was declared as blocking two different attackers.
    BlockingTwice { blocker: ObjectId },
    /// "Can't be blocked by more than one creature."
    TooManyBlockers { attacker: ObjectId, max: usize },
    /// "Can't block alone": the only creature declared as a blocker (CR 506.5).
    BlockingAlone { blocker: ObjectId },
    /// Another legal declaration obeys more block requirements (CR 509.1c) — `better`
    /// is one, as blocker/attacker pairs.
    RequirementUnmet { better: Vec<(ObjectId, ObjectId)> },
}

/// Check a complete block declaration.
///
/// `blocks` maps each attacker to the creatures blocking it.
pub fn validate_blocks(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
    blocks: &BTreeMap<ObjectId, Vec<ObjectId>>,
) -> Result<(), BlockError> {
    validate_restrictions(state, cards, defender, blocks)?;
    let requirements = block_requirements(state, cards, defender);
    match better_blocks(state, cards, defender, &requirements, blocks) {
        Some(better) => Err(BlockError::RequirementUnmet {
            better: pairs(&better),
        }),
        None => Ok(()),
    }
}

/// One block requirement (CR 509.1c): `blocker` must block (`attacker`, or anything when
/// `None`), or `attacker` must be blocked by someone when `blocker` is `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Requirement {
    pub blocker: Option<ObjectId>,
    pub attacker: Option<ObjectId>,
}

/// Every block requirement on this declaration that some creature could obey.
pub fn block_requirements(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
) -> Vec<Requirement> {
    let effects: Vec<_> = crate::layers::effects(state, cards)
        .into_iter()
        .filter(|e| {
            matches!(
                &e.modification,
                Modification::Restriction(
                    Restriction::MustBlock
                        | Restriction::MustBlockSource
                        | Restriction::MustBeBlocked
                        | Restriction::MustBeBlockedByAll(_)
                )
            )
        })
        .collect();
    if effects.is_empty() {
        return Vec::new();
    }
    let eligible = eligible_blockers(state, cards, defender);
    let chars = ComputedChars(cards);
    let mut out = Vec::new();
    for e in &effects {
        let Modification::Restriction(r) = &e.modification else {
            continue;
        };
        match r {
            Restriction::MustBlock => out.extend(
                eligible
                    .iter()
                    .filter(|(b, _)| crate::layers::applies(state, cards, e, *b))
                    .map(|(b, _)| Requirement {
                        blocker: Some(*b),
                        attacker: None,
                    }),
            ),
            Restriction::MustBlockSource => out.extend(
                eligible
                    .iter()
                    .filter(|(b, can)| {
                        can.contains(&e.source) && crate::layers::applies(state, cards, e, *b)
                    })
                    .map(|(b, _)| Requirement {
                        blocker: Some(*b),
                        attacker: Some(e.source),
                    }),
            ),
            Restriction::MustBeBlocked => {
                for a in state.combat.attackers.keys() {
                    if crate::layers::applies(state, cards, e, *a)
                        && eligible.iter().any(|(_, can)| can.contains(a))
                    {
                        out.push(Requirement {
                            blocker: None,
                            attacker: Some(*a),
                        });
                    }
                }
            }
            Restriction::MustBeBlockedByAll(filter) => {
                for a in state.combat.attackers.keys() {
                    if !crate::layers::applies(state, cards, e, *a) {
                        continue;
                    }
                    let ctx = ctx_for(state, cards, &chars, *a);
                    for (b, can) in &eligible {
                        if can.contains(a)
                            && crate::eval::matches(&ctx, filter, *b).unwrap_or(false)
                        {
                            out.push(Requirement {
                                blocker: Some(*b),
                                attacker: Some(*a),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// How many requirements a declaration obeys.
fn obeyed(requirements: &[Requirement], blocks: &BTreeMap<ObjectId, Vec<ObjectId>>) -> usize {
    requirements
        .iter()
        .filter(|r| match (r.blocker, r.attacker) {
            (Some(b), Some(a)) => blocks.get(&a).is_some_and(|bs| bs.contains(&b)),
            (Some(b), None) => blocks.values().any(|bs| bs.contains(&b)),
            (None, Some(a)) => blocks.get(&a).is_some_and(|bs| !bs.is_empty()),
            (None, None) => false,
        })
        .count()
}

/// A legal declaration one step away from `blocks` that obeys more requirements, if there
/// is one (CR 509.1c: the maximum number of requirements must be obeyed).
///
/// A step moves one creature onto an attacker — instead of what it was blocking, or as
/// well when it may block several — with a second creature joining when the attacker
/// needs more blockers (menace). This is a local search: it never rejects a declaration
/// unless it has a better one in hand, but it can miss improvements that take several
/// creatures changing at once.
fn better_blocks(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
    requirements: &[Requirement],
    blocks: &BTreeMap<ObjectId, Vec<ObjectId>>,
) -> Option<BTreeMap<ObjectId, Vec<ObjectId>>> {
    if requirements.is_empty() {
        return None;
    }
    let now = obeyed(requirements, blocks);
    let eligible = eligible_blockers(state, cards, defender);
    let moved = |from: &BTreeMap<ObjectId, Vec<ObjectId>>, b: ObjectId, a: ObjectId, keep: bool| {
        let mut next = from.clone();
        if !keep {
            for bs in next.values_mut() {
                bs.retain(|x| *x != b);
            }
        }
        let bs = next.entry(a).or_default();
        if !bs.contains(&b) {
            bs.push(b);
        }
        next.retain(|_, bs| !bs.is_empty());
        next
    };
    for (b, can) in &eligible {
        for a in can {
            let mut candidates = vec![moved(blocks, *b, *a, false)];
            let current = blocks.values().filter(|bs| bs.contains(b)).count();
            if current > 0 && current < block_capacity(state, cards, *b) {
                candidates.push(moved(blocks, *b, *a, true));
            }
            let need = minimum_blockers(state, cards, *a);
            for next in candidates {
                if next.get(a).map_or(0, Vec::len) < need {
                    // Menace: try each second creature alongside.
                    for (d, dcan) in &eligible {
                        if d == b || !dcan.contains(a) {
                            continue;
                        }
                        let pair = moved(&next, *d, *a, false);
                        if obeyed(requirements, &pair) > now
                            && validate_restrictions(state, cards, defender, &pair).is_ok()
                        {
                            return Some(pair);
                        }
                    }
                    continue;
                }
                if obeyed(requirements, &next) > now
                    && validate_restrictions(state, cards, defender, &next).is_ok()
                {
                    return Some(next);
                }
            }
        }
    }
    None
}

/// A legal declaration obeying as many block requirements as the search finds: what a
/// player who makes no decisions declares. Empty when nothing is required.
pub fn required_blocks(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
) -> Vec<(ObjectId, ObjectId)> {
    let requirements = block_requirements(state, cards, defender);
    let mut blocks = BTreeMap::new();
    while let Some(next) = better_blocks(state, cards, defender, &requirements, &blocks) {
        blocks = next;
    }
    pairs(&blocks)
}

fn pairs(blocks: &BTreeMap<ObjectId, Vec<ObjectId>>) -> Vec<(ObjectId, ObjectId)> {
    blocks
        .iter()
        .flat_map(|(a, bs)| bs.iter().map(move |b| (*b, *a)))
        .collect()
}

/// Check a declaration against everything but block requirements.
fn validate_restrictions(
    state: &GameState,
    cards: &dyn PrintedCards,
    defender: PlayerId,
    blocks: &BTreeMap<ObjectId, Vec<ObjectId>>,
) -> Result<(), BlockError> {
    let mut seen: Vec<ObjectId> = Vec::new();
    let mut count: BTreeMap<ObjectId, usize> = BTreeMap::new();

    for (attacker, blockers) in blocks {
        if blockers.is_empty() {
            continue;
        }
        for (i, b) in blockers.iter().enumerate() {
            let before = *count.get(b).unwrap_or(&0);
            if blockers[..i].contains(b) {
                return Err(BlockError::BlockingTwice { blocker: *b });
            }
            if before >= block_capacity(state, cards, *b) {
                return Err(BlockError::BlockingTwice { blocker: *b });
            }
            count.insert(*b, before + 1);
            if before == 0 {
                seen.push(*b);
            }

            let ok = state
                .objects
                .get(b)
                .is_some_and(|o| o.zone.zone == mtg_core::Zone::Battlefield && !o.tapped)
                && crate::layers::controller(state, *b) == Some(defender)
                && is_creature(state, cards, *b);
            if !ok {
                return Err(BlockError::NotAvailable { blocker: *b });
            }
            if !can_block(state, cards, *b, *attacker) {
                return Err(BlockError::IllegalPair {
                    blocker: *b,
                    attacker: *attacker,
                });
            }
        }

        if blockers.len() > 1
            && restricted(state, cards, *attacker, |r| {
                matches!(r, Restriction::CantBeBlockedByMoreThanOne)
            })
        {
            return Err(BlockError::TooManyBlockers {
                attacker: *attacker,
                max: 1,
            });
        }
        // Menace is checked here, not pairwise, because it is a property of the
        // whole declaration.
        let need = minimum_blockers(state, cards, *attacker);
        if blockers.len() < need {
            return Err(BlockError::NeedsMoreBlockers {
                attacker: *attacker,
                have: blockers.len(),
                need,
            });
        }
    }

    if let [only] = seen[..]
        && restricted(state, cards, only, |r| {
            matches!(r, Restriction::CantBlockAlone)
        })
    {
        return Err(BlockError::BlockingAlone { blocker: only });
    }

    Ok(())
}

/// How many attackers this creature may block (CR 509.1a): one, plus "can block an
/// additional creature each combat"; `usize::MAX` for "any number".
pub fn block_capacity(state: &GameState, cards: &dyn PrintedCards, blocker: ObjectId) -> usize {
    let mut extra = 0usize;
    for e in crate::layers::effects(state, cards) {
        if let Modification::Restriction(Restriction::BlockAdditional(n)) = &e.modification
            && crate::layers::applies(state, cards, &e, blocker)
        {
            match n {
                Some(n) => extra = extra.saturating_add(usize::from(*n)),
                None => return usize::MAX,
            }
        }
    }
    1 + extra
}

/// Whether this attacker's controller may assign its combat damage as though it weren't
/// blocked (CR 510.1c): all of it to the player or planeswalker it attacks.
pub fn assigns_as_though_unblocked(
    state: &GameState,
    cards: &dyn PrintedCards,
    attacker: ObjectId,
) -> bool {
    restricted(state, cards, attacker, |r| {
        matches!(r, Restriction::AssignAsThoughUnblocked)
    })
}

/// The attackers a creature is blocking, in declaration order.
pub fn blocked_by(state: &GameState, blocker: ObjectId) -> Vec<ObjectId> {
    state
        .combat
        .blocks
        .iter()
        .filter(|(_, bs)| bs.contains(&blocker))
        .map(|(a, _)| *a)
        .collect()
}

/// The fewest creatures that may block this attacker, if any block it: two for menace
/// (CR 702.110b), more for "can't be blocked except by three or more creatures".
pub fn minimum_blockers(state: &GameState, cards: &dyn PrintedCards, attacker: ObjectId) -> usize {
    let menace = if has(state, cards, attacker, Keyword::Menace) {
        2
    } else {
        1
    };
    crate::layers::effects(state, cards)
        .iter()
        .filter_map(|e| match &e.modification {
            Modification::Restriction(Restriction::MinimumBlockers(n))
                if crate::layers::applies(state, cards, e, attacker) =>
            {
                Some(usize::from(*n))
            }
            _ => None,
        })
        .fold(menace, usize::max)
}

/// How much damage is needed to destroy a creature right now (CR 510.1c).
///
/// Damage already marked counts, so a creature that has taken damage this turn needs
/// less. Deathtouch makes any nonzero amount lethal (CR 702.2b).
pub fn lethal_damage(
    state: &GameState,
    cards: &dyn PrintedCards,
    victim: ObjectId,
    source_has_deathtouch: bool,
) -> u32 {
    let toughness = crate::layers::compute(state, cards, victim)
        .and_then(|c| c.toughness)
        .unwrap_or(0);
    let marked = state.objects.get(&victim).map_or(0, |o| o.damage);
    let remaining = (i64::from(toughness) - i64::from(marked)).max(0) as u32;
    if source_has_deathtouch {
        remaining.min(1)
    } else {
        remaining
    }
}

/// One creature's combat damage assignment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Assignment {
    pub source: ObjectId,
    pub deathtouch: bool,
    /// Where the damage goes, in the order it was assigned.
    pub to: Vec<(Target, u32)>,
}

/// The canonical assignment for an attacking creature.
///
/// A convenient legal division under CR 510.1c: assign lethal to each blocker
/// in turn. The controller may instead choose any division among the blockers.
/// Whatever remains goes to the last blocker, or — with trample — to the player being
/// attacked (CR 702.19b).
///
/// This is *an* assignment, not the only legal one: a player may assign more than
/// lethal to an early blocker. That is a real choice, and the engine only asks when
/// it could matter — see [`assignment_is_forced`].
pub fn assign_attacker_damage(
    state: &GameState,
    cards: &dyn PrintedCards,
    attacker: ObjectId,
    defender: Target,
) -> Option<Assignment> {
    let power = crate::layers::compute(state, cards, attacker)
        .and_then(|c| c.power)
        .unwrap_or(0)
        .max(0) as u32;
    let deathtouch = has(state, cards, attacker, Keyword::Deathtouch);
    let trample = has(state, cards, attacker, Keyword::Trample);

    let blockers = state
        .combat
        .blocks
        .get(&attacker)
        .cloned()
        .unwrap_or_default();
    let mut out = Assignment {
        source: attacker,
        deathtouch,
        to: Vec::new(),
    };

    if blockers.is_empty() {
        // Blocked by nothing but still blocked deals no damage (CR 509.1h) — unless it
        // may assign its damage as though it weren't blocked, which it then does.
        if state.combat.was_blocked.contains(&attacker)
            && !trample
            && !assigns_as_though_unblocked(state, cards, attacker)
        {
            return Some(out);
        }
        if power > 0 {
            out.to.push((defender, power));
        }
        return Some(out);
    }

    let mut left = power;
    for (i, b) in blockers.iter().enumerate() {
        if left == 0 {
            break;
        }
        let need = lethal_damage(state, cards, *b, deathtouch).min(left);
        let last = i + 1 == blockers.len();
        let amount = if last && !trample { left } else { need };
        if amount > 0 {
            out.to.push((Target::Object(*b), amount));
        }
        left -= amount.min(left);
    }

    // Trample sends the excess through to the defending player.
    if trample && left > 0 {
        out.to.push((defender, left));
    }

    Some(out)
}

/// Whether an attacker's damage assignment has only one legal division.
/// Multiple blockers always allow a choice with positive power (CR 510.1c).
/// Trample also allows choosing between excess damage and overassigning to a blocker.
pub fn assignment_is_forced(
    state: &GameState,
    cards: &dyn PrintedCards,
    attacker: ObjectId,
) -> bool {
    let blockers = state
        .combat
        .blocks
        .get(&attacker)
        .cloned()
        .unwrap_or_default();
    let power = crate::layers::compute(state, cards, attacker)
        .and_then(|c| c.power)
        .unwrap_or(0)
        .max(0) as u32;
    if power == 0 || blockers.is_empty() {
        return true;
    }
    if blockers.len() > 1 || assigns_as_though_unblocked(state, cards, attacker) {
        return false;
    }
    !has(state, cards, attacker, Keyword::Trample)
        || power
            <= lethal_damage(
                state,
                cards,
                blockers[0],
                has(state, cards, attacker, Keyword::Deathtouch),
            )
}

/// A blocking creature's damage assignment (CR 510.1d): all of it to the attacker it
/// blocks; blocking several, lethal damage to each in turn and the rest to the last — *a*
/// division, which its controller may change (see `Engine::next_damage_assignment`).
pub fn assign_blocker_damage(
    state: &GameState,
    cards: &dyn PrintedCards,
    blocker: ObjectId,
    attackers: &[ObjectId],
) -> Option<Assignment> {
    let power = crate::layers::compute(state, cards, blocker)
        .and_then(|c| c.power)
        .unwrap_or(0)
        .max(0) as u32;
    if power == 0 || attackers.is_empty() {
        return None;
    }
    let deathtouch = has(state, cards, blocker, Keyword::Deathtouch);
    let mut left = power;
    let mut to = Vec::new();
    for (i, a) in attackers.iter().enumerate() {
        let amount = if i + 1 == attackers.len() {
            left
        } else {
            lethal_damage(state, cards, *a, deathtouch).min(left)
        };
        if amount > 0 {
            to.push((Target::Object(*a), amount));
        }
        left -= amount;
    }
    Some(Assignment {
        source: blocker,
        deathtouch,
        to,
    })
}

/// Whether a creature deals damage in the given combat damage step (CR 510.4).
pub fn deals_damage_now(
    state: &GameState,
    cards: &dyn PrintedCards,
    id: ObjectId,
    first_strike_step: bool,
) -> bool {
    let first = has(state, cards, id, Keyword::FirstStrike);
    let double = has(state, cards, id, Keyword::DoubleStrike);
    if first_strike_step {
        first || double
    } else {
        state
            .combat
            .first_strike_participants
            .as_ref()
            .is_none_or(|participants| !participants.contains(&id))
            || double
    }
}

/// Whether combat needs a first-strike damage step (CR 510.4).
pub fn needs_first_strike_step(state: &GameState, cards: &dyn PrintedCards) -> bool {
    state
        .combat
        .attackers
        .keys()
        .copied()
        .chain(state.combat.blocks.values().flatten().copied())
        .any(|id| {
            has(state, cards, id, Keyword::FirstStrike)
                || has(state, cards, id, Keyword::DoubleStrike)
        })
}
