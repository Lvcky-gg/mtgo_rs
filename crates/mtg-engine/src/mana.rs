//! Mana: what is available, and whether a cost can be paid with it.
//!
//! # Why this is not a counting problem
//!
//! The obvious implementation is to total up available mana per colour and compare
//! against the cost. That is wrong, and it is wrong in a way that shows up
//! immediately in real games, because a source that can produce one of several
//! colours gets counted once *per colour*.
//!
//! One land producing "white or blue" makes a per-colour count report one white
//! available and one blue available. A cost of `{W}{U}` then looks payable. It is
//! not: the land produces a single mana, and spending it on white means it is not
//! available for blue. The count double-counts the same physical mana.
//!
//! Note the converse, which the algorithm must *not* get wrong either: two such
//! lands genuinely do pay `{W}{W}`, because each one independently chooses white.
//! An implementation that simply capped each colour at the number of sources
//! dedicated to it would wrongly reject that.
//!
//! The correct model is a **bipartite matching**: coloured symbols on one side,
//! individual units of producible mana on the other, an edge where that unit could
//! satisfy that symbol. The cost is payable when every coloured symbol can be
//! matched to a **distinct** unit, with enough units left over for the generic part.
//! Distinctness is the whole point — it is what stops one mana paying for two
//! symbols, and what makes both cases above come out right.
//! This module solves that with augmenting paths (Kuhn's algorithm), which is ample
//! for the handful of sources a real board has.
//!
//! # One algorithm, not two
//!
//! "Can I cast this?" and "how do I pay for it?" are the same question asked twice,
//! so they share one implementation: [`plan`] returns a concrete payment or `None`,
//! and [`can_pay`] is `plan(..).is_some()`. Keeping them separate would create two
//! pieces of code that must agree about a subtle algorithm forever, and they would
//! eventually disagree — offering a player a spell they cannot actually pay for, or
//! hiding one they can.

use mtg_core::{AbilityId, Color, ManaCost, ManaSymbol, ObjectId, PlayerId};
use mtg_ir::{AbilityKind, Effect, effect::ManaOutput};

use crate::{layers::PrintedCards, state::GameState};

/// An activatable mana ability on the battlefield.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ManaSource {
    pub object: ObjectId,
    pub ability: AbilityId,
    /// What activating it produces. More than one entry means more than one mana.
    pub outputs: Vec<ManaOutput>,
    /// Whether activating it taps the permanent.
    pub taps: bool,
    /// Whether activating it sacrifices the permanent (a Treasure). Such sources are
    /// spent last, so paying for a spell never uses up a Treasure while a land would do.
    pub sacrifices: bool,
    /// Mana that can pay only for spells matching this ("spend this mana only to cast
    /// creature spells"); such a source is used only when planning for such a spell.
    pub only: Option<mtg_ir::ObjectFilter>,
    /// `only` limits which spells it may cast, but not what else it may pay for.
    pub spells_only: bool,
}

/// The ability id that marks a creature or artifact tapped to help pay for a spell
/// (convoke, improvise) rather than a mana ability.
pub const HELPER: AbilityId = AbilityId(u16::MAX);

/// The ability id that marks a card exiled from a graveyard to help pay (delve).
pub const DELVE: AbilityId = AbilityId(u16::MAX - 1);

/// One unit of mana that could exist, and the colours it could be.
///
/// A source producing two mana contributes two units. Floating mana already in the
/// pool contributes units with a single fixed colour.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Unit {
    /// `None` for mana already floating in the pool.
    source: Option<usize>,
    /// Colored alternatives this unit can produce.
    colors: Vec<Color>,
    /// Colorless may be an alternative to a colored tapping ability.
    colorless: bool,
    /// A convoking or improvising permanent with no colour to give: it pays generic
    /// only, never `{C}`.
    generic_only: bool,
}

impl Unit {
    fn satisfies(&self, req: &ColorRequirement) -> bool {
        if self.generic_only {
            return false;
        }
        match req {
            ColorRequirement::OneOf(want) => self.colors.iter().any(|c| want.contains(c)),
            // {C} requires a colorless output, including an alternative ability
            // on a permanent that can also produce colored mana.
            ColorRequirement::Colorless => self.colorless,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ColorRequirement {
    /// Payable with any one of these colours. A plain `{W}` is `OneOf([White])`;
    /// hybrid `{W/U}` is `OneOf([White, Blue])`.
    OneOf(Vec<Color>),
    /// `{C}` — must be colorless.
    Colorless,
}

/// A mana cost broken into what the matching needs.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Requirements {
    pub colored: Vec<ColorRequirement>,
    pub generic: u32,
    /// Phyrexian symbols, payable with 2 life each if mana is not available.
    pub phyrexian: Vec<Color>,
    /// `{2/W}`-style symbols: either the colour, or that much generic.
    pub mono_hybrid: Vec<(u8, Color)>,
    pub has_x: bool,
}

/// Break a cost into requirements. `x` is the announced value for `{X}`.
pub fn requirements(cost: &ManaCost, x: u32) -> Requirements {
    let mut r = Requirements::default();
    for s in &cost.symbols {
        match s {
            ManaSymbol::Generic(n) => r.generic += u32::from(*n),
            ManaSymbol::Variable => {
                r.has_x = true;
                r.generic += x;
            }
            ManaSymbol::Colored(c) => r.colored.push(ColorRequirement::OneOf(vec![*c])),
            ManaSymbol::Colorless => r.colored.push(ColorRequirement::Colorless),
            ManaSymbol::Hybrid(a, b) => r.colored.push(ColorRequirement::OneOf(vec![*a, *b])),
            ManaSymbol::MonoHybrid(n, c) => r.mono_hybrid.push((*n, *c)),
            ManaSymbol::Phyrexian(c) => r.phyrexian.push(*c),
            // Snow mana needs the source to be snow, which is a property of the
            // permanent rather than of the mana. Treated as generic until snow
            // sources exist.
            ManaSymbol::Snow => r.generic += 1,
        }
    }
    r
}

/// A concrete way to pay a cost.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Payment {
    /// Sources to activate, and which colour each should make. `None` means
    /// colorless, or that the source has no choice to make.
    pub activate: Vec<(ObjectId, AbilityId, Option<Color>)>,
    /// Mana to remove from the pool once the activations have happened, by colour
    /// slot. This covers both mana that was already floating and mana the
    /// activations produce, because produced mana goes to the pool first — there is
    /// no separate path by which a source pays a cost directly.
    pub spend: [u16; 6],
    /// Life paid for phyrexian symbols.
    pub life: u32,
    eligible_restricted: Vec<usize>,
    prior_restricted: usize,
    eligible_sources: Vec<ObjectId>,
}

impl Payment {
    pub fn total_mana(&self) -> usize {
        self.spend.iter().map(|n| *n as usize).sum()
    }
}

/// Whether a cost can be paid. Defined in terms of [`plan`] so the two can never
/// disagree.
pub fn can_pay(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    cost: &ManaCost,
    x: u32,
) -> bool {
    plan(state, cards, player, cost, x).is_some()
}

/// Work out one way to pay a cost, or `None` if it cannot be paid.
///
/// The returned plan is deterministic, so both peers compute the same one. Among the
/// payments that work it prefers floating mana, then the least flexible sources — a
/// basic before a dual — so auto-tapping leaves the most options open. That preference
/// only orders the search; it never rejects a payment that exists. A player who wants a
/// specific payment taps by hand first (`Action::ActivateManaAbility`), and the planner
/// then spends the floating mana before tapping anything else.
pub fn plan(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    cost: &ManaCost,
    x: u32,
) -> Option<Payment> {
    let mut sources = mana_sources(state, cards, player);
    // Restricted mana pays only for the spells it names (`plan_spell`) — unless only
    // spells are restricted, and this is not one.
    sources.retain(|s| s.only.is_none() || s.spells_only);
    let (pool, eligible) = eligible_pool(state, cards, player, None);
    let life = state.player(player).life;
    let req = requirements(cost, x);
    annotate_payment(
        plan_with(&req, &sources, pool, life)?,
        state,
        &sources,
        eligible,
    )
}

/// [`plan`] for casting a particular spell: with convoke or improvise, untapped creatures
/// or artifacts its controller controls can help pay (CR 702.51, 702.126).
pub fn plan_spell(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    cost: &ManaCost,
    x: u32,
    spell: ObjectId,
) -> Option<Payment> {
    let mut sources = mana_sources(state, cards, player);
    // An activated ability announced through the same path is not a spell: mana
    // restricted to spells pays for it only when the restriction is about spells alone.
    let is_ability = state
        .objects
        .get(&spell)
        .and_then(|o| o.cast_context.as_ref())
        .is_some_and(|c| c.ability.is_some());
    // "Spend this mana only to cast creature spells": kept only if this spell qualifies.
    sources.retain(|s| {
        if is_ability {
            return s.only.is_none() || s.spells_only;
        }
        s.only.as_ref().is_none_or(|f| {
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source: s.object,
                controller: player,
                targets: &[],
                target_legal: &[],
                x: 0,
                bindings: crate::empty_bindings(),
            };
            crate::eval::matches(&ctx, f, spell).unwrap_or(false)
        })
    });
    let has = |k| {
        state
            .objects
            .get(&spell)
            .and_then(|o| cards.face(o.card, o.face))
            .is_some_and(|f| {
                f.abilities
                    .iter()
                    .any(|a| matches!(a.kind, AbilityKind::Keyword(x) if x == k))
            })
    };
    let (convoke, improvise) = (
        has(mtg_core::Keyword::Convoke),
        has(mtg_core::Keyword::Improvise),
    );
    if convoke || improvise {
        let tapped_for_mana: Vec<ObjectId> = sources.iter().map(|s| s.object).collect();
        for id in state.battlefield() {
            let Some(obj) = state.objects.get(&id) else {
                continue;
            };
            if obj.tapped || crate::layers::controller(state, id) != Some(player) {
                continue;
            }
            let Some(ch) = crate::layers::compute(state, cards, id) else {
                continue;
            };
            // One use per permanent: a land-creature with a mana ability is left to it.
            if tapped_for_mana.contains(&id) {
                continue;
            }
            let output = if convoke && ch.has_type(mtg_core::CardType::Creature) {
                let colors: Vec<Color> = [
                    Color::White,
                    Color::Blue,
                    Color::Black,
                    Color::Red,
                    Color::Green,
                ]
                .into_iter()
                .filter(|c| ch.colors.contains(*c))
                .collect();
                match colors.len() {
                    0 => ManaOutput::Colorless,
                    1 => ManaOutput::Colored(colors[0]),
                    _ => ManaOutput::AnyOf(colors),
                }
            } else if improvise && ch.has_type(mtg_core::CardType::Artifact) {
                ManaOutput::Colorless
            } else {
                continue;
            };
            sources.push(ManaSource {
                object: id,
                ability: HELPER,
                outputs: vec![output],
                taps: true,
                sacrifices: false,
                only: None,
                spells_only: false,
            });
        }
    }
    // CR 702.66 — delve: each card exiled from your graveyard pays for {1}. Spent after
    // everything else, like a sacrificed source.
    if has(mtg_core::Keyword::Delve) {
        for id in state.objects_in(mtg_core::ZoneRef::of(mtg_core::Zone::Graveyard, player)) {
            if id == spell {
                continue;
            }
            sources.push(ManaSource {
                object: id,
                ability: DELVE,
                outputs: vec![ManaOutput::Colorless],
                taps: false,
                sacrifices: true,
                only: None,
                spells_only: false,
            });
        }
    }
    let (pool, eligible) = eligible_pool(state, cards, player, (!is_ability).then_some(spell));
    let life = state.player(player).life;
    annotate_payment(
        plan_with(&requirements(cost, x), &sources, pool, life)?,
        state,
        &sources,
        eligible,
    )
}

fn eligible_pool(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    spell: Option<ObjectId>,
) -> ([u16; 6], Vec<usize>) {
    let mut pool = state.player(player).mana.amounts;
    let mut eligible = Vec::new();
    for (index, bucket) in state.restricted_mana.iter().enumerate() {
        if bucket.player != player {
            continue;
        }
        let allowed = bucket.restrictions.iter().all(|(filter, spells_only)| {
            let Some(spell) = spell else {
                return *spells_only;
            };
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source: bucket.source,
                controller: player,
                targets: &[],
                target_legal: &[],
                x: 0,
                bindings: crate::empty_bindings(),
            };
            crate::eval::matches(&ctx, filter, spell).unwrap_or(false)
        });
        if allowed {
            eligible.push(index);
        } else {
            for (slot, amount) in bucket.amounts.iter().enumerate() {
                pool[slot] = pool[slot].saturating_sub(*amount);
            }
        }
    }
    (pool, eligible)
}

fn annotate_payment(
    mut payment: Payment,
    state: &GameState,
    sources: &[ManaSource],
    eligible: Vec<usize>,
) -> Option<Payment> {
    payment.eligible_restricted = eligible;
    payment.prior_restricted = state.restricted_mana.len();
    payment.eligible_sources = sources.iter().map(|source| source.object).collect();
    Some(payment)
}

/// Remove only provenance for units authorized by this payment. Aggregate counts
/// still flow through ManaSpent events, so the event log remains unchanged.
pub(crate) fn consume_restricted(state: &mut GameState, player: PlayerId, payment: &Payment) {
    let pool = state.player(player).mana.amounts;
    for (slot, total) in pool.iter().enumerate() {
        let restricted: u32 = state
            .restricted_mana
            .iter()
            .filter(|bucket| bucket.player == player)
            .map(|bucket| u32::from(bucket.amounts[slot]))
            .sum();
        let unrestricted = u32::from(*total).saturating_sub(restricted);
        let mut remaining = u32::from(payment.spend[slot]).saturating_sub(unrestricted);
        for (index, bucket) in state.restricted_mana.iter_mut().enumerate() {
            if bucket.player != player
                || !(payment.eligible_restricted.contains(&index)
                    || (index >= payment.prior_restricted
                        && payment.eligible_sources.contains(&bucket.source)))
            {
                continue;
            }
            let used = remaining.min(u32::from(bucket.amounts[slot]));
            bucket.amounts[slot] -= used as u16;
            remaining -= used;
        }
    }
    state
        .restricted_mana
        .retain(|bucket| bucket.amounts.iter().any(|amount| *amount > 0));
}

/// The algorithm proper, separated from state so it can be tested directly.
pub fn plan_with(
    req: &Requirements,
    sources: &[ManaSource],
    pool: [u16; 6],
    life: i32,
) -> Option<Payment> {
    // Each mono-hybrid symbol offers color or generic payment. Reject infeasible
    // prefixes and remember equivalent failed allocations instead of truncating
    // the cost or relying on a machine-sized bit mask.
    fn alternatives(
        index: usize,
        symbols: &[(u8, Color)],
        trial: &mut Requirements,
        sources: &[ManaSource],
        pool: [u16; 6],
        life: i32,
        failed: &mut std::collections::BTreeSet<(usize, u32, [usize; 5])>,
    ) -> Option<Payment> {
        let mut counts = [0; 5];
        for requirement in &trial.colored {
            if let ColorRequirement::OneOf(colors) = requirement
                && colors.len() == 1
            {
                counts[colors[0] as usize] += 1;
            }
        }
        let key = (index, trial.generic, counts);
        if failed.contains(&key) {
            return None;
        }
        let Some(payment) = solve_source_choices(trial, sources, pool, life) else {
            failed.insert(key);
            return None;
        };
        let Some((amount, color)) = symbols.get(index) else {
            return Some(payment);
        };
        trial.colored.push(ColorRequirement::OneOf(vec![*color]));
        let colored = alternatives(index + 1, symbols, trial, sources, pool, life, failed);
        trial.colored.pop();
        if colored.is_some() {
            return colored;
        }
        let original_generic = trial.generic;
        trial.generic = trial.generic.checked_add(u32::from(*amount))?;
        let generic = alternatives(index + 1, symbols, trial, sources, pool, life, failed);
        trial.generic = original_generic;
        if generic.is_none() {
            failed.insert(key);
        }
        generic
    }
    if req.mono_hybrid.is_empty() {
        return solve_source_choices(req, sources, pool, life);
    }
    let mut trial = req.clone();
    trial.mono_hybrid.clear();
    alternatives(
        0,
        &req.mono_hybrid,
        &mut trial,
        sources,
        pool,
        life,
        &mut Default::default(),
    )
}

fn source_units(source: &ManaSource) -> usize {
    source
        .outputs
        .iter()
        .map(|output| match output {
            ManaOutput::Repeated {
                amount: mtg_ir::Value::Fixed(n),
                ..
            } => (*n).max(0) as usize,
            _ => 1,
        })
        .sum()
}

/// Multi-mana outputs are correlated by one activation. Select a whole ability
/// and its color before matching units; ordinary one-mana sources keep the fast
/// matching path, including their shared-permanent alternatives.
fn solve_source_choices(
    req: &Requirements,
    sources: &[ManaSource],
    pool: [u16; 6],
    life: i32,
) -> Option<Payment> {
    fn concrete(output: &ManaOutput, color: Option<Color>) -> ManaOutput {
        match output {
            ManaOutput::AnyOf(colors) => color
                .filter(|c| colors.contains(c))
                .or_else(|| colors.first().copied())
                .map(ManaOutput::Colored)
                .unwrap_or_else(|| output.clone()),
            ManaOutput::Repeated { amount, output } => ManaOutput::Repeated {
                amount: amount.clone(),
                output: Box::new(concrete(output, color)),
            },
            _ => output.clone(),
        }
    }
    let mut excluded = vec![false; sources.len()];
    let mut branches = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if excluded[index] || source.ability == HELPER || source.ability == DELVE {
            continue;
        }
        let group: Vec<usize> = if source.taps {
            (index..sources.len())
                .filter(|j| {
                    sources[*j].taps
                        && sources[*j].object == source.object
                        && sources[*j].ability != HELPER
                        && sources[*j].ability != DELVE
                })
                .collect()
        } else {
            vec![index]
        };
        let multi = group.iter().any(|j| source_units(&sources[*j]) > 1);
        let correlated = group.iter().any(|j| {
            source_units(&sources[*j]) > 1
                && sources[*j].outputs.iter().any(ManaOutput::is_ambiguous)
        });
        if !(correlated || (multi && group.len() > 1)) {
            continue;
        }
        let mut alternatives = Vec::new();
        for j in group {
            excluded[j] = true;
            for color in color_options(&sources[j]) {
                let mut selected = sources[j].clone();
                selected.outputs = selected
                    .outputs
                    .iter()
                    .map(|out| concrete(out, color))
                    .collect();
                alternatives.push((selected, color));
            }
        }
        branches.push(alternatives);
    }
    if branches.is_empty() {
        return solve(req, sources, pool, life);
    }
    fn select(
        index: usize,
        branches: &[Vec<(ManaSource, Option<Color>)>],
        selected: &mut Vec<ManaSource>,
        req: &Requirements,
        pool: [u16; 6],
        life: i32,
    ) -> Option<Payment> {
        // A necessary capacity bound avoids enumerating choices when even each
        // remaining permanent's largest output cannot cover the mana cost.
        // Phyrexian symbols are omitted because life may pay for them.
        let available = pool.iter().map(|n| u64::from(*n)).sum::<u64>()
            + selected.iter().map(|s| source_units(s) as u64).sum::<u64>()
            + branches[index..]
                .iter()
                .map(|options| {
                    options
                        .iter()
                        .map(|(s, _)| source_units(s) as u64)
                        .max()
                        .unwrap_or(0)
                })
                .sum::<u64>();
        if available < req.colored.len() as u64 + u64::from(req.generic) {
            return None;
        }
        if index == branches.len() {
            return solve(req, selected, pool, life);
        }
        for (source, color) in &branches[index] {
            selected.push(source.clone());
            let payment = select(index + 1, branches, selected, req, pool, life);
            selected.pop();
            if let Some(mut payment) = payment {
                // The matcher may spend any of several produced units first;
                // the recorded choice must remain the whole ability's choice.
                if color.is_some() {
                    for (object, ability, choice) in &mut payment.activate {
                        if *object == source.object && *ability == source.ability {
                            *choice = *color;
                        }
                    }
                }
                return Some(payment);
            }
        }
        None
    }
    let mut selected = sources
        .iter()
        .enumerate()
        .filter(|(j, _)| !excluded[*j])
        .map(|(_, s)| s.clone())
        .collect();
    select(0, &branches, &mut selected, req, pool, life)
}

fn solve(req: &Requirements, sources: &[ManaSource], pool: [u16; 6], life: i32) -> Option<Payment> {
    let units = build_units(sources, pool);

    // Coloured symbols are mandatory; phyrexian ones are matched if possible and
    // paid with life otherwise.
    let mut mandatory: Vec<ColorRequirement> = req.colored.clone();
    let phyrexian_start = mandatory.len();
    mandatory.extend(
        req.phyrexian
            .iter()
            .map(|c| ColorRequirement::OneOf(vec![*c])),
    );

    let mut matching = match_requirements(&mandatory, &units);

    // Every non-phyrexian requirement must be matched.
    if matching[..phyrexian_start].iter().any(Option::is_none) {
        return None;
    }

    // Mana assigned to optional Phyrexian symbols may be needed for mandatory
    // generic payment. Free those assignments and pay those symbols with life.
    let free = units.len() - matching.iter().flatten().count();
    let mut release = (req.generic as usize).saturating_sub(free);
    for assigned in matching[phyrexian_start..].iter_mut().rev() {
        if release > 0 && assigned.take().is_some() {
            release -= 1;
        }
    }
    if release > 0 {
        return None;
    }

    // Unmatched phyrexian symbols are paid with 2 life each.
    let unpaid_phyrexian = (phyrexian_start..mandatory.len())
        .filter(|i| matching[*i].is_none())
        .count();
    let life_cost = unpaid_phyrexian as u32 * 2;
    // CR 118.4 — a player may pay only life they have.
    if (life_cost as i32) > life {
        return None;
    }

    // Whatever is left over covers the generic part.
    let used: Vec<usize> = matching.iter().flatten().copied().collect();
    let leftover = units.len() - used.len();
    if leftover < req.generic as usize {
        return None;
    }

    Some(assemble(
        req, &mandatory, &matching, &units, sources, life_cost,
    ))
}

/// Turn a matching plus the generic remainder into a concrete payment.
fn assemble(
    req: &Requirements,
    mandatory: &[ColorRequirement],
    matching: &[Option<usize>],
    units: &[Unit],
    sources: &[ManaSource],
    life_cost: u32,
) -> Payment {
    let mut payment = Payment {
        life: life_cost,
        ..Default::default()
    };
    let mut spent = vec![false; units.len()];

    // Coloured and phyrexian symbols, in requirement order.
    for (i, unit_ix) in matching.iter().enumerate() {
        let Some(unit_ix) = unit_ix else { continue };
        spent[*unit_ix] = true;
        let color = match &mandatory[i] {
            ColorRequirement::Colorless => None,
            ColorRequirement::OneOf(want) => units[*unit_ix]
                .colors
                .iter()
                .copied()
                .find(|c| want.contains(c)),
        };
        charge(&mut payment, &units[*unit_ix], sources, color);
    }

    // Generic, taken from whatever is left, in a stable order.
    let mut remaining = req.generic;
    for (ix, unit) in units.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if spent[ix] {
            continue;
        }
        spent[ix] = true;
        let color = unit.colors.first().copied();
        charge(&mut payment, unit, sources, color);
        remaining -= 1;
    }

    payment
}

fn charge(payment: &mut Payment, unit: &Unit, sources: &[ManaSource], color: Option<Color>) {
    // Every spent unit is spent from the pool, wherever it came from.
    let slot = color.map_or(mtg_core::ManaPool::COLORLESS_SLOT, |c| c as usize);
    payment.spend[slot] += 1;

    // A source is activated once even when it makes several mana, so it is recorded
    // only the first time one of its units is spent. A permanent's tapping abilities
    // merged into one unit (see `build_units`) activate the one that makes this color.
    if let Some(si) = unit.source {
        let makes = |s: &ManaSource| {
            s.outputs.iter().any(|o| match color {
                Some(c) => o.possible_colors().contains(&c),
                None => o.possible_colors().is_empty(),
            })
        };
        let s = if sources[si].taps && !makes(&sources[si]) {
            sources
                .iter()
                .find(|o| o.object == sources[si].object && o.taps && makes(o))
                .unwrap_or(&sources[si])
        } else {
            &sources[si]
        };
        if !payment
            .activate
            .iter()
            .any(|(o, a, _)| *o == s.object && *a == s.ability)
        {
            payment.activate.push((s.object, s.ability, color));
        }
    }
}

/// Every unit of mana that could be spent: floating mana first, then sources from the
/// least flexible to the most.
///
/// Floating mana comes first so that already-available mana is preferred over
/// tapping something new, which is what a player would do. Sources follow in order of
/// how many colours they could make, because the matching and the generic remainder
/// both take the first unit that works: a basic land is spent before a dual, which is
/// left untapped for whatever is cast next. The sort is stable, so the order — and so
/// the payment both peers compute — stays deterministic.
fn build_units(sources: &[ManaSource], pool: [u16; 6]) -> Vec<Unit> {
    let mut units = Vec::new();

    for (slot, count) in pool.iter().enumerate() {
        let colors = match slot {
            0 => vec![Color::White],
            1 => vec![Color::Blue],
            2 => vec![Color::Black],
            3 => vec![Color::Red],
            4 => vec![Color::Green],
            _ => Vec::new(),
        };
        for _ in 0..*count {
            units.push(Unit {
                source: None,
                colors: colors.clone(),
                colorless: slot == 5,
                generic_only: false,
            });
        }
    }

    // A permanent taps once (CR 106.1/605): its tapping abilities are one unit of mana
    // between them when each makes one ("{T}: Add {G}" and the Forest's own), which
    // [`charge`] maps back to the ability that makes the color spent. Where one makes
    // more, `solve_source_choices` selects the whole ability before reaching here.
    let tapping = |s: &ManaSource| s.taps && s.ability != HELPER && s.ability != DELVE;
    let mut skip = vec![false; sources.len()];
    let mut from_sources = Vec::new();
    for (i, s) in sources.iter().enumerate() {
        if skip[i] || !tapping(s) {
            continue;
        }
        let group: Vec<usize> = (i..sources.len())
            .filter(|j| tapping(&sources[*j]) && sources[*j].object == s.object)
            .collect();
        if group.len() < 2 {
            continue;
        }
        for j in &group {
            skip[*j] = true;
        }
        if group.iter().all(|j| source_units(&sources[*j]) == 1) {
            let mut colors: Vec<Color> = Vec::new();
            for j in &group {
                for out in &sources[*j].outputs {
                    for c in out.possible_colors() {
                        if !colors.contains(&c) {
                            colors.push(c);
                        }
                    }
                }
            }
            from_sources.push(Unit {
                source: Some(i),
                colors,
                colorless: group.iter().any(|j| {
                    sources[*j]
                        .outputs
                        .iter()
                        .any(|out| out.possible_colors().is_empty())
                }),
                generic_only: false,
            });
        } else if let Some(best) = group
            .iter()
            .copied()
            .max_by_key(|j| source_units(&sources[*j]))
        {
            skip[best] = false;
        }
    }
    for (i, s) in sources.iter().enumerate() {
        if skip[i] {
            continue;
        }
        for out in &s.outputs {
            let n = match out {
                // A `Repeated` output with a dynamic amount cannot be sized without
                // evaluation, so it contributes one unit conservatively. Better to
                // understate available mana than to offer an unpayable spell.
                ManaOutput::Repeated {
                    amount: mtg_ir::Value::Fixed(n),
                    ..
                } => (*n).max(0) as usize,
                _ => 1,
            };
            for _ in 0..n {
                let colors = out.possible_colors();
                from_sources.push(Unit {
                    source: Some(i),
                    colorless: colors.is_empty(),
                    generic_only: (s.ability == HELPER || s.ability == DELVE) && colors.is_empty(),
                    colors,
                });
            }
        }
    }

    // Sources that are used up come last — creatures and artifacts tapped to help pay
    // after those — then the least flexible first.
    from_sources.sort_by_key(|u| {
        let consumed = u.source.is_some_and(|i| sources[i].sacrifices);
        let helper = u.source.is_some_and(|i| sources[i].ability == HELPER);
        (consumed, helper, u.colors.len())
    });
    units.extend(from_sources);
    units
}

/// The colour choices for activating a source by hand: one entry per colour it could
/// make, or a single `None` when there is nothing to choose.
///
/// One choice covers the whole ability, matching how resolution applies a single
/// `mana_choice` to every "one of" output the ability has.
pub fn color_options(source: &ManaSource) -> Vec<Option<Color>> {
    let choosable = source
        .outputs
        .iter()
        .map(ManaOutput::possible_colors)
        .find(|c| c.len() > 1);
    match choosable {
        Some(colors) => {
            let mut out: Vec<Option<Color>> = Vec::new();
            for c in colors {
                if !out.contains(&Some(c)) {
                    out.push(Some(c));
                }
            }
            out
        }
        None => vec![None],
    }
}

/// Maximum bipartite matching from requirements to units (Kuhn's algorithm).
///
/// Returns, for each requirement, the unit assigned to it. A `None` means that
/// requirement could not be satisfied without stealing a unit another requirement
/// needs more.
fn match_requirements(reqs: &[ColorRequirement], units: &[Unit]) -> Vec<Option<usize>> {
    let mut assigned_to: Vec<Option<usize>> = vec![None; units.len()];

    for (ri, req) in reqs.iter().enumerate() {
        let mut seen = vec![false; units.len()];
        augment(ri, req, reqs, units, &mut assigned_to, &mut seen);
    }

    // Invert: unit -> requirement becomes requirement -> unit.
    let mut out = vec![None; reqs.len()];
    for (ui, owner) in assigned_to.iter().enumerate() {
        if let Some(ri) = owner {
            out[*ri] = Some(ui);
        }
    }
    out
}

/// Try to find an augmenting path for one requirement.
fn augment(
    ri: usize,
    req: &ColorRequirement,
    reqs: &[ColorRequirement],
    units: &[Unit],
    assigned_to: &mut Vec<Option<usize>>,
    seen: &mut Vec<bool>,
) -> bool {
    for (ui, unit) in units.iter().enumerate() {
        if seen[ui] || !unit.satisfies(req) {
            continue;
        }
        seen[ui] = true;
        // Free, or its current owner can be rehoused elsewhere.
        let free = assigned_to[ui].is_none();
        if free {
            assigned_to[ui] = Some(ri);
            return true;
        }
        let other = assigned_to[ui].expect("checked above");
        if augment(other, &reqs[other], reqs, units, assigned_to, seen) {
            assigned_to[ui] = Some(ri);
            return true;
        }
    }
    false
}

/// Every mana ability a player could activate right now.
pub fn mana_sources(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
) -> Vec<ManaSource> {
    let mut out = Vec::new();

    for id in state.battlefield() {
        let Some(obj) = state.objects.get(&id) else {
            continue;
        };
        if crate::layers::controller(state, id) != Some(player)
            || crate::combat::abilities_blocked(state, cards, id)
        {
            continue;
        }
        // The object's *current* abilities, so granted and removed ones count.
        for ability in &crate::abilities::current(state, cards, id) {
            let AbilityKind::Activated {
                cost,
                effect,
                is_mana_ability,
                timing,
                ..
            } = &ability.kind
            else {
                continue;
            };
            if !is_mana_ability {
                continue;
            }
            // "Activate only once each turn": spent for the turn, it makes nothing more.
            if timing.per_turn().is_some_and(|n| {
                state
                    .activated_this_turn
                    .get(&(id, ability.id))
                    .is_some_and(|done| *done >= n)
            }) {
                continue;
            }
            // "Activate only if you control a Swamp": not a source while it doesn't hold.
            if !crate::cost::conditions_hold(state, cards, id, player, &cost.timing) {
                continue;
            }
            // Only tapping and sacrificing the source are understood so far. An ability
            // with another cost is skipped rather than assumed free, which keeps the
            // available-mana estimate an understatement.
            use mtg_ir::{AdditionalCost as A, Selector as S};
            let taps = cost.additional.iter().any(|a| {
                matches!(
                    a,
                    A::Tap {
                        what: S::SelfSource
                    }
                )
            });
            let sacrifices = cost.additional.iter().any(|a| {
                matches!(
                    a,
                    A::Sacrifice {
                        what: S::SelfSource,
                        ..
                    }
                )
            });
            let understood = cost.mana.symbols.is_empty()
                && cost.additional.len() == usize::from(taps) + usize::from(sacrifices);
            if !understood {
                continue;
            }
            if taps && (obj.tapped || !crate::cost::tap_symbol_allowed(state, cards, id)) {
                continue;
            }

            let mut outputs: Vec<ManaOutput> = collect_mana_outputs(effect)
                .iter()
                .flat_map(|o| concrete_outputs(state, cards, id, player, o))
                .map(|o| counted(state, cards, id, player, 0, o))
                .collect();
            // Wild Growth: tapping it for mana makes more.
            if taps && !outputs.is_empty() {
                outputs.extend(additional_mana(state, cards, id));
            }
            // "The chosen color" with none chosen makes nothing.
            if !outputs.is_empty() && !outputs.iter().any(unresolved) {
                out.push(ManaSource {
                    object: id,
                    ability: ability.id,
                    outputs,
                    taps,
                    sacrifices,
                    only: spend_only(effect),
                    spells_only: spends_on_non_spells(effect),
                });
            }
        }
    }

    // "Exile this card from your hand: Add {R}." (Simian Spirit Guide): a mana ability of a
    // card in hand, used up as it is activated.
    for id in state.objects_in(mtg_core::ZoneRef::of(mtg_core::Zone::Hand, player)) {
        let Some(face) = state
            .objects
            .get(&id)
            .and_then(|o| cards.face(o.card, o.face))
        else {
            continue;
        };
        for ability in &face.abilities {
            let AbilityKind::Activated {
                cost,
                effect,
                is_mana_ability: true,
                functions_from: mtg_core::Zone::Hand,
                ..
            } = &ability.kind
            else {
                continue;
            };
            if !cost.mana.symbols.is_empty() || !exiles_self_from_hand(cost) {
                continue;
            }
            let outputs = collect_mana_outputs(effect);
            if !outputs.is_empty() {
                out.push(ManaSource {
                    object: id,
                    ability: ability.id,
                    outputs,
                    taps: false,
                    sacrifices: true,
                    only: spend_only(effect),
                    spells_only: spends_on_non_spells(effect),
                });
            }
        }
    }

    out
}

/// Whether a cost is exactly "exile this card from your hand".
pub fn exiles_self_from_hand(cost: &mtg_ir::Cost) -> bool {
    matches!(
        cost.additional.as_slice(),
        [mtg_ir::AdditionalCost::ExileFrom {
            zone: mtg_core::Zone::Hand,
            filter: mtg_ir::ObjectFilter::IsSelf,
            ..
        }]
    )
}

/// Whether a mana ability's cost is a choice — "tap an untapped creature you control",
/// "remove X storage counters" — so that activating it is announced like any ability,
/// asking for those, before it resolves at once (CR 605.3b). Automatic payment never
/// uses such an ability; it is activated by hand.
pub fn announced(cost: &mtg_ir::Cost) -> bool {
    use mtg_ir::{AdditionalCost as A, Selector as S, Value};
    cost.additional.iter().any(|part| {
        matches!(
            part,
            A::TapUntapped { .. }
                | A::RemoveCounters {
                    what: S::SelfSource,
                    amount: Value::X,
                    ..
                }
                // "Sacrifice a creature: Add {C}{C}." — which one is chosen.
                | A::Sacrifice {
                    what: S::All { .. },
                    ..
                }
                // "Discard your hand, sacrifice this artifact: Add three mana …" — never
                // spent automatically, and only when its controller has priority, which
                // makes it "activate only as an instant".
                | A::DiscardHand
        )
    })
}

/// A source offered for manual activation, including abilities whose input mana
/// must be paid first. Paid sources are excluded from automatic payment planning.
pub fn manual_source(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
    object: ObjectId,
    ability: AbilityId,
) -> Option<ManaSource> {
    use mtg_ir::{AdditionalCost as A, Selector as S, Value};
    let a = crate::abilities::find(state, cards, object, ability)?;
    let AbilityKind::Activated {
        cost,
        effect,
        is_mana_ability: true,
        ..
    } = &a.kind
    else {
        return None;
    };
    if !cost.additional.iter().all(|part| {
        matches!(
            part,
            A::Tap {
                what: S::SelfSource
            } | A::Sacrifice {
                what: S::SelfSource,
                count: Value::Fixed(1)
            } | A::PayLife {
                amount: Value::Fixed(0..)
            } | A::RemoveCounters {
                what: S::SelfSource,
                amount: Value::Fixed(1..) | Value::X,
                ..
            } | A::TapUntapped {
                count: Value::Fixed(1..),
                ..
            } | A::Sacrifice {
                what: S::All { .. },
                count: Value::Fixed(1)
            } | A::DiscardHand
        )
    }) || !crate::cost::additional_payable(state, cards, object, player, cost)
    {
        return None;
    }
    // Restricted mana can't float: it is made only as part of paying for a spell.
    if spend_only(effect).is_some() {
        return None;
    }
    if cost.mana.symbols.is_empty()
        && !announced(cost)
        && cost
            .additional
            .iter()
            .all(|part| !matches!(part, A::PayLife { .. } | A::RemoveCounters { .. }))
    {
        return mana_sources(state, cards, player)
            .into_iter()
            .find(|s| s.object == object && s.ability == ability);
    }
    let mut payment_state = state.clone();
    // Reserve this permanent: it cannot tap for the input and the output as well.
    payment_state.objects.remove(&object)?;
    let life: i32 = cost
        .additional
        .iter()
        .filter_map(|part| match part {
            A::PayLife {
                amount: Value::Fixed(n),
            } => Some(*n),
            _ => None,
        })
        .sum();
    payment_state.players.get_mut(&player)?.life -= life;
    if payment_state.player(player).life < 0
        || !can_pay(&payment_state, cards, player, &cost.mana, 0)
    {
        return None;
    }
    let outputs: Vec<ManaOutput> = collect_mana_outputs(effect)
        .iter()
        .flat_map(|o| concrete_outputs(state, cards, object, player, o))
        .collect();
    if outputs.is_empty() || outputs.iter().any(unresolved) {
        return None;
    }
    Some(ManaSource {
        object,
        ability,
        outputs,
        taps: cost.additional.iter().any(|p| matches!(p, A::Tap { .. })),
        sacrifices: cost
            .additional
            .iter()
            .any(|p| matches!(p, A::Sacrifice { .. })),
        only: None,
        spells_only: false,
    })
}

/// Pull the mana an effect produces out of its tree.
/// "Add {G} for each creature you control", as the number it is now, so the planner
/// neither counts mana that isn't there nor misses mana that is.
pub(crate) fn counted(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    player: PlayerId,
    x: u32,
    out: ManaOutput,
) -> ManaOutput {
    match out {
        ManaOutput::Repeated { amount, output } if !matches!(amount, mtg_ir::Value::Fixed(_)) => {
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source,
                controller: player,
                targets: &[],
                target_legal: &[],
                x,
                bindings: crate::empty_bindings(),
            };
            let n = crate::eval::value(&ctx, &amount).unwrap_or(0).max(0);
            ManaOutput::Repeated {
                amount: mtg_ir::Value::Fixed(n),
                output,
            }
        }
        other => other,
    }
}

/// A mana output as its source makes it now: "one mana of the chosen color" as the color it
/// chose, "any color in your commander's color identity" as those colors, "any of the
/// exiled card's colors", "any color among …". An output with nothing to make stays
/// unresolved (see [`unresolved`]) and makes no mana.
pub(crate) fn concrete_output(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    player: PlayerId,
    out: &ManaOutput,
) -> ManaOutput {
    let any_of = |colors: Vec<Color>, unresolved: ManaOutput| match colors.as_slice() {
        [] => unresolved,
        [one] => ManaOutput::Colored(*one),
        several => ManaOutput::AnyOf(several.to_vec()),
    };
    match out {
        ManaOutput::ChosenColor => match state.objects.get(&source).and_then(|o| o.chosen_color) {
            Some(c) => ManaOutput::Colored(c),
            None => ManaOutput::ChosenColor,
        },
        ManaOutput::OrChosen(color) => {
            match state.objects.get(&source).and_then(|o| o.chosen_color) {
                Some(c) if c != *color => ManaOutput::AnyOf(vec![*color, c]),
                _ => ManaOutput::Colored(*color),
            }
        }
        ManaOutput::CommanderIdentity => any_of(
            commander_identity(state, cards, player),
            ManaOutput::CommanderIdentity,
        ),
        ManaOutput::ExiledCardColors => {
            let exiled = state
                .exiled_with
                .iter()
                .filter(|(by, _)| *by == source)
                .map(|(_, card)| *card);
            any_of(
                colors_of(state, cards, exiled),
                ManaOutput::ExiledCardColors,
            )
        }
        ManaOutput::ColorsAmong(sel) => {
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source,
                controller: player,
                targets: &[],
                target_legal: &[],
                x: 0,
                bindings: crate::empty_bindings(),
            };
            let among = crate::eval::objects(&ctx, sel).unwrap_or_default();
            any_of(colors_of(state, cards, among.into_iter()), out.clone())
        }
        // CR 106.7 — the colors those lands' mana abilities could make, without looking
        // through another land that asks the same question (two Exotic Orchards).
        ManaOutput::LandColors(whose) => {
            let chars = crate::eval::ComputedChars(cards);
            let ctx = crate::eval::Ctx {
                state,
                cards,
                chars: &chars,
                source,
                controller: player,
                targets: &[],
                target_legal: &[],
                x: 0,
                bindings: crate::empty_bindings(),
            };
            let players = crate::eval::players(&ctx, whose).unwrap_or_default();
            let mut colors = Vec::new();
            for land in state.battlefield() {
                let Some(owner) = crate::layers::controller(state, land) else {
                    continue;
                };
                if !players.contains(&owner)
                    || !crate::layers::compute(state, cards, land)
                        .is_some_and(|c| c.has_type(mtg_core::CardType::Land))
                {
                    continue;
                }
                for ability in &crate::abilities::current(state, cards, land) {
                    let AbilityKind::Activated {
                        effect,
                        is_mana_ability: true,
                        ..
                    } = &ability.kind
                    else {
                        continue;
                    };
                    for made in collect_mana_outputs(effect) {
                        if matches!(made, ManaOutput::LandColors(_)) {
                            continue;
                        }
                        let made = concrete_output(state, cards, land, owner, &made);
                        for c in made.possible_colors() {
                            if !colors.contains(&c) {
                                colors.push(c);
                            }
                        }
                    }
                }
            }
            let ordered = [
                Color::White,
                Color::Blue,
                Color::Black,
                Color::Red,
                Color::Green,
            ]
            .into_iter()
            .filter(|c| colors.contains(c))
            .collect();
            any_of(ordered, out.clone())
        }
        ManaOutput::Repeated { amount, output } => ManaOutput::Repeated {
            amount: amount.clone(),
            output: Box::new(concrete_output(state, cards, source, player, output)),
        },
        other => other.clone(),
    }
}

/// What else tapping `object` for mana makes: "whenever enchanted land is tapped for mana,
/// its controller adds an additional {G}" (Wild Growth), each effect's mana made concrete
/// from its own source. Only fixed outputs: a choice would need its own color answer.
pub(crate) fn additional_mana(
    state: &GameState,
    cards: &dyn PrintedCards,
    object: ObjectId,
) -> Vec<ManaOutput> {
    let mut out = Vec::new();
    for e in crate::layers::effects(state, cards) {
        let mtg_ir::effect::Modification::Restriction(
            mtg_ir::effect::Restriction::AddsAdditionalMana { produces },
        ) = &e.modification
        else {
            continue;
        };
        if !crate::layers::applies(state, cards, &e, object) {
            continue;
        }
        let Some(controller) = crate::layers::controller(state, e.source) else {
            continue;
        };
        for p in produces {
            let made = concrete_output(state, cards, e.source, controller, p);
            if made.possible_colors().len() <= 1 && !unresolved(&made) {
                out.push(made);
            }
        }
    }
    out
}

/// [`concrete_output`], with "one mana of each color among …" as one output per color.
pub(crate) fn concrete_outputs(
    state: &GameState,
    cards: &dyn PrintedCards,
    source: ObjectId,
    player: PlayerId,
    out: &ManaOutput,
) -> Vec<ManaOutput> {
    let ManaOutput::EachColorAmong(sel) = out else {
        return vec![concrete_output(state, cards, source, player, out)];
    };
    let chars = crate::eval::ComputedChars(cards);
    let ctx = crate::eval::Ctx {
        state,
        cards,
        chars: &chars,
        source,
        controller: player,
        targets: &[],
        target_legal: &[],
        x: 0,
        bindings: crate::empty_bindings(),
    };
    let among = crate::eval::objects(&ctx, sel).unwrap_or_default();
    colors_of(state, cards, among.into_iter())
        .into_iter()
        .map(ManaOutput::Colored)
        .collect()
}

/// The colors among these objects, in WUBRG order.
fn colors_of(
    state: &GameState,
    cards: &dyn PrintedCards,
    objects: impl Iterator<Item = ObjectId>,
) -> Vec<Color> {
    let computed: Vec<_> = objects
        .filter_map(|id| crate::layers::compute(state, cards, id))
        .collect();
    [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ]
    .into_iter()
    .filter(|c| computed.iter().any(|ch| ch.colors.contains(*c)))
    .collect()
}

/// Whether an output is still waiting on its source (see [`with_chosen_color`]).
pub(crate) fn unresolved(out: &ManaOutput) -> bool {
    match out {
        ManaOutput::ChosenColor
        | ManaOutput::CommanderIdentity
        | ManaOutput::ExiledCardColors
        | ManaOutput::ColorsAmong(_)
        | ManaOutput::LandColors(_)
        | ManaOutput::EachColorAmong(_) => true,
        ManaOutput::Repeated { output, .. } => unresolved(output),
        _ => false,
    }
}

/// CR 903.4 — a player's commander's color identity: the colors of the mana symbols in its
/// mana cost and rules text, and its color indicator, on every face.
pub fn commander_identity(
    state: &GameState,
    cards: &dyn PrintedCards,
    player: PlayerId,
) -> Vec<Color> {
    let Some(card) = state.commander.commanders.get(&player) else {
        return Vec::new();
    };
    let mut colors: Vec<Color> = Vec::new();
    let mut add = |c: Color| {
        if !colors.contains(&c) {
            colors.push(c);
        }
    };
    for face in (0..=1).filter_map(|f| cards.face(*card, f)) {
        for symbol in &face.mana_cost.symbols {
            symbol_colors(symbol, &mut add);
        }
        for c in face.colors.iter().flatten() {
            add(*c);
        }
        let text = face.oracle_text.as_deref().unwrap_or("");
        for symbol in text.split('{').skip(1).filter_map(|s| s.split_once('}')) {
            for part in symbol.0.split('/') {
                if let Some(c) = match part {
                    "W" => Some(Color::White),
                    "U" => Some(Color::Blue),
                    "B" => Some(Color::Black),
                    "R" => Some(Color::Red),
                    "G" => Some(Color::Green),
                    _ => None,
                } {
                    add(c);
                }
            }
        }
    }
    // WUBRG order, so the same commander always offers the same list.
    [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ]
    .into_iter()
    .filter(|c| colors.contains(c))
    .collect()
}

fn symbol_colors(symbol: &mtg_core::ManaSymbol, add: &mut impl FnMut(Color)) {
    use mtg_core::ManaSymbol as S;
    match symbol {
        S::Colored(c) | S::Phyrexian(c) | S::MonoHybrid(_, c) => add(*c),
        S::Hybrid(a, b) => {
            add(*a);
            add(*b);
        }
        _ => {}
    }
}

/// Whether a mana ability's restriction is only about casting spells (a Powerstone's).
fn spends_on_non_spells(effect: &Effect) -> bool {
    fn collect(effect: &Effect, restrictions: &mut Vec<bool>) {
        match effect {
            Effect::SpendOnly {
                spells_only,
                effect,
                ..
            } => {
                restrictions.push(*spells_only);
                collect(effect, restrictions);
            }
            Effect::Sequence(items) => {
                for item in items {
                    collect(item, restrictions);
                }
            }
            _ => {}
        }
    }
    let mut restrictions = Vec::new();
    collect(effect, &mut restrictions);
    !restrictions.is_empty() && restrictions.iter().all(|only| *only)
}

/// Conjoin nested restrictions; treating different branches conservatively cannot
/// unlock mana whose restriction is not represented by the automatic source.
fn spend_only(effect: &Effect) -> Option<mtg_ir::ObjectFilter> {
    fn collect(effect: &Effect, filters: &mut Vec<mtg_ir::ObjectFilter>) {
        match effect {
            Effect::SpendOnly { only, effect, .. } => {
                filters.push(only.clone());
                collect(effect, filters);
            }
            Effect::Sequence(items) => {
                for item in items {
                    collect(item, filters);
                }
            }
            _ => {}
        }
    }
    let mut filters = Vec::new();
    collect(effect, &mut filters);
    match filters.len() {
        0 => None,
        1 => filters.pop(),
        _ => Some(mtg_ir::ObjectFilter::And(filters)),
    }
}

fn collect_mana_outputs(effect: &Effect) -> Vec<ManaOutput> {
    match effect {
        Effect::AddMana { produces, .. } => produces.clone(),
        Effect::SpendOnly { effect, .. } => collect_mana_outputs(effect),
        Effect::Sequence(items) => items.iter().flat_map(collect_mana_outputs).collect(),
        _ => Vec::new(),
    }
}
