//! Compiling printed rules text into the card IR.
//!
//! This amends ADR-001 (see ADR-013): behaviour *is* read off English, but only through a
//! grammar that either accounts for every word of a line or rejects it. A line the grammar
//! does not fully understand produces nothing, and the card is reported as not playable
//! as printed — never approximated. Wrong behaviour is worse than none.
//!
//! The grammar is compositional rather than a list of whole-line templates: "target
//! creature", "deals N damage to", "until end of turn" and "whenever ~ attacks" each parse
//! once and combine, which is what lets a few hundred rules cover thousands of cards.
//!
//! Every shape emitted here is one the engine carries out; a shape the engine would refuse
//! or get wrong is not compiled. That is the second half of the contract, and the
//! end-to-end tests in `tests/compiled.rs` hold it.
//!
//! ## Architecture
//!
//! The compiler processes a card's oracle text through these stages:
//!
//! 1. **Normalization** (`normalise` in `words` module) — normalize text, remove reminder text
//! 2. **Tokenization** — split into lines, identify modal structures
//! 3. **Grammar Parsing** — main compilation loop that parses each line into abilities
//! 4. **IR Emission** — construct MTG IR objects (Ability, Effect, Trigger, etc.)
//!
//! ## Public API
//!
//! - `compile()` — Compile one card face to abilities
//! - `understood()` — Check if a card is playable as printed  
//! - `row_understood()` — Check if a database row compiles
//!
//! ## Sub-modules
//!
//! - `clauses` — Effect and selector parsing
//! - `conditions` — Trigger condition parsing
//! - `nouns` — Object reference parsing (creature, land, spell, etc.)
//! - `words` — Word-level parsing (colors, keywords, numbers)

mod clauses;
mod conditions;
mod nouns;
mod words;

use mtg_core::{AbilityId, CardType, Color, Step, Subtype, Zone};
use mtg_ir::{
    Ability, AbilityKind, AdditionalCost, Cost, Effect, EventPattern, ObjectFilter, Selector,
    TargetSpec, Trigger, TriggerTiming, Value,
    ability::ActivationTiming,
    effect::{ManaOutput, Modification, Replacement, ReplacementKind, Restriction},
    trigger::DamageRecipient,
};

use crate::convert::Subtypes;

pub use words::{normalise, strip_reminder};

/// What the compiler needs to know about a face.
pub struct FaceText<'a> {
    pub name: &'a str,
    pub card_types: &'a [CardType],
    /// Subtype names as printed.
    pub subtypes: &'a [String],
    pub oracle_text: Option<&'a str>,
    /// The printed mana cost, e.g. `{X}{R}`: whether `X` means anything depends on it.
    pub mana_cost: &'a str,
}

/// The result of compiling one face.
#[derive(Debug, Default)]
pub struct Compiled {
    pub abilities: Vec<Ability>,
    /// Colours that override the mana cost's, when the text defines them (devoid).
    pub colors: Option<Vec<Color>>,
    /// Lines not understood, as printed (after reminder text is removed).
    pub unparsed: Vec<String>,
}

impl Compiled {
    /// Whether every line was accounted for: the face is playable as printed.
    pub fn understood(&self) -> bool {
        self.unparsed.is_empty()
    }
}

// ===== MAIN COMPILER ENTRY POINT =================================================

/// Compile one card face to IR abilities.
///
/// # Processing Steps
///
/// 1. **Initialize** — Add basic land mana abilities, check for unsupported features
/// 2. **Text Normalization** — Normalize oracle text, handle special card types (leveler, class)
/// 3. **Line Parsing** — Iterate through each line, parse as:
///    - Modal ("choose one —") abilities
///    - Triggered abilities ("when", "whenever")
///    - Activated abilities (cost in braces)
///    - Static effects ("~" and ":Creature/Spell effects)
/// 4. **Validation** — For spells, all lines must parse or none compile (all-or-nothing rule)
/// 5. **Return** — Compiled abilities plus unparsed lines for error reporting
///
/// # Examples
///
/// - "Flying" → keyword ability
/// - "{1}{R}: ~ deals 1 damage to target creature or player" → activated ability with cost
/// - "When ~ enters, draw a card" → triggered ability (ETB)
/// - "Choose one — • +1/+1 • Create a token" → modal ability
pub fn compile(face: &FaceText, subtypes: &dyn Subtypes) -> Compiled {
    let mut out = Compiled::default();

    // CR 305.6: basic land types grant their mana ability intrinsically.
    for st in face.subtypes {
        if let Some(c) = basic_land_color(st) {
            push(
                &mut out.abilities,
                mana_ability(vec![ManaOutput::Colored(c)]),
                None,
            );
        }
    }

    // `{S}` must be paid with mana from a snow source, which the engine does not track
    // yet (it would pay it as generic), so such a card is not playable as printed.
    if face.mana_cost.contains("{S}") {
        out.unparsed.push("{S} in the mana cost".into());
    }

    let Some(text) = face.oracle_text else {
        return out;
    };
    let is_spell = face
        .card_types
        .iter()
        .any(|t| matches!(t, CardType::Instant | CardType::Sorcery));
    let normalised = normalise(text, face.name);
    // A leveler (CR 711) or a Class (CR 716): its brackets are read as a whole.
    if normalised
        .lines()
        .any(|l| l.starts_with("level up ") || l == "station")
        || face.subtypes.iter().any(|s| s == "Class")
    {
        return leveler(face, subtypes, &normalised, text, out);
    }
    let printed: Vec<&str> = text.lines().collect();

    // An instant or sorcery's effect lines are one spell ability, in order.
    let mut spell_lines: Vec<Effect> = Vec::new();
    let mut spell_cx = Cx::new(subtypes);
    spell_cx.x = face.mana_cost.contains("{X}");

    let mut lines: Vec<&str> = normalised.lines().collect();
    // CR 706 — a die roll's results table ("1—9 | …", "20 | …") follows the line that
    // rolls: its rows are compiled here and attached to that roll once every line is.
    let mut tables: Vec<Vec<(u32, u32, Effect)>> = Vec::new();
    let mut k = 1;
    while k < lines.len() {
        if die_row(lines[k]).is_none() || die_row(lines[k - 1]).is_some() {
            k += 1;
            continue;
        }
        let start = k;
        while k < lines.len() && die_row(lines[k]).is_some() {
            k += 1;
        }
        let rows: Option<Vec<(u32, u32, Effect)>> = lines[start..k]
            .iter()
            .map(|l| {
                let (lo, hi, text) = die_row(l)?;
                let mut cx = Cx::new(subtypes);
                cx.x = spell_cx.x;
                let e = clauses::effect(text, &mut cx)?;
                // A row that targets would need its targets chosen up front.
                cx.targets.is_empty().then_some((lo, hi, e))
            })
            .collect();
        if let Some(rows) = rows {
            tables.push(rows);
            for l in &mut lines[start..k] {
                *l = "";
            }
        }
    }
    let lines = lines;
    let mut i = 0;
    let mut spell_modal = false;
    // "Backup N": where the abilities it hands on begin.
    let mut backup: Option<(i32, usize, String)> = None;
    // An instant's or sorcery's effect lines as text, and its overload cost.
    let mut spell_texts: Vec<&str> = Vec::new();
    let mut overload: Option<(mtg_core::ManaCost, String)> = None;
    // "Awaken 3—{4}{W}": the counters, its cost, and the printed line.
    let mut awaken: Option<(i32, mtg_core::ManaCost, String)> = None;
    while i < lines.len() {
        let line = lines[i];
        if is_spell && line == "ascend" {
            spell_lines.push(Effect::Ascend);
            i += 1;
            continue;
        }
        if backup.is_none()
            && let Some(n) = line.strip_prefix("backup ")
            && let Ok(n) = n.parse::<i32>()
        {
            let original = printed
                .get(i)
                .map(|l| strip_reminder(l))
                .unwrap_or_default();
            backup = Some((n, out.abilities.len(), original));
            i += 1;
            continue;
        }
        if is_spell
            && let Some(r) = line.strip_prefix("overload ")
            && let Some((mana, "")) = mana_cost(r)
        {
            let original = printed
                .get(i)
                .map(|l| strip_reminder(l))
                .unwrap_or_default();
            overload = Some((mana, original));
            i += 1;
            continue;
        }
        if is_spell
            && let Some(r) = line.strip_prefix("awaken ")
            && let Some((n, r)) = r.split_once('—')
            && let Ok(n) = n.parse::<i32>()
            && let Some((mana, "")) = mana_cost(r.trim_end_matches('.'))
        {
            let original = printed
                .get(i)
                .map(|l| strip_reminder(l))
                .unwrap_or_default();
            awaken = Some((n, mana, original));
            i += 1;
            continue;
        }
        // CR 702.110 — exploit, and the "when ~ exploits a creature" ability that reads it:
        // one ability, "you may sacrifice a creature. When you do, …", whose second half
        // chooses its targets after the sacrifice, as the separate trigger would.
        if line == "exploit"
            && face.card_types.contains(&CardType::Creature)
            && let Some(then) = lines
                .get(i + 1)
                .and_then(|l| l.strip_prefix("when ~ exploits a creature, "))
        {
            let whole = printed
                .get(i..i + 2)
                .unwrap_or_default()
                .iter()
                .map(|l| strip_reminder(l))
                .collect::<Vec<_>>()
                .join("\n");
            let text = format!("when ~ enters, you may sacrifice a creature. when you do, {then}");
            let mut cx = Cx::new(subtypes);
            match compile_line(&text, face, &mut cx) {
                Some(Line::Abilities(list)) => {
                    for kind_targets in list {
                        push(&mut out.abilities, kind_targets, Some(&whole));
                    }
                }
                _ => out.unparsed.push(whole),
            }
            i += 2;
            continue;
        }
        let original = printed
            .get(i)
            .map(|l| strip_reminder(l))
            .unwrap_or_default();
        i += 1;
        if line.is_empty() {
            continue;
        }

        // CR 702.172 — spree: "+ {1} — Destroy target artifact." for each mode, choose one
        // or more and pay each chosen mode's cost.
        if is_spell && line == "spree" {
            let start = i;
            while i < lines.len() && lines[i].starts_with("+ ") {
                i += 1;
            }
            let mut costs = Vec::new();
            let mut bullets = Vec::new();
            for l in &lines[start..i] {
                match mana_cost(&l[2..]).and_then(|(m, r)| Some((m, r.strip_prefix(" — ")?))) {
                    Some((mana, effect)) => {
                        costs.push(mana);
                        bullets.push(effect);
                    }
                    None => bullets.clear(),
                }
            }
            let labels: Vec<String> = printed
                .get(start..i)
                .unwrap_or_default()
                .iter()
                .map(|b| strip_reminder(b).trim_start_matches("+ ").to_string())
                .collect();
            let whole = std::iter::once(original.clone())
                .chain(labels.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n");
            let mut cx = Cx::new(subtypes);
            let n = bullets.len() as i32;
            match (bullets.len() == costs.len() && n > 0)
                .then(|| modal(&bullets, &labels, n, Some(1), None, &mut cx))
                .flatten()
            {
                Some(effect) => {
                    spell_lines.push(effect);
                    spell_cx.targets.extend(std::mem::take(&mut cx.targets));
                    spell_modal = true;
                    push(
                        &mut out.abilities,
                        (AbilityKind::Spree { costs }, Vec::new()),
                        None,
                    );
                }
                None => out.unparsed.push(whole),
            }
            continue;
        }
        // "Choose one —" and its bullets are one ability (CR 700.2).
        if let Some((prefix, n, at_least)) = modal_head(line) {
            let start = i;
            while i < lines.len() && lines[i].starts_with("• ") {
                i += 1;
            }
            let bullets: Vec<&str> = lines[start..i]
                .iter()
                .map(|b| b.trim_start_matches("• "))
                .collect();
            let labels: Vec<String> = printed
                .get(start..i)
                .unwrap_or_default()
                .iter()
                .map(|b| strip_reminder(b).trim_start_matches("• ").to_string())
                .collect();
            let whole = std::iter::once(original.clone())
                .chain(labels.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n");
            let mut cx = Cx::new(subtypes);
            cx.x = is_spell && spell_cx.x;
            // A modal trigger's "it" comes from its condition.
            let it = if prefix.is_empty() {
                None
            } else {
                trigger_condition(prefix, &mut cx).and_then(|(_, _, it, _)| it)
            };
            // "choose one or more": as many as there are modes.
            let n = if n < 0 { bullets.len() as i32 } else { n };
            let compiled = modal(&bullets, &labels, n, at_least, it, &mut cx).and_then(|effect| {
                if prefix.is_empty() {
                    is_spell.then_some((None, effect))
                } else {
                    let kind = triggered_with(prefix, effect, &mut cx)?;
                    Some((Some(kind), Effect::Nothing))
                }
            });
            match compiled {
                Some((Some(kind), _)) => push(
                    &mut out.abilities,
                    (kind, std::mem::take(&mut cx.targets)),
                    Some(&whole),
                ),
                Some((None, effect)) => {
                    spell_lines.push(effect);
                    spell_cx.targets.extend(std::mem::take(&mut cx.targets));
                    spell_modal = true;
                }
                None => out.unparsed.push(whole),
            }
            continue;
        }

        let mut cx = Cx::new(subtypes);
        match compile_line(line, face, &mut cx) {
            Some(Line::Abilities(list)) => {
                for kind_targets in list {
                    push(&mut out.abilities, kind_targets, Some(&original));
                }
            }
            Some(Line::Colorless) => out.colors = Some(Vec::new()),
            None if is_spell => {
                let saved = spell_cx.clone();
                match clauses::effect(line, &mut spell_cx) {
                    Some(e) => {
                        spell_lines.push(e);
                        spell_texts.push(line);
                    }
                    None => {
                        spell_cx = saved;
                        out.unparsed.push(original);
                    }
                }
            }
            None => {
                // A line not understood that can make this enter tapped ("As this land
                // enters, you may pay 2 life. If you don't, it enters tapped.") enters
                // tapped regardless: never better for the player than the card allows.
                if line.contains("enters tapped") || line.contains("enters the battlefield tapped")
                {
                    push(
                        &mut out.abilities,
                        (
                            AbilityKind::ReplacementEffect(Replacement {
                                matches: EventPattern::Enters {
                                    who: ObjectFilter::IsSelf,
                                },
                                kind: ReplacementKind::EntersTapped,
                            }),
                            Vec::new(),
                        ),
                        Some(&original),
                    );
                }
                out.unparsed.push(original);
            }
        }
    }

    // The engine pays a plain alternative cost's other parts by finding it on the face, so
    // a face has at most one.
    let plain = |a: &&Ability| {
        matches!(
            a.kind,
            AbilityKind::AlternativeCost {
                kind: mtg_ir::ability::AltCost::Pay,
                ..
            }
        )
    };
    if out.abilities.iter().filter(plain).count() > 1 {
        out.unparsed
            .push("more than one alternative cost to pay rather than the mana cost".into());
    }

    if let Some((n, from, original)) = backup {
        match backup_trigger(n, &out.abilities[from..], face, subtypes) {
            Some(kt) => push(&mut out.abilities, kt, Some(&original)),
            None => out.unparsed.push(original),
        }
    }

    // A Saga: its lore counters and final chapter (CR 714).
    if face.subtypes.iter().any(|s| s == "Saga") {
        let last = out
            .abilities
            .iter()
            .filter_map(|a| match &a.kind {
                AbilityKind::Triggered { trigger, .. } => match trigger.on {
                    EventPattern::ChapterReached { chapter, .. } => Some(chapter),
                    _ => None,
                },
                _ => None,
            })
            .max();
        match (last, lore()) {
            (Some(chapters), Some(lore)) => {
                push(
                    &mut out.abilities,
                    (AbilityKind::Saga { lore, chapters }, Vec::new()),
                    None,
                );
            }
            _ => out.unparsed.push("a Saga without chapters".into()),
        }
    }

    // CR 702.96 — overload: the same text with "each" for "target", and no targets.
    if let Some((mana, original)) = overload {
        let mut cx = Cx::new(subtypes);
        let each = spell_texts
            .iter()
            .map(|l| l.replace("target ", "each "))
            .map(|l| clauses::effect(&l, &mut cx))
            .collect::<Option<Vec<_>>>();
        match each {
            Some(mut effects) if !effects.is_empty() && cx.targets.is_empty() => {
                let instead = if effects.len() == 1 {
                    effects.pop()
                } else {
                    Some(Effect::Sequence(effects))
                };
                push(
                    &mut out.abilities,
                    (
                        AbilityKind::AlternativeCost {
                            cost: Cost {
                                mana,
                                ..Cost::free()
                            },
                            kind: mtg_ir::ability::AltCost::Overload,
                            instead: instead.map(Box::new),
                        },
                        Vec::new(),
                    ),
                    Some(&original),
                );
            }
            _ => out.unparsed.push(original),
        }
    }

    // CR 702.113 — awaken: cast for this cost, the spell also puts N +1/+1 counters on
    // target land you control, which becomes a 0/0 Elemental creature with haste that is
    // still a land. Its targets are the spell's and then the land.
    if let Some((n, mana, original)) = awaken {
        let mut cx = Cx::new(subtypes);
        let effects = spell_texts
            .iter()
            .map(|l| clauses::effect(l, &mut cx))
            .collect::<Option<Vec<_>>>();
        match (effects, cx.subtype("elemental")) {
            (Some(mut effects), Some(elemental)) if !effects.is_empty() => {
                let land = cx.target(TargetSpec {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(vec![
                        ObjectFilter::HasType(CardType::Land),
                        ObjectFilter::ControlledBy(Box::new(Selector::You)),
                    ]),
                    allows_players: false,
                    players: None,
                    mode: None,
                    count: Value::ONE,
                    up_to: false,
                    distinct_from_other_targets: false,
                });
                effects.push(Effect::AddCounters {
                    what: land.clone(),
                    kind: mtg_core::CounterKind::PlusOnePlusOne,
                    amount: Value::Fixed(n),
                });
                for modification in [
                    Modification::AddTypes(vec![CardType::Creature]),
                    Modification::AddSubtypes(vec![elemental]),
                    Modification::SetBasePowerToughness {
                        power: Value::ZERO,
                        toughness: Value::ZERO,
                    },
                    clauses::grant(mtg_core::Keyword::Haste),
                ] {
                    effects.push(Effect::Continuous {
                        what: land.clone(),
                        modification,
                        duration: mtg_ir::effect::Duration::Permanent,
                    });
                }
                push(
                    &mut out.abilities,
                    (
                        AbilityKind::AlternativeCost {
                            cost: Cost {
                                mana,
                                ..Cost::free()
                            },
                            kind: mtg_ir::ability::AltCost::Awaken,
                            instead: Some(Box::new(Effect::Sequence(effects))),
                        },
                        cx.targets,
                    ),
                    Some(&original),
                );
            }
            _ => out.unparsed.push(original),
        }
    }

    // Kicker and buyback share one announcement; a card with both is not understood.
    if out
        .abilities
        .iter()
        .filter(|a| matches!(a.kind, AbilityKind::Kicker { .. }))
        .count()
        > 1
    {
        out.unparsed.push("more than one kicker or buyback".into());
    }

    // The spell's effect is one ability: if any line of it was not understood, none of it
    // is compiled. A spell that did half of what it says is worse than one that does
    // nothing and is reported as not playable.
    // A modal spell's modes are chosen on casting only when they are the whole effect,
    // so a modal line beside other effect lines is not compiled.
    let spell_failed =
        is_spell && (!out.unparsed.is_empty() || (spell_modal && spell_lines.len() > 1));
    if is_spell && spell_modal && spell_lines.len() > 1 {
        out.unparsed.push("a modal spell with other effects".into());
    }
    if !spell_lines.is_empty() && !spell_failed {
        let effect = if spell_lines.len() == 1 {
            spell_lines.pop().unwrap_or(Effect::Nothing)
        } else {
            Effect::Sequence(spell_lines)
        };
        push(
            &mut out.abilities,
            (AbilityKind::SpellEffect(effect), spell_cx.targets),
            None,
        );
    }
    // Each results table goes to the next roll without one, in order.
    if !tables.is_empty() {
        let mut tables = tables.into_iter();
        for a in &mut out.abilities {
            if let AbilityKind::SpellEffect(e)
            | AbilityKind::Activated { effect: e, .. }
            | AbilityKind::Triggered { effect: e, .. } = &mut a.kind
            {
                fill_die_tables(e, &mut tables);
            }
        }
        if tables.next().is_some() {
            out.unparsed.push("a results table with no die roll".into());
        }
    }
    out
}

/// Put `MarkOnceEachTurn` at the start of what saying yes to a "you may" does. `false`
/// when the effect isn't a "you may …".
fn mark_once_each_turn(e: &mut Effect) -> bool {
    match e {
        Effect::May { then, .. } | Effect::MayPay { then, .. } => {
            let rest = std::mem::replace(&mut **then, Effect::Nothing);
            **then = Effect::Sequence(vec![Effect::MarkOnceEachTurn, rest]);
            true
        }
        // "You may discard a card. If you do, …" is offered only when it can be done.
        Effect::If {
            then, otherwise, ..
        } if **otherwise == Effect::Nothing => mark_once_each_turn(then),
        _ => false,
    }
}

/// "1—9 | Draw a card.", "20 | …": a row of a die roll's results table.
fn die_row(line: &str) -> Option<(u32, u32, &str)> {
    let (range, text) = line.split_once(" | ")?;
    let (lo, hi) = range.split_once('—').unwrap_or((range, range));
    Some((lo.parse().ok()?, hi.parse().ok()?, text))
}

/// Give each `RollDie` with no table the next table, in order.
fn fill_die_tables(e: &mut Effect, tables: &mut impl Iterator<Item = Vec<(u32, u32, Effect)>>) {
    match e {
        Effect::RollDie { outcomes, then, .. } => {
            if outcomes.is_empty()
                && let Some(t) = tables.next()
            {
                *outcomes = t;
            }
            fill_die_tables(then, tables);
        }
        Effect::Sequence(items) => {
            for item in items {
                fill_die_tables(item, tables);
            }
        }
        Effect::May { then, .. } | Effect::MayPay { then, .. } => fill_die_tables(then, tables),
        Effect::If {
            then, otherwise, ..
        } => {
            fill_die_tables(then, tables);
            fill_die_tables(otherwise, tables);
        }
        Effect::Reflexive { effect, .. } => fill_die_tables(effect, tables),
        _ => {}
    }
}

/// Backup N (CR 702.165): "When this creature enters, put N +1/+1 counters on target
/// creature. If that's another creature, it gains the following abilities until end of
/// turn." The following abilities are every ability printed after it.
fn backup_trigger(
    n: i32,
    following: &[Ability],
    face: &FaceText,
    subtypes: &dyn Subtypes,
) -> Option<(AbilityKind, Vec<TargetSpec>)> {
    let mut grants = Vec::new();
    for a in following {
        // Granted static abilities are not supported (ADR-014).
        if !matches!(
            a.kind,
            AbilityKind::Keyword(_) | AbilityKind::Activated { .. } | AbilityKind::Triggered { .. }
        ) {
            return None;
        }
        grants.push(Effect::Continuous {
            what: Selector::Target { index: 0 },
            modification: Modification::GrantAbility(Box::new(Ability {
                id: AbilityId(0),
                ..a.clone()
            })),
            duration: mtg_ir::effect::Duration::UntilEndOfTurn,
        });
    }
    let counters = if n == 1 {
        "a +1/+1 counter".to_string()
    } else {
        format!("{n} +1/+1 counters")
    };
    let mut cx = Cx::new(subtypes);
    let line = format!("when ~ enters, put {counters} on target creature.");
    let Some(Line::Abilities(mut list)) = compile_line(&line, face, &mut cx) else {
        return None;
    };
    let (AbilityKind::Triggered { trigger, effect }, targets) = list.pop()? else {
        return None;
    };
    let effect = if grants.is_empty() {
        effect
    } else {
        Effect::Sequence(vec![
            effect,
            Effect::If {
                cond: mtg_ir::trigger::Condition::Exists(Selector::Except(
                    Box::new(Selector::Target { index: 0 }),
                    Box::new(Selector::SelfSource),
                )),
                then: Box::new(Effect::Sequence(grants)),
                otherwise: Box::new(Effect::Nothing),
            },
        ])
    };
    Some((AbilityKind::Triggered { trigger, effect }, targets))
}

/// A leveler card (CR 711): "Level up {cost}", then brackets — "LEVEL 2-6", a power and
/// toughness, and abilities — each true while it has that many level counters. A Class
/// (CR 716) is read the same way: "{1}{W}: Level 2" gains the level as a sorcery, and the
/// abilities below it are the Class's from that level on.
fn leveler(
    face: &FaceText,
    subtypes: &dyn Subtypes,
    normalised: &str,
    text: &str,
    mut out: Compiled,
) -> Compiled {
    use mtg_ir::trigger::Condition as C;
    // A Spacecraft or Planet with station (CR 702.184, 721) counts charge counters.
    let station = normalised.lines().any(|l| l == "station");
    let Some((level, _)) = words::counter(if station {
        "charge counter"
    } else {
        "level counter"
    }) else {
        out.unparsed.push("level counters".into());
        return out;
    };
    let count = || Value::Counters(Box::new(Selector::SelfSource), level);
    let printed: Vec<String> = text.lines().map(strip_reminder).collect();
    let mut bracket: Option<C> = None;
    for (i, line) in normalised.lines().enumerate() {
        let original = printed.get(i).cloned().unwrap_or_default();
        if line.is_empty() {
            continue;
        }
        // CR 702.184a — "Station": tap another untapped creature you control to put charge
        // counters equal to its power on this, as a sorcery. Its reminder says at what
        // count a Spacecraft is an artifact creature (CR 721.2).
        if station && line == "station" {
            push(
                &mut out.abilities,
                (
                    AbilityKind::Activated {
                        cost: Cost {
                            additional: vec![AdditionalCost::TapUntapped {
                                filter: ObjectFilter::And(vec![
                                    ObjectFilter::HasType(CardType::Creature),
                                    ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
                                ]),
                                count: Value::ONE,
                            }],
                            ..Cost::free()
                        },
                        effect: Effect::AddCounters {
                            what: Selector::SelfSource,
                            kind: level,
                            amount: Value::Power(Box::new(Selector::Bound(
                                mtg_ir::selector::Binding::TAPPED,
                            ))),
                        },
                        functions_from: Zone::Battlefield,
                        is_mana_ability: false,
                        is_loyalty_ability: false,
                        timing: ActivationTiming::SorcerySpeed,
                    },
                    Vec::new(),
                ),
                Some(&original),
            );
            let raw = text.lines().nth(i).unwrap_or_default();
            if let Some(n) = raw
                .split("artifact creature at ")
                .nth(1)
                .and_then(|r| r.split('+').next())
                .and_then(|n| n.parse::<i32>().ok())
            {
                push(
                    &mut out.abilities,
                    (
                        AbilityKind::Static {
                            what: Selector::SelfSource,
                            modification: Modification::AddTypes(vec![CardType::Creature]),
                            condition: Some(C::ValueAtLeast {
                                lhs: count(),
                                rhs: Value::Fixed(n),
                            }),
                        },
                        Vec::new(),
                    ),
                    Some(&original),
                );
            }
            continue;
        }
        // "9+ | flying, first strike": true from that many charge counters on.
        let station_line = station
            .then(|| {
                let (n, r) = line.split_once("+ | ")?;
                Some((n.parse::<i32>().ok()?, r))
            })
            .flatten();
        let line = match station_line {
            Some((n, r)) => {
                bracket = Some(C::ValueAtLeast {
                    lhs: count(),
                    rhs: Value::Fixed(n),
                });
                r
            }
            // A line after a station section belongs to it: only a section's first
            // line carries the "N+ |".
            None => line,
        };
        // "level up {2}": {2}: put a level counter on this; level up only as a sorcery.
        if let Some(r) = line.strip_prefix("level up ") {
            match mana_cost(r) {
                Some((mana, "")) => push(
                    &mut out.abilities,
                    (
                        AbilityKind::Activated {
                            cost: Cost {
                                mana,
                                ..Cost::free()
                            },
                            effect: Effect::AddCounters {
                                what: Selector::SelfSource,
                                kind: level,
                                amount: Value::ONE,
                            },
                            functions_from: Zone::Battlefield,
                            is_mana_ability: false,
                            is_loyalty_ability: false,
                            timing: ActivationTiming::SorcerySpeed,
                        },
                        Vec::new(),
                    ),
                    Some(&original),
                ),
                _ => out.unparsed.push(original),
            }
            continue;
        }
        // "{3}{u}: level 2"
        if let Some((mana, r)) = mana_cost(line)
            && let Some(n) = r.strip_prefix(": level ")
        {
            match n.parse::<u8>() {
                Ok(n) if n >= 2 => {
                    push(
                        &mut out.abilities,
                        (
                            AbilityKind::Activated {
                                cost: Cost {
                                    mana,
                                    ..Cost::free()
                                },
                                effect: Effect::GainClassLevel { level: n },
                                functions_from: Zone::Battlefield,
                                is_mana_ability: false,
                                is_loyalty_ability: false,
                                timing: ActivationTiming::ClassLevel(n),
                            },
                            Vec::new(),
                        ),
                        Some(&original),
                    );
                    bracket = Some(C::ClassLevelAtLeast(n));
                }
                _ => out.unparsed.push(original),
            }
            continue;
        }
        // "level 2-6", "level 7+"
        if let Some(r) = line.strip_prefix("level ") {
            let range = if let Some(lo) = r.strip_suffix('+') {
                lo.parse::<i32>().ok().map(|lo| (lo, None))
            } else {
                r.split_once('-').and_then(|(lo, hi)| {
                    Some((lo.parse::<i32>().ok()?, Some(hi.parse::<i32>().ok()?)))
                })
            };
            match range {
                Some((lo, hi)) => {
                    let at_least = C::ValueAtLeast {
                        lhs: count(),
                        rhs: Value::Fixed(lo),
                    };
                    bracket = Some(match hi {
                        Some(hi) => C::And(vec![
                            at_least,
                            C::ValueAtLeast {
                                lhs: Value::Fixed(hi),
                                rhs: count(),
                            },
                        ]),
                        None => at_least,
                    });
                }
                None => out.unparsed.push(original),
            }
            continue;
        }
        let Some(cond) = bracket.clone() else {
            // Before any bracket: an ordinary line.
            let mut cx = Cx::new(subtypes);
            match compile_line(line, face, &mut cx) {
                Some(Line::Abilities(list)) => {
                    for kt in list {
                        push(&mut out.abilities, kt, Some(&original));
                    }
                }
                _ => out.unparsed.push(original),
            }
            continue;
        };
        // The bracket's power and toughness.
        if let Some(((p, t), "")) = words::pt(line) {
            push(
                &mut out.abilities,
                (
                    AbilityKind::Static {
                        what: Selector::SelfSource,
                        modification: Modification::SetBasePowerToughness {
                            power: Value::Fixed(p),
                            toughness: Value::Fixed(t),
                        },
                        condition: Some(cond),
                    },
                    Vec::new(),
                ),
                Some(&original),
            );
            continue;
        }
        // The bracket's abilities, true only within it.
        let mut cx = Cx::new(subtypes);
        let Some(Line::Abilities(list)) = compile_line(line, face, &mut cx) else {
            out.unparsed.push(original);
            continue;
        };
        for (kind, targets) in list {
            // "When this Class becomes level 2" can only trigger at that level.
            if let AbilityKind::Triggered { trigger, .. } = &kind
                && matches!(trigger.on, EventPattern::BecomesClassLevel { .. })
            {
                push(&mut out.abilities, (kind, targets), Some(&original));
                continue;
            }
            let conditional = match kind {
                AbilityKind::Static {
                    what,
                    modification,
                    condition,
                } => Some(AbilityKind::Static {
                    what,
                    modification,
                    condition: Some(match condition {
                        Some(c) => C::And(vec![cond.clone(), c]),
                        None => cond.clone(),
                    }),
                }),
                kind @ (AbilityKind::Keyword(_)
                | AbilityKind::Activated { .. }
                | AbilityKind::Triggered { .. }) => Some(AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::GrantAbility(Box::new(Ability {
                        id: AbilityId(0),
                        kind,
                        targets,
                        source_text: Some(line.into()),
                    })),
                    condition: Some(cond.clone()),
                }),
                _ => None,
            };
            match conditional {
                Some(k) => push(&mut out.abilities, (k, Vec::new()), Some(&original)),
                None => {
                    out.unparsed.push(original.clone());
                }
            }
        }
    }
    out
}

/// Whether a face is playable from its printed text alone.
pub fn understood(face: &FaceText, subtypes: &dyn Subtypes) -> bool {
    compile(face, subtypes).understood()
}

/// [`understood`] for a stored face row, parsing its type line on the way.
pub fn row_understood(row: &mtg_store::FaceRow, subtypes: &dyn Subtypes) -> bool {
    let types = crate::typeline::parse(&row.type_line);
    understood(
        &FaceText {
            name: &row.name,
            card_types: &types.card_types,
            subtypes: &types.subtypes,
            oracle_text: row.oracle_text.as_deref(),
            mana_cost: row.mana_cost.as_str(),
        },
        subtypes,
    )
}

/// Whether the engine supports this card's face relationship and face count.
/// Other layouts stay excluded even when every face's rules text compiles.
pub fn layout_understood(card: &mtg_store::StoredCard) -> bool {
    match (card.layout.as_str(), card.faces.len()) {
        ("normal", 1)
        | ("modal_dfc", 2)
        | ("saga", 1)
        | ("transform", 2)
        | ("leveler", 1)
        | ("class", 1) => true,
        ("split", 2) => card.faces.iter().all(|face| {
            let types = crate::typeline::parse(&face.type_line).card_types;
            types
                .iter()
                .any(|ty| matches!(ty, CardType::Instant | CardType::Sorcery))
                && !types.iter().any(|ty| ty.is_permanent())
        }),
        ("adventure", 2) => {
            let normal = crate::typeline::parse(&card.faces[0].type_line);
            let adventure = crate::typeline::parse(&card.faces[1].type_line);
            let spell = |types: &[CardType]| {
                types.contains(&CardType::Instant) || types.contains(&CardType::Sorcery)
            };
            !spell(&normal.card_types) && spell(&adventure.card_types)
        }
        _ => false,
    }
}

/// Plain split casting supports spell abilities and stack-only modifiers. Hand or
/// graveyard activations and triggers need combined ability lookup before admission.
pub fn split_abilities_understood(compiled: &Compiled) -> bool {
    compiled
        .abilities
        .iter()
        .all(|ability| match &ability.kind {
            AbilityKind::SpellEffect(_)
            | AbilityKind::Keyword(_)
            | AbilityKind::CastFrom { .. }
            | AbilityKind::Aftermath
            | AbilityKind::AdditionalCastCost { .. }
            | AbilityKind::Kicker { .. } => true,
            AbilityKind::Static {
                what: Selector::SelfSource,
                ..
            } => true,
            AbilityKind::Triggered { trigger, .. } => trigger.functions_from == Zone::Stack,
            _ => false,
        })
}

/// Playable as printed only when its layout and every face are understood.
pub fn card_understood(card: &mtg_store::StoredCard, subtypes: &dyn Subtypes) -> bool {
    layout_understood(card)
        && card.faces.iter().all(|row| {
            let types = crate::typeline::parse(&row.type_line);
            let compiled = compile(
                &FaceText {
                    name: &row.name,
                    card_types: &types.card_types,
                    subtypes: &types.subtypes,
                    oracle_text: row.oracle_text.as_deref(),
                    mana_cost: &row.mana_cost,
                },
                subtypes,
            );
            compiled.understood()
                && (card.layout != "split" || split_abilities_understood(&compiled))
        })
}

/// A subtype table held by value, for callers that have the store's list rather than a
/// loaded [`crate::CardLibrary`].
pub struct SubtypeNames(pub Vec<(u16, String)>);

impl Subtypes for SubtypeNames {
    fn intern(&self, name: &str) -> Option<u16> {
        self.0.iter().find(|(_, n)| n == name).map(|(id, _)| *id)
    }
}

fn push(
    list: &mut Vec<Ability>,
    (kind, targets): (AbilityKind, Vec<TargetSpec>),
    text: Option<&str>,
) {
    // A keyword already present from another line is not repeated.
    if let AbilityKind::Keyword(k) = kind
        && list
            .iter()
            .any(|a| matches!(a.kind, AbilityKind::Keyword(x) if x == k))
    {
        return;
    }
    list.push(Ability {
        id: AbilityId(list.len() as u16),
        kind,
        targets,
        source_text: text.map(Into::into),
    });
}

/// A quoted ability a token or effect grants: `"{T}: Add {C}."`, `"This token can't
/// block."`. Compiled like a printed line of the object that will have it.
pub(crate) fn quoted_abilities(text: &str, cx: &Cx) -> Option<Vec<Ability>> {
    let face = FaceText {
        name: "",
        card_types: &[CardType::Creature],
        subtypes: &[],
        oracle_text: None,
        mana_cost: "",
    };
    let mut out = Vec::new();
    for line in text.split("\n") {
        let mut inner = cx.fresh();
        let Line::Abilities(list) = compile_line(line.trim(), &face, &mut inner)? else {
            return None;
        };
        for (kind, targets) in list {
            push(&mut out, (kind, targets), Some(line));
        }
    }
    Some(out)
}

/// What one line compiles to.
enum Line {
    Abilities(Vec<(AbilityKind, Vec<TargetSpec>)>),
    /// Devoid: the face has no colour whatever its mana cost says.
    Colorless,
}

fn compile_line(line: &str, face: &FaceText, cx: &mut Cx) -> Option<Line> {
    let one = |kind, cx: &mut Cx| {
        Some(Line::Abilities(vec![(
            kind,
            std::mem::take(&mut cx.targets),
        )]))
    };
    let line = words::strip_ability_word(line);

    // "Whenever you gain life, you may put that many +1/+1 counters on each creature you
    // control. Do this only once each turn." — only doing it counts, not declining.
    if let Some(head) = line.strip_suffix(" do this only once each turn.")
        && let Some(Line::Abilities(mut list)) = compile_line(head, face, cx)
        && let [(AbilityKind::Triggered { effect, .. }, _)] = list.as_mut_slice()
        && mark_once_each_turn(effect)
    {
        let body = std::mem::replace(effect, Effect::Nothing);
        *effect = Effect::OnceEachTurn {
            body: Box::new(body),
        };
        return Some(Line::Abilities(list));
    }
    // "Enchanted creature has protection from green. This effect doesn't remove this Aura."
    if let Some(head) = line.strip_suffix(" this effect doesn't remove ~.")
        && face.subtypes.iter().any(|s| s.eq_ignore_ascii_case("aura"))
        && let Some(Line::Abilities(mut list)) = compile_line(head, face, cx)
    {
        let mut spared = false;
        for (kind, _) in &mut list {
            if let AbilityKind::Static {
                modification:
                    Modification::Restriction(mtg_ir::effect::Restriction::Protection {
                        spares_source,
                        ..
                    }),
                ..
            } = kind
            {
                *spares_source = true;
                spared = true;
            }
        }
        if spared {
            return Some(Line::Abilities(list));
        }
    }

    // "Max speed — <ability>" (CR 702.179f): the ability exists only at max speed. Speed
    // never goes down, so a trigger's condition is checked as an intervening "if".
    if let Some(rest) = line.strip_prefix("max speed — ") {
        use mtg_ir::trigger::Condition as C;
        let Some(Line::Abilities(list)) = compile_line(rest, face, cx) else {
            return None;
        };
        let and = |c: Option<C>| match c {
            Some(c) => C::And(vec![C::MaxSpeed, c]),
            None => C::MaxSpeed,
        };
        let mut out = Vec::new();
        for (kind, targets) in list {
            let kind = match kind {
                AbilityKind::Static {
                    what,
                    modification,
                    condition,
                } => AbilityKind::Static {
                    what,
                    modification,
                    condition: Some(and(condition)),
                },
                AbilityKind::Triggered {
                    mut trigger,
                    effect,
                } => {
                    trigger.intervening_if = Some(and(trigger.intervening_if.take()));
                    AbilityKind::Triggered { trigger, effect }
                }
                AbilityKind::Activated {
                    cost,
                    effect,
                    functions_from,
                    is_mana_ability: false,
                    is_loyalty_ability: false,
                    timing: ActivationTiming::Instant,
                } => AbilityKind::Activated {
                    cost,
                    effect,
                    functions_from,
                    is_mana_ability: false,
                    is_loyalty_ability: false,
                    timing: ActivationTiming::MaxSpeed,
                },
                AbilityKind::Keyword(k) => AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: clauses::grant(k),
                    condition: Some(C::MaxSpeed),
                },
                _ => return None,
            };
            out.push((kind, targets));
        }
        return Some(Line::Abilities(out));
    }

    // CR 702.114: devoid is a characteristic-defining ability — the card has no colour,
    // in every zone.
    if line == "devoid" {
        return Some(Line::Colorless);
    }
    if let Some(list) = chapter(line, face, cx) {
        return Some(Line::Abilities(list));
    }
    // CR 702.82 — devour N.
    if let Some(n) = line.strip_prefix("devour ")
        && let Ok(n) = n.trim_end_matches('.').parse::<u8>()
        && face.card_types.contains(&CardType::Creature)
    {
        return one(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind: ReplacementKind::Devour(n),
            }),
            cx,
        );
    }
    // CR 702.52 — dredge N.
    if let Some(n) = line.strip_prefix("dredge ")
        && let Ok(n) = n.trim_end_matches('.').parse::<u8>()
        && n > 0
    {
        return one(AbilityKind::Dredge(n), cx);
    }
    // CR 702.125a — undaunted, written out.
    if line == "undaunted" {
        return compile_line(
            "~ costs {1} less to cast for each opponent you have.",
            face,
            cx,
        );
    }
    // CR 702.166 — bargain: an optional additional cost, paid and remembered like a kicker
    // whose cost is a sacrifice.
    if line == "bargain" {
        return compile_line(
            "kicker—sacrifice an artifact, enchantment, or token.",
            face,
            cx,
        );
    }
    // CR 702.39a — provoke, written out.
    if line == "provoke" && face.card_types.contains(&CardType::Creature) {
        return compile_line(
            "whenever ~ attacks, you may have target creature defending player controls untap \
             and block it this combat if able.",
            face,
            cx,
        );
    }
    if let Some(kind) = keyword_trigger(line) {
        // Flanking is also a keyword other creatures' flanking looks for.
        if line.trim_end_matches('.') == "flanking" {
            return Some(Line::Abilities(vec![
                (
                    AbilityKind::Keyword(mtg_core::Keyword::Flanking),
                    Vec::new(),
                ),
                (kind, Vec::new()),
            ]));
        }
        return one(kind, cx);
    }
    if let Some(list) = counter_keyword(line) {
        return Some(Line::Abilities(list));
    }
    if let Some(list) = mentor(line) {
        return Some(Line::Abilities(list));
    }
    // Deck construction only (CR 702.124, 903.3): nothing to do during a game.
    if matches!(
        line.trim_end_matches('.'),
        "partner"
            | "~ can be your commander"
            | "choose a background"
            | "doctor's companion"
            | "friends forever"
    ) {
        return one(AbilityKind::DeckRule(line.trim_end_matches('.').into()), cx);
    }
    if let Some(list) = suspend(line, face) {
        return Some(Line::Abilities(list));
    }
    if let Some(list) = alternative_cost(line, face) {
        return Some(Line::Abilities(list));
    }
    if let Some(list) = enter_choice_keyword(line, face) {
        return Some(Line::Abilities(list));
    }
    if let Some(kind) = fabricate(line, cx) {
        return one(kind, cx);
    }
    if let Some(lines) = keyword_expansion(line) {
        let mut out = Vec::new();
        for l in lines {
            let mut inner = cx.fresh();
            let Line::Abilities(list) = compile_line(&l, face, &mut inner)? else {
                return None;
            };
            out.extend(list);
        }
        return Some(Line::Abilities(out));
    }
    if let Some(kind) = cycling(line, cx) {
        return one(kind, cx);
    }
    if let Some(kind) = additional_cast_cost(line, cx) {
        return one(kind, cx);
    }
    if line.starts_with("affinity for ")
        && let Some(kind) = self_cost(line, cx)
    {
        return one(kind, cx);
    }
    if let Some(kind) = unearth(line) {
        return one(kind, cx);
    }
    if let Some(kind) = scavenge(line, cx) {
        return one(kind, cx);
    }
    if face.card_types.contains(&CardType::Creature)
        && let Some(kind) = encore(line)
    {
        return one(kind, cx);
    }
    if let Some(kind) = conditional_alternative_cost(line) {
        return one(kind, cx);
    }
    if let Some(kind) = plain_alternative_cost(line, cx) {
        return one(kind, cx);
    }
    // CR 702.49 — "ninjutsu {1}{U}": {1}{U}, return an unblocked attacker you control to
    // hand: put this card onto the battlefield from your hand tapped and attacking.
    if let Some(r) = line.strip_prefix("ninjutsu ")
        && let Some((mana, "")) = mana_cost(r)
        && face.card_types.contains(&CardType::Creature)
    {
        return one(
            AbilityKind::Activated {
                cost: Cost {
                    mana,
                    additional: vec![AdditionalCost::ReturnUnblockedAttacker],
                    ..Cost::free()
                },
                effect: Effect::EnterAttacking {
                    what: Selector::SelfSource,
                    like: mtg_ir::selector::Binding::NINJUTSU_DEFENDER,
                },
                functions_from: Zone::Hand,
                is_mana_ability: false,
                is_loyalty_ability: false,
                timing: ActivationTiming::Instant,
            },
            cx,
        );
    }
    // CR 702.143 — "foretell {1}{U}": during your turn, pay {2} and exile it face down; cast
    // it on a later turn for this. CR 702.170 — "plot {1}{R}": as a sorcery, pay this and
    // exile it; cast it on a later turn without paying its mana cost, as a sorcery.
    for (prefix, foretell) in [("foretell ", true), ("plot ", false)] {
        if let Some(r) = line.strip_prefix(prefix)
            && let Some((mana, "")) = mana_cost(r)
            && !face.card_types.contains(&CardType::Land)
        {
            let two = mana_cost("{2}")?.0;
            let (cost, later) = if foretell {
                (two, Some(mana))
            } else {
                (mana, None)
            };
            return one(
                AbilityKind::ExileToCastLater {
                    cost: Cost {
                        mana: cost,
                        ..Cost::free()
                    },
                    face_down: foretell,
                    later,
                    sorcery: !foretell,
                },
                cx,
            );
        }
    }
    // CR 702.37 — "morph {2}{U}", "megamorph {1}{G}": a creature card only, and only a
    // mana cost to turn it face up.
    for (prefix, megamorph, disguise) in [
        ("morph ", false, false),
        ("megamorph ", true, false),
        ("disguise ", false, true),
    ] {
        if let Some(r) = line.strip_prefix(prefix)
            && let Some((mana, "")) = mana_cost(r)
            && face.card_types.contains(&CardType::Creature)
        {
            return one(
                AbilityKind::Morph {
                    cost: Cost {
                        mana,
                        ..Cost::free()
                    },
                    megamorph,
                    disguise,
                },
                cx,
            );
        }
    }
    // CR 702.77 — "reinforce 2—{1}{W}": {1}{W}, discard this card: put two +1/+1 counters on
    // target creature. CR 702.53 — "transmute {1}{U}{U}": {1}{U}{U}, discard this card:
    // search for a card with the same mana value, as a sorcery.
    let expanded = if let Some(r) = line.strip_prefix("reinforce ")
        && let Some((n, r)) = words::number(r)
        && let Some(cost) = r.strip_prefix('—')
        && let Some((_, "")) = mana_cost(cost)
    {
        let counters = if n == 1 {
            "a +1/+1 counter".to_string()
        } else {
            format!("{n} +1/+1 counters")
        };
        Some(format!(
            "{cost}, discard ~: put {counters} on target creature."
        ))
    } else if let Some((cost, except)) = line
        .strip_prefix("embalm ")
        .map(|c| (c, "a white zombie"))
        .or_else(|| {
            line.strip_prefix("eternalize ")
                .map(|c| (c, "a 4/4 black zombie"))
        })
        && let Some((_, "")) = mana_cost(cost)
        && face.card_types.contains(&CardType::Creature)
    {
        // CR 702.128 embalm, CR 702.129 eternalize.
        Some(format!(
            "{cost}, exile ~ from your graveyard: create a token that's a copy of it, except \
             it's {except} with no mana cost. activate only as a sorcery."
        ))
    } else if let Some(cost) = line.strip_prefix("transmute ")
        && let Some((_, "")) = mana_cost(cost)
        && let Some((mana, _)) = mana_cost(&face.mana_cost.to_lowercase())
    {
        let mv = mana.mana_value();
        Some(format!(
            "{cost}, discard ~: search your library for a card with mana value {mv}, reveal \
             it, put it into your hand, then shuffle. activate only as a sorcery."
        ))
    } else {
        None
    };
    if let Some(text) = expanded {
        return compile_line(&text, face, cx);
    }
    // CR 702.171a — "saddle N": tap other creatures with total power N or more: this Mount
    // becomes saddled until end of turn; only as a sorcery.
    if let Some(r) = line.strip_prefix("saddle ")
        && let Some((n, "")) = words::number(r)
    {
        return one(
            AbilityKind::Activated {
                cost: Cost {
                    additional: vec![AdditionalCost::TapCreaturesWithPower {
                        power: Value::Fixed(n),
                    }],
                    ..Cost::free()
                },
                effect: Effect::Continuous {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::Saddled),
                    duration: mtg_ir::effect::Duration::UntilEndOfTurn,
                },
                functions_from: Zone::Battlefield,
                is_mana_ability: false,
                is_loyalty_ability: false,
                timing: ActivationTiming::SorcerySpeed,
            },
            cx,
        );
    }
    // CR 702.122a — "crew N": tap creatures with total power N or more, and this becomes
    // an artifact creature until end of turn.
    if let Some(r) = line.strip_prefix("crew ")
        && let Some((n, "")) = words::number(r)
    {
        return one(
            AbilityKind::Activated {
                cost: Cost {
                    additional: vec![AdditionalCost::TapCreaturesWithPower {
                        power: Value::Fixed(n),
                    }],
                    ..Cost::free()
                },
                effect: Effect::Continuous {
                    what: Selector::SelfSource,
                    modification: Modification::AddTypes(vec![CardType::Creature]),
                    duration: mtg_ir::effect::Duration::UntilEndOfTurn,
                },
                functions_from: Zone::Battlefield,
                is_mana_ability: false,
                is_loyalty_ability: false,
                timing: ActivationTiming::Instant,
            },
            cx,
        );
    }
    // CR 702.33 — "kicker {2}{R}". A permanent remembers being kicked. CR 702.27 —
    // "buyback {3}" on an instant or sorcery is announced and paid the same way.
    let buyback = (line.starts_with("buyback ") || line.starts_with("buyback—"))
        && face
            .card_types
            .iter()
            .any(|t| matches!(t, CardType::Instant | CardType::Sorcery));
    if let Some(r) = line
        .strip_prefix("kicker ")
        .or_else(|| line.strip_prefix("buyback ").filter(|_| buyback))
        && let Some((mana, "")) = mana_cost(r)
    {
        return one(
            AbilityKind::Kicker {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                buyback,
                entwine: false,
                multi: false,
            },
            cx,
        );
    }
    // CR 702.103 — "Strive — This spell costs {1}{U} more to cast for each target beyond
    // the first." (the ability word is already gone).
    if let Some(r) = line
        .strip_prefix("~ costs ")
        .or_else(|| line.strip_prefix("this spell costs "))
        && let Some((per_target, r)) = mana_cost(r)
        && r == " more to cast for each target beyond the first."
        && !per_target.symbols.is_empty()
    {
        return one(AbilityKind::Strive { per_target }, cx);
    }
    // "You may cast this spell as though it had flash. If you cast it any time a sorcery
    // couldn't have been cast, the controller of the permanent it becomes sacrifices it at
    // the beginning of the next cleanup step."
    if line
        == "you may cast ~ as though it had flash. if you cast it any time a sorcery couldn't \
            have been cast, the controller of the permanent it becomes sacrifices it at the \
            beginning of the next cleanup step."
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        return Some(Line::Abilities(vec![
            (
                AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::FlashFor(
                        ObjectFilter::IsSelf,
                    )),
                    condition: None,
                },
                Vec::new(),
            ),
            (AbilityKind::SacrificeIfFlashed, Vec::new()),
        ]));
    }
    // "You may cast this spell as though it had flash if you pay {2} more to cast it."
    if let Some(r) = line.strip_prefix("you may cast ~ as though it had flash if you pay ")
        && let Some((cost, " more to cast it.")) = mana_cost(r)
        && !cost.symbols.is_empty()
    {
        return Some(Line::Abilities(vec![
            (
                AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::FlashFor(
                        ObjectFilter::IsSelf,
                    )),
                    condition: None,
                },
                Vec::new(),
            ),
            (AbilityKind::FlashSurcharge { cost }, Vec::new()),
        ]));
    }
    // CR 702.119 — "emerge {5}{U}{U}": cast by sacrificing a creature and paying this,
    // less that creature's mana value.
    if let Some(r) = line.strip_prefix("emerge ")
        && let Some((mana, "")) = mana_cost(r)
    {
        return one(
            AbilityKind::AlternativeCost {
                cost: Cost {
                    mana,
                    additional: vec![AdditionalCost::Sacrifice {
                        what: Selector::All {
                            zone: Zone::Battlefield,
                            filter: ObjectFilter::And(vec![
                                ObjectFilter::HasType(CardType::Creature),
                                ObjectFilter::ControlledBy(Box::new(Selector::You)),
                            ]),
                        },
                        count: Value::ONE,
                    }],
                    ..Cost::free()
                },
                kind: mtg_ir::ability::AltCost::Emerge,
                instead: None,
            },
            cx,
        );
    }
    // CR 702.120 — "escalate {1}": more for each mode chosen beyond the first.
    if let Some(r) = line.strip_prefix("escalate ")
        && let Some((per_mode, "")) = mana_cost(r)
        && !per_mode.symbols.is_empty()
    {
        return one(AbilityKind::Escalate { per_mode }, cx);
    }
    // CR 702.33c — "multikicker {1}{G}": a kicker that may be paid any number of times.
    if let Some(r) = line.strip_prefix("multikicker ")
        && let Some((mana, "")) = mana_cost(r)
    {
        return one(
            AbilityKind::Kicker {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                buyback: false,
                entwine: false,
                multi: true,
            },
            cx,
        );
    }
    // CR 702.56 — "replicate {1}{U}": a multikicker on an instant or sorcery; when it is
    // cast, it is copied once for each time it was paid.
    if let Some(r) = line.strip_prefix("replicate ")
        && let Some((mana, "")) = mana_cost(r)
        && face
            .card_types
            .iter()
            .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
    {
        let kicker = AbilityKind::Kicker {
            cost: Cost {
                mana,
                ..Cost::free()
            },
            buyback: false,
            entwine: false,
            multi: true,
        };
        let copy = AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Cast {
                    who: ObjectFilter::IsSelf,
                    by: Selector::You,
                },
                functions_from: Zone::Stack,
                intervening_if: Some(mtg_ir::trigger::Condition::Kicked),
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::Repeat {
                times: Value::TimesKicked,
                body: Box::new(Effect::CopySpell {
                    what: Selector::SelfSource,
                    may_change_targets: true,
                }),
            },
        };
        return Some(Line::Abilities(vec![
            (kicker, Vec::new()),
            (copy, Vec::new()),
        ]));
    }
    // Kicker and buyback may include chosen costs or a fixed life payment.
    if let Some(r) = line
        .strip_prefix("kicker—")
        .or_else(|| line.strip_prefix("buyback—").filter(|_| buyback))
        && let Some(cost) = cost(r.trim_end_matches('.'), cx)
        && !cost.additional.is_empty()
        && cost.additional.iter().all(|p| {
            matches!(
                p,
                AdditionalCost::Sacrifice {
                    what: Selector::All { .. },
                    ..
                } | AdditionalCost::TapUntapped { .. }
                    | AdditionalCost::Discard { .. }
                    | AdditionalCost::PayLife {
                        amount: Value::Fixed(0..),
                    }
            )
        })
    {
        return one(
            AbilityKind::Kicker {
                cost,
                buyback,
                entwine: false,
                multi: false,
            },
            cx,
        );
    }
    // CR 702.153 — "casualty 2": as it is cast, its controller may sacrifice a creature with
    // power 2 or greater; when they do, it is copied. CR 702.78 — conspire: the same, by
    // tapping two untapped creatures that share a color with it. Both are paid like a kicker.
    let copy_kicker = if let Some(n) = line.strip_prefix("casualty ")
        && let Ok(n) = n.trim_end_matches('.').parse::<i32>()
    {
        Some(AdditionalCost::Sacrifice {
            what: Selector::All {
                zone: Zone::Battlefield,
                filter: ObjectFilter::And(vec![
                    ObjectFilter::HasType(CardType::Creature),
                    ObjectFilter::PowerAtLeast(Value::Fixed(n)),
                ]),
            },
            count: Value::ONE,
        })
    } else if line.trim_end_matches('.') == "conspire" {
        Some(AdditionalCost::TapUntapped {
            filter: ObjectFilter::And(vec![
                ObjectFilter::HasType(CardType::Creature),
                ObjectFilter::SharesColorWith(Box::new(Selector::SelfSource)),
            ]),
            count: Value::Fixed(2),
        })
    } else {
        None
    };
    if let Some(part) = copy_kicker {
        let kicker = AbilityKind::Kicker {
            cost: Cost {
                additional: vec![part],
                ..Cost::free()
            },
            buyback: false,
            entwine: false,
            multi: false,
        };
        let copy = AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Cast {
                    who: ObjectFilter::IsSelf,
                    by: Selector::You,
                },
                functions_from: Zone::Stack,
                intervening_if: Some(mtg_ir::trigger::Condition::Kicked),
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::CopySpell {
                what: Selector::SelfSource,
                may_change_targets: true,
            },
        };
        return Some(Line::Abilities(vec![
            (kicker, Vec::new()),
            (copy, Vec::new()),
        ]));
    }
    // "Whenever ~ blocks or becomes blocked by a non-Wall creature, destroy that creature
    // at end of combat.": two triggers, one per side of the block, each binding the other
    // creature. A creature is never on both sides of one block, so they never both fire.
    if let Some(rest) = line.strip_prefix("whenever ~ blocks or becomes blocked by ")
        && (rest.starts_with("a ") || rest.starts_with("an "))
        && !rest.starts_with("a creature,")
        && let Some(Line::Abilities(mut a)) =
            compile_line(&format!("whenever ~ blocks {rest}"), face, &mut cx.fresh())
        && let Some(Line::Abilities(b)) = compile_line(
            &format!("whenever ~ becomes blocked by {rest}"),
            face,
            &mut cx.fresh(),
        )
    {
        a.extend(b);
        return Some(Line::Abilities(a));
    }
    // "Lightbringer and Hero's Shield — Equip {4}": a named equip ability (flavor word).
    if let Some((_, rest)) = line.rsplit_once(" — ")
        && rest.starts_with("equip ")
        && let Some(found) = compile_line(rest, face, &mut cx.fresh())
    {
        return Some(found);
    }
    // CR 701.43 — "You may exert this creature as it attacks. When you do, it gets +1/+1
    // until end of turn." The "when you do" part is a reflexive trigger of its own.
    if let Some(r) = line.strip_prefix("you may exert ~ as it attacks.")
        && face.card_types.contains(&CardType::Creature)
    {
        let effect = if r.is_empty() {
            Some(Effect::Nothing)
        } else {
            let mut inner = cx.fresh();
            inner.it = Some(Selector::SelfSource);
            r.strip_prefix(" when you do, ")
                .and_then(|r| clauses::reflexive(r, &mut inner))
                .and_then(|(e, rest)| matches!(rest, "" | ".").then_some(e))
        };
        if let Some(effect) = effect {
            return one(AbilityKind::ExertAsAttacks { effect }, cx);
        }
    }
    // CR 103.6 — leylines.
    if line == "if ~ is in your opening hand, you may begin the game with it on the battlefield."
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        return one(AbilityKind::BeginOnBattlefield, cx);
    }
    // CR 702.121 — melee: "Whenever this creature attacks, it gets +1/+1 until end of turn
    // for each opponent you attacked with a creature this combat."
    if line == "melee" && face.card_types.contains(&CardType::Creature) {
        return Some(Line::Abilities(vec![(
            AbilityKind::Triggered {
                trigger: Trigger {
                    on: EventPattern::Attacks {
                        who: ObjectFilter::IsSelf,
                    },
                    functions_from: Zone::Battlefield,
                    intervening_if: None,
                    optional: false,
                    limit: None,
                    timing: TriggerTiming::Normal,
                },
                effect: Effect::Continuous {
                    what: Selector::SelfSource,
                    modification: Modification::ModifyPowerToughness {
                        power: Value::OpponentsAttacked,
                        toughness: Value::OpponentsAttacked,
                    },
                    duration: mtg_ir::effect::Duration::UntilEndOfTurn,
                },
            },
            Vec::new(),
        )]));
    }
    // CR 702.157 — "squad {2}": pay it any number of times as you cast it; as many token
    // copies of it are made when it enters.
    if let Some(r) = line.strip_prefix("squad ")
        && let Some((mana, "")) = mana_cost(r)
        && face.card_types.contains(&CardType::Creature)
        && let Some(Line::Abilities(trigger)) = compile_line(
            "when ~ enters, create a token that's a copy of it for each time it was kicked.",
            face,
            &mut cx.fresh(),
        )
    {
        let mut out = vec![(
            AbilityKind::Kicker {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                buyback: false,
                entwine: false,
                multi: true,
            },
            Vec::new(),
        )];
        out.extend(trigger);
        return Some(Line::Abilities(out));
    }
    // CR 702.156 — ravenous: "This creature enters with X +1/+1 counters on it. If X is 5
    // or more, draw a card when it enters."
    if line == "ravenous"
        && face.mana_cost.contains("{X}")
        && let Some(Line::Abilities(mut a)) = compile_line(
            "~ enters with x +1/+1 counters on it.",
            face,
            &mut cx.fresh(),
        )
    {
        a.push((
            AbilityKind::Triggered {
                trigger: Trigger {
                    on: EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    },
                    functions_from: Zone::Battlefield,
                    intervening_if: Some(mtg_ir::trigger::Condition::ValueAtLeast {
                        lhs: Value::CastX,
                        rhs: Value::Fixed(5),
                    }),
                    optional: false,
                    limit: None,
                    timing: TriggerTiming::Normal,
                },
                effect: Effect::Draw {
                    who: Selector::You,
                    count: Value::ONE,
                },
            },
            Vec::new(),
        ));
        return Some(Line::Abilities(a));
    }
    // CR 702.149 — training.
    if line == "training" && face.card_types.contains(&CardType::Creature) {
        let mut inner = cx.fresh();
        return compile_line(
            "whenever ~ attacks with another creature with greater power, put a +1/+1 counter \
             on ~.",
            face,
            &mut inner,
        );
    }
    // CR 702.175 — "offspring {1}{U}": an optional additional cost; paid, a 1/1 token copy
    // of it is made as it enters.
    if let Some(r) = line.strip_prefix("offspring ")
        && let Some((mana, "")) = mana_cost(r)
        && face.card_types.contains(&CardType::Creature)
    {
        let mut inner = cx.fresh();
        let Some(Line::Abilities(mut trigger)) = compile_line(
            "when ~ enters, if it was kicked, create a token that's a copy of it, except it's \
             1/1.",
            face,
            &mut inner,
        ) else {
            return None;
        };
        let mut out = vec![(
            AbilityKind::Kicker {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                buyback: false,
                entwine: false,
                multi: false,
            },
            Vec::new(),
        )];
        out.append(&mut trigger);
        return Some(Line::Abilities(out));
    }
    // CR 702.42 — "entwine {2}": pay it to choose every mode of this modal spell.
    if let Some(r) = line.strip_prefix("entwine ")
        && let Some((mana, "")) = mana_cost(r)
    {
        return one(
            AbilityKind::Kicker {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                buyback: false,
                entwine: true,
                multi: false,
            },
            cx,
        );
    }
    // CR 702.127 — the selected split half is restricted to the graveyard.
    if line == "aftermath"
        && face
            .card_types
            .iter()
            .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
    {
        return one(AbilityKind::Aftermath, cx);
    }
    // CR 702.146 — "disturb {1}{W}": cast from the graveyard transformed, for that.
    if let Some(r) = line.strip_prefix("disturb ")
        && let Some((mana, "")) = mana_cost(r)
    {
        return one(
            AbilityKind::CastFrom {
                zone: Zone::Graveyard,
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                exile: false,
                transformed: true,
            },
            cx,
        );
    }
    // CR 702.34 — "flashback {2}{R}": cast from the graveyard for that, then exile it.
    if let Some(r) = line
        .strip_prefix("flashback ")
        .or_else(|| line.strip_prefix("flashback—"))
        && let Some(cost) = cost(r.strip_suffix('.').unwrap_or(r), cx)
        && cost.additional.iter().all(|part| {
            matches!(
                part,
                AdditionalCost::PayLife {
                    amount: Value::Fixed(0..)
                } | AdditionalCost::Discard {
                    count: Value::Fixed(1..),
                    at_random: false,
                    ..
                } | AdditionalCost::Sacrifice {
                    what: Selector::All {
                        zone: Zone::Battlefield,
                        ..
                    },
                    count: Value::Fixed(1..)
                } | AdditionalCost::ExileFrom {
                    zone: Zone::Graveyard,
                    count: Value::Fixed(1..),
                    ..
                }
            )
        })
        && !cost.additional.iter().any(|part| {
            matches!(
                part,
                AdditionalCost::Discard {
                    filter: ObjectFilter::IsSelf,
                    ..
                } | AdditionalCost::ExileFrom {
                    filter: ObjectFilter::IsSelf,
                    ..
                }
            )
        })
        && face
            .card_types
            .iter()
            .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
    {
        return one(
            AbilityKind::CastFrom {
                zone: Zone::Graveyard,
                cost,
                exile: true,
                transformed: false,
            },
            cx,
        );
    }

    // Cast from the graveyard with an additional cost: retrace (CR 702.81, a land card
    // discarded), jump-start (CR 702.133, any card discarded, then exiled), escape
    // (CR 702.138, other cards exiled from the graveyard).
    if let Some(kind) = graveyard_cast(line, face, cx) {
        return one(kind, cx);
    }

    if let Some(ks) = keyword_line(line, cx) {
        return Some(Line::Abilities(
            ks.into_iter().map(|k| (k, Vec::new())).collect(),
        ));
    }
    // "{T}: Add {R} or {G}. This land doesn't untap during your next untap step."
    if let Some(head) = line.strip_suffix(" ~ doesn't untap during your next untap step.")
        && let Some(outputs) = mana_line(head)
    {
        let (mut kind, targets) = mana_ability(outputs);
        if let AbilityKind::Activated { effect, .. } = &mut kind {
            *effect = Effect::Sequence(vec![
                effect.clone(),
                Effect::Continuous {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::CantUntapDuringUntapStep),
                    duration: mtg_ir::effect::Duration::ThroughNextUntapStep,
                },
            ]);
        }
        return Some(Line::Abilities(vec![(kind, targets)]));
    }
    // "{T}: Add {C}{C}. Spend this mana only to cast colorless spells.", and a Powerstone's
    // "{T}: Add {C}. This mana can't be spent to cast a nonartifact spell." — which limits
    // only spells, so the mana still pays for abilities.
    let spend = line
        .split_once(" spend this mana only to cast ")
        .map(|(h, o)| (h, o, false))
        .or_else(|| {
            line.split_once(" this mana can't be spent to cast ")
                .map(|(h, o)| (h, o, true))
        });
    if let Some((head, only, spells_only)) = spend
        && let Some(only) = only.strip_suffix('.')
    {
        let only = only
            .strip_prefix("a ")
            .or_else(|| only.strip_prefix("an "))
            .unwrap_or(only);
        let (noun, rest) = nouns::noun(only, cx)?;
        if !rest.is_empty() || noun.zone != Zone::Stack {
            return None;
        }
        let only = strip_is_spell(noun.filter).unwrap_or(ObjectFilter::Any);
        // "can't be spent to cast a nonartifact spell": only to cast artifact spells.
        let only = if spells_only {
            let only = match only {
                ObjectFilter::And(mut parts) if parts.len() == 1 => parts.pop()?,
                other => other,
            };
            match only {
                ObjectFilter::Not(inner) => *inner,
                ObjectFilter::Any => return None,
                other => ObjectFilter::Not(Box::new(other)),
            }
        } else {
            only
        };
        let Some(Line::Abilities(mut list)) = compile_line(head, face, cx) else {
            return None;
        };
        if list.len() != 1 {
            return None;
        }
        let (kind, targets) = list.pop()?;
        let AbilityKind::Activated {
            cost,
            effect: effect @ Effect::AddMana { .. },
            functions_from,
            is_mana_ability: true,
            is_loyalty_ability,
            timing,
        } = kind
        else {
            return None;
        };
        return Some(Line::Abilities(vec![(
            AbilityKind::Activated {
                cost,
                effect: Effect::SpendOnly {
                    only,
                    effect: Box::new(effect),
                    spells_only,
                },
                functions_from,
                is_mana_ability: true,
                is_loyalty_ability,
                timing,
            },
            targets,
        )]));
    }
    if let Some(outputs) = mana_line(line) {
        return Some(Line::Abilities(vec![mana_ability(outputs)]));
    }
    // "{T}: Add {G} for each creature you control." — a mana ability whose amount is
    // counted as it resolves.
    if let Some((input, output)) = line.split_once(": ")
        && let Some((head, per)) = output.split_once(" for each ")
        && head.starts_with("add ")
        && let Some(per) = per.strip_suffix('.')
        && let Some((by, "")) = clauses::per(per, cx)
        && let Some(Line::Abilities(mut list)) =
            compile_line(&format!("{input}: {head}."), face, &mut cx.fresh())
        && let [
            (
                AbilityKind::Activated {
                    effect: effect @ Effect::AddMana { .. },
                    is_mana_ability: true,
                    ..
                },
                _,
            ),
        ] = list.as_mut_slice()
        && clauses::scale(effect, by)
    {
        return Some(Line::Abilities(list));
    }
    if let Some(list) = activate_only_if(line, face, cx) {
        return Some(Line::Abilities(list));
    }
    // "{T}: Add one mana of any color. Activate only once each turn." — any mana ability,
    // limited. Anything else falls through to the activated-ability grammar.
    if let Some(head) = line.strip_suffix(" activate only once each turn.")
        && let Some(Line::Abilities(mut list)) = compile_line(head, face, &mut cx.clone())
        && list.len() == 1
        && let Some((
            AbilityKind::Activated {
                is_mana_ability: true,
                timing: timing @ ActivationTiming::Instant,
                ..
            },
            _,
        )) = list.first_mut()
    {
        *timing = ActivationTiming::InstantOncePerTurn;
        return Some(Line::Abilities(list));
    }
    // A mana ability can also have an unconditional, target-free effect such as
    // "Draw a card" (CR 605.1a). It still resolves immediately, including that effect.
    if let Some((head, followup)) = line.split_once(". ") {
        let mut inner = cx.fresh();
        // With no targets, "it" can only be the source: "… on this land, sacrifice it."
        inner.it = Some(Selector::SelfSource);
        if let Some(extra) = clauses::effect(followup, &mut inner)
            && mana_followup(&extra)
            && let Some(Line::Abilities(mut abilities)) =
                compile_line(&format!("{head}."), face, &mut inner)
            && !abilities.is_empty()
            && abilities.iter().all(|(kind, targets)| {
                targets.is_empty()
                    && matches!(
                        kind,
                        AbilityKind::Activated {
                            is_mana_ability: true,
                            ..
                        }
                    )
            })
        {
            for (kind, _) in &mut abilities {
                if let AbilityKind::Activated { effect, .. } = kind {
                    *effect = Effect::Sequence(vec![effect.clone(), extra.clone()]);
                }
            }
            return Some(Line::Abilities(abilities));
        }
    }
    // Paid mana abilities resolve immediately; their input is paid before output. A cost
    // that is a choice — "tap an untapped creature you control", "remove X storage
    // counters" — is asked for first (the engine's `mana::announced`).
    if let Some((input, output)) = line.split_once(": ")
        && let Some(cost) = cost(input, cx)
        && cost.additional.iter().all(|p| {
            matches!(
                p,
                AdditionalCost::Tap {
                    what: Selector::SelfSource
                } | AdditionalCost::Sacrifice {
                    what: Selector::SelfSource,
                    count: Value::Fixed(1)
                } | AdditionalCost::PayLife {
                    amount: Value::Fixed(0..)
                } | AdditionalCost::RemoveCounters {
                    what: Selector::SelfSource,
                    amount: Value::Fixed(1..) | Value::X,
                    ..
                } | AdditionalCost::TapUntapped {
                    count: Value::Fixed(1..),
                    ..
                }
            )
        })
        && let Some(alternatives) = mana_line(&format!("{{t}}: {output}"))
            .map(|outputs| vec![outputs])
            .or_else(|| mana_alternatives(output))
            .or_else(|| removed_this_way(output, &cost).map(|outputs| vec![outputs]))
    {
        let abilities = alternatives
            .into_iter()
            .map(|outputs| {
                let (mut kind, targets) = mana_ability(outputs);
                if let AbilityKind::Activated {
                    cost: ability_cost, ..
                } = &mut kind
                {
                    *ability_cost = cost.clone();
                }
                (kind, targets)
            })
            .collect();
        return Some(Line::Abilities(abilities));
    }
    // Painlands: "{T}: Add {R} or {G}. This land deals 1 damage to you."
    if let Some((mana, pain)) = line.split_once(". ~ deals ")
        && let Some(outputs) = mana_line(&format!("{mana}."))
        && let Some((n, ".")) = pain
            .strip_suffix(" damage to you.")
            .map(|n| (n, "."))
            .and_then(|(n, dot)| Some((words::number(n)?, dot)))
            .map(|((n, rest), dot)| (n, if rest.is_empty() { dot } else { "" }))
    {
        let (mut kind, targets) = mana_ability(outputs);
        if let AbilityKind::Activated { effect, .. } = &mut kind {
            let add = std::mem::replace(effect, Effect::Nothing);
            *effect = Effect::Sequence(vec![
                add,
                Effect::DealDamage {
                    source: Selector::SelfSource,
                    to: Selector::You,
                    amount: Value::Fixed(n),
                },
            ]);
        }
        return Some(Line::Abilities(vec![(kind, targets)]));
    }
    // "Sacrifice ~: Add {C}.", "{T}, Sacrifice ~: Add one mana of any color." (a Treasure).
    for (prefix, taps) in [("{t}, sacrifice ~: ", true), ("sacrifice ~: ", false)] {
        if let Some(rest) = line.strip_prefix(prefix)
            && let Some(outputs) = mana_line(&format!("{{t}}: {rest}"))
        {
            let (mut kind, targets) = mana_ability(outputs);
            if let AbilityKind::Activated { cost, .. } = &mut kind {
                if !taps {
                    cost.additional.clear();
                }
                cost.additional.push(AdditionalCost::Sacrifice {
                    what: Selector::SelfSource,
                    count: Value::ONE,
                });
            }
            return Some(Line::Abilities(vec![(kind, targets)]));
        }
    }
    if let Some(spec) = enchant(line, cx) {
        return Some(Line::Abilities(vec![(AbilityKind::Enchant, vec![spec])]));
    }
    if let Some(kind) = equip(line) {
        return Some(Line::Abilities(vec![kind]));
    }
    if let Some(kind) = enters_replacement(line, face, cx) {
        return one(kind, cx);
    }
    if let Some(kind) = enters_as_copy(line, face, cx) {
        return one(kind, cx);
    }
    // "If a creature an opponent controls would die, exile it instead." (CR 614.1a)
    if let Some(r) = line
        .strip_prefix("if a ")
        .or_else(|| line.strip_prefix("if an "))
        && let Some((noun, r)) = nouns::noun(r, cx)
        && r == " would die, exile it instead."
        && noun.zone == Zone::Battlefield
        && !noun.plural
    {
        return one(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::ZoneChange {
                    who: noun.filter,
                    from: Some(Zone::Battlefield),
                    to: Some(Zone::Graveyard),
                },
                kind: ReplacementKind::RedirectZoneChange { to: Zone::Exile },
            }),
            cx,
        );
    }
    if line
        == "if ~ would be put into a graveyard from anywhere, reveal ~ and shuffle it into its \
            owner's library instead."
    {
        return one(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::ZoneChange {
                    who: ObjectFilter::IsSelf,
                    from: None,
                    to: Some(Zone::Graveyard),
                },
                kind: ReplacementKind::RedirectZoneChange { to: Zone::Library },
            }),
            cx,
        );
    }
    if line == "if ~ would be put into a graveyard from anywhere, exile it instead." {
        return one(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::ZoneChange {
                    who: ObjectFilter::IsSelf,
                    from: None,
                    to: Some(Zone::Graveyard),
                },
                kind: ReplacementKind::RedirectZoneChange { to: Zone::Exile },
            }),
            cx,
        );
    }
    // "As this enchantment enters, choose a color." — and the land form, "This land
    // enters tapped. As it enters, choose a color."
    let entering = |choice| {
        (
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind: ReplacementKind::EntersChoosing(choice),
            }),
            Vec::new(),
        )
    };
    let choice_of = |r: &str| match r {
        "choose a color." => Some(mtg_ir::effect::EntryChoice::Color),
        "choose a creature type." => Some(mtg_ir::effect::EntryChoice::CreatureType),
        _ => None,
    };
    if face.card_types.iter().any(|t| t.is_permanent()) {
        if let Some(choice) = line.strip_prefix("as ~ enters, ").and_then(choice_of) {
            return Some(Line::Abilities(vec![entering(choice)]));
        }
        if let Some(choice) = line
            .strip_prefix("~ enters tapped. as it enters, ")
            .and_then(choice_of)
        {
            return Some(Line::Abilities(vec![
                (
                    AbilityKind::ReplacementEffect(Replacement {
                        matches: EventPattern::Enters {
                            who: ObjectFilter::IsSelf,
                        },
                        kind: ReplacementKind::EntersTapped,
                    }),
                    Vec::new(),
                ),
                entering(choice),
            ]));
        }
    }
    // "This land enters tapped with two charge counters on it.": both replacements.
    if let Some(rest) = line.strip_prefix("~ enters tapped with ")
        && let Some(Line::Abilities(mut a)) =
            compile_line("~ enters tapped.", face, &mut cx.fresh())
        && let Some(Line::Abilities(b)) =
            compile_line(&format!("~ enters with {rest}"), face, &mut cx.fresh())
    {
        a.extend(b);
        return Some(Line::Abilities(a));
    }
    // CR 614.12 — shock lands: "As this land enters, you may pay 2 life. If you don't, it
    // enters tapped."
    if face.card_types.contains(&CardType::Land)
        && let Some(r) = line.strip_prefix("as ~ enters, you may pay ")
        && let Some((n, r)) = words::number(r)
        && r == " life. if you don't, it enters tapped."
        && n > 0
    {
        return one(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind: ReplacementKind::EntersTappedUnlessPaysLife { amount: n as u32 },
            }),
            cx,
        );
    }
    if let Some(kind) = triggered(line, cx) {
        return one(kind, cx);
    }
    if let Some(kind) = activated(line, cx) {
        return one(kind, cx);
    }
    if let Some(list) = statics(line, face, cx) {
        return Some(Line::Abilities(
            list.into_iter().map(|k| (k, Vec::new())).collect(),
        ));
    }
    None
}

// ---- the parse context ----------------------------------------------------

/// State threaded through one ability's parse.
#[derive(Clone)]
pub(crate) struct Cx<'a> {
    subtypes: &'a dyn Subtypes,
    /// Target slots allocated so far, in the order the text names them.
    pub targets: Vec<TargetSpec>,
    /// What "it" and "that creature" refer to.
    pub it: Option<Selector>,
    /// What "that creature" refers to when it is not "it": the other creature in "~
    /// blocks a creature, destroy that creature".
    pub that_creature: Option<Selector>,
    /// Whether `X` is defined here: by `{X}` in the spell's or the ability's cost.
    pub x: bool,
    /// What "that player" refers to: the player a triggering event involved.
    pub that_player: Option<Selector>,
    /// Whether the last clause moved "it" to another zone (exile), so "return it" means
    /// the card where it went, not the object it was.
    pub moved: bool,
    /// Whether this sentence began "Until end of turn, …", so a later "and <subject>
    /// gets …" in it lasts until end of turn too.
    pub until_eot: bool,
    /// The group the last group effect acted on, for "them" / "those creatures" after
    /// "Creatures you control get +1/+1 until end of turn."
    pub group: Option<Selector>,
    /// What the last "destroy all …" named, for "for each creature destroyed this way".
    pub destroyed: Option<ObjectFilter>,
    /// The player slot of "creatures target player controls", registered before the noun
    /// that names it is read.
    pub controller_target: Option<Selector>,
    /// Whether "that much" / "that many" means the triggering event's amount.
    pub that_much: bool,
    /// Whether the trigger is "one or more …": once per batch of events.
    pub batch: bool,
    /// A condition the trigger's wording adds ("attacks while saddled").
    pub while_: Option<mtg_ir::trigger::Condition>,
    /// "… for the first time each turn": the trigger's wording limits it to once a turn.
    pub once: bool,
}

impl<'a> Cx<'a> {
    fn new(subtypes: &'a dyn Subtypes) -> Self {
        Self {
            subtypes,
            targets: Vec::new(),
            it: None,
            that_creature: None,
            x: false,
            that_player: None,
            moved: false,
            until_eot: false,
            group: None,
            destroyed: None,
            controller_target: None,
            that_much: false,
            batch: false,
            while_: None,
            once: false,
        }
    }

    /// A new context for a nested ability (a token's), sharing only the subtype table.
    pub fn fresh(&self) -> Cx<'a> {
        Cx::new(self.subtypes)
    }

    /// Allocate a target slot.
    pub fn target(&mut self, spec: TargetSpec) -> Selector {
        self.targets.push(spec);
        Selector::Target {
            index: (self.targets.len() - 1) as u8,
        }
    }

    /// A subtype named by a lowercase word, singular or plural.
    pub fn subtype(&self, word: &str) -> Option<Subtype> {
        subtype_candidates(word)
            .into_iter()
            .find_map(|w| self.subtypes.intern(&capitalise(&w)).map(Subtype))
    }

    /// Whether a word names a subtype in the plural ("Elves", "Goblins").
    pub fn subtype_is_plural(&self, word: &str) -> bool {
        self.subtypes.intern(&capitalise(word)).is_none() && self.subtype(word).is_some()
    }
}

/// The singular spellings a word could be the plural of.
fn subtype_candidates(word: &str) -> Vec<String> {
    let mut out = vec![word.to_string()];
    if let Some(stem) = word.strip_suffix("ves") {
        out.push(format!("{stem}f"));
        out.push(format!("{stem}fe"));
    }
    if let Some(stem) = word.strip_suffix("ies") {
        out.push(format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("es") {
        out.push(stem.to_string());
    }
    if let Some(stem) = word.strip_suffix('s') {
        out.push(stem.to_string());
    }
    // Irregular plurals printed on cards.
    for (plural, singular) in [("mice", "mouse"), ("fungi", "fungus"), ("djinn", "djinn")] {
        if word == plural {
            out.push(singular.to_string());
        }
    }
    out
}

fn capitalise(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ---- keywords and mana -----------------------------------------------------

/// A line that is only keywords: "flying, vigilance", "protection from red".
fn keyword_line(line: &str, cx: &Cx) -> Option<Vec<AbilityKind>> {
    let line = line.trim_end_matches('.');
    line.split([',', ';'])
        .map(|w| {
            let w = w.trim();
            words::keyword(w)
                .map(AbilityKind::Keyword)
                .or_else(|| protection(w, cx))
                .or_else(|| ward(w))
                .or_else(|| toxic(w))
        })
        .collect::<Option<Vec<_>>>()
        .filter(|v| !v.is_empty())
}

/// "ward {2}" — CR 702.21a: "Whenever this permanent becomes the target of a spell or
/// ability an opponent controls, counter it unless that player pays {2}."
fn ward(w: &str) -> Option<AbilityKind> {
    // "ward {2}", "ward—pay 3 life", "ward—discard a card"
    let discard = w == "ward—discard a card";
    let (mana, life) = if discard {
        (mtg_core::ManaCost::default(), None)
    } else if let Some(r) = w.strip_prefix("ward—pay ") {
        let (n, r) = words::number(r)?;
        if r != " life" {
            return None;
        }
        (mtg_core::ManaCost::default(), Some(u32::try_from(n).ok()?))
    } else {
        let (mana, rest) = mana_cost(w.strip_prefix("ward ")?)?;
        if !rest.is_empty() {
            return None;
        }
        (mana, None)
    };
    Some(AbilityKind::Triggered {
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
        effect: Effect::CounterUnlessPays {
            what: Selector::Bound(mtg_ir::selector::Binding::EventOther),
            mana,
            life,
            discard,
            exile: false,
            times: None,
        },
    })
}

/// "toxic 2" (CR 702.164).
fn toxic(w: &str) -> Option<AbilityKind> {
    let (n, rest) = words::number(w.strip_prefix("toxic ")?)?;
    (rest.is_empty() && n > 0 && n < 256)
        .then_some(AbilityKind::Keyword(mtg_core::Keyword::Toxic(n as u8)))
}

/// "protection from red", "protection from creatures", "protection from everything"
/// (CR 702.16).
fn protection(w: &str, cx: &Cx) -> Option<AbilityKind> {
    let from = protection_quality(w.strip_prefix("protection from ")?, cx)?;
    Some(AbilityKind::Protection { from })
}

/// What protection is from: "red", "red and from black", "creatures", "everything".
pub(crate) fn protection_quality(q: &str, cx: &Cx) -> Option<ObjectFilter> {
    let from = if q == "everything" {
        ObjectFilter::Any
    } else if q == "all colors" {
        ObjectFilter::Not(Box::new(ObjectFilter::Colorless))
    } else if q == "multicolored" {
        ObjectFilter::Multicolored
    } else if q == "monocolored" {
        ObjectFilter::And(vec![
            ObjectFilter::Not(Box::new(ObjectFilter::Colorless)),
            ObjectFilter::Not(Box::new(ObjectFilter::Multicolored)),
        ])
    } else if q == "colorless" {
        ObjectFilter::Colorless
    } else if let Some(c) = words::color(q) {
        ObjectFilter::HasColor(c)
    } else if let Some((a, b)) = q.split_once(" and from ") {
        ObjectFilter::Or(vec![protection_quality(a, cx)?, protection_quality(b, cx)?])
    } else {
        let (noun, rest) = nouns::noun(q, cx)?;
        if !rest.is_empty() || !noun.plural {
            return None;
        }
        noun.filter
    };
    Some(from)
}

fn mana_followup(effect: &Effect) -> bool {
    match effect {
        Effect::Draw {
            who: Selector::You,
            count: Value::Fixed(0..),
        }
        | Effect::GainLife {
            who: Selector::You,
            amount: Value::Fixed(0..),
        }
        | Effect::LoseLife {
            who: Selector::You,
            amount: Value::Fixed(0..),
        }
        // A depletion land: "If there are no depletion counters on this land, sacrifice it."
        | Effect::Sacrifice {
            who: Selector::You,
            what: Selector::SelfSource,
        }
        | Effect::Nothing => true,
        Effect::Sequence(items) => items.iter().all(mana_followup),
        Effect::If {
            then, otherwise, ..
        } => mana_followup(then) && mana_followup(otherwise),
        _ => false,
    }
}

/// "{t}: add {g}.", "{t}: add {c}{c}.", "{t}: add {w} or {u}.", "{t}: add one mana of any
/// color." — tap-only costs, because only those can pay for spells.
/// "Add {B}{B}{B}." as an effect (a ritual, a chapter): fixed mana only — a choice of
/// color during resolution has no prompt, so it is not compiled.
pub(crate) fn add_mana_effect(s: &str) -> Option<(Effect, &str)> {
    let r = s.strip_prefix("add ")?;
    let end = r.find(['.', ',']).unwrap_or(r.len());
    let (mana, rest) = r.split_at(end);
    if !mana.starts_with('{') {
        return None;
    }
    let outputs = mana_line(&format!("{{t}}: add {mana}."))?;
    if outputs.iter().any(|o| o.is_ambiguous()) {
        return None;
    }
    Some((
        Effect::AddMana {
            who: Selector::You,
            produces: outputs,
        },
        rest,
    ))
}

fn mana_line(line: &str) -> Option<Vec<ManaOutput>> {
    let rest = line.strip_prefix("{t}: add ")?.strip_suffix('.')?;
    if rest == "one mana of the chosen color" {
        return Some(vec![ManaOutput::ChosenColor]);
    }
    let any = || {
        ManaOutput::AnyOf(vec![
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ])
    };
    if rest == "one mana of any color" {
        return Some(vec![any()]);
    }
    // "two mana of any one color": one choice, that many. ("In any combination of colors"
    // is not read: an activation records one color choice, which would make it one color.)
    if let Some((n, " mana of any one color")) = words::number(rest)
        && n > 1
    {
        return Some(vec![ManaOutput::Repeated {
            amount: Value::Fixed(n),
            output: Box::new(any()),
        }]);
    }
    if rest.contains(" or ") {
        let options: Vec<Color> = rest
            .split([',', ' '])
            .filter(|w| !w.is_empty() && *w != "or")
            .map(|w| match mana_symbol(w)? {
                ManaOutput::Colored(c) => Some(c),
                _ => None,
            })
            .collect::<Option<_>>()?;
        return (options.len() >= 2).then(|| vec![ManaOutput::AnyOf(options)]);
    }
    let symbols: Vec<&str> = rest.split_inclusive('}').collect();
    if symbols.is_empty() {
        return None;
    }
    if symbols.iter().any(|s| *s != symbols[0]) {
        return symbols.into_iter().map(mana_symbol).collect();
    }
    let one = mana_symbol(symbols[0])?;
    Some(match symbols.len() {
        1 => vec![one],
        n => vec![ManaOutput::Repeated {
            amount: Value::Fixed(n as i32),
            output: Box::new(one),
        }],
    })
}

/// A storage land's "add {c} for each storage counter removed this way." and a battery's
/// "add {c}, then add an additional {c} for each charge counter removed this way.": the
/// counters its cost removed, announced as X.
fn removed_this_way(output: &str, cost: &Cost) -> Option<Vec<ManaOutput>> {
    let kind = cost.additional.iter().find_map(|p| match p {
        AdditionalCost::RemoveCounters {
            what: Selector::SelfSource,
            kind,
            amount: Value::X,
        } => Some(*kind),
        _ => None,
    })?;
    let (head, counter) = output
        .strip_suffix(" counter removed this way.")?
        .rsplit_once(" for each ")?;
    if words::counter(&format!("{counter} counter")) != Some((kind, "")) {
        return None;
    }
    let one = |s: &str| match mana_line(&format!("{{t}}: add {s}."))?.as_slice() {
        [single] => Some(single.clone()),
        _ => None,
    };
    let each = |out: ManaOutput| ManaOutput::Repeated {
        amount: Value::X,
        output: Box::new(out),
    };
    let head = head.strip_prefix("add ")?;
    Some(match head.split_once(", then add an additional ") {
        Some((first, extra)) => vec![one(first)?, each(one(extra)?)],
        None => vec![each(one(head)?)],
    })
}

/// "add {w}{w}, {w}{u}, or {u}{u}." (a filter land) — a choice between fixed bundles of
/// mana. Compiled as one ability per bundle with the same cost: activating any of them does
/// exactly what choosing that bundle would, and a single colour choice can't name a mixed
/// bundle like {w}{u}.
fn mana_alternatives(output: &str) -> Option<Vec<Vec<ManaOutput>>> {
    let list = output.strip_prefix("add ")?.strip_suffix('.')?;
    let (head, last) = list.rsplit_once(", or ")?;
    let options: Vec<&str> = head.split(", ").chain([last]).collect();
    if options.len() < 2 || options.iter().all(|o| o.matches('{').count() < 2) {
        return None;
    }
    options
        .into_iter()
        .map(|o| {
            o.split_inclusive('}')
                .map(mana_symbol)
                .collect::<Option<Vec<_>>>()
        })
        .collect()
}

fn mana_symbol(s: &str) -> Option<ManaOutput> {
    Some(match s {
        "{w}" => ManaOutput::Colored(Color::White),
        "{u}" => ManaOutput::Colored(Color::Blue),
        "{b}" => ManaOutput::Colored(Color::Black),
        "{r}" => ManaOutput::Colored(Color::Red),
        "{g}" => ManaOutput::Colored(Color::Green),
        "{c}" => ManaOutput::Colorless,
        _ => return None,
    })
}

fn basic_land_color(subtype: &str) -> Option<Color> {
    Some(match subtype {
        "Plains" => Color::White,
        "Island" => Color::Blue,
        "Swamp" => Color::Black,
        "Mountain" => Color::Red,
        "Forest" => Color::Green,
        _ => return None,
    })
}

fn mana_ability(produces: Vec<ManaOutput>) -> (AbilityKind, Vec<TargetSpec>) {
    (
        AbilityKind::Activated {
            cost: Cost {
                additional: vec![AdditionalCost::Tap {
                    what: Selector::SelfSource,
                }],
                ..Cost::free()
            },
            effect: Effect::AddMana {
                who: Selector::You,
                produces,
            },
            functions_from: Zone::Battlefield,
            is_mana_ability: true,
            is_loyalty_ability: false,
            timing: ActivationTiming::Instant,
        },
        Vec::new(),
    )
}

// ---- auras, equipment, enters tapped ----------------------------------------

/// "enchant creature", "enchant creature you control", "enchant land".
fn enchant(line: &str, cx: &mut Cx) -> Option<TargetSpec> {
    let r = line.strip_prefix("enchant ")?;
    // A Curse: "enchant player", "enchant opponent".
    let players = match r {
        "player" => Some(None),
        "opponent" => Some(Some(Selector::Opponents)),
        _ => None,
    };
    if let Some(players) = players {
        return Some(TargetSpec {
            zone: Zone::Battlefield,
            filter: ObjectFilter::Not(Box::new(ObjectFilter::Any)),
            allows_players: true,
            players,
            mode: None,
            count: Value::ONE,
            up_to: false,
            distinct_from_other_targets: false,
        });
    }
    let (n, rest) = nouns::noun(r, cx)?;
    if !rest.is_empty() || n.plural || n.zone != Zone::Battlefield {
        return None;
    }
    Some(TargetSpec {
        zone: Zone::Battlefield,
        filter: n.filter,
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    })
}

/// "equip {2}" — CR 702.6a: attach to target creature you control, as a sorcery.
fn equip(line: &str) -> Option<(AbilityKind, Vec<TargetSpec>)> {
    let r = line.strip_prefix("equip ")?;
    let (mana, rest) = mana_cost(r)?;
    if !rest.is_empty() {
        return None;
    }
    let target = TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::And(vec![
            ObjectFilter::HasType(CardType::Creature),
            ObjectFilter::ControlledBy(Box::new(Selector::You)),
        ]),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    };
    Some((
        AbilityKind::Activated {
            cost: Cost {
                mana,
                ..Cost::free()
            },
            effect: Effect::Attach {
                what: Selector::SelfSource,
                to: Selector::Target { index: 0 },
            },
            functions_from: Zone::Battlefield,
            is_mana_ability: false,
            is_loyalty_ability: false,
            timing: ActivationTiming::SorcerySpeed,
        },
        vec![target],
    ))
}

/// "~ enters tapped.", "~ enters with two +1/+1 counters on it."
/// "You may have this creature enter as a copy of any creature on the battlefield." (CR
/// 707.9) A copy with exceptions ("except it's a Rogue …") is refused: the exceptions
/// would need an overlay on the copied values.
fn enters_as_copy(line: &str, face: &FaceText, cx: &Cx) -> Option<AbilityKind> {
    if !face.card_types.iter().any(|t| t.is_permanent()) {
        return None;
    }
    let r = line.strip_prefix("you may have ~ enter as a copy of ")?;
    let r = r
        .strip_prefix("any ")
        .or_else(|| r.strip_prefix("an "))
        .or_else(|| r.strip_prefix("a "))?;
    let (noun, rest) = nouns::noun(r, cx)?;
    let rest = rest.strip_prefix(" on the battlefield").unwrap_or(rest);
    if rest != "." || noun.plural || noun.zone != Zone::Battlefield {
        return None;
    }
    Some(AbilityKind::ReplacementEffect(Replacement {
        matches: EventPattern::Enters {
            who: ObjectFilter::IsSelf,
        },
        kind: ReplacementKind::EntersAsCopy {
            of: noun.filter,
            optional: true,
        },
    }))
}

fn enters_replacement(line: &str, face: &FaceText, cx: &Cx) -> Option<AbilityKind> {
    // CR 702.44 — sunburst: a +1/+1 counter (a charge counter, if not a creature) for each
    // color of mana spent to cast it.
    if line == "sunburst" {
        let kind = if face.card_types.contains(&CardType::Creature) {
            mtg_core::CounterKind::PlusOnePlusOne
        } else {
            words::counter("charge counter")?.0
        };
        return Some(AbilityKind::ReplacementEffect(Replacement {
            matches: EventPattern::Enters {
                who: ObjectFilter::IsSelf,
            },
            kind: ReplacementKind::EntersWithCounters {
                kind,
                amount: Value::ColorsSpent,
                condition: None,
            },
        }));
    }
    // "If ~ was kicked, it enters with two +1/+1 counters on it."
    if let Some(r) = line.strip_prefix("if ~ was kicked, it enters ") {
        let mut kind = enters_replacement(&format!("~ enters {r}"), face, cx)?;
        if let AbilityKind::ReplacementEffect(Replacement {
            kind: ReplacementKind::EntersWithCounters { condition, .. },
            ..
        }) = &mut kind
        {
            *condition = Some(mtg_ir::trigger::Condition::Kicked);
            return Some(kind);
        }
        return None;
    }
    // The longer spelling first: "~ enters " would also match it and leave "the
    // battlefield tapped." behind.
    let r = line
        .strip_prefix("~ enters the battlefield ")
        .or_else(|| line.strip_prefix("~ enters "))?;
    let kind = if r == "tapped." {
        ReplacementKind::EntersTapped
    } else if let Some(c) = r.strip_prefix("tapped unless ") {
        let (condition, rest) = conditions::condition(c, cx)?;
        if rest != "." {
            return None;
        }
        ReplacementKind::EntersTappedUnless { condition }
    } else {
        let r = r.strip_prefix("with ")?;
        // "with X +1/+1 counters": the X it was cast with.
        let (n, r) = match r.strip_prefix("x ") {
            Some(r) if face.mana_cost.contains("{X}") => (Value::X, r),
            _ => {
                let (n, r) = words::number(r)?;
                (Value::Fixed(n), r.strip_prefix(' ')?)
            }
        };
        let (kind, r) = words::counter(r)?;
        let r = if n == Value::ONE {
            r
        } else {
            r.strip_prefix('s')?
        };
        // "… on it for each color of mana spent to cast it." (converge)
        if let Some(each) = r.strip_prefix(" on it for each ")
            && let Some((per, ".")) = clauses::per(each, cx)
        {
            let amount = if n == Value::ONE {
                per
            } else {
                Value::Product(vec![n, per])
            };
            return Some(AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind: ReplacementKind::EntersWithCounters {
                    kind,
                    amount,
                    condition: None,
                },
            }));
        }
        // "… on it if you attacked this turn." (raid)
        let condition = match r.strip_prefix(" on it if ") {
            Some(c) => {
                let (cond, rest) = conditions::condition(c, cx)?;
                if rest != "." {
                    return None;
                }
                Some(cond)
            }
            None if r == " on it." => None,
            None => return None,
        };
        ReplacementKind::EntersWithCounters {
            kind,
            amount: n,
            condition,
        }
    };
    Some(AbilityKind::ReplacementEffect(Replacement {
        matches: EventPattern::Enters {
            who: ObjectFilter::IsSelf,
        },
        kind,
    }))
}

// ---- triggered abilities -----------------------------------------------------

/// "choose one —" → ("", 1); "when ~ enters, choose two —" → ("when ~ enters, ", 2).
/// The head of a modal ability: what precedes it, the most modes (-1 for all of them),
/// and the fewest when that differs ("choose one or both").
fn modal_head(line: &str) -> Option<(&str, i32, Option<u8>)> {
    for (w, n, at_least) in [
        ("choose one —", 1, None),
        ("choose two —", 2, None),
        ("choose three —", 3, None),
        ("choose one or both —", 2, Some(1)),
        ("choose one or more —", -1, Some(1)),
    ] {
        if let Some(prefix) = line.strip_suffix(w) {
            return Some((prefix, n, at_least));
        }
    }
    None
}

/// The modes of a modal ability. Each mode's targets are tagged with that mode, so only
/// the chosen modes' targets are chosen (CR 700.2b).
fn modal(
    bullets: &[&str],
    labels: &[String],
    n: i32,
    at_least: Option<u8>,
    it: Option<Selector>,
    cx: &mut Cx,
) -> Option<Effect> {
    if bullets.len() < n as usize || bullets.is_empty() {
        return None;
    }
    let mut modes = Vec::new();
    for (m, (b, label)) in bullets.iter().zip(labels).enumerate() {
        let first = cx.targets.len();
        cx.it = it.clone();
        let e = clauses::effect(b, cx)?;
        for t in &mut cx.targets[first..] {
            t.mode = Some(m as u8);
        }
        modes.push((label.as_str().into(), e));
    }
    Some(Effect::Modal {
        choose: Value::Fixed(n),
        modes,
        at_least,
    })
}

/// A triggered ability whose condition is `prefix` ("when ~ enters, ") and whose effect
/// has already been compiled.
fn triggered_with(prefix: &str, effect: Effect, cx: &mut Cx) -> Option<AbilityKind> {
    let (on, timing, _, rest) = trigger_condition(prefix, cx)?;
    if rest != ", " {
        return None;
    }
    Some(AbilityKind::Triggered {
        trigger: Trigger {
            on,
            functions_from: Zone::Battlefield,
            intervening_if: None,
            optional: false,
            limit: None,
            timing,
        },
        effect,
    })
}

/// "… during your turn", "… during an opponent's turn", "… for the first time each turn"
/// after a trigger's event: a condition on, or a limit to, its triggering.
fn turn_qualifier<'s>(r: &'s str, cx: &mut Cx) -> &'s str {
    use mtg_ir::trigger::Condition as C;
    if let Some(r) = r.strip_prefix(" during your turn") {
        cx.while_ = Some(C::YourTurn);
        return r;
    }
    if let Some(r) = r.strip_prefix(" during an opponent's turn") {
        cx.while_ = Some(C::Not(Box::new(C::YourTurn)));
        return r;
    }
    if let Some(r) = r.strip_prefix(" for the first time each turn") {
        cx.once = true;
        return r;
    }
    r
}

/// Where a trigger on this object binds the one other creature it involves: "that
/// creature" in "whenever ~ blocks or becomes blocked by a creature, destroy that
/// creature" is that one, not ~.
fn other_creature(on: &EventPattern) -> Option<mtg_ir::selector::Binding> {
    use mtg_ir::selector::Binding;
    let me = ObjectFilter::IsSelf;
    match on {
        EventPattern::Blocks { who } | EventPattern::BecomesBlocked { who } if *who == me => {
            Some(Binding::EventOther)
        }
        // The blocker is the subject of a block pair.
        EventPattern::BlockedBy { blocker, .. } if *blocker == me => Some(Binding::EventOther),
        EventPattern::BlockedBy { attacker, .. } if *attacker == me => Some(Binding::EventSubject),
        EventPattern::DealsDamage {
            source,
            to: DamageRecipient::Object(_),
            ..
        } if *source == me => Some(Binding::EventOther),
        EventPattern::AnyOf(ons) => {
            let first = other_creature(ons.first()?)?;
            ons.iter()
                .all(|o| other_creature(o) == Some(first))
                .then_some(first)
        }
        _ => None,
    }
}

/// "When ~ enters, …", "Whenever ~ attacks, …", "At the beginning of your upkeep, …".
fn triggered(line: &str, cx: &mut Cx) -> Option<AbilityKind> {
    // "This ability triggers only once each turn." (once per turn covers once per batch).
    let (line, once) = match line.strip_suffix(" this ability triggers only once each turn.") {
        Some(l) => (l, true),
        None => (line, false),
    };
    let (on, timing, it, rest) = trigger_condition(line, cx)?;
    let mut body = rest.strip_prefix(", ")?;
    // "When ~ dies, if it was a creature, …": only a creature dies, so it was.
    if matches!(
        on,
        EventPattern::Dies {
            who: ObjectFilter::IsSelf
        }
    ) && let Some(r) = body.strip_prefix("if it was a creature, ")
    {
        body = r;
    }
    cx.it = it;
    cx.that_creature = other_creature(&on).map(Selector::Bound);
    // "Whenever ~ deals combat damage to a player, that player …" — the other party to
    // the event.
    if matches!(
        on,
        EventPattern::DealsDamage {
            to: DamageRecipient::Player(_),
            ..
        }
    ) {
        cx.that_player = Some(Selector::Bound(mtg_ir::selector::Binding::EventOther));
    }
    // "Whenever an opponent draws a card, that player …": the player the event happened to.
    if let EventPattern::Draws { whose }
    | EventPattern::NthDraw { whose, .. }
    | EventPattern::Discards { whose }
    | EventPattern::LifeGained { whose }
    | EventPattern::LifeLost { whose } = &on
        && *whose != Selector::You
    {
        cx.that_player = Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject));
    }
    // "Whenever an opponent casts a spell, that player …": the caster.
    if let EventPattern::Cast { by, .. } = &on
        && *by != Selector::You
    {
        cx.that_player = Some(Selector::Bound(mtg_ir::selector::Binding::EventOther));
    }
    // "At the beginning of each player's upkeep, that player …": whose step it is.
    if let EventPattern::StepBegins { whose, .. } = &on
        && *whose != Selector::You
    {
        cx.that_player = Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject));
    }
    cx.that_much = !cx.batch
        && matches!(
            on,
            EventPattern::DealsDamage { .. }
                | EventPattern::TakesDamage { .. }
                | EventPattern::LifeGained { .. }
                | EventPattern::LifeLost { .. }
        );
    // A trigger of the card in its owner's graveyard: "if this card is in your graveyard",
    // "return this card from your graveyard to the battlefield" (CR 113.6k).
    let mut in_graveyard = false;
    if let Some(r) = body.strip_prefix("if ~ is in your graveyard, ") {
        in_graveyard = true;
        body = r;
    }
    if body.contains("~ from your graveyard") {
        in_graveyard = true;
    }
    // CR 603.4 — "When ~ enters, if you control a Swamp, …" is an intervening "if": checked
    // as the ability triggers and again as it resolves.
    let mut intervening_if = None;
    if let Some(r) = body.strip_prefix("if ")
        && let Some((cond, r)) = conditions::condition(r, cx)
        && let Some(r) = r.strip_prefix(", ")
    {
        // "if this land is tapped, put a storage counter on it": with no other antecedent,
        // "it" is the object the condition names.
        if cx.it.is_none() && body.starts_with("if ~ ") {
            cx.it = Some(Selector::SelfSource);
        }
        intervening_if = Some(cond);
        body = r;
    }
    // Kinship: "you may look at the top card of your library. If it shares a creature type
    // with ~, you may reveal it. If you do, …" — looking changes nothing, so it is the
    // reveal, offered when the top card shares a type.
    let kinship = body
        .strip_prefix(
            "you may look at the top card of your library. if it shares a creature type with \
             ~, you may reveal it. if you do, ",
        )
        .and_then(|payoff| {
            let then = clauses::effect(payoff, cx)?;
            let it = mtg_ir::selector::Binding::It;
            Some(Effect::Let {
                slot: it,
                what: Selector::TopOfLibrary {
                    player: Box::new(Selector::You),
                    count: Value::ONE,
                },
                body: Box::new(Effect::If {
                    cond: mtg_ir::trigger::Condition::Exists(Selector::All {
                        zone: Zone::Library,
                        filter: ObjectFilter::And(vec![
                            ObjectFilter::InBinding(it),
                            ObjectFilter::SharesCreatureTypeWith(Box::new(Selector::SelfSource)),
                        ]),
                    }),
                    then: Box::new(Effect::May {
                        prompt: "reveal the top card of your library".into(),
                        then: Box::new(then),
                        otherwise: None,
                    }),
                    otherwise: Box::new(Effect::Nothing),
                }),
            })
        });
    let effect = match kinship {
        Some(e) => e,
        None => clauses::effect(body, cx)?,
    };
    if let Some(c) = cx.while_.take() {
        intervening_if = Some(match intervening_if {
            Some(i) => mtg_ir::trigger::Condition::And(vec![c, i]),
            None => c,
        });
    }
    // "When you cast this spell" triggers while it is a spell.
    let functions_from = match on {
        EventPattern::Cast {
            who: ObjectFilter::IsSelf,
            ..
        } => Zone::Stack,
        // The cycled card is in the graveyard by the time it triggers.
        EventPattern::Cycled {
            who: ObjectFilter::IsSelf,
            ..
        } => Zone::Graveyard,
        // A self trigger like "when ~ dies" is not one of a card already in the graveyard.
        _ if in_graveyard && !matches!(timing, TriggerTiming::LeavesBattlefield) => Zone::Graveyard,
        _ => Zone::Battlefield,
    };
    Some(AbilityKind::Triggered {
        trigger: Trigger {
            on,
            functions_from,
            intervening_if,
            optional: false,
            limit: if once || cx.once {
                Some(mtg_ir::trigger::TriggerLimit::OncePerTurn)
            } else {
                cx.batch
                    .then_some(mtg_ir::trigger::TriggerLimit::OncePerBatch)
            },
            timing,
        },
        effect,
    })
}

type Condition<'s> = (EventPattern, TriggerTiming, Option<Selector>, &'s str);

/// A trigger on this object itself, after "~ ": "dies", "attacks", "is turned face up".
fn self_trigger(r: &str) -> Option<Condition<'_>> {
    let normal = TriggerTiming::Normal;
    let leaves = TriggerTiming::LeavesBattlefield;

    let me = ObjectFilter::IsSelf;
    let it = Some(Selector::SelfSource);
    // Having left the battlefield, "it" is the card where it went (CR 400.7): "when ~
    // dies, return it to its owner's hand".
    let gone = Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject));
    // "when this Class becomes level 2" (CR 716.2c)
    if let Some(r) = r.strip_prefix("becomes level ")
        && let Some((n, r)) = words::number(r)
        && let Ok(level) = u8::try_from(n)
    {
        return Some((EventPattern::BecomesClassLevel { level }, normal, it, r));
    }
    for (phrase, pattern) in [
        ("dies", EventPattern::Dies { who: me.clone() }),
        (
            "is put into a graveyard from the battlefield",
            EventPattern::Dies { who: me.clone() },
        ),
        (
            "leaves the battlefield",
            EventPattern::Leaves { who: me.clone() },
        ),
    ] {
        if let Some(r) = r.strip_prefix(phrase) {
            return Some((pattern, leaves, gone, r));
        }
    }
    for (phrase, pattern, timing) in [
        (
            "is turned face up",
            EventPattern::TurnedFaceUp { who: me.clone() },
            normal,
        ),
        (
            "becomes tapped",
            EventPattern::BecomesTapped { who: me.clone() },
            normal,
        ),
        (
            "becomes untapped",
            EventPattern::BecomesUntapped { who: me.clone() },
            normal,
        ),
        (
            "is dealt combat damage",
            EventPattern::TakesDamage {
                who: me.clone(),
                combat_only: true,
            },
            normal,
        ),
        (
            "is dealt damage",
            EventPattern::TakesDamage {
                who: me.clone(),
                combat_only: false,
            },
            normal,
        ),
        (
            "becomes blocked by a creature",
            EventPattern::BecomesBlocked { who: me.clone() },
            normal,
        ),
        (
            "deals combat damage to a creature",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Object(ObjectFilter::HasType(CardType::Creature)),
                combat_only: true,
            },
            normal,
        ),
        (
            "deals damage to an opponent",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Player(Selector::Opponents),
                combat_only: false,
            },
            normal,
        ),
        (
            "deals damage to a player",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Player(Selector::EachPlayer),
                combat_only: false,
            },
            normal,
        ),
        (
            "enters the battlefield",
            EventPattern::Enters { who: me.clone() },
            normal,
        ),
        ("enters", EventPattern::Enters { who: me.clone() }, normal),
        ("dies", EventPattern::Dies { who: me.clone() }, leaves),
        (
            "leaves the battlefield",
            EventPattern::Leaves { who: me.clone() },
            leaves,
        ),
        // The pattern does not tell spells from abilities, so only "a spell or
        // ability" is read.
        (
            "becomes the target of a spell or ability an opponent controls",
            EventPattern::BecomesTarget {
                who: me.clone(),
                by: Selector::Opponents,
            },
            normal,
        ),
        (
            "becomes the target of a spell or ability",
            EventPattern::BecomesTarget {
                who: me.clone(),
                by: Selector::EachPlayer,
            },
            normal,
        ),
        (
            "attacks and isn't blocked",
            EventPattern::AttacksUnblocked { who: me.clone() },
            normal,
        ),
        (
            "attacks alone",
            EventPattern::Attacks {
                who: ObjectFilter::And(vec![me.clone(), ObjectFilter::AttackingAlone]),
            },
            normal,
        ),
        ("attacks", EventPattern::Attacks { who: me.clone() }, normal),
        (
            "becomes monstrous",
            EventPattern::BecomesMonstrous { who: me.clone() },
            normal,
        ),
        (
            "blocks a creature",
            EventPattern::Blocks { who: me.clone() },
            normal,
        ),
        ("blocks", EventPattern::Blocks { who: me.clone() }, normal),
        (
            "becomes blocked",
            EventPattern::BecomesBlocked { who: me.clone() },
            normal,
        ),
        (
            "deals combat damage to a player",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Player(Selector::EachPlayer),
                combat_only: true,
            },
            normal,
        ),
        (
            "deals combat damage to an opponent",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Player(Selector::Opponents),
                combat_only: true,
            },
            normal,
        ),
        (
            "deals combat damage",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Any,
                combat_only: true,
            },
            normal,
        ),
        (
            "deals damage",
            EventPattern::DealsDamage {
                source: me.clone(),
                to: DamageRecipient::Any,
                combat_only: false,
            },
            normal,
        ),
    ] {
        if let Some(r) = r.strip_prefix(phrase) {
            return Some((pattern, timing, it, r));
        }
    }
    None
}

fn trigger_condition<'s>(line: &'s str, cx: &mut Cx) -> Option<Condition<'s>> {
    let normal = TriggerTiming::Normal;
    let leaves = TriggerTiming::LeavesBattlefield;
    let subject = Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject));

    if let Some(r) = line
        .strip_prefix("at the beginning of ")
        .map(|r| r.strip_prefix("the ").unwrap_or(r))
    {
        for (phrase, step, whose) in [
            ("your upkeep", Step::Upkeep, Selector::You),
            (
                "enchanted player's upkeep",
                Step::Upkeep,
                Selector::EnchantedPlayer,
            ),
            (
                "enchanted player's draw step",
                Step::Draw,
                Selector::EnchantedPlayer,
            ),
            (
                "enchanted player's end step",
                Step::End,
                Selector::EnchantedPlayer,
            ),
            ("each upkeep", Step::Upkeep, Selector::EachPlayer),
            ("each player's upkeep", Step::Upkeep, Selector::EachPlayer),
            ("each opponent's upkeep", Step::Upkeep, Selector::Opponents),
            ("each opponent's end step", Step::End, Selector::Opponents),
            ("each player's end step", Step::End, Selector::EachPlayer),
            ("each player's draw step", Step::Draw, Selector::EachPlayer),
            ("each draw step", Step::Draw, Selector::EachPlayer),
            ("each opponent's draw step", Step::Draw, Selector::Opponents),
            (
                "each of your postcombat main phases",
                Step::PostcombatMain,
                Selector::You,
            ),
            ("each combat", Step::BeginCombat, Selector::EachPlayer),
            ("your first main phase", Step::PrecombatMain, Selector::You),
            (
                "your postcombat main phase",
                Step::PostcombatMain,
                Selector::You,
            ),
            (
                "your second main phase",
                Step::PostcombatMain,
                Selector::You,
            ),
            ("your end step", Step::End, Selector::You),
            ("each end step", Step::End, Selector::EachPlayer),
            ("end step", Step::End, Selector::EachPlayer),
            ("combat on your turn", Step::BeginCombat, Selector::You),
            ("your draw step", Step::Draw, Selector::You),
            (
                "your precombat main phase",
                Step::PrecombatMain,
                Selector::You,
            ),
        ] {
            if let Some(r) = r.strip_prefix(phrase) {
                return Some((EventPattern::StepBegins { step, whose }, normal, None, r));
            }
        }
        // "At the beginning of the upkeep of enchanted creature's controller"
        if let Some(r) = r.strip_prefix("upkeep of ")
            && let Some((whose, _, r)) = nouns::player(r, cx)
        {
            return Some((
                EventPattern::StepBegins {
                    step: Step::Upkeep,
                    whose,
                },
                normal,
                None,
                r,
            ));
        }
        return None;
    }
    // "At end of combat, …": each combat's end of combat step.
    if let Some(r) = line.strip_prefix("at end of combat") {
        return Some((
            EventPattern::StepBegins {
                step: Step::EndCombat,
                whose: Selector::EachPlayer,
            },
            normal,
            None,
            r,
        ));
    }

    // A state trigger (CR 603.8): "When you control no Islands, sacrifice ~."
    if let Some(r) = line.strip_prefix("when ")
        && (r.starts_with("you control no ") || r.starts_with("you have no "))
    {
        let (cond, r) = conditions::condition(r, cx)?;
        return Some((
            EventPattern::StateIs(cond),
            TriggerTiming::StateTrigger,
            None,
            r,
        ));
    }

    let r = line
        .strip_prefix("whenever ")
        .or_else(|| line.strip_prefix("when "))?;

    // Cycling (CR 702.29): "when you cycle ~" triggers from the graveyard it went to.
    // Cycling is discarding, so "you cycle or discard a card" is one discard.
    for (phrase, pattern) in [
        (
            "you cycle ~",
            EventPattern::Cycled {
                who: ObjectFilter::IsSelf,
                by: Selector::You,
            },
        ),
        (
            "you cycle another card",
            EventPattern::Cycled {
                who: ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
                by: Selector::You,
            },
        ),
        (
            "you cycle a card",
            EventPattern::Cycled {
                who: ObjectFilter::Any,
                by: Selector::You,
            },
        ),
        (
            "a player cycles a card",
            EventPattern::Cycled {
                who: ObjectFilter::Any,
                by: Selector::EachPlayer,
            },
        ),
        (
            "you cycle or discard a card",
            EventPattern::Discards {
                whose: Selector::You,
            },
        ),
    ] {
        if let Some(r) = r.strip_prefix(phrase) {
            return Some((pattern, normal, None, r));
        }
    }
    // "you scry", "you surveil".
    for (phrase, surveil) in [("you scry", false), ("you surveil", true)] {
        if let Some(r) = r.strip_prefix(phrase)
            && (r.is_empty() || r.starts_with(','))
        {
            return Some((
                EventPattern::Scried {
                    whose: Selector::You,
                    surveil,
                },
                normal,
                None,
                r,
            ));
        }
    }
    // "you sacrifice a Food", "you sacrifice another permanent" (CR 701.21).
    if let Some(r) = r.strip_prefix("you sacrifice ") {
        let (other, r) = match r.strip_prefix("another ") {
            Some(r) => (true, r),
            None => (
                false,
                r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?,
            ),
        };
        let (n, r) = nouns::noun(r, cx)?;
        if n.plural || n.zone != Zone::Battlefield {
            return None;
        }
        let who = if other {
            ObjectFilter::And(vec![
                n.filter,
                ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
            ])
        } else {
            n.filter
        };
        return Some((
            EventPattern::Sacrificed {
                who,
                by: Selector::You,
            },
            normal,
            Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject)),
            r,
        ));
    }
    // "~ or another Ally you control enters": itself, or another matching permanent.
    if let Some(r) = r.strip_prefix("~ or another ") {
        let (n, r) = nouns::noun(r, cx)?;
        if n.plural || n.zone != Zone::Battlefield {
            return None;
        }
        let who = ObjectFilter::Or(vec![ObjectFilter::IsSelf, n.filter]);
        for (phrase, pattern, timing) in [
            (" enters", EventPattern::Enters { who: who.clone() }, normal),
            (" dies", EventPattern::Dies { who: who.clone() }, leaves),
            (
                " leaves the battlefield",
                EventPattern::Leaves { who: who.clone() },
                leaves,
            ),
            (
                " attacks",
                EventPattern::Attacks { who: who.clone() },
                normal,
            ),
            (
                " deals combat damage to a player",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Player(Selector::EachPlayer),
                    combat_only: true,
                },
                normal,
            ),
        ] {
            if let Some(r) = r.strip_prefix(phrase) {
                return Some((pattern, timing, subject, r));
            }
        }
        return None;
    }

    // "~ attacks with another creature with greater power" (training, CR 702.149):
    // another attacking creature is stronger than this one as attackers are declared.
    if let Some(r) = r.strip_prefix("~ attacks with another creature with greater power") {
        cx.while_ = Some(mtg_ir::trigger::Condition::CountAtLeast {
            what: Selector::All {
                zone: Zone::Battlefield,
                filter: ObjectFilter::And(vec![
                    ObjectFilter::Attacking,
                    ObjectFilter::HasType(CardType::Creature),
                    ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
                    ObjectFilter::PowerAtLeast(Value::Sum(vec![
                        Value::Power(Box::new(Selector::SelfSource)),
                        Value::ONE,
                    ])),
                ]),
            },
            at_least: Value::ONE,
        });
        return Some((
            EventPattern::Attacks {
                who: ObjectFilter::IsSelf,
            },
            normal,
            Some(Selector::SelfSource),
            r,
        ));
    }
    // "~ attacks while saddled" (CR 702.171c).
    if let Some(r) = r.strip_prefix("~ attacks while saddled") {
        cx.while_ = Some(mtg_ir::trigger::Condition::Saddled);
        return Some((
            EventPattern::Attacks {
                who: ObjectFilter::IsSelf,
            },
            normal,
            Some(Selector::SelfSource),
            r,
        ));
    }
    // "~ blocks a creature with flying", "~ becomes blocked by a Wall": one creature on
    // each side, the other described. Plain "a creature" is the self-trigger table's.
    for (prefix, blocking) in [
        ("~ blocks a ", true),
        ("~ blocks an ", true),
        ("~ becomes blocked by a ", false),
        ("~ becomes blocked by an ", false),
    ] {
        if let Some(rest) = r.strip_prefix(prefix)
            && let Some((n, rest)) = nouns::noun(rest, cx)
            && !n.plural
            && n.zone == Zone::Battlefield
            && n.filter != ObjectFilter::HasType(CardType::Creature)
        {
            let (attacker, blocker) = if blocking {
                (n.filter, ObjectFilter::IsSelf)
            } else {
                (ObjectFilter::IsSelf, n.filter)
            };
            return Some((
                EventPattern::BlockedBy { attacker, blocker },
                normal,
                Some(Selector::SelfSource),
                rest,
            ));
        }
    }
    // Self triggers: "~ attacks", and two at once — "~ enters or attacks".
    if let Some(r) = r.strip_prefix("~ ") {
        let (on, timing, it, r) = self_trigger(r)?;
        if let Some(rest) = r.strip_prefix(" or ")
            && let Some((on2, timing2, it2, rest)) = self_trigger(rest)
        {
            let timing = if timing2 == leaves { leaves } else { timing };
            let it = if it == it2 { it } else { None };
            return Some((EventPattern::AnyOf(vec![on, on2]), timing, it, rest));
        }
        return Some((on, timing, it, r));
    }

    // "a player casts a white spell", "an opponent casts a spell".
    for (prefix, by) in [
        ("a player casts ", Selector::EachPlayer),
        ("an opponent casts ", Selector::Opponents),
    ] {
        if let Some(r) = r.strip_prefix(prefix) {
            let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
            let (n, r) = nouns::noun(r, cx)?;
            if n.zone != Zone::Stack || n.plural {
                return None;
            }
            return Some((
                EventPattern::Cast { who: n.filter, by },
                normal,
                Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject)),
                r,
            ));
        }
    }
    // "when you cast this spell": a cast trigger, working from the stack.
    if let Some(r) = r.strip_prefix("you cast ~") {
        return Some((
            EventPattern::Cast {
                who: ObjectFilter::IsSelf,
                by: Selector::You,
            },
            normal,
            Some(Selector::SelfSource),
            r,
        ));
    }
    // Heroic: "you cast a spell that targets ~"; "you cast an instant or sorcery spell that
    // targets a creature you control".
    if let Some(r) = r.strip_prefix("you cast ")
        && let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
        && let Some((spell, r)) = r
            .strip_prefix("spell that targets ")
            .map(|r| (ObjectFilter::Any, r))
            .or_else(|| {
                let (n, r) = nouns::noun(r, cx)?;
                let r = r.strip_prefix(" that targets ")?;
                (n.zone == Zone::Stack && !n.plural).then_some((n.filter, r))
            })
        && let Some((target, it, r)) = r
            .strip_prefix('~')
            .map(|r| (ObjectFilter::IsSelf, Some(Selector::SelfSource), r))
            .or_else(|| {
                let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
                let (n, r) = nouns::noun(r, cx)?;
                (n.zone == Zone::Battlefield && !n.plural).then_some((n.filter, None, r))
            })
    {
        return Some((
            EventPattern::CastTargeting {
                by: Selector::You,
                target: Box::new(target),
                spell: Box::new(spell),
            },
            normal,
            it,
            r,
        ));
    }
    // Magecraft: "you cast or copy an instant or sorcery spell".
    if let Some(r) = r.strip_prefix("you cast or copy ")
        && let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
        && let Some((n, r)) = nouns::noun(r, cx)
        && n.zone == Zone::Stack
        && !n.plural
    {
        return Some((
            EventPattern::AnyOf(vec![
                EventPattern::Cast {
                    who: n.filter.clone(),
                    by: Selector::You,
                },
                EventPattern::Copied {
                    who: n.filter,
                    by: Selector::You,
                },
            ]),
            normal,
            subject,
            r,
        ));
    }
    // "you draw your second card each turn".
    for (prefix, whose) in [
        ("you draw your second card each turn", Selector::You),
        (
            "a player draws their second card each turn",
            Selector::EachPlayer,
        ),
        (
            "an opponent draws their second card each turn",
            Selector::Opponents,
        ),
    ] {
        if let Some(r) = r.strip_prefix(prefix) {
            return Some((EventPattern::NthDraw { whose, n: 2 }, normal, None, r));
        }
    }
    // "you cast your second spell each turn".
    for (prefix, n) in [
        ("you cast your second spell each turn", 2),
        ("you cast your third spell each turn", 3),
    ] {
        if let Some(r) = r.strip_prefix(prefix) {
            return Some((EventPattern::NthSpellCast { n }, normal, subject, r));
        }
    }
    // "you cast a creature spell", "you gain life".
    if let Some(r) = r.strip_prefix("you cast ")
        && let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
        && let Some((n, r)) = nouns::noun(r, cx)
        && n.zone == Zone::Stack
        && !n.plural
    {
        let r = turn_qualifier(r, cx);
        return Some((
            EventPattern::Cast {
                who: n.filter,
                by: Selector::You,
            },
            normal,
            subject,
            r,
        ));
    }
    // "you discard a card", "an opponent discards a card".
    for (prefix, whose) in [
        ("you discard a card", Selector::You),
        ("an opponent discards a card", Selector::Opponents),
        ("a player discards a card", Selector::EachPlayer),
    ] {
        if let Some(r) = r.strip_prefix(prefix) {
            // The discarded card is the event's other party; its subject is the player.
            let card = Some(Selector::Bound(mtg_ir::selector::Binding::EventOther));
            return Some((EventPattern::Discards { whose }, normal, card, r));
        }
    }
    // "you draw a card", "an opponent draws a card", "a player draws a card".
    for (prefix, whose) in [
        ("you draw a card", Selector::You),
        ("an opponent draws a card", Selector::Opponents),
        ("a player draws a card", Selector::EachPlayer),
    ] {
        if let Some(r) = r.strip_prefix(prefix) {
            return Some((EventPattern::Draws { whose }, normal, None, r));
        }
    }
    if let Some(r) = r.strip_prefix("you gain life") {
        let r = turn_qualifier(r, cx);
        return Some((
            EventPattern::LifeGained {
                whose: Selector::You,
            },
            normal,
            None,
            r,
        ));
    }
    if let Some(r) = r.strip_prefix("you lose life") {
        let r = turn_qualifier(r, cx);
        return Some((
            EventPattern::LifeLost {
                whose: Selector::You,
            },
            normal,
            None,
            r,
        ));
    }
    if let Some(r) = r
        .strip_prefix("you're dealt damage")
        .or_else(|| r.strip_prefix("you are dealt damage"))
    {
        return Some((
            EventPattern::DealsDamage {
                source: ObjectFilter::Any,
                to: DamageRecipient::Player(Selector::You),
                combat_only: false,
            },
            normal,
            None,
            r,
        ));
    }

    // "whenever you attack": once each combat, as you declare attackers.
    if let Some(r) = r.strip_prefix("you attack")
        && (r.is_empty() || r.starts_with(','))
    {
        cx.batch = true;
        let who = ObjectFilter::ControlledBy(Box::new(Selector::You));
        return Some((EventPattern::Attacks { who }, normal, None, r));
    }
    // "whenever you attack with two or more creatures", "… with one or more Knights".
    if let Some(r) = r.strip_prefix("you attack with ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more ")
        && let Some((noun, r)) = nouns::noun(r, cx)
        && noun.plural
        && noun.zone == Zone::Battlefield
        && r.starts_with(',')
    {
        cx.batch = true;
        let who = ObjectFilter::And(vec![
            noun.filter,
            ObjectFilter::ControlledBy(Box::new(Selector::You)),
        ]);
        if n > 1 {
            cx.while_ = Some(mtg_ir::trigger::Condition::CountAtLeast {
                what: Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(vec![who.clone(), ObjectFilter::Attacking]),
                },
                at_least: Value::Fixed(n),
            });
        }
        return Some((EventPattern::Attacks { who }, normal, None, r));
    }
    // "enchanted creature dies", "equipped creature attacks": what this is attached to.
    for host in ["enchanted creature ", "equipped creature "] {
        let Some(r) = r.strip_prefix(host) else {
            continue;
        };
        let who = ObjectFilter::And(vec![
            ObjectFilter::AttachedToSelf,
            ObjectFilter::HasType(CardType::Creature),
        ]);
        for (phrase, pattern) in [
            ("dies", EventPattern::Dies { who: who.clone() }),
            ("attacks", EventPattern::Attacks { who: who.clone() }),
            ("blocks", EventPattern::Blocks { who: who.clone() }),
            (
                "becomes tapped",
                EventPattern::BecomesTapped { who: who.clone() },
            ),
            (
                "becomes untapped",
                EventPattern::BecomesUntapped { who: who.clone() },
            ),
            (
                "becomes blocked",
                EventPattern::BecomesBlocked { who: who.clone() },
            ),
            (
                "deals combat damage to a player",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Player(Selector::EachPlayer),
                    combat_only: true,
                },
            ),
            (
                "deals combat damage to an opponent",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Player(Selector::Opponents),
                    combat_only: true,
                },
            ),
            (
                "deals damage to an opponent",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Player(Selector::Opponents),
                    combat_only: false,
                },
            ),
            (
                "deals damage to a player",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Player(Selector::EachPlayer),
                    combat_only: false,
                },
            ),
            (
                "deals damage",
                EventPattern::DealsDamage {
                    source: who.clone(),
                    to: DamageRecipient::Any,
                    combat_only: false,
                },
            ),
            (
                "is dealt damage",
                EventPattern::TakesDamage {
                    who: who.clone(),
                    combat_only: false,
                },
            ),
        ] {
            if let Some(r) = r.strip_prefix(phrase) {
                let timing = if phrase == "dies" { leaves } else { normal };
                return Some((pattern, timing, subject, r));
            }
        }
        return None;
    }

    // "one or more creatures you control die" (CR 603.2c): once per batch of events.
    if let Some(r) = r.strip_prefix("one or more ") {
        let (other, r) = match r.strip_prefix("other ") {
            Some(r) => (true, r),
            None => (false, r),
        };
        let (n, r) = nouns::noun(r, cx)?;
        // The quantifier supplies plurality, including invariant subtypes such as Merfolk.
        if n.zone != Zone::Battlefield {
            return None;
        }
        let mut who = n.filter;
        if other {
            who = ObjectFilter::And(vec![who, ObjectFilter::Not(Box::new(ObjectFilter::IsSelf))]);
        }
        let (pattern, timing, r) = if let Some(r) = r.strip_prefix(" enter") {
            (EventPattern::Enters { who }, normal, r)
        } else if let Some(r) = r.strip_prefix(" die") {
            (EventPattern::Dies { who }, leaves, r)
        } else if let Some(r) = r.strip_prefix(" attack") {
            (EventPattern::Attacks { who }, normal, r)
        } else if let Some(r) = r.strip_prefix(" become tapped") {
            (EventPattern::BecomesTapped { who }, normal, r)
        } else if let Some(r) = r.strip_prefix(" become untapped") {
            (EventPattern::BecomesUntapped { who }, normal, r)
        } else {
            let r = r.strip_prefix(" deal combat damage to a player")?;
            (
                EventPattern::DealsDamage {
                    source: who,
                    to: DamageRecipient::Player(Selector::EachPlayer),
                    combat_only: true,
                },
                normal,
                r,
            )
        };
        if !(r.is_empty() || r.starts_with(',')) {
            return None;
        }
        cx.batch = true;
        return Some((pattern, timing, None, r));
    }

    // "another creature you control enters", "a creature you control dies".
    let (other, r) = if let Some(r) = r.strip_prefix("another ") {
        (true, r)
    } else {
        let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
        (false, r)
    };
    let (n, r) = nouns::noun(r, cx)?;
    if n.plural || n.zone != Zone::Battlefield {
        return None;
    }
    let mut who = n.filter;
    if other {
        who = ObjectFilter::And(vec![who, ObjectFilter::Not(Box::new(ObjectFilter::IsSelf))]);
    }
    for (phrase, timing) in [
        (" enters the battlefield under your control", normal),
        (" enters", normal),
        (" enters the battlefield", normal),
        (" dies", leaves),
        (" is put into a graveyard from the battlefield", leaves),
        (" leaves the battlefield", leaves),
        (" attacks alone", normal),
        (" attacks", normal),
        (" becomes blocked", normal),
        (" becomes tapped", normal),
        (" becomes untapped", normal),
        (" is dealt damage", normal),
        (" is dealt combat damage", normal),
        (" deals combat damage to a player", normal),
        (" deals combat damage to an opponent", normal),
        (" deals damage to an opponent", normal),
        (" deals damage to a player", normal),
        (
            " becomes the target of a spell or ability an opponent controls",
            normal,
        ),
    ] {
        let Some(r) = r.strip_prefix(phrase) else {
            continue;
        };
        let who = if phrase.ends_with("under your control") {
            ObjectFilter::And(vec![
                who.clone(),
                ObjectFilter::ControlledBy(Box::new(Selector::You)),
            ])
        } else {
            who.clone()
        };
        let pattern = match phrase {
            " dies" | " is put into a graveyard from the battlefield" => EventPattern::Dies { who },
            " leaves the battlefield" => EventPattern::Leaves { who },
            " attacks" => EventPattern::Attacks { who },
            // CR 506.5: the only creature declared as an attacker.
            " attacks alone" => EventPattern::Attacks {
                who: ObjectFilter::And(vec![who, ObjectFilter::AttackingAlone]),
            },
            " becomes blocked" => EventPattern::BecomesBlocked { who },
            " becomes tapped" => EventPattern::BecomesTapped { who },
            " becomes untapped" => EventPattern::BecomesUntapped { who },
            " is dealt damage" | " is dealt combat damage" => EventPattern::TakesDamage {
                who,
                combat_only: phrase.contains("combat"),
            },
            " deals combat damage to a player" => EventPattern::DealsDamage {
                source: who,
                to: DamageRecipient::Player(Selector::EachPlayer),
                combat_only: true,
            },
            " deals combat damage to an opponent" | " deals damage to an opponent" => {
                EventPattern::DealsDamage {
                    source: who,
                    to: DamageRecipient::Player(Selector::Opponents),
                    combat_only: phrase.contains("combat"),
                }
            }
            " deals damage to a player" => EventPattern::DealsDamage {
                source: who,
                to: DamageRecipient::Player(Selector::EachPlayer),
                combat_only: false,
            },
            " becomes the target of a spell or ability an opponent controls" => {
                EventPattern::BecomesTarget {
                    who,
                    by: Selector::Opponents,
                }
            }
            _ => EventPattern::Enters { who },
        };
        return Some((pattern, timing, subject, r));
    }
    None
}

// ---- activated abilities ------------------------------------------------------

/// "{T}: Add {U} or {B}. Activate only if you control a Swamp." — any activated ability,
/// with a condition checked as it is activated (CR 602.5b).
fn activate_only_if(
    line: &str,
    face: &FaceText,
    cx: &mut Cx,
) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    let (head, rest) = line.rsplit_once(" activate only if ")?;
    if !head.ends_with('.') || head.contains('"') {
        return None;
    }
    let rest = rest.strip_suffix('.')?;
    let (rest, only) = if let Some(r) = rest.strip_suffix(" and only as a sorcery") {
        (r, Some(ActivationTiming::SorcerySpeed))
    } else if let Some(r) = rest.strip_suffix(" and only once each turn") {
        (r, Some(ActivationTiming::InstantOncePerTurn))
    } else {
        (rest, None)
    };
    let mut inner = cx.fresh();
    // "if this land entered this turn or if you control a basic land": either suffices.
    let condition = match rest.split(" or if ").collect::<Vec<_>>().as_slice() {
        [one] => match conditions::condition(one, &inner)? {
            (c, "") => c,
            _ => return None,
        },
        some => mtg_ir::trigger::Condition::Or(
            some.iter()
                .map(|part| match conditions::condition(part, &inner)? {
                    (c, "") => Some(c),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
    };
    let Some(Line::Abilities(mut list)) = compile_line(head, face, &mut inner) else {
        return None;
    };
    let [(AbilityKind::Activated { cost, timing, .. }, _)] = list.as_mut_slice() else {
        return None;
    };
    if let Some(only) = only {
        if *timing != ActivationTiming::Instant {
            return None;
        }
        *timing = only;
    }
    cost.timing.push(condition);
    Some(list)
}

/// "<cost>: <effect>", with an optional "Activate only as a sorcery."
pub(crate) fn activated(line: &str, cx: &mut Cx) -> Option<AbilityKind> {
    let (cost_text, body) = line.split_once(": ")?;
    let loyalty = loyalty_delta(cost_text);
    let mut cost = if let Some(delta) = loyalty {
        Cost {
            additional: vec![AdditionalCost::Loyalty { delta }],
            ..Cost::free()
        }
    } else {
        cost(cost_text, cx)?
    };
    let (body, mut timing) = if let Some(b) = body.strip_suffix(" activate only as a sorcery.") {
        (b, ActivationTiming::SorcerySpeed)
    } else if let Some(b) = body.strip_suffix(" any player may activate this ability.") {
        (b, ActivationTiming::AnyPlayer)
    } else if let Some(b) =
        body.strip_suffix(" any player may activate this ability but only as a sorcery.")
    {
        (b, ActivationTiming::AnyPlayerSorcery)
    } else if let Some(b) = body.strip_suffix(" activate only once each turn.") {
        (b, ActivationTiming::InstantOncePerTurn)
    } else if let Some(b) = body.strip_suffix(" activate no more than twice each turn.") {
        (b, ActivationTiming::InstantTwicePerTurn)
    } else if let Some(b) =
        body.strip_suffix(" activate only as a sorcery and only once each turn.")
    {
        (b, ActivationTiming::SorceryOncePerTurn)
    } else if let Some(b) =
        body.strip_suffix(" activate only during your turn, before attackers are declared.")
    {
        (b, ActivationTiming::YourTurnBeforeAttackers)
    } else if let Some(b) = body.strip_suffix(" activate only during your turn.") {
        (b, ActivationTiming::YourTurn)
    } else if let Some(b) = body.strip_suffix(" activate only during your upkeep.") {
        (b, ActivationTiming::YourUpkeep)
    } else if let Some(b) = body.strip_suffix(" activate only during combat.") {
        cost.timing.push(mtg_ir::trigger::Condition::Or(
            [
                Step::BeginCombat,
                Step::DeclareAttackers,
                Step::DeclareBlockers,
                Step::FirstStrikeCombatDamage,
                Step::CombatDamage,
                Step::EndCombat,
            ]
            .into_iter()
            .map(mtg_ir::trigger::Condition::DuringStep)
            .collect(),
        ));
        (b, ActivationTiming::Instant)
    } else {
        (body, ActivationTiming::Instant)
    };
    if body.contains("activate ")
        || body.contains(" add {")
        || (body.starts_with("add ") && loyalty.is_none())
    {
        return None;
    }
    if loyalty.is_some() {
        timing = ActivationTiming::SorcerySpeed;
    }
    cx.it = Some(Selector::SelfSource);
    cx.x = cost_text.contains("{x}");
    // "Put this card from your graveyard into your library second/third from the top."
    if let Some(dest) = body.strip_prefix("put ~ from your graveyard into your library ") {
        let position = match dest {
            "second from the top." => mtg_ir::effect::ZonePosition::FromTop(1),
            "third from the top." => mtg_ir::effect::ZonePosition::FromTop(2),
            _ => return None,
        };
        if loyalty.is_some() || !graveyard_cost(&cost) {
            return None;
        }
        let mut effect = return_self(Zone::Library, false);
        if let Effect::MoveZone { position: at, .. } = &mut effect {
            *at = position;
        }
        return Some(AbilityKind::Activated {
            cost,
            effect,
            functions_from: Zone::Graveyard,
            is_mana_ability: false,
            is_loyalty_ability: false,
            timing,
        });
    }
    // "Return this card from your graveyard to your hand." — an ability of a card in the
    // graveyard (CR 113.6j). Its cost cannot involve the card being on the battlefield.
    if let Some(dest) = body.strip_prefix("return ~ from your graveyard to ") {
        let (to, tapped) = match dest {
            "your hand." => (Zone::Hand, false),
            "the battlefield." => (Zone::Battlefield, false),
            "the battlefield tapped." => (Zone::Battlefield, true),
            _ => return None,
        };
        if loyalty.is_some() || !graveyard_cost(&cost) {
            return None;
        }
        return Some(AbilityKind::Activated {
            cost,
            effect: return_self(to, tapped),
            functions_from: Zone::Graveyard,
            is_mana_ability: false,
            is_loyalty_ability: false,
            timing,
        });
    }
    let effect = if loyalty.is_some() && body.starts_with("add ") {
        let outputs = mana_line(&format!("{{t}}: {body}"))?;
        // Colour choices during stack resolution need their own prompt; reject
        // those until implemented rather than silently picking a colour.
        if outputs.iter().any(|o| o.possible_colors().len() > 1) {
            return None;
        }
        Effect::AddMana {
            who: Selector::You,
            produces: outputs,
        }
    } else {
        clauses::effect(body, cx)?
    };
    // "{2}, Exile this card from your graveyard: …" — an ability of the card in the
    // graveyard (CR 113.6j).
    let exiles_self = cost.additional.iter().any(|c| {
        matches!(
            c,
            AdditionalCost::ExileFrom {
                zone: Zone::Graveyard,
                filter: ObjectFilter::IsSelf,
                ..
            }
        )
    });
    if exiles_self && (loyalty.is_some() || !graveyard_cost(&cost)) {
        return None;
    }
    // "{1}{G}, Discard this card: …" — an ability of the card in hand (CR 113.6j).
    let discards_self = cost.additional.iter().any(|c| {
        matches!(
            c,
            AdditionalCost::Discard {
                filter: ObjectFilter::IsSelf,
                ..
            }
        )
    });
    if discards_self && (loyalty.is_some() || exiles_self || !graveyard_cost(&cost)) {
        return None;
    }
    Some(AbilityKind::Activated {
        cost,
        effect,
        functions_from: if exiles_self {
            Zone::Graveyard
        } else if discards_self {
            Zone::Hand
        } else {
            Zone::Battlefield
        },
        is_mana_ability: false,
        is_loyalty_ability: loyalty.is_some(),
        timing,
    })
}

/// Whether a cost can be paid for a card in a graveyard: mana, life, and other things the
/// player has, but nothing that taps, untaps or sacrifices the card itself.
fn graveyard_cost(cost: &Cost) -> bool {
    cost.additional.iter().all(|c| {
        !matches!(
            c,
            AdditionalCost::Tap { .. }
                | AdditionalCost::Untap { .. }
                | AdditionalCost::Sacrifice {
                    what: Selector::SelfSource,
                    ..
                }
        )
    })
}

/// Move the ability's own card from where it is to `to`.
fn return_self(to: Zone, tapped: bool) -> Effect {
    Effect::MoveZone {
        what: Selector::SelfSource,
        to,
        owner_relative_to: None,
        position: mtg_ir::effect::ZonePosition::Natural,
        tapped,
        face_down: false,
        under_control_of: None,
    }
}

/// CR 702.141a — "encore {5}{R}": exile this card from your graveyard: for each opponent,
/// create a token copy of it that attacks that opponent this turn if able. They gain haste.
/// Sacrifice them at the beginning of the next end step. Activate only as a sorcery.
fn encore(line: &str) -> Option<AbilityKind> {
    let (mana, rest) = mana_cost(line.trim_end_matches('.').strip_prefix("encore ")?)?;
    if !rest.is_empty() {
        return None;
    }
    let made = || Selector::Bound(mtg_ir::selector::Binding::It);
    Some(AbilityKind::Activated {
        cost: Cost {
            mana,
            additional: vec![AdditionalCost::ExileFrom {
                zone: Zone::Graveyard,
                filter: ObjectFilter::IsSelf,
                count: Value::ONE,
            }],
            ..Cost::free()
        },
        effect: Effect::Sequence(vec![
            Effect::CreateTokenCopy {
                of: Selector::SelfSource,
                count: Value::Count(Box::new(Selector::Opponents)),
                controller: Selector::You,
            },
            Effect::Continuous {
                what: made(),
                modification: clauses::grant(mtg_core::Keyword::Haste),
                duration: mtg_ir::effect::Duration::Permanent,
            },
            // Two players: "attacks that opponent" is attacking the only one there is.
            Effect::Continuous {
                what: made(),
                modification: Modification::Restriction(Restriction::MustAttackIfAble),
                duration: mtg_ir::effect::Duration::UntilEndOfTurn,
            },
            Effect::Delayed {
                on: EventPattern::StepBegins {
                    step: Step::End,
                    whose: Selector::EachPlayer,
                },
                effect: Box::new(Effect::Sacrifice {
                    who: Selector::You,
                    what: made(),
                }),
            },
        ]),
        functions_from: Zone::Graveyard,
        is_mana_ability: false,
        is_loyalty_ability: false,
        timing: ActivationTiming::SorcerySpeed,
    })
}

/// CR 702.97a — "scavenge {3}{G}{G}": exile this card from your graveyard: put a number of
/// +1/+1 counters equal to its power on target creature. Scavenge only as a sorcery.
fn scavenge(line: &str, cx: &mut Cx) -> Option<AbilityKind> {
    let (mana, rest) = mana_cost(line.trim_end_matches('.').strip_prefix("scavenge ")?)?;
    if !rest.is_empty() {
        return None;
    }
    let target = cx.target(TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::HasType(CardType::Creature),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    });
    Some(AbilityKind::Activated {
        cost: Cost {
            mana,
            additional: vec![AdditionalCost::ExileFrom {
                zone: Zone::Graveyard,
                filter: ObjectFilter::IsSelf,
                count: Value::ONE,
            }],
            ..Cost::free()
        },
        effect: Effect::AddCounters {
            what: target,
            kind: mtg_core::CounterKind::PlusOnePlusOne,
            amount: Value::Power(Box::new(Selector::SelfSource)),
        },
        functions_from: Zone::Graveyard,
        is_mana_ability: false,
        is_loyalty_ability: false,
        timing: ActivationTiming::SorcerySpeed,
    })
}

/// CR 702.84a — "unearth {B}": return this card from your graveyard to the battlefield. It
/// gains haste. Exile it at the beginning of the next end step or if it would leave the
/// battlefield. Unearth only as a sorcery.
fn unearth(line: &str) -> Option<AbilityKind> {
    let (mana, rest) = mana_cost(line.strip_prefix("unearth ")?)?;
    if !rest.is_empty() {
        return None;
    }
    let it = || Selector::Bound(mtg_ir::selector::Binding::It);
    let exile_it = Effect::MoveZone {
        what: it(),
        to: Zone::Exile,
        owner_relative_to: None,
        position: mtg_ir::effect::ZonePosition::Natural,
        tapped: false,
        face_down: false,
        under_control_of: None,
    };
    Some(AbilityKind::Activated {
        cost: Cost {
            mana,
            ..Cost::free()
        },
        effect: Effect::Sequence(vec![
            return_self(Zone::Battlefield, false),
            Effect::Continuous {
                what: it(),
                modification: clauses::grant(mtg_core::Keyword::Haste),
                duration: mtg_ir::effect::Duration::Permanent,
            },
            Effect::ExileIfLeaves { what: it() },
            Effect::Delayed {
                on: EventPattern::StepBegins {
                    step: Step::End,
                    whose: Selector::EachPlayer,
                },
                effect: Box::new(exile_it),
            },
        ]),
        functions_from: Zone::Graveyard,
        is_mana_ability: false,
        is_loyalty_ability: false,
        timing: ActivationTiming::SorcerySpeed,
    })
}

/// A printed loyalty symbol: +N, -N, or 0. Variable costs remain unsupported.
fn loyalty_delta(text: &str) -> Option<i32> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty()
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || (!text.starts_with(['+', '-']) && text != "0")
    {
        return None;
    }
    text.parse::<i32>().ok().filter(|n| *n != i32::MIN)
}

/// A cost part that is a choice: "sacrifice a creature", "sacrifice another artifact",
/// "sacrifice two lands", "discard a card", "discard two cards".
fn chosen_cost(part: &str, cx: &Cx) -> Option<AdditionalCost> {
    // "tap an untapped creature you control", "tap two untapped artifacts you control"
    if let Some(r) = part.strip_prefix("tap ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" untapped ")
        && r.ends_with(" you control")
        && let Some((noun, "")) = nouns::noun(r, cx)
        && noun.zone == Zone::Battlefield
        && noun.plural == (n > 1)
    {
        return Some(AdditionalCost::TapUntapped {
            filter: noun.filter,
            count: Value::Fixed(n),
        });
    }
    if let Some(r) = part.strip_prefix("discard ") {
        let (n, r) = words::number(r)?;
        let card = if n == 1 { "card" } else { "cards" };
        let r = r.strip_prefix(' ')?;
        let filter = if r == card {
            ObjectFilter::Any
        } else {
            // The hand is fixed by the discard cost, while the noun supplies its
            // printed type/color/subtype restrictions. Require "card(s)" so a
            // battlefield noun cannot silently become a hand cost.
            if !r.ends_with(card) {
                return None;
            }
            let in_hand = format!("{r} in your hand");
            let (noun, rest) = nouns::noun(&in_hand, cx)?;
            if !rest.is_empty() || noun.zone != Zone::Hand || noun.plural != (n > 1) {
                return None;
            }
            noun.filter
        };
        return Some(AdditionalCost::Discard {
            count: Value::Fixed(n),
            filter,
            at_random: false,
        });
    }
    // "exile three other cards from your graveyard", "exile a creature card from your
    // graveyard".
    if let Some(r) = part.strip_prefix("exile ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(' ')
    {
        let (other, r) = match r.strip_prefix("other ") {
            Some(r) => (true, r),
            None => (false, r),
        };
        let card = if n == 1 { "card" } else { "cards" };
        // Whether the noun is a card in the hand, which "from your hand" also allows.
        let (mut filter, r, hand) = if let Some(r) = r.strip_prefix(card) {
            (ObjectFilter::Any, r, true)
        } else {
            let (noun, r) = nouns::noun(r, cx)?;
            if noun.plural != (n > 1) {
                return None;
            }
            (noun.filter, r, noun.zone == Zone::Hand)
        };
        // "exile a blue card from your hand": an alternative cost's pitched card.
        if part.ends_with(" from your hand")
            && !other
            && (r.is_empty() || r == " from your hand")
            && hand
        {
            return Some(AdditionalCost::ExileFrom {
                zone: Zone::Hand,
                filter,
                count: Value::Fixed(n),
            });
        }
        // The noun may have read "from your graveyard" itself.
        if !part.ends_with(" from your graveyard") || !(r.is_empty() || r == " from your graveyard")
        {
            return None;
        }
        if other {
            filter = ObjectFilter::And(vec![
                filter,
                ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
            ]);
        }
        return Some(AdditionalCost::ExileFrom {
            zone: Zone::Graveyard,
            filter,
            count: Value::Fixed(n),
        });
    }
    let r = part.strip_prefix("sacrifice ")?;
    let (n, other, r) = if let Some(r) = r.strip_prefix("another ") {
        (1, true, r)
    } else {
        let (n, r) = words::number(r)?;
        (n, false, r.strip_prefix(' ')?)
    };
    let (noun, rest) = nouns::noun(r, cx)?;
    if !rest.is_empty() || noun.zone != Zone::Battlefield || noun.plural != (n > 1) {
        return None;
    }
    let filter = if other {
        ObjectFilter::And(vec![
            noun.filter,
            ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
        ])
    } else {
        noun.filter
    };
    Some(AdditionalCost::Sacrifice {
        what: Selector::All {
            zone: Zone::Battlefield,
            filter,
        },
        count: Value::Fixed(n),
    })
}

/// "As an additional cost to cast this spell, sacrifice a creature." (CR 601.2f)
fn additional_cast_cost(line: &str, cx: &Cx) -> Option<AbilityKind> {
    let r = line
        .strip_prefix("as an additional cost to cast ~, ")?
        .strip_suffix('.')?;
    if let Some(cost) = one_additional_cost(r, cx) {
        return Some(AbilityKind::AdditionalCastCost { cost });
    }
    // "sacrifice a creature or pay {3}": either, chosen as it is cast. The split is tried
    // at each " or ", since a noun can hold one ("an artifact or creature").
    r.match_indices(" or ").find_map(|(at, _)| {
        let (a, b) = (&r[..at], &r[at + 4..]);
        let (ca, cb) = (one_additional_cost(a, cx)?, one_additional_cost(b, cx)?);
        Some(AbilityKind::AdditionalCastCostChoice {
            options: vec![(a.into(), ca), (b.into(), cb)],
        })
    })
}

/// One additional casting cost: "pay 2 life", "pay {3}", "sacrifice a creature".
fn one_additional_cost(r: &str, cx: &Cx) -> Option<Cost> {
    let mut cost = Cost::free();
    if let Some(r) = r.strip_prefix("pay ") {
        if let Some((mana, "")) = mana_cost(r) {
            cost.mana = mana;
            return Some(cost);
        }
        let (n, rest) = words::number(r)?;
        if rest != " life" {
            return None;
        }
        cost.additional.push(AdditionalCost::PayLife {
            amount: Value::Fixed(n),
        });
    } else {
        cost.additional
            .push(reveal_hand_cost(r, cx).or_else(|| chosen_cost(r, cx))?);
    }
    Some(cost)
}

/// Reveal matching cards from hand as an additional casting cost.
fn reveal_hand_cost(part: &str, cx: &Cx) -> Option<AdditionalCost> {
    let r = part.strip_prefix("reveal ")?;
    let (n, r) = words::number(r)?;
    let r = r.strip_prefix(' ')?.strip_suffix(" from your hand")?;
    let phrase = format!("{r} in your hand");
    let (noun, "") = nouns::noun(&phrase, cx)? else {
        return None;
    };
    if n < 1 || noun.zone != Zone::Hand || noun.plural != (n > 1) {
        return None;
    }
    Some(AdditionalCost::Reveal {
        filter: noun.filter,
        count: Value::Fixed(n),
    })
}

/// A cost the engine can pay: mana, {T}, {Q}, "sacrifice ~", "pay N life", "remove a
/// +1/+1 counter from ~", "sacrifice a creature", "discard a card".
fn cost(text: &str, cx: &Cx) -> Option<Cost> {
    let mut out = Cost::free();
    // "Sacrifice an artifact, enchantment, or token": a list in one part, not three parts.
    let mut parts: Vec<String> = Vec::new();
    let raw: Vec<&str> = text.split(", ").collect();
    let mut i = 0;
    while i < raw.len() {
        let take = if raw.get(i + 2).is_some_and(|p| p.starts_with("or ")) {
            3
        } else if raw.get(i + 1).is_some_and(|p| p.starts_with("or ")) {
            2
        } else {
            1
        };
        let end = (i + take).min(raw.len());
        parts.push(raw[i..end].join(", "));
        i = end;
    }
    for part in parts.iter().map(String::as_str) {
        if part == "discard ~" {
            out.additional.push(AdditionalCost::Discard {
                count: Value::ONE,
                filter: ObjectFilter::IsSelf,
                at_random: false,
            });
        } else if part == "exile ~ from your graveyard" {
            out.additional.push(AdditionalCost::ExileFrom {
                zone: Zone::Graveyard,
                filter: ObjectFilter::IsSelf,
                count: Value::ONE,
            });
        } else if let Some(c) = chosen_cost(part, cx) {
            out.additional.push(c);
        } else if part == "{t}" {
            out.additional.push(AdditionalCost::Tap {
                what: Selector::SelfSource,
            });
        } else if part == "exert ~" {
            out.additional.push(AdditionalCost::Exert);
        } else if part == "{q}" {
            out.additional.push(AdditionalCost::Untap {
                what: Selector::SelfSource,
            });
        } else if part == "sacrifice ~" {
            out.additional.push(AdditionalCost::Sacrifice {
                what: Selector::SelfSource,
                count: Value::ONE,
            });
        } else if let Some((n, "")) = part.strip_prefix("pay ").and_then(words::energy) {
            out.additional.push(AdditionalCost::PayEnergy {
                amount: Value::Fixed(n),
            });
        } else if let Some(r) = part.strip_prefix("pay ") {
            let (n, r) = words::number(r)?;
            if r != " life" {
                return None;
            }
            out.additional.push(AdditionalCost::PayLife {
                amount: Value::Fixed(n),
            });
        } else if let Some(r) = part.strip_prefix("remove ") {
            // "remove X storage counters", "remove any number of charge counters": how many
            // is announced as X.
            let (amount, plural, r) = if let Some(r) = r
                .strip_prefix("x ")
                .or_else(|| r.strip_prefix("any number of "))
            {
                (Value::X, true, r)
            } else {
                let (n, r) = words::number(r)?;
                (Value::Fixed(n), n != 1, r.strip_prefix(' ')?)
            };
            let (kind, r) = words::counter(r)?;
            let r = if plural { r.strip_prefix('s')? } else { r };
            if r != " from ~" {
                return None;
            }
            out.additional.push(AdditionalCost::RemoveCounters {
                what: Selector::SelfSource,
                kind,
                amount,
            });
        } else if part.starts_with('{') {
            if !out.mana.symbols.is_empty() {
                return None;
            }
            let (mana, rest) = mana_cost(part)?;
            if !rest.is_empty() {
                return None;
            }
            out.mana = mana;
        } else {
            return None;
        }
    }
    // "{T}, Tap an untapped creature you control": the source taps for {T}, so it can't
    // also be the creature tapped.
    if out.additional.iter().any(|c| {
        matches!(
            c,
            AdditionalCost::Tap {
                what: Selector::SelfSource
            }
        )
    }) {
        for c in &mut out.additional {
            if let AdditionalCost::TapUntapped { filter, .. } = c {
                *filter = ObjectFilter::And(vec![
                    filter.clone(),
                    ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
                ]);
            }
        }
    }
    Some(out)
}

/// A run of mana symbols at the start of `s`. `{X}` is not accepted: announcing X is not
/// implemented.
pub(crate) fn mana_cost(s: &str) -> Option<(mtg_core::ManaCost, &str)> {
    let end = s
        .char_indices()
        .scan(0, |depth, (i, c)| {
            match c {
                '{' => *depth += 1,
                '}' => *depth -= 1,
                _ if *depth == 0 => return None,
                _ => {}
            }
            Some(i + c.len_utf8())
        })
        .last()?;
    let (symbols, rest) = s.split_at(end);
    if symbols.contains("{s}") {
        return None;
    }
    let (cost, unparsed) = crate::manacost::parse(&symbols.to_uppercase());
    if !unparsed.symbols.is_empty() {
        return None;
    }
    Some((cost, rest))
}

// ---- static abilities -----------------------------------------------------------

/// "Creatures you control get +1/+1.", "Enchanted creature gets +2/+2 and has flying.",
/// "~ can't block.", "This spell can't be countered."
fn statics(line: &str, face: &FaceText, cx: &mut Cx) -> Option<Vec<AbilityKind>> {
    // A line ending in a quoted ability has its full stop inside the quotes.
    let line = line
        .strip_suffix('.')
        .or_else(|| line.ends_with(".\"").then_some(line))?;
    // "During your turn, ~ has first strike." / "As long as …, …" / "… as long as …".
    let owned: String;
    let (line, condition) = if let Some(r) = line.strip_prefix("during your turn, ") {
        (r, Some(mtg_ir::trigger::Condition::YourTurn))
    } else if let Some(r) = line.strip_prefix("during turns other than yours, ") {
        (
            r,
            Some(mtg_ir::trigger::Condition::Not(Box::new(
                mtg_ir::trigger::Condition::YourTurn,
            ))),
        )
    } else if let Some(r) = line.strip_prefix("as long as ") {
        // "As long as ~ is equipped, it gets +2/+2": "it" is this. "As long as equipped
        // creature is a Human, it has lifelink": "it" is the equipped creature.
        let about = [
            "~",
            "enchanted creature",
            "equipped creature",
            "enchanted permanent",
        ]
        .into_iter()
        .find(|host| r.starts_with(&format!("{host} ")));
        let (cond, r) = conditions::condition(r, cx)?;
        let r = r.strip_prefix(", ")?;
        let r = match (r.strip_prefix("it "), about) {
            (Some(rest), Some(host)) => {
                owned = format!("{host} {rest}");
                owned.as_str()
            }
            _ => r,
        };
        (r, Some(cond))
    } else if let Some((head, tail)) = line.split_once(" as long as ") {
        let (cond, r) = conditions::condition(tail, cx)?;
        if !r.is_empty() {
            return None;
        }
        (head, Some(cond))
    } else {
        (line, None)
    };
    let mut list = statics_unconditional(line, face, cx)?;
    // The engine reads "you control enchanted …" unconditionally (see
    // `mtg_engine::layers::controller`), so a conditional one is not compiled.
    if condition.is_some()
        && list.iter().any(|k| {
            matches!(
                k,
                AbilityKind::Static {
                    modification: Modification::Control(_),
                    ..
                }
            )
        })
    {
        return None;
    }
    // A condition of the line's own ("costs {2} less to cast if …") is kept alongside.
    if let Some(condition) = condition {
        for k in &mut list {
            if let AbilityKind::Static { condition: c, .. } = k {
                *c = Some(match c.take() {
                    Some(own) => mtg_ir::trigger::Condition::And(vec![condition.clone(), own]),
                    None => condition.clone(),
                });
            }
        }
    }
    Some(list)
}

fn statics_unconditional(line: &str, face: &FaceText, cx: &mut Cx) -> Option<Vec<AbilityKind>> {
    // "During your turn, this Vehicle is an artifact creature": its printed power and
    // toughness apply, as when it is crewed.
    if line == "~ is an artifact creature" && face.subtypes.iter().any(|s| s == "Vehicle") {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::AddTypes(vec![CardType::Creature]),
            condition: None,
        }]);
    }
    // "Enchanted creature gets +2/+2 and can't block": two statics about one subject.
    if let Some((subject, rest)) = line.split_once(" gets ")
        && let Some((pt, tail)) = rest.split_once(" and ")
        && !tail.starts_with("has ")
        && !tail.starts_with("gains ")
        && !pt.contains(' ')
        && let Some(mut first) = statics_unconditional(&format!("{subject} gets {pt}"), face, cx)
        && let Some(second) = statics_unconditional(&format!("{subject} {tail}"), face, cx)
    {
        first.extend(second);
        return Some(first);
    }
    // "Enchanted creature loses all abilities."
    if let Some(subject) = line.strip_suffix(" loses all abilities")
        && let Some((what, _, "")) = nouns::object(subject, &mut cx.clone())
        && matches!(
            &what,
            Selector::All {
                zone: Zone::Battlefield,
                ..
            } | Selector::SelfSource
        )
    {
        return Some(vec![AbilityKind::Static {
            what,
            modification: Modification::LoseAllAbilities,
            condition: None,
        }]);
    }
    if line == "~ can't be countered" {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::CantBeCountered),
            condition: None,
        }]);
    }
    if let Some(k) = cost_static(line, cx) {
        return Some(vec![k]);
    }
    if let Some(k) = self_cost(line, cx) {
        return Some(vec![k]);
    }
    if let Some(k) = cast_only(line, cx) {
        return Some(vec![k]);
    }
    if let Some(k) = flash_permission(line, cx) {
        return Some(vec![k]);
    }
    // "If you would gain life, you gain that much life plus 1 instead." / "… twice that
    // much life instead." (CR 614.1a), from a permanent.
    if let Some(r) = line.strip_prefix("if you would gain life, you gain ")
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        let (plus, double) = if r == "twice that much life instead" {
            (0, true)
        } else {
            let n = r.strip_prefix("that much life plus ")?;
            let (n, rest) = words::number(n)?;
            if rest != " instead" {
                return None;
            }
            (n, false)
        };
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::LifeGainBoost {
                who: Selector::You,
                plus,
                double,
            }),
            condition: None,
        }]);
    }
    if line == "if damage would be dealt to ~, prevent that damage. remove a +1/+1 counter from ~"
        && face.card_types.contains(&CardType::Creature)
    {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::PreventDamageRemoveCounter),
            condition: None,
        }]);
    }
    if let Some(k) = static_prevention(line, face, cx) {
        return Some(vec![k]);
    }
    // "~'s power and toughness are each equal to the number of creatures you control." —
    // a characteristic-defining ability (CR 604.3), read on the battlefield.
    if let Some(r) = line.strip_prefix("~'s power and toughness are each equal to ")
        && face.card_types.contains(&CardType::Creature)
    {
        let (v, rest) = clauses::value_phrase(r, cx)?;
        if !rest.is_empty() {
            return None;
        }
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::SetBasePowerToughness {
                power: v.clone(),
                toughness: v,
            },
            condition: None,
        }]);
    }
    if let Some(r) = line.strip_prefix("~'s power is equal to ")
        && face.card_types.contains(&CardType::Creature)
        && let Some((v, "")) = clauses::value_phrase(r, cx)
    {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::SetBasePower(v),
            condition: None,
        }]);
    }
    if line == "you may play an additional land on each of your turns"
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::AdditionalLandPlay),
            condition: None,
        }]);
    }
    if line == "you may choose not to untap ~ during your untap step"
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::MayChooseNotToUntap),
            condition: None,
        }]);
    }
    let permanent = face.card_types.iter().any(|t| t.is_permanent());
    // "you may play lands and cast spells from the top of your library", "you may cast
    // creature spells from the top of your library", "you may play lands from …".
    if let Some(r) = line
        .strip_suffix(" from the top of your library")
        .and_then(|l| l.strip_prefix("you may "))
        && permanent
    {
        let (lands, r) = match r.strip_prefix("play lands") {
            Some(r) => (true, r.strip_prefix(" and ").unwrap_or(r)),
            None => (false, r),
        };
        let spells = match r.strip_prefix("cast ") {
            Some("spells") => Some(ObjectFilter::Any),
            Some(noun) => match nouns::noun(noun, cx) {
                Some((n, "")) if n.plural && n.zone == Zone::Stack => {
                    Some(strip_is_spell(n.filter).unwrap_or(ObjectFilter::Any))
                }
                _ => None,
            },
            None if r.is_empty() && lands => Some(ObjectFilter::Not(Box::new(ObjectFilter::Any))),
            None => None,
        };
        if let Some(spells) = spells {
            return Some(vec![AbilityKind::Static {
                what: Selector::SelfSource,
                modification: Modification::Restriction(Restriction::PlayFromTopOfLibrary {
                    spells,
                    lands,
                }),
                condition: None,
            }]);
        }
    }
    for (phrase, r) in [
        (
            "you may play lands from your graveyard",
            Restriction::PlayLandsFromGraveyard,
        ),
        ("skip your draw step", Restriction::SkipDrawStep),
        (
            "you may look at the top card of your library any time",
            Restriction::LookAtTopOfLibrary,
        ),
        (
            "play with the top card of your library revealed",
            Restriction::TopOfLibraryRevealed,
        ),
    ] {
        if line == phrase && permanent {
            return Some(vec![AbilityKind::Static {
                what: Selector::SelfSource,
                modification: Modification::Restriction(r),
                condition: None,
            }]);
        }
    }
    // "Creatures with power less than ~'s power can't block it."
    if line == "creatures with power less than ~'s power can't block it" && permanent {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::CantBeBlockedExceptBy(
                ObjectFilter::PowerAtLeast(Value::Power(Box::new(Selector::SelfSource))),
            )),
            condition: None,
        }]);
    }
    if line == "you have hexproof" && permanent {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::PlayerHexproof),
            condition: None,
        }]);
    }
    for (phrase, who) in [
        ("players can't gain life", Selector::EachPlayer),
        ("your opponents can't gain life", Selector::Opponents),
        ("you can't gain life", Selector::You),
    ] {
        if line == phrase && permanent {
            return Some(vec![AbilityKind::Static {
                what: Selector::SelfSource,
                modification: Modification::Restriction(Restriction::CantGainLife(who)),
                condition: None,
            }]);
        }
    }
    // "Activated abilities of artifacts can't be activated."
    if permanent
        && let Some(rest) = line.strip_prefix("activated abilities of ")
        && let Some(rest) = rest.strip_suffix(" can't be activated")
        && let Some((noun, "")) = nouns::noun(rest, cx)
        && noun.plural
        && noun.zone == Zone::Battlefield
    {
        return Some(vec![AbilityKind::Static {
            what: Selector::All {
                zone: Zone::Battlefield,
                filter: noun.filter,
            },
            modification: Modification::Restriction(Restriction::CantActivateAbilities),
            condition: None,
        }]);
    }
    // "Your opponents can't cast creature spells.", "Each player can't cast more than one
    // spell each turn.", "Your opponents can't cast spells during your turn."
    if permanent
        && let Some((who, rest)) = cant_cast_who(line)
        && let Some(rest) = rest.strip_prefix(" can't cast ")
    {
        let (rest, condition) = match rest.strip_suffix(" during your turn") {
            Some(r) => (r, Some(mtg_ir::trigger::Condition::YourTurn)),
            None => (rest, None),
        };
        if let Some(modification) = cant_cast(who, rest, cx) {
            return Some(vec![AbilityKind::Static {
                what: Selector::SelfSource,
                modification,
                condition,
            }]);
        }
    }
    if line == "you have no maximum hand size" && face.card_types.iter().any(|t| t.is_permanent()) {
        return Some(vec![AbilityKind::Static {
            what: Selector::SelfSource,
            modification: Modification::Restriction(Restriction::NoMaximumHandSize),
            condition: None,
        }]);
    }
    // "You control enchanted creature." (layer 2)
    if let Some(r) = line.strip_prefix("you control ")
        && r.starts_with("enchanted ")
    {
        let (what, plural, rest) = nouns::object(r, cx)?;
        if plural || !rest.is_empty() || !cx.targets.is_empty() {
            return None;
        }
        return Some(vec![AbilityKind::Static {
            what,
            modification: Modification::Control(Selector::You),
            condition: None,
        }]);
    }
    // "You may have this creature assign its combat damage as though it weren't blocked."
    // "For each non-Human creature you control, you may have that creature assign …"
    if face.card_types.iter().any(|t| t.is_permanent()) && cx.targets.is_empty() {
        const UNBLOCKED: &str = " assign its combat damage as though it weren't blocked";
        let what = if line.strip_prefix("you may have ~") == Some(UNBLOCKED) {
            Some(Selector::SelfSource)
        } else if let Some(rest) = line.strip_prefix("for each ")
            && let Some((n, rest)) = nouns::noun(rest, cx)
            && !n.plural
            && n.zone == Zone::Battlefield
            && rest.strip_prefix(", you may have that creature") == Some(UNBLOCKED)
        {
            Some(Selector::All {
                zone: Zone::Battlefield,
                filter: n.filter,
            })
        } else {
            None
        };
        if let Some(what) = what {
            return Some(vec![AbilityKind::Static {
                what,
                modification: Modification::Restriction(Restriction::AssignAsThoughUnblocked),
                condition: None,
            }]);
        }
    }
    // "All creatures able to block this creature do so." (CR 509.1c)
    if let Some(rest) = line.strip_prefix("all ")
        && let Some((blockers, rest)) = nouns::noun(rest, cx)
        && blockers.plural
        && blockers.zone == Zone::Battlefield
        && let Some(rest) = rest.strip_prefix(" able to block ")
        && let Some((what, false, " do so")) = nouns::object(rest, cx)
        && cx.targets.is_empty()
        && face.card_types.iter().any(|t| t.is_permanent())
    {
        return Some(vec![AbilityKind::Static {
            what,
            modification: Modification::Restriction(Restriction::MustBeBlockedByAll(
                blockers.filter,
            )),
            condition: None,
        }]);
    }
    let (what, plural, r) = nouns::object(line, cx)?;
    let plural = plural && !nouns::each_singular(line);
    if !cx.targets.is_empty() {
        return None;
    }
    // A creature's "~ gets +1/+1" is fine; "~" on an instant is not a static subject.
    if matches!(what, Selector::SelfSource) && !face.card_types.iter().any(|t| t.is_permanent()) {
        return None;
    }
    // "Creatures your opponents control enter tapped." — a replacement for other
    // permanents entering (CR 614.1c).
    if r == " enter tapped"
        && plural
        && let Selector::All {
            zone: Zone::Battlefield,
            filter,
        } = &what
    {
        return Some(vec![AbilityKind::ReplacementEffect(Replacement {
            matches: EventPattern::Enters {
                who: filter.clone(),
            },
            kind: ReplacementKind::EntersTapped,
        })]);
    }
    // "can't block unless you control a Vampire", "can't attack if an opponent controls an
    // untapped land": the restriction applies while the condition says so.
    for verb in [" can't attack or block", " can't attack", " can't block"] {
        for (word, negate) in [(" unless ", true), (" if ", false)] {
            // There is no defending player yet when attackers are chosen: "can't attack
            // unless defending player controls an Island" is its own restriction.
            if let Some(after) = r.strip_prefix(verb)
                && let Some(after) = after.strip_prefix(word)
                && !after.starts_with("defending player")
                && let Some((cond, "")) = conditions::condition(after, cx)
                && let Some(mods) = restriction_predicate(verb, cx)
            {
                let condition = if negate {
                    mtg_ir::trigger::Condition::Not(Box::new(cond))
                } else {
                    cond
                };
                return Some(
                    mods.into_iter()
                        .map(|modification| AbilityKind::Static {
                            what: what.clone(),
                            modification,
                            condition: Some(condition.clone()),
                        })
                        .collect(),
                );
            }
        }
    }
    // "This creature gets +1/+1 for each oil counter on it": "it" is the card itself.
    if what == Selector::SelfSource && cx.it.is_none() {
        cx.it = Some(Selector::SelfSource);
    }
    let mods = match restriction_predicate(r, cx) {
        Some(mods) => mods,
        None if r.contains('"') => quoted_grants(r, plural, face, cx)?,
        None => {
            let (mut mods, mut r) = clauses::grants(r, plural, "have", false, cx)?;
            // "for each artifact you control", "for each charge counter on ~".
            if let Some(after) = r.strip_prefix(" for each ") {
                let (by, after) = clauses::per(after, cx)?;
                clauses::scale_mods(&mut mods, by);
                r = after;
            }
            if !r.is_empty() {
                return None;
            }
            mods
        }
    };
    Some(
        mods.into_iter()
            .map(|modification| AbilityKind::Static {
                what: what.clone(),
                modification,
                condition: None,
            })
            .collect(),
    )
}

/// "Prevent all combat damage that would be dealt to this creature.", "Prevent all damage
/// that would be dealt by enchanted creature.", "Prevent all combat damage that would be
/// dealt to and dealt by enchanted creature.", "Prevent all damage that would be dealt to
/// this creature by creatures." (CR 615)
fn static_prevention(line: &str, face: &FaceText, cx: &mut Cx) -> Option<AbilityKind> {
    let (amount, r) = clauses::prevention_head(line, cx)?;
    if amount.is_some() {
        return None;
    }
    let (combat_only, r) = match r.strip_prefix("combat ") {
        Some(r) => (true, r),
        None => (false, r),
    };
    let r = r.strip_prefix("damage that would be dealt ")?;
    let (dealt_to, dealt_by, r) = if let Some(r) = r.strip_prefix("to and dealt by ") {
        (true, true, r)
    } else if let Some(r) = r.strip_prefix("to ") {
        (true, false, r)
    } else {
        (false, true, r.strip_prefix("by ")?)
    };
    let (what, plural, r) = nouns::object(r, cx)?;
    let subject = match &what {
        Selector::SelfSource => face.card_types.iter().any(|t| t.is_permanent()),
        Selector::All { filter, .. } => {
            matches!(filter, ObjectFilter::And(f) if f.first() == Some(&ObjectFilter::AttachedToSelf))
        }
        _ => false,
    };
    if !subject || plural || !cx.targets.is_empty() {
        return None;
    }
    let (from, r) = match r.strip_prefix(" by ") {
        Some(r) if dealt_to && !dealt_by => {
            let (n, r) = nouns::noun(r, cx)?;
            if !n.plural || n.zone != Zone::Battlefield {
                return None;
            }
            (n.filter, r)
        }
        Some(_) => return None,
        None => (ObjectFilter::Any, r),
    };
    if !r.is_empty() {
        return None;
    }
    Some(AbilityKind::Static {
        what,
        modification: Modification::Restriction(Restriction::PreventDamage {
            dealt_to,
            dealt_by,
            combat_only,
            from,
        }),
        condition: None,
    })
}

/// `has "{T}: Add {G}{G}."`, `gets +1/+1 and has "…"`, `has flying and "…"`: abilities
/// granted in quotes (CR 613.1f). Activated and triggered abilities are granted; the engine
/// does not look for granted static abilities, nor for granted "leaves the battlefield"
/// triggers, which would need the grant's last-known information.
fn quoted_grants(r: &str, plural: bool, face: &FaceText, cx: &Cx) -> Option<Vec<Modification>> {
    // Inside the quotes "~" must mean the object that has the ability. Normalising also
    // turns the card's own name into "~", so a card that names itself in quotes is
    // refused rather than guessed at.
    if names_itself_in_quotes(face) {
        return None;
    }
    let (head, quoted) = r.split_at(r.find('"')?);
    let body = quoted.strip_prefix('"')?;
    let end = body.find('"')?;
    // `…, has "Whenever this creature attacks, you gain 1 life," and is a Cleric in
    // addition to its other types`: a type after the quote.
    let mut also = Vec::new();
    let tail = body[end + 1..].trim_end_matches('.');
    if !tail.is_empty() {
        let r = tail
            .strip_prefix(", and is ")
            .or_else(|| tail.strip_prefix(" and is "))?;
        let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
        let (w, r) = words::first_word(r);
        if r != " in addition to its other types" || plural {
            return None;
        }
        also.push(Modification::AddSubtypes(vec![cx.subtype(w)?]));
    }
    let quoted_text = match body[..end].strip_suffix(',') {
        Some(t) => format!("{t}."),
        None => body[..end].to_string(),
    };
    let has = if plural { " have " } else { " has " };
    let mut mods = if head == has {
        Vec::new()
    } else {
        // " gets +1/+1 and has ", " has flying and ", " gets +1/+0 and has haste and ".
        let lead = head
            .strip_suffix(&format!(" and{has}"))
            .or_else(|| head.strip_suffix(&format!(",{has}")))
            .or_else(|| head.strip_suffix(" and "))?;
        let (mods, rest) = clauses::grants(lead, plural, "have", false, cx)?;
        if !rest.is_empty() {
            return None;
        }
        mods
    };
    for a in quoted_abilities(&quoted_text, cx)? {
        if !grantable(&a.kind) {
            return None;
        }
        mods.push(Modification::GrantAbility(Box::new(a)));
    }
    mods.extend(also);
    Some(mods)
}

/// Whether the engine carries out an ability of this kind when it is granted.
pub(crate) fn grantable(kind: &AbilityKind) -> bool {
    match kind {
        AbilityKind::Activated { functions_from, .. } => *functions_from == Zone::Battlefield,
        AbilityKind::Triggered { trigger, .. } => {
            trigger.functions_from == Zone::Battlefield
                && !matches!(
                    trigger.on,
                    EventPattern::Leaves { .. }
                        | EventPattern::Dies { .. }
                        | EventPattern::ZoneChange { .. }
                        | EventPattern::StateIs(_)
                )
        }
        _ => false,
    }
}

/// Whether the card's own name appears inside quotation marks in its text.
fn names_itself_in_quotes(face: &FaceText) -> bool {
    let Some(text) = face.oracle_text else {
        return false;
    };
    !face.name.is_empty()
        && text
            .split('"')
            .skip(1)
            .step_by(2)
            .any(|quoted| quoted.contains(face.name))
}

/// "You may cast creature spells as though they had flash." (CR 702.8d), and a spell's
/// own "You may cast this spell as though it had flash if you control a Human."
fn flash_permission(line: &str, cx: &Cx) -> Option<AbilityKind> {
    let flash = |f| Modification::Restriction(Restriction::FlashFor(f));
    if let Some(r) = line.strip_prefix("you may cast ~ as though it had flash if ") {
        let (cond, "") = conditions::condition(r, cx)? else {
            return None;
        };
        return Some(AbilityKind::Static {
            what: Selector::SelfSource,
            modification: flash(ObjectFilter::IsSelf),
            condition: Some(cond),
        });
    }
    let r = line.strip_prefix("you may cast ")?;
    let what = r.strip_suffix(" as though they had flash")?;
    let filter = if what == "spells" {
        ObjectFilter::Any
    } else {
        let (noun, "") = nouns::noun(what, cx)? else {
            return None;
        };
        if noun.zone != Zone::Stack || !noun.plural {
            return None;
        }
        strip_is_spell(noun.filter).unwrap_or(ObjectFilter::Any)
    };
    Some(AbilityKind::Static {
        what: Selector::SelfSource,
        modification: flash(filter),
        condition: None,
    })
}

/// "Cast this spell only during combat.", "… only during the declare attackers step and
/// only if you've been attacked this step.", "… only if you control a creature." (CR 601.3)
fn cast_only(line: &str, cx: &Cx) -> Option<AbilityKind> {
    use mtg_ir::trigger::Condition as C;
    let r = line.strip_prefix("cast ~ only ")?;
    let steps = |ss: &[Step]| C::Or(ss.iter().map(|s| C::DuringStep(*s)).collect());
    let combat = [
        Step::BeginCombat,
        Step::DeclareAttackers,
        Step::DeclareBlockers,
        Step::FirstStrikeCombatDamage,
        Step::CombatDamage,
        Step::EndCombat,
    ];
    let mut parts = Vec::new();
    for part in r.split(" and only ") {
        let c = match part {
            "during combat" => steps(&combat),
            "during combat on an opponent's turn" => {
                C::And(vec![steps(&combat), C::Not(Box::new(C::YourTurn))])
            }
            "during combat before blockers are declared" => {
                steps(&[Step::BeginCombat, Step::DeclareAttackers])
            }
            "during combat after blockers are declared" => steps(&combat[2..]),
            "during the declare attackers step" => C::DuringStep(Step::DeclareAttackers),
            "during the declare blockers step" => C::DuringStep(Step::DeclareBlockers),
            "before the combat damage step" => steps(&[
                Step::Upkeep,
                Step::Draw,
                Step::PrecombatMain,
                Step::BeginCombat,
                Step::DeclareAttackers,
                Step::DeclareBlockers,
            ]),
            "after combat" => steps(&[Step::PostcombatMain, Step::End, Step::Cleanup]),
            "during your turn" => C::YourTurn,
            "during an opponent's turn" => C::Not(Box::new(C::YourTurn)),
            "if you've been attacked this step" | "if a creature is attacking you" => {
                C::YouAreAttacked
            }
            // Read as it is offered, before this spell counts.
            "if you've cast another spell this turn" => C::ValueAtLeast {
                lhs: Value::SpellsCastThisTurn(Box::new(Selector::You)),
                rhs: Value::ONE,
            },
            _ => {
                let (c, rest) = conditions::condition(part.strip_prefix("if ")?, cx)?;
                if !rest.is_empty() {
                    return None;
                }
                c
            }
        };
        parts.push(c);
    }
    let condition = if parts.len() == 1 {
        parts.pop()?
    } else {
        C::And(parts)
    };
    Some(AbilityKind::CastOnlyIf { condition })
}

/// A spell's cost changing itself: "This spell costs {1} less to cast for each artifact you
/// control.", and affinity (CR 702.41), "affinity for artifacts" — the same with {1}.
fn self_cost(line: &str, cx: &Cx) -> Option<AbilityKind> {
    let (per, counted) = if let Some(what) = line.strip_prefix("affinity for ") {
        (-1, format!("{what} you control"))
    } else {
        let r = line.strip_prefix("~ costs ")?;
        let (mana, r) = mana_cost(r)?;
        // "This spell costs {X} less to cast, where X is the greatest power among creatures
        // you control."
        if let [mtg_core::ManaSymbol::Variable] = mana.symbols.as_slice()
            && let Some(what) = r
                .strip_prefix(" less to cast, where x is ")
                .map(|r| r.trim_end_matches('.'))
            && let Some((count, "")) = clauses::value_phrase(what, cx)
        {
            return Some(AbilityKind::Static {
                what: Selector::SelfSource,
                modification: Modification::Restriction(Restriction::CostModifier {
                    what: ObjectFilter::IsSelf,
                    delta: Value::Negate(Box::new(count)),
                }),
                condition: None,
            });
        }
        let [mtg_core::ManaSymbol::Generic(n)] = mana.symbols.as_slice() else {
            return None;
        };
        let n = i32::from(*n);
        // "This spell costs {2} less to cast if you control a Wizard."
        for (phrase, sign) in [(" less to cast if ", -1), (" more to cast if ", 1)] {
            // "This spell costs {2} less to cast if it targets a tapped creature." — read
            // from the targets chosen as it is cast (CR 601.2f).
            let targets = r
                .strip_prefix(phrase)
                .and_then(|r| r.strip_prefix("it targets "))
                .and_then(|r| r.strip_prefix("a ").or_else(|| r.strip_prefix("an ")))
                .and_then(|r| nouns::noun(r, cx))
                .filter(|(noun, rest)| rest.is_empty() && !noun.plural)
                .map(|(noun, _)| mtg_ir::trigger::Condition::TargetsMatching(noun.filter));
            if let Some(cond) = targets.or_else(|| {
                r.strip_prefix(phrase)
                    .and_then(|r| conditions::condition(r, cx))
                    .filter(|(_, rest)| rest.is_empty())
                    .map(|(cond, _)| cond)
            }) {
                return Some(AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::CostModifier {
                        what: ObjectFilter::IsSelf,
                        delta: Value::Fixed(sign * n),
                    }),
                    condition: Some(cond),
                });
            }
        }
        let (sign, r) = if let Some(r) = r.strip_prefix(" less to cast for each ") {
            (-1, r)
        } else {
            (1, r.strip_prefix(" more to cast for each ")?)
        };
        (sign * n, r.to_string())
    };
    let (count, rest) = clauses::per(&counted, cx)?;
    if !rest.is_empty() {
        return None;
    }
    Some(AbilityKind::Static {
        what: Selector::SelfSource,
        modification: Modification::Restriction(Restriction::CostModifier {
            what: ObjectFilter::IsSelf,
            delta: Value::Product(vec![Value::Fixed(per), count]),
        }),
        condition: None,
    })
}

/// "Creature spells you cast cost {1} less to cast.", "Noncreature spells cost {1} more to
/// cast.", "Spells your opponents cast cost {2} more to cast." (CR 601.2f)
fn cost_static(line: &str, cx: &Cx) -> Option<AbilityKind> {
    let (spells, rest) = line.split_once(" cost ")?;
    let (mana, rest) = mana_cost(rest)?;
    let generic: i32 = mana
        .symbols
        .iter()
        .map(|s| match s {
            mtg_core::ManaSymbol::Generic(n) => i32::from(*n),
            _ => 1000,
        })
        .sum();
    if generic >= 1000 {
        return None;
    }
    let delta = match rest {
        " less to cast" => -generic,
        " more to cast" => generic,
        _ => return None,
    };
    let (who, spells) = if let Some(s) = spells.strip_suffix(" you cast") {
        (Some(Selector::You), s)
    } else if let Some(s) = spells.strip_suffix(" your opponents cast") {
        (Some(Selector::Opponents), s)
    } else {
        (None, spells)
    };
    let (noun, left) = nouns::noun(spells, cx)?;
    if !left.is_empty() || noun.zone != Zone::Stack || !noun.plural {
        return None;
    }
    // Costs are worked out for a card in hand as well as on the stack, so "is a spell" is
    // dropped: what matters is the card's own qualities, and whose it is.
    let mut filter = vec![strip_is_spell(noun.filter).unwrap_or(ObjectFilter::Any)];
    if let Some(who) = who {
        filter.push(ObjectFilter::ControlledBy(Box::new(who)));
    }
    Some(AbilityKind::Static {
        what: Selector::SelfSource,
        modification: Modification::Restriction(Restriction::CostModifier {
            what: ObjectFilter::And(filter),
            delta: Value::Fixed(delta),
        }),
        condition: None,
    })
}

/// Who a "can't cast" line restricts, and the rest of the line.
pub(crate) fn cant_cast_who(line: &str) -> Option<(Selector, &str)> {
    [
        ("you", Selector::You),
        ("your opponents", Selector::Opponents),
        ("each opponent", Selector::Opponents),
        ("players", Selector::EachPlayer),
        ("each player", Selector::EachPlayer),
        ("enchanted player", Selector::EnchantedPlayer),
    ]
    .into_iter()
    .find_map(|(lead, who)| {
        line.strip_prefix(lead)
            .filter(|r| r.starts_with(" can't cast "))
            .map(|r| (who, r))
    })
}

/// "creature spells", "spells", "more than one spell each turn": what can't be cast.
pub(crate) fn cant_cast(who: Selector, what: &str, cx: &Cx) -> Option<Modification> {
    let (spells, beyond) = if what == "more than one spell each turn" {
        (ObjectFilter::Any, Some(1))
    } else if what == "spells" {
        (ObjectFilter::Any, None)
    } else {
        let (noun, rest) = nouns::noun(what, cx)?;
        if !rest.is_empty() || noun.zone != Zone::Stack || !noun.plural {
            return None;
        }
        (
            strip_is_spell(noun.filter).unwrap_or(ObjectFilter::Any),
            None,
        )
    };
    Some(Modification::Restriction(Restriction::CantCast {
        who,
        spells,
        beyond,
    }))
}

/// A filter without its "is a spell" parts, at any depth ("instant and sorcery spells"
/// is an `Or` of two spell filters). `None` when nothing else was left.
fn strip_is_spell(f: ObjectFilter) -> Option<ObjectFilter> {
    match f {
        ObjectFilter::IsSpell => None,
        ObjectFilter::And(parts) => Some(ObjectFilter::And(
            parts.into_iter().filter_map(strip_is_spell).collect(),
        )),
        ObjectFilter::Or(parts) => Some(ObjectFilter::Or(
            parts
                .into_iter()
                .map(|p| strip_is_spell(p).unwrap_or(ObjectFilter::Any))
                .collect(),
        )),
        other => Some(other),
    }
}

/// The restrictions a static ability can state about its subject, as the whole rest of
/// the line: "can't block", "can't be blocked", "attacks each combat if able", …
fn restriction_predicate(r: &str, cx: &Cx) -> Option<Vec<Modification>> {
    use Restriction as R;
    let one = |r: R| Some(vec![Modification::Restriction(r)]);
    // "can't attack unless defending player controls an Island"
    if let Some(n) = r
        .strip_prefix(" can't attack unless defending player controls a ")
        .or_else(|| r.strip_prefix(" can't attack unless defending player controls an "))
    {
        let (noun, rest) = nouns::noun(n, cx)?;
        if !rest.is_empty() || noun.plural || noun.zone != Zone::Battlefield {
            return None;
        }
        return one(R::CantAttackUnlessDefenderControls(noun.filter));
    }
    // "can block an additional seven creatures each combat"
    if let Some(n) = r.strip_prefix(" can block an additional ")
        && let Some((n, " creatures each combat")) = words::number(n)
        && let Ok(n) = u8::try_from(n)
    {
        return one(R::BlockAdditional(Some(n)));
    }
    match r {
        " can't be blocked by more than one creature" => {
            return one(R::CantBeBlockedByMoreThanOne);
        }
        " can't block" => return one(R::CantBlock),
        " can attack as though it didn't have defender"
        | " can attack as though they didn't have defender" => {
            return one(R::AttackDespiteDefender);
        }
        " assigns combat damage equal to its toughness rather than its power"
        | " assign combat damage equal to their toughness rather than their power" => {
            return one(R::AssignDamageByToughness);
        }
        " can't block and can't be blocked" => {
            return Some(vec![
                Modification::Restriction(R::CantBlock),
                Modification::Restriction(R::CantBeBlockedExceptBy(ObjectFilter::Not(Box::new(
                    ObjectFilter::Any,
                )))),
            ]);
        }
        " can't attack alone" => return one(R::CantAttackAlone),
        " can't block alone" => return one(R::CantBlockAlone),
        " can block an additional creature each combat" => {
            return one(R::BlockAdditional(Some(1)));
        }
        " can block any number of creatures" => return one(R::BlockAdditional(None)),
        " can't attack or block alone" => {
            return Some(vec![
                Modification::Restriction(R::CantAttackAlone),
                Modification::Restriction(R::CantBlockAlone),
            ]);
        }
        " can't be blocked except by three or more creatures" => {
            return one(R::MinimumBlockers(3));
        }
        " can't attack" => return one(R::CantAttack),
        " can't attack or block" => {
            return Some(vec![
                Modification::Restriction(R::CantAttack),
                Modification::Restriction(R::CantBlock),
            ]);
        }
        " can't attack or block, and its activated abilities can't be activated"
        | " can't attack or block and its activated abilities can't be activated" => {
            return Some(vec![
                Modification::Restriction(R::CantAttack),
                Modification::Restriction(R::CantBlock),
                Modification::Restriction(R::CantActivateAbilities),
            ]);
        }
        "'s activated abilities can't be activated" => {
            return one(R::CantActivateAbilities);
        }
        " can't be blocked" => {
            return one(R::CantBeBlockedExceptBy(ObjectFilter::Not(Box::new(
                ObjectFilter::Any,
            ))));
        }
        " attacks each combat if able" | " attack each combat if able" => {
            return one(R::MustAttackIfAble);
        }
        " must be blocked if able" => return one(R::MustBeBlocked),
        " blocks each combat if able" | " block each combat if able" => {
            return one(R::MustBlock);
        }
        " attacks or blocks each combat if able" | " attack or block each combat if able" => {
            return Some(vec![
                Modification::Restriction(R::MustAttackIfAble),
                Modification::Restriction(R::MustBlock),
            ]);
        }
        " is goaded" | " are goaded" => return one(R::Goaded),
        " doesn't untap during its controller's untap step and its activated abilities can't be activated"
        | " doesn't untap during its controller's untap step, and its activated abilities can't be activated" =>
        {
            return Some(vec![
                Modification::Restriction(R::CantUntapDuringUntapStep),
                Modification::Restriction(R::CantActivateAbilities),
            ]);
        }
        " doesn't untap during its controller's untap step"
        | " doesn't untap during your untap step"
        | " don't untap during their controllers' untap steps" => {
            return one(R::CantUntapDuringUntapStep);
        }
        _ => {}
    }
    // "can't block creatures with power 2 or greater", "can't block black creatures":
    // it may block only the others.
    if let Some(rest) = r.strip_prefix(" can't block ")
        && let Some((n, "")) = nouns::noun(rest, cx)
        && n.plural
    {
        return one(R::CanBlockOnly(ObjectFilter::Not(Box::new(n.filter))));
    }
    // "can block only creatures with flying", "can't be blocked except by Walls",
    // "can't be blocked by creatures with power 2 or less".
    if let Some(rest) = r.strip_prefix(" can block only ") {
        let (n, rest) = nouns::noun(rest, cx)?;
        return (rest.is_empty() && n.plural)
            .then(|| vec![Modification::Restriction(R::CanBlockOnly(n.filter))]);
    }
    if let Some(rest) = r.strip_prefix(" can't be blocked except by ") {
        let (n, rest) = nouns::noun(rest, cx)?;
        return (rest.is_empty() && n.plural).then(|| {
            vec![Modification::Restriction(R::CantBeBlockedExceptBy(
                n.filter,
            ))]
        });
    }
    if let Some(rest) = r.strip_prefix(" can't be blocked by ") {
        let (n, rest) = nouns::noun(rest, cx)?;
        return (rest.is_empty() && n.plural).then(|| {
            vec![Modification::Restriction(R::CantBeBlockedExceptBy(
                ObjectFilter::Not(Box::new(n.filter)),
            ))]
        });
    }
    None
}

/// Keywords that are really triggered abilities, written out (CR 702.108 prowess,
/// 702.83 exalted, 702.45 bushido).
/// Keywords that ask a question as the creature enters: unleash (CR 702.98) — a +1/+1
/// counter if you like, and it can't block while it has one — and riot (CR 702.136) — a
/// +1/+1 counter or haste.
fn enter_choice_keyword(
    line: &str,
    face: &FaceText,
) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    if !face.card_types.contains(&CardType::Creature) {
        return None;
    }
    let enters = |kind| {
        (
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind,
            }),
            Vec::new(),
        )
    };
    match line.trim_end_matches('.') {
        "riot" => Some(vec![enters(ReplacementKind::EntersWithCounterOrHaste)]),
        "unleash" => Some(vec![
            enters(ReplacementKind::EntersWithCounterIfChosen),
            (
                AbilityKind::Static {
                    what: Selector::SelfSource,
                    modification: Modification::Restriction(Restriction::CantBlock),
                    condition: Some(mtg_ir::trigger::Condition::ValueAtLeast {
                        lhs: Value::Counters(
                            Box::new(Selector::SelfSource),
                            mtg_core::CounterKind::PlusOnePlusOne,
                        ),
                        rhs: Value::ONE,
                    }),
                },
                Vec::new(),
            ),
        ]),
        _ => None,
    }
}

/// Alternative costs any spell may have under a condition: "surge {1}{U}" (CR 702.117),
/// "spectacle {R}" (CR 702.137). The condition is the cost's timing.
fn conditional_alternative_cost(line: &str) -> Option<AbilityKind> {
    use mtg_ir::ability::AltCost;
    use mtg_ir::trigger::Condition as C;
    let line = line.trim_end_matches('.');
    let (kind, condition, r) = if let Some(r) = line.strip_prefix("surge ") {
        (
            AltCost::Surge,
            C::ValueAtLeast {
                lhs: Value::SpellsCastThisTurn(Box::new(Selector::You)),
                rhs: Value::ONE,
            },
            r,
        )
    } else {
        let r = line.strip_prefix("spectacle ")?;
        (AltCost::Spectacle, C::OpponentLostLifeThisTurn, r)
    };
    let (mana, "") = mana_cost(r)? else {
        return None;
    };
    Some(AbilityKind::AlternativeCost {
        cost: Cost {
            mana,
            timing: vec![condition],
            ..Cost::free()
        },
        kind,
        instead: None,
    })
}

/// CR 118.9 — "You may pay 1 life and exile a blue card from your hand rather than pay this
/// spell's mana cost.", "If you control a Plains, you may tap an untapped creature you
/// control rather than pay this spell's mana cost."
fn plain_alternative_cost(line: &str, cx: &Cx) -> Option<AbilityKind> {
    let (condition, r) = match line.strip_prefix("if ") {
        Some(r) => {
            let (c, r) = conditions::condition(r, cx)?;
            (Some(c), r.strip_prefix(", ")?)
        }
        None => (None, line),
    };
    let r = r
        .strip_prefix("you may ")?
        .strip_suffix(" rather than pay ~'s mana cost.")?;
    // "pay {1}{U} and tap an untapped artifact you control": the parts, then the mana.
    let mut mana = None;
    let mut rest = Vec::new();
    for part in r.split(" and ") {
        match part.strip_prefix("pay ").and_then(mana_cost) {
            Some((m, "")) if mana.is_none() => mana = Some(m),
            _ => rest.push(part),
        }
    }
    let mut cost = if rest.is_empty() {
        Cost::free()
    } else if rest.len() == 1 && rest[0].starts_with("return ") {
        Cost {
            additional: vec![return_hand_cost(rest[0], cx)?],
            ..Cost::free()
        }
    } else {
        cost(&rest.join(", "), cx)?
    };
    if !cost.mana.symbols.is_empty()
        || !cost.additional.iter().all(|p| {
            matches!(
                p,
                AdditionalCost::PayLife { .. }
                    | AdditionalCost::ReturnToHand { .. }
                    | AdditionalCost::Sacrifice {
                        what: Selector::All { .. },
                        ..
                    }
                    | AdditionalCost::TapUntapped { .. }
                    | AdditionalCost::ExileFrom {
                        zone: Zone::Hand,
                        ..
                    }
            )
        })
    {
        return None;
    }
    cost.mana = mana.unwrap_or_default();
    cost.timing.extend(condition);
    Some(AbilityKind::AlternativeCost {
        cost,
        kind: mtg_ir::ability::AltCost::Pay,
        instead: None,
    })
}

/// A non-mana alternative cost that returns permanents to their owners' hands.
fn return_hand_cost(part: &str, cx: &Cx) -> Option<AdditionalCost> {
    let r = part.strip_prefix("return ")?;
    let (n, r) = words::number(r)?;
    let (noun, r) = nouns::noun(r.strip_prefix(' ')?, cx)?;
    let owner = if n == 1 {
        " to its owner's hand"
    } else {
        " to their owner's hand"
    };
    if n < 1
        || noun.zone != Zone::Battlefield
        || noun.plural != (n > 1)
        || (r != owner && r != " to their owners' hands")
        || !part.contains(" you control")
    {
        return None;
    }
    Some(AdditionalCost::ReturnToHand {
        filter: noun.filter,
        count: Value::Fixed(n),
    })
}

/// Alternative costs of a permanent card: "dash {2}{R}" (CR 702.109), and "evoke {1}{U}"
/// (CR 702.74) with its "when it enters, if it was evoked, sacrifice it".
fn alternative_cost(line: &str, face: &FaceText) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    use mtg_ir::ability::AltCost;
    if !face.card_types.iter().any(|t| t.is_permanent()) {
        return None;
    }
    let line = line.trim_end_matches('.');
    let (kind, r) = if let Some(r) = line.strip_prefix("dash ") {
        (AltCost::Dash, r)
    } else if let Some(r) = line
        .strip_prefix("blitz ")
        .filter(|_| face.card_types.contains(&CardType::Creature))
    {
        (AltCost::Blitz, r)
    } else if let Some(r) = line.strip_prefix("bestow ") {
        (AltCost::Bestow, r)
    } else if let Some(r) = line.strip_prefix("warp ") {
        (AltCost::Warp, r)
    } else {
        (AltCost::Evoke, line.strip_prefix("evoke ")?)
    };
    let (mana, "") = mana_cost(r)? else {
        return None;
    };
    let mut out = vec![(
        AbilityKind::AlternativeCost {
            cost: Cost {
                mana,
                ..Cost::free()
            },
            kind,
            instead: None,
        },
        Vec::new(),
    )];
    // A bestowed Aura enchants a creature.
    if kind == AltCost::Bestow {
        if !face.card_types.contains(&CardType::Creature) {
            return None;
        }
        out[0].1.push(TargetSpec {
            zone: Zone::Battlefield,
            filter: ObjectFilter::HasType(CardType::Creature),
            allows_players: false,
            players: None,
            mode: None,
            count: Value::ONE,
            up_to: false,
            distinct_from_other_targets: false,
        });
    }
    if kind == AltCost::Evoke {
        out.push((
            AbilityKind::Triggered {
                trigger: Trigger {
                    on: EventPattern::Enters {
                        who: ObjectFilter::IsSelf,
                    },
                    functions_from: Zone::Battlefield,
                    intervening_if: Some(mtg_ir::trigger::Condition::CastFor(AltCost::Evoke)),
                    optional: false,
                    limit: None,
                    timing: TriggerTiming::Normal,
                },
                effect: Effect::Sacrifice {
                    who: Selector::You,
                    what: Selector::SelfSource,
                },
            },
            Vec::new(),
        ));
    }
    Some(out)
}

/// CR 702.62 — "suspend 4—{1}{R}": the special action that exiles it with time counters,
/// the upkeep trigger that removes them while it is suspended, and the trigger that casts
/// it without paying its mana cost when the last is removed (a creature gains haste).
fn suspend(line: &str, face: &FaceText) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    let r = line.trim_end_matches('.').strip_prefix("suspend ")?;
    let (n, r) = words::number(r)?;
    let (mana, "") = mana_cost(r.strip_prefix('—')?)? else {
        return None;
    };
    let (time, _) = words::counter("time counter")?;
    let me = || ObjectFilter::IsSelf;
    let in_exile = |on, intervening_if, effect| AbilityKind::Triggered {
        trigger: Trigger {
            on,
            functions_from: Zone::Exile,
            intervening_if,
            optional: false,
            limit: None,
            timing: TriggerTiming::Normal,
        },
        effect,
    };
    let special = AbilityKind::Suspend {
        time: u32::try_from(n).ok()?,
        cost: Cost {
            mana,
            ..Cost::free()
        },
        counter: time,
    };
    let upkeep = in_exile(
        EventPattern::StepBegins {
            step: Step::Upkeep,
            whose: Selector::You,
        },
        Some(mtg_ir::trigger::Condition::ValueAtLeast {
            lhs: Value::Counters(Box::new(Selector::SelfSource), time),
            rhs: Value::ONE,
        }),
        Effect::RemoveCounters {
            what: Selector::SelfSource,
            kind: time,
            amount: Value::ONE,
        },
    );
    let cast = in_exile(
        EventPattern::LastCounterRemoved {
            from: me(),
            kind: time,
        },
        None,
        Effect::CastWithoutPaying {
            what: Selector::SelfSource,
            from: Zone::Exile,
            haste: face.card_types.contains(&CardType::Creature),
        },
    );
    Some(vec![
        (special, Vec::new()),
        (upkeep, Vec::new()),
        (cast, Vec::new()),
    ])
}

/// Add renown's intervening "if it isn't renowned" (CR 603.4) to its trigger.
fn not_renowned(mut kind: AbilityKind) -> AbilityKind {
    if let AbilityKind::Triggered { trigger, .. } = &mut kind {
        trigger.intervening_if = Some(mtg_ir::trigger::Condition::Not(Box::new(
            mtg_ir::trigger::Condition::Renowned,
        )));
    }
    kind
}

/// The lore counter (CR 714.1).
fn lore() -> Option<mtg_core::CounterKind> {
    words::counter("lore counter").map(|(k, _)| k)
}

/// A Saga's chapter ability (CR 714.2): "I — …", "II, III — …". One trigger per chapter
/// number, each when lore counters reach it.
fn chapter(
    line: &str,
    face: &FaceText,
    cx: &mut Cx,
) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    if !face.subtypes.iter().any(|s| s == "Saga") {
        return None;
    }
    let (numerals, body) = line.split_once(" — ")?;
    let chapters: Vec<u8> = numerals
        .split(", ")
        .map(|n| match n {
            "i" => Some(1),
            "ii" => Some(2),
            "iii" => Some(3),
            "iv" => Some(4),
            "v" => Some(5),
            "vi" => Some(6),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let lore = lore()?;
    let mut out = Vec::new();
    for c in chapters {
        let mut inner = cx.fresh();
        let effect = clauses::effect(body, &mut inner)?;
        out.push((
            AbilityKind::Triggered {
                trigger: Trigger {
                    on: EventPattern::ChapterReached { lore, chapter: c },
                    functions_from: Zone::Battlefield,
                    intervening_if: None,
                    optional: false,
                    limit: None,
                    timing: TriggerTiming::Normal,
                },
                effect,
            },
            inner.targets,
        ));
    }
    Some(out)
}

/// CR 702.134 — mentor: whenever this creature attacks, put a +1/+1 counter on target
/// attacking creature with lesser power.
fn mentor(line: &str) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    if line.trim_end_matches('.') != "mentor" {
        return None;
    }
    let target = TargetSpec {
        zone: Zone::Battlefield,
        filter: ObjectFilter::And(vec![
            ObjectFilter::HasType(CardType::Creature),
            ObjectFilter::Attacking,
            ObjectFilter::PowerAtMost(Value::Sum(vec![
                Value::Power(Box::new(Selector::SelfSource)),
                Value::Fixed(-1),
            ])),
        ]),
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    };
    Some(vec![(
        AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Attacks {
                    who: ObjectFilter::IsSelf,
                },
                functions_from: Zone::Battlefield,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::AddCounters {
                what: Selector::Target { index: 0 },
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                amount: Value::ONE,
            },
        },
        vec![target],
    )])
}

/// Keywords built on counters, each a replacement plus a trigger:
/// - modular N (CR 702.43): enters with N +1/+1 counters; when it dies, you may put its
///   +1/+1 counters on target artifact creature.
/// - fading N (CR 702.32): enters with N fade counters; at the beginning of your upkeep,
///   remove one, and if you can't, sacrifice it.
fn counter_keyword(line: &str) -> Option<Vec<(AbilityKind, Vec<TargetSpec>)>> {
    let line = line.trim_end_matches('.');
    let (word, n) = line.split_once(' ')?;
    let (n, "") = words::number(n)? else {
        return None;
    };
    let enters_with = |kind| {
        AbilityKind::ReplacementEffect(Replacement {
            matches: EventPattern::Enters {
                who: ObjectFilter::IsSelf,
            },
            kind: ReplacementKind::EntersWithCounters {
                kind,
                amount: Value::Fixed(n),
                condition: None,
            },
        })
    };
    let trig = |on, timing, optional, effect| AbilityKind::Triggered {
        trigger: Trigger {
            on,
            functions_from: Zone::Battlefield,
            intervening_if: None,
            optional,
            limit: None,
            timing,
        },
        effect,
    };
    match word {
        "modular" => {
            let plus = mtg_core::CounterKind::PlusOnePlusOne;
            let target = TargetSpec {
                zone: Zone::Battlefield,
                filter: ObjectFilter::And(vec![
                    ObjectFilter::HasType(CardType::Artifact),
                    ObjectFilter::HasType(CardType::Creature),
                ]),
                allows_players: false,
                players: None,
                mode: None,
                count: Value::ONE,
                up_to: false,
                distinct_from_other_targets: false,
            };
            // "Its" counters: the creature as it last existed, which the dies trigger
            // binds as the event's other party.
            let dies = trig(
                EventPattern::Dies {
                    who: ObjectFilter::IsSelf,
                },
                TriggerTiming::LeavesBattlefield,
                true,
                Effect::AddCounters {
                    what: Selector::Target { index: 0 },
                    kind: plus,
                    amount: Value::Counters(
                        Box::new(Selector::Bound(mtg_ir::selector::Binding::EventOther)),
                        plus,
                    ),
                },
            );
            Some(vec![(enters_with(plus), Vec::new()), (dies, vec![target])])
        }
        // CR 702.112 — "renown N": when it deals combat damage to a player, if it isn't
        // renowned, put N +1/+1 counters on it and it becomes renowned.
        "renown" => Some(vec![(
            not_renowned(trig(
                EventPattern::DealsDamage {
                    source: ObjectFilter::IsSelf,
                    to: mtg_ir::trigger::DamageRecipient::Player(Selector::EachPlayer),
                    combat_only: true,
                },
                TriggerTiming::Normal,
                false,
                Effect::Sequence(vec![
                    Effect::AddCounters {
                        what: Selector::SelfSource,
                        kind: mtg_core::CounterKind::PlusOnePlusOne,
                        amount: Value::Fixed(n),
                    },
                    Effect::BecomeRenowned {
                        what: Selector::SelfSource,
                    },
                ]),
            )),
            Vec::new(),
        )]),
        // CR 702.54 — "bloodthirst N": if an opponent was dealt damage this turn, this
        // creature enters with N +1/+1 counters.
        "bloodthirst" => Some(vec![(
            AbilityKind::ReplacementEffect(Replacement {
                matches: EventPattern::Enters {
                    who: ObjectFilter::IsSelf,
                },
                kind: ReplacementKind::EntersWithCounters {
                    kind: mtg_core::CounterKind::PlusOnePlusOne,
                    amount: Value::Fixed(n),
                    condition: Some(mtg_ir::trigger::Condition::OpponentDamagedThisTurn),
                },
            }),
            Vec::new(),
        )]),
        // CR 702.63 — "vanishing N": enters with N time counters; at the beginning of your
        // upkeep, remove one; when the last is removed, sacrifice it.
        "vanishing" => {
            let (time, _) = words::counter("time counter")?;
            let upkeep = trig(
                EventPattern::StepBegins {
                    step: Step::Upkeep,
                    whose: Selector::You,
                },
                TriggerTiming::Normal,
                false,
                Effect::If {
                    cond: mtg_ir::trigger::Condition::ValueAtLeast {
                        lhs: Value::Counters(Box::new(Selector::SelfSource), time),
                        rhs: Value::ONE,
                    },
                    then: Box::new(Effect::RemoveCounters {
                        what: Selector::SelfSource,
                        kind: time,
                        amount: Value::ONE,
                    }),
                    otherwise: Box::new(Effect::Nothing),
                },
            );
            let last = trig(
                EventPattern::LastCounterRemoved {
                    from: ObjectFilter::IsSelf,
                    kind: time,
                },
                TriggerTiming::Normal,
                false,
                Effect::Sacrifice {
                    who: Selector::You,
                    what: Selector::SelfSource,
                },
            );
            Some(vec![
                (enters_with(time), Vec::new()),
                (upkeep, Vec::new()),
                (last, Vec::new()),
            ])
        }
        // CR 702.58 — "graft N": enters with N +1/+1 counters; whenever another creature
        // enters, you may move a +1/+1 counter from this onto it.
        "graft" => {
            let plus = mtg_core::CounterKind::PlusOnePlusOne;
            let mut moving = trig(
                EventPattern::Enters {
                    who: ObjectFilter::And(vec![
                        ObjectFilter::HasType(CardType::Creature),
                        ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
                    ]),
                },
                TriggerTiming::Normal,
                true,
                Effect::Sequence(vec![
                    Effect::RemoveCounters {
                        what: Selector::SelfSource,
                        kind: plus,
                        amount: Value::ONE,
                    },
                    Effect::AddCounters {
                        what: Selector::Bound(mtg_ir::selector::Binding::EventSubject),
                        kind: plus,
                        amount: Value::ONE,
                    },
                ]),
            );
            // Only while it has a counter to move (CR 702.58a).
            if let AbilityKind::Triggered { trigger, .. } = &mut moving {
                trigger.intervening_if = Some(mtg_ir::trigger::Condition::ValueAtLeast {
                    lhs: Value::Counters(Box::new(Selector::SelfSource), plus),
                    rhs: Value::ONE,
                });
            }
            Some(vec![(enters_with(plus), Vec::new()), (moving, Vec::new())])
        }
        "fading" => {
            let (fade, _) = words::counter("fade counter")?;
            let upkeep = trig(
                EventPattern::StepBegins {
                    step: Step::Upkeep,
                    whose: Selector::You,
                },
                TriggerTiming::Normal,
                false,
                Effect::If {
                    cond: mtg_ir::trigger::Condition::ValueAtLeast {
                        lhs: Value::Counters(Box::new(Selector::SelfSource), fade),
                        rhs: Value::ONE,
                    },
                    then: Box::new(Effect::RemoveCounters {
                        what: Selector::SelfSource,
                        kind: fade,
                        amount: Value::ONE,
                    }),
                    otherwise: Box::new(Effect::Sacrifice {
                        who: Selector::You,
                        what: Selector::SelfSource,
                    }),
                },
            );
            Some(vec![(enters_with(fade), Vec::new()), (upkeep, Vec::new())])
        }
        _ => None,
    }
}

/// CR 702.123 — "fabricate N": when this creature enters, put N +1/+1 counters on it or
/// create N 1/1 Servos. Not a modal ability: the choice is made as it resolves (so the
/// `Modal` is nested rather than at the top, where it would be chosen on the stack), and
/// if the creature has left the battlefield only the Servos can be made.
fn fabricate(line: &str, cx: &mut Cx) -> Option<AbilityKind> {
    let (n, "") = words::number(line.trim_end_matches('.').strip_prefix("fabricate ")?)? else {
        return None;
    };
    let counters = clauses::effect(
        &if n == 1 {
            "put a +1/+1 counter on ~.".to_string()
        } else {
            format!("put {n} +1/+1 counters on ~.")
        },
        cx,
    )?;
    let servos = clauses::effect(
        &if n == 1 {
            "create a 1/1 colorless servo artifact creature token.".to_string()
        } else {
            format!("create {n} 1/1 colorless servo artifact creature tokens.")
        },
        cx,
    )?;
    let here = mtg_ir::trigger::Condition::Exists(Selector::All {
        zone: Zone::Battlefield,
        filter: ObjectFilter::IsSelf,
    });
    Some(AbilityKind::Triggered {
        trigger: Trigger {
            on: EventPattern::Enters {
                who: ObjectFilter::IsSelf,
            },
            functions_from: Zone::Battlefield,
            intervening_if: None,
            optional: false,
            limit: None,
            timing: TriggerTiming::Normal,
        },
        effect: Effect::If {
            cond: here,
            then: Box::new(Effect::Sequence(vec![Effect::Modal {
                at_least: None,
                choose: Value::ONE,
                modes: vec![
                    ("+1/+1 counters".into(), counters),
                    ("Servo tokens".into(), servos.clone()),
                ],
            }])),
            otherwise: Box::new(servos),
        },
    })
}

/// A keyword that is shorthand for rules text the grammar already reads, expanded to its
/// reminder text (normalised: lowercase, "~" for the card). The expansion is compiled like
/// any other line, under the same all-or-nothing rule.
fn keyword_expansion(line: &str) -> Option<Vec<String>> {
    let line = line.trim_end_matches('.');
    // "soulshift 3": a keyword and its number; "living weapon": all keyword.
    let (word, n) = match line
        .rsplit_once(' ')
        .and_then(|(w, n)| Some((w, words::number(n).filter(|(_, r)| r.is_empty())?.0)))
    {
        Some((w, n)) => (w, Some(n)),
        None => (line, None),
    };
    Some(match (word, n) {
        // CR 702.92
        ("living weapon", None) => vec![
            "when ~ enters, create a 0/0 black phyrexian germ creature token, then attach ~ \
             to it."
                .into(),
        ],
        // CR 702.182a — job select
        ("job select", None) => vec![
            "when ~ enters, create a 1/1 colorless hero creature token, then attach ~ to it."
                .into(),
        ],
        // CR 702.161a
        ("living metal", None) => vec!["during your turn, ~ is an artifact creature.".into()],
        // CR 702.163a
        ("for mirrodin!", None) => vec![
            "when ~ enters, create a 2/2 red rebel creature token, then attach ~ to it.".into(),
        ],
        // CR 702.107
        _ if word.starts_with("outlast ") && n.is_none() => {
            let cost = word.strip_prefix("outlast ")?;
            mana_cost(cost).filter(|(_, r)| r.is_empty())?;
            vec![format!(
                "{cost}, {{t}}: put a +1/+1 counter on ~. activate only as a sorcery."
            )]
        }
        // CR 702.135
        ("afterlife", Some(1)) => vec![
            "when ~ dies, create a 1/1 white and black spirit creature token with flying.".into(),
        ],
        ("afterlife", Some(n)) => vec![format!(
            "when ~ dies, create {n} 1/1 white and black spirit creature tokens with flying."
        )],
        // CR 702.91a
        ("battle cry", None) => vec![
            "whenever ~ attacks, each other attacking creature gets +1/+0 until end of turn."
                .into(),
        ],
        // CR 702.115a
        ("ingest", None) => vec![
            "whenever ~ deals combat damage to a player, that player exiles the top card of \
             their library."
                .into(),
        ],
        // CR 702.86a
        ("annihilator", Some(1)) => {
            vec!["whenever ~ attacks, defending player sacrifices a permanent.".into()]
        }
        ("annihilator", Some(n)) => vec![format!(
            "whenever ~ attacks, defending player sacrifices {n} permanents."
        )],
        // CR 702.181a
        ("mobilize", Some(1)) => vec![
            "whenever ~ attacks, create a tapped and attacking 1/1 red warrior creature \
             token. sacrifice it at the beginning of the next end step."
                .into(),
        ],
        ("mobilize", Some(n)) => vec![format!(
            "whenever ~ attacks, create {n} tapped and attacking 1/1 red warrior creature \
             tokens. sacrifice them at the beginning of the next end step."
        )],
        // CR 702.130a
        ("afflict", Some(n)) => vec![format!(
            "whenever ~ becomes blocked, defending player loses {n} life."
        )],
        // CR 702.23
        ("rampage", Some(n)) => vec![format!(
            "whenever ~ becomes blocked, it gets +{n}/+{n} until end of turn for each \
             creature blocking it beyond the first."
        )],
        // CR 702.46
        ("soulshift", Some(n)) => vec![format!(
            "when ~ dies, you may return target spirit card with mana value {n} or less \
             from your graveyard to your hand."
        )],
        _ => return None,
    })
}

fn keyword_trigger(line: &str) -> Option<AbilityKind> {
    let me = || ObjectFilter::IsSelf;
    let pump = |n: i32, what: Selector| Effect::Continuous {
        what,
        modification: Modification::ModifyPowerToughness {
            power: Value::Fixed(n),
            toughness: Value::Fixed(n),
        },
        duration: mtg_ir::effect::Duration::UntilEndOfTurn,
    };
    let trig = |on, intervening_if, effect| AbilityKind::Triggered {
        trigger: Trigger {
            on,
            functions_from: Zone::Battlefield,
            intervening_if,
            optional: false,
            limit: None,
            timing: TriggerTiming::Normal,
        },
        effect,
    };
    let line = line.trim_end_matches('.');
    // CR 702.40 — storm: when you cast this spell, copy it for each spell cast before it
    // this turn; you may choose new targets for the copies.
    // CR 702.35 — "madness {1}{R}": discarded, it is exiled instead, and its owner may
    // cast it for the madness cost; if not, it goes to the graveyard.
    if let Some(r) = line.strip_prefix("madness ")
        && let Some((cost, "")) = mana_cost(r)
    {
        return Some(AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::ExiledForMadness { who: me() },
                functions_from: Zone::Exile,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::Madness { cost },
        });
    }
    // CR 702.85 — cascade, like storm a trigger of the spell as it is cast.
    // CR 702.105 — dethrone.
    if line == "dethrone" {
        return Some(trig(
            EventPattern::AttacksMostLife { who: me() },
            None,
            Effect::AddCounters {
                what: Selector::SelfSource,
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                amount: Value::ONE,
            },
        ));
    }
    if line == "cascade" {
        return Some(AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Cast {
                    who: me(),
                    by: Selector::You,
                },
                functions_from: Zone::Stack,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::Cascade,
        });
    }
    if line == "storm" {
        return Some(AbilityKind::Triggered {
            trigger: Trigger {
                on: EventPattern::Cast {
                    who: me(),
                    by: Selector::You,
                },
                functions_from: Zone::Stack,
                intervening_if: None,
                optional: false,
                limit: None,
                timing: TriggerTiming::Normal,
            },
            effect: Effect::Repeat {
                times: Value::SpellsCastBefore,
                body: Box::new(Effect::CopySpell {
                    what: Selector::SelfSource,
                    may_change_targets: true,
                }),
            },
        });
    }
    // CR 702.31 — "echo {2}{R}": at the beginning of your upkeep, if this came under your
    // control since your last upkeep, sacrifice it unless you pay the echo cost.
    if let Some(r) = line.strip_prefix("echo ")
        && let Some((mana, "")) = mana_cost(r)
    {
        return Some(trig(
            EventPattern::StepBegins {
                step: Step::Upkeep,
                whose: Selector::You,
            },
            Some(mtg_ir::trigger::Condition::ControlledSinceLastUpkeep),
            Effect::UnlessPays {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                times: Value::ONE,
                otherwise: Box::new(Effect::Sacrifice {
                    who: Selector::You,
                    what: Selector::SelfSource,
                }),
            },
        ));
    }
    // CR 702.24 — "cumulative upkeep {1}": at the beginning of your upkeep, put an age
    // counter on it, then sacrifice it unless you pay the cost for each age counter.
    let cumulative = line
        .strip_prefix("cumulative upkeep ")
        .and_then(|r| match mana_cost(r) {
            Some((mana, "")) => Some(Cost {
                mana,
                ..Cost::free()
            }),
            _ => None,
        })
        .or_else(|| {
            // "cumulative upkeep—pay 1 life."
            let r = line.strip_prefix("cumulative upkeep—pay ")?;
            let (n, r) = words::number(r)?;
            (r.trim_end_matches('.') == " life" && n > 0).then(|| Cost {
                additional: vec![AdditionalCost::PayLife {
                    amount: Value::Fixed(n),
                }],
                ..Cost::free()
            })
        });
    if let Some(cost) = cumulative {
        let (age, _) = words::counter("age counter")?;
        return Some(trig(
            EventPattern::StepBegins {
                step: Step::Upkeep,
                whose: Selector::You,
            },
            None,
            Effect::Sequence(vec![
                Effect::AddCounters {
                    what: Selector::SelfSource,
                    kind: age,
                    amount: Value::ONE,
                },
                Effect::UnlessPays {
                    cost,
                    times: Value::Counters(Box::new(Selector::SelfSource), age),
                    otherwise: Box::new(Effect::Sacrifice {
                        who: Selector::You,
                        what: Selector::SelfSource,
                    }),
                },
            ]),
        ));
    }
    // CR 702.100 — evolve: whenever a creature you control enters, if it has greater
    // power or greater toughness than this creature, put a +1/+1 counter on this one.
    if line == "evolve" {
        let entering = || Box::new(Selector::Bound(mtg_ir::selector::Binding::EventSubject));
        let more = |a: Value, b: Value| mtg_ir::trigger::Condition::ValueAtLeast {
            lhs: a,
            rhs: Value::Sum(vec![b, Value::ONE]),
        };
        return Some(trig(
            EventPattern::Enters {
                who: ObjectFilter::And(vec![
                    ObjectFilter::HasType(CardType::Creature),
                    ObjectFilter::ControlledBy(Box::new(Selector::You)),
                ]),
            },
            Some(mtg_ir::trigger::Condition::Or(vec![
                more(
                    Value::Power(entering()),
                    Value::Power(Box::new(Selector::SelfSource)),
                ),
                more(
                    Value::Toughness(entering()),
                    Value::Toughness(Box::new(Selector::SelfSource)),
                ),
            ])),
            Effect::AddCounters {
                what: Selector::SelfSource,
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                amount: Value::ONE,
            },
        ));
    }
    // CR 702.101 — extort: whenever you cast a spell, you may pay {W/B}; if you do, each
    // opponent loses 1 life and you gain that much life.
    if line == "extort" {
        let (mana, _) = mana_cost("{w/b}")?;
        return Some(trig(
            EventPattern::Cast {
                who: ObjectFilter::IsSpell,
                by: Selector::You,
            },
            None,
            Effect::MayPay {
                cost: Cost {
                    mana,
                    ..Cost::free()
                },
                then: Box::new(Effect::Sequence(vec![
                    Effect::LoseLife {
                        who: Selector::Opponents,
                        amount: Value::ONE,
                    },
                    Effect::GainLife {
                        who: Selector::You,
                        amount: Value::Count(Box::new(Selector::Opponents)),
                    },
                ])),
            },
        ));
    }
    // CR 702.25 — flanking: whenever a creature without flanking blocks this creature,
    // the blocking creature gets -1/-1 until end of turn. Flanking stays a keyword too, so
    // "without flanking" can see it.
    if line == "flanking" {
        return Some(trig(
            EventPattern::BlockedBy {
                attacker: me(),
                blocker: ObjectFilter::Not(Box::new(ObjectFilter::HasKeyword(
                    mtg_core::Keyword::Flanking,
                ))),
            },
            None,
            pump(-1, Selector::Bound(mtg_ir::selector::Binding::EventSubject)),
        ));
    }
    if line == "prowess" {
        return Some(trig(
            EventPattern::Cast {
                who: ObjectFilter::And(vec![
                    ObjectFilter::IsSpell,
                    ObjectFilter::Not(Box::new(ObjectFilter::HasType(CardType::Creature))),
                ]),
                by: Selector::You,
            },
            None,
            pump(1, Selector::SelfSource),
        ));
    }
    if line == "exalted" {
        // "Whenever a creature you control attacks alone": the only attacker once
        // attackers are declared.
        return Some(trig(
            EventPattern::Attacks {
                who: ObjectFilter::And(vec![
                    ObjectFilter::HasType(CardType::Creature),
                    ObjectFilter::AttackingAlone,
                    ObjectFilter::ControlledBy(Box::new(Selector::You)),
                ]),
            },
            None,
            pump(1, Selector::Bound(mtg_ir::selector::Binding::EventSubject)),
        ));
    }
    // CR 702.79 persist / 702.93 undying: "When this creature dies, if it had no -1/-1
    // (+1/+1) counters on it, return it to the battlefield under its owner's control with
    // a -1/-1 (+1/+1) counter on it."
    for (word, kind) in [
        ("persist", mtg_core::CounterKind::MinusOneMinusOne),
        ("undying", mtg_core::CounterKind::PlusOnePlusOne),
    ] {
        if line == word {
            let before = Selector::Bound(mtg_ir::selector::Binding::EventOther);
            let card = Selector::Bound(mtg_ir::selector::Binding::EventSubject);
            return Some(AbilityKind::Triggered {
                trigger: Trigger {
                    on: EventPattern::Dies { who: me() },
                    functions_from: Zone::Battlefield,
                    intervening_if: Some(mtg_ir::trigger::Condition::ValueEquals {
                        lhs: Value::Counters(Box::new(before), kind),
                        rhs: Value::ZERO,
                    }),
                    optional: false,
                    limit: None,
                    timing: TriggerTiming::LeavesBattlefield,
                },
                effect: Effect::Sequence(vec![
                    Effect::MoveZone {
                        what: card,
                        to: Zone::Battlefield,
                        owner_relative_to: None,
                        position: mtg_ir::effect::ZonePosition::Natural,
                        tapped: false,
                        face_down: false,
                        under_control_of: None,
                    },
                    Effect::AddCounters {
                        what: Selector::Bound(mtg_ir::selector::Binding::It),
                        kind,
                        amount: Value::ONE,
                    },
                ]),
            });
        }
    }
    if let Some(r) = line.strip_prefix("bushido ") {
        let (n, rest) = words::number(r)?;
        if !rest.is_empty() {
            return None;
        }
        return Some(trig(
            EventPattern::AnyOf(vec![
                EventPattern::Blocks { who: me() },
                EventPattern::BecomesBlocked { who: me() },
            ]),
            None,
            pump(n, Selector::SelfSource),
        ));
    }
    None
}

/// Retrace, jump-start and escape: see [`compile_line`].
fn graveyard_cast(line: &str, face: &FaceText, cx: &mut Cx) -> Option<AbilityKind> {
    let spell = face
        .card_types
        .iter()
        .any(|t| matches!(t, CardType::Instant | CardType::Sorcery));
    let printed = || mana_cost(&face.mana_cost.to_lowercase()).map(|(m, _)| m);
    let discard = |filter| AdditionalCost::Discard {
        count: Value::ONE,
        filter,
        at_random: false,
    };
    let (cost, exile) = match line {
        "retrace" => (
            Cost {
                mana: printed()?,
                additional: vec![discard(ObjectFilter::HasType(CardType::Land))],
                ..Cost::free()
            },
            false,
        ),
        "jump-start" if spell => (
            Cost {
                mana: printed()?,
                additional: vec![discard(ObjectFilter::Any)],
                ..Cost::free()
            },
            true,
        ),
        _ => {
            let r = line.strip_prefix("escape—")?;
            let r = r.strip_suffix('.').unwrap_or(r);
            let cost = cost(r, cx)?;
            if !cost.additional.iter().all(|a| {
                matches!(
                    a,
                    AdditionalCost::ExileFrom {
                        zone: Zone::Graveyard,
                        ..
                    }
                )
            }) {
                return None;
            }
            (cost, false)
        }
    };
    Some(AbilityKind::CastFrom {
        zone: Zone::Graveyard,
        cost,
        exile,
        transformed: false,
    })
}

/// "cycling {2}" — CR 702.29a: "{cost}, Discard this card: Draw a card", from the hand.
/// "basic landcycling {1}", "forestcycling {2}" — CR 702.29e: search for that land instead.
fn cycling(line: &str, cx: &mut Cx) -> Option<AbilityKind> {
    let (effect, r) = if let Some(r) = line.strip_prefix("cycling ") {
        (
            Effect::Draw {
                who: Selector::You,
                count: Value::ONE,
            },
            r,
        )
    } else {
        let (kind, r) = line.split_once("cycling ")?;
        let what = match kind {
            "basic land" => "basic land",
            "plains" | "island" | "swamp" | "mountain" | "forest" => kind,
            _ => return None,
        };
        let text = format!(
            "search your library for a {what} card, reveal it, put it into your hand, then shuffle"
        );
        (clauses::effect(&text, cx)?, r)
    };
    let (mana, rest) = mana_cost(r)?;
    if !rest.is_empty() {
        return None;
    }
    Some(AbilityKind::Activated {
        cost: Cost {
            mana,
            additional: vec![
                AdditionalCost::Discard {
                    count: Value::ONE,
                    filter: ObjectFilter::IsSelf,
                    at_random: false,
                },
                AdditionalCost::Cycling,
            ],
            ..Cost::free()
        },
        effect,
        functions_from: Zone::Hand,
        is_mana_ability: false,
        is_loyalty_ability: false,
        timing: ActivationTiming::Instant,
    })
}

#[cfg(test)]
mod tests;
