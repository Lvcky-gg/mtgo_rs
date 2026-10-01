//! Noun phrases: what an effect acts on.
//!
//! Three kinds, because the rules treat them differently:
//!
//! - **Filters** describe a class of objects ("nonblack creature an opponent controls").
//! - **Objects** pick out which ones: `~`, "target …" (which allocates a target slot),
//!   "each …"/"all …", "enchanted creature".
//! - **Players**: "you", "target opponent", "each player".

use mtg_core::{CardType, Supertype, Zone};
use mtg_ir::{ObjectFilter, Selector, TargetSpec, Value};

use super::{Cx, words};

/// A parsed filter and where the objects it describes live.
#[derive(Clone, Debug)]
pub struct Noun {
    pub filter: ObjectFilter,
    pub zone: Zone,
    pub plural: bool,
}

/// A class of objects. `other` has already been consumed by the caller if present.
pub fn noun<'s>(s: &'s str, cx: &Cx) -> Option<(Noun, &'s str)> {
    let mut parts: Vec<ObjectFilter> = Vec::new();
    let mut rest = s;

    // Adjectives.
    loop {
        if let Some(r) = rest.strip_prefix("attacking or blocking ") {
            parts.push(ObjectFilter::AttackingOrBlocking);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("attacking ") {
            parts.push(ObjectFilter::Attacking);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("blocking ") {
            parts.push(ObjectFilter::Blocking);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("tapped ") {
            parts.push(ObjectFilter::Tapped(true));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("untapped ") {
            parts.push(ObjectFilter::Tapped(false));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("legendary ") {
            parts.push(ObjectFilter::HasSupertype(Supertype::Legendary));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("nonlegendary ") {
            parts.push(not(ObjectFilter::HasSupertype(Supertype::Legendary)));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("basic ") {
            parts.push(ObjectFilter::HasSupertype(Supertype::Basic));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("nonbasic ") {
            parts.push(not(ObjectFilter::HasSupertype(Supertype::Basic)));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("colorless ") {
            parts.push(ObjectFilter::Colorless);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("multicolored ") {
            parts.push(ObjectFilter::Multicolored);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("historic ")
            && let Some(saga) = cx.subtype("saga")
        {
            // CR 700.6: artifacts, legendaries, and Sagas.
            parts.push(ObjectFilter::Or(vec![
                ObjectFilter::HasType(CardType::Artifact),
                ObjectFilter::HasSupertype(Supertype::Legendary),
                ObjectFilter::HasSubtype(saga),
            ]));
            rest = r;
        } else if let Some((f, r)) = negated(rest, cx) {
            parts.push(f);
            rest = r;
        } else if let Some((f, r)) = rest
            .split_once(", ")
            .filter(|(w, _)| w.starts_with("non") && !w.contains(' '))
            .and_then(|(w, r)| negated(&format!("{w} "), cx).map(|(f, _)| (f, r)))
        {
            // "nonartifact, nonblack creature".
            parts.push(f);
            rest = r;
        } else if let Some((c, r)) = color_adjective(rest) {
            parts.push(c);
            rest = r;
        } else {
            break;
        }
    }

    // The head noun, possibly "X or Y".
    let (head, zone, plural, r) = head(rest, cx)?;
    parts.push(head);
    rest = r;

    // Qualifiers after the noun.
    loop {
        if let Some(r) = rest.strip_prefix(" you control") {
            parts.push(ObjectFilter::ControlledBy(Box::new(Selector::You)));
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" you don't control") {
            parts.push(not(ObjectFilter::ControlledBy(Box::new(Selector::You))));
            rest = r;
        } else if let Some(r) = rest
            .strip_prefix(" an opponent controls")
            .or_else(|| rest.strip_prefix(" your opponents control"))
        {
            parts.push(ObjectFilter::ControlledBy(Box::new(Selector::Opponents)));
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" defending player controls") {
            parts.push(ObjectFilter::ControlledBy(Box::new(
                Selector::DefendingPlayer,
            )));
            rest = r;
        } else if let Some(r) = rest
            .strip_prefix(" you own")
            .filter(|_| zone == Zone::Battlefield)
        {
            parts.push(ObjectFilter::OwnedBy(Box::new(Selector::You)));
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" with ") {
            let (f, r) = with_clause(r)?;
            parts.push(f);
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" of the chosen type") {
            parts.push(ObjectFilter::HasChosenSubtype);
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" of the chosen color") {
            parts.push(ObjectFilter::HasChosenColor);
            rest = r;
        } else if let Some(r) = rest.strip_prefix(" without ")
            && let Some((k, r)) = words::keyword_prefix(r)
        {
            // "creature without flying"
            parts.push(not(ObjectFilter::HasKeyword(k)));
            rest = r;
        } else if let Some(r) = rest
            .strip_prefix(" from your graveyard")
            .or_else(|| rest.strip_prefix(" in your graveyard"))
            .filter(|_| zone == Zone::Graveyard)
        {
            parts.push(ObjectFilter::OwnedBy(Box::new(Selector::You)));
            rest = r;
        } else if let Some(r) = rest
            .strip_prefix(" from a graveyard")
            .or_else(|| rest.strip_prefix(" in a graveyard"))
            .filter(|_| zone == Zone::Graveyard)
        {
            rest = r;
        } else if let Some(r) = rest
            .strip_prefix(" in your hand")
            .or_else(|| rest.strip_prefix(" from your hand"))
            .filter(|_| zone == Zone::Hand)
        {
            parts.push(ObjectFilter::OwnedBy(Box::new(Selector::You)));
            rest = r;
        } else {
            break;
        }
    }
    let filter = if parts.len() == 1 {
        parts.pop()?
    } else {
        ObjectFilter::And(parts)
    };
    Some((
        Noun {
            filter,
            zone,
            plural,
        },
        rest,
    ))
}

fn not(f: ObjectFilter) -> ObjectFilter {
    ObjectFilter::Not(Box::new(f))
}

/// "nonland", "noncreature", "nonblack", "non-Human".
fn negated<'s>(s: &'s str, cx: &Cx) -> Option<(ObjectFilter, &'s str)> {
    let r = s.strip_prefix("non")?;
    let r = r.strip_prefix('-').unwrap_or(r);
    let (word, rest) = words::first_word(r);
    let rest = rest.strip_prefix(' ')?;
    if word == "token" {
        return Some((not(ObjectFilter::Token), rest));
    }
    if let Some((t, false)) = words::card_type(word) {
        return Some((not(ObjectFilter::HasType(t)), rest));
    }
    if let Some(c) = words::color(word) {
        return Some((not(ObjectFilter::HasColor(c)), rest));
    }
    if let Some(st) = cx.subtype(word) {
        return Some((not(ObjectFilter::HasSubtype(st)), rest));
    }
    None
}

/// "red ", "white and blue "? — only a single colour, or "X or Y" colours.
fn color_adjective(s: &str) -> Option<(ObjectFilter, &str)> {
    let (w, rest) = words::first_word(s);
    let c = words::color(w)?;
    if let Some(r) = rest.strip_prefix(" or ") {
        let (w2, rest2) = words::first_word(r);
        let c2 = words::color(w2)?;
        let rest2 = rest2.strip_prefix(' ')?;
        return Some((
            ObjectFilter::Or(vec![ObjectFilter::HasColor(c), ObjectFilter::HasColor(c2)]),
            rest2,
        ));
    }
    Some((ObjectFilter::HasColor(c), rest.strip_prefix(' ')?))
}

/// The head noun: a type, "permanent", "spell", a subtype, or "card", with "X or Y".
fn head<'s>(s: &'s str, cx: &Cx) -> Option<(ObjectFilter, Zone, bool, &'s str)> {
    let (first, zone, plural, rest) = one_head(s, cx)?;
    // "creature or planeswalker", "artifact or enchantment", "instant or sorcery spell" —
    // and, for a plural, "instant and sorcery spells", "artifacts and enchantments",
    // which likewise name everything that is either.
    let joined = rest.strip_prefix(" or ").or_else(|| {
        rest.strip_prefix(" and ").filter(|r| {
            one_head(r, cx).is_some_and(|(_, _, plural2, _)| plural2)
                // "for each instant and sorcery card in your graveyard": Oracle's singular
                // for the pair, meaning either.
                || (s.starts_with("instant") && r.starts_with("sorcery "))
        })
    });
    if let Some(r) = joined
        && let Some((second, zone2, plural2, rest2)) = one_head(r, cx)
    {
        // "instant or sorcery spell": the first half was only a type word, so the zone
        // comes from the second.
        let zone = if zone == Zone::Battlefield {
            zone2
        } else {
            zone
        };
        return Some((
            ObjectFilter::Or(vec![first, second]),
            zone,
            plural || plural2,
            rest2,
        ));
    }
    if let Some(r) = rest.strip_prefix(", ")
        && let Some((second, _, _, r2)) = one_head(r, cx)
        && let Some(r3) = r2.strip_prefix(", or ")
        && let Some((third, zone3, plural3, rest3)) = one_head(r3, cx)
    {
        return Some((
            ObjectFilter::Or(vec![first, second, third]),
            if zone == Zone::Battlefield {
                zone3
            } else {
                zone
            },
            plural || plural3,
            rest3,
        ));
    }
    Some((first, zone, plural, rest))
}

fn one_head<'s>(s: &'s str, cx: &Cx) -> Option<(ObjectFilter, Zone, bool, &'s str)> {
    let (word, rest) = words::first_word(s);
    let mut filter;
    let mut plural;
    if let Some((t, p)) = words::card_type(word) {
        filter = ObjectFilter::HasType(t);
        plural = p;
    } else if word == "permanent" || word == "permanents" {
        // "permanent card": a card of a permanent type, somewhere other than the battlefield.
        if let Some(r) = rest
            .strip_prefix(" card")
            .or_else(|| rest.strip_prefix(" cards"))
        {
            let plural = rest.starts_with(" cards");
            let permanent = ObjectFilter::Or(
                [
                    CardType::Artifact,
                    CardType::Creature,
                    CardType::Enchantment,
                    CardType::Land,
                    CardType::Planeswalker,
                    CardType::Battle,
                ]
                .into_iter()
                .map(ObjectFilter::HasType)
                .collect(),
            );
            return card_zone(permanent, plural, r);
        }
        return Some((
            ObjectFilter::Any,
            Zone::Battlefield,
            word.ends_with('s'),
            rest,
        ));
    } else if word == "activated" || word == "triggered" {
        // "activated ability", "activated or triggered ability" — abilities on the stack.
        let r = rest
            .strip_prefix(" or triggered ability")
            .or_else(|| rest.strip_prefix(" ability"))?;
        return Some((ObjectFilter::IsAbility, Zone::Stack, false, r));
    } else if word == "spell" || word == "spells" {
        return Some((ObjectFilter::IsSpell, Zone::Stack, word == "spells", rest));
    } else if word == "card" || word == "cards" {
        // A bare "card": its zone comes from a qualifier.
        return card_zone(ObjectFilter::Any, word == "cards", rest);
    } else {
        let st = cx.subtype(word)?;
        filter = ObjectFilter::HasSubtype(st);
        plural = cx.subtype_is_plural(word);
    }

    let mut rest = rest;
    // "Mercenary permanent card": a subtype, of a card of any permanent type.
    if matches!(filter, ObjectFilter::HasSubtype(_))
        && let Some(r) = rest.strip_prefix(" permanent")
        && (r.starts_with(" card") || r.starts_with(" cards"))
    {
        let (next, r2) = words::first_word(r.trim_start());
        return card_zone(
            ObjectFilter::And(vec![filter, permanent_card()]),
            next == "cards",
            r2,
        );
    }
    // "Goblin creature", "artifact creature", "creature spell", "creature card".
    if let Some(r) = rest.strip_prefix(' ') {
        let (next, r2) = words::first_word(r);
        if let Some((t, p)) = words::card_type(next)
            && matches!(
                filter,
                ObjectFilter::HasSubtype(_) | ObjectFilter::HasType(_)
            )
            && filter != ObjectFilter::HasType(t)
        {
            filter = ObjectFilter::And(vec![filter, ObjectFilter::HasType(t)]);
            plural = p;
            rest = r2;
        }
    }
    if let Some(r) = rest.strip_prefix(' ') {
        let (next, r2) = words::first_word(r);
        match next {
            "spell" | "spells" => {
                return Some((
                    ObjectFilter::And(vec![ObjectFilter::IsSpell, filter]),
                    Zone::Stack,
                    next == "spells",
                    r2,
                ));
            }
            "card" | "cards" => return card_zone(filter, next == "cards", r2),
            _ => {}
        }
    }
    Some((filter, Zone::Battlefield, plural, rest))
}

/// A "card" lives somewhere other than the battlefield; which zone comes next, after an
/// optional quality ("creature card with mana value 3 or less from your graveyard").
fn card_zone(
    filter: ObjectFilter,
    plural: bool,
    rest: &str,
) -> Option<(ObjectFilter, Zone, bool, &str)> {
    let (filter, rest) = match rest.strip_prefix(" with ").and_then(with_clause) {
        Some((quality, r)) => (ObjectFilter::And(vec![filter, quality]), r),
        None => (filter, rest),
    };
    let zone = if rest.starts_with(" from your graveyard")
        || rest.starts_with(" in your graveyard")
        || rest.starts_with(" from a graveyard")
        || rest.starts_with(" in a graveyard")
    {
        Zone::Graveyard
    } else if rest.starts_with(" in your hand") || rest.starts_with(" from your hand") {
        Zone::Hand
    } else {
        return None;
    };
    Some((filter, zone, plural, rest))
}

/// "with flying", "with power 2 or less", "with mana value 3 or greater".
pub fn with_clause(s: &str) -> Option<(ObjectFilter, &str)> {
    if let Some((k, rest)) = words::keyword_prefix(s) {
        // "with flying or reach"
        if let Some(r) = rest.strip_prefix(" or ")
            && let Some((k2, r)) = words::keyword_prefix(r)
        {
            return Some((
                ObjectFilter::Or(vec![
                    ObjectFilter::HasKeyword(k),
                    ObjectFilter::HasKeyword(k2),
                ]),
                r,
            ));
        }
        return Some((ObjectFilter::HasKeyword(k), rest));
    }
    // "with a +1/+1 counter on it", "with one or more +1/+1 counters on it"
    if let Some(r) = s.strip_prefix("a ").or_else(|| s.strip_prefix("an "))
        && let Some((kind, r)) = words::counter(r)
        && let Some(r) = r.strip_prefix(" on it")
    {
        return Some((ObjectFilter::HasCounter(kind), r));
    }
    if let Some(r) = s.strip_prefix("one or more ")
        && let Some((kind, r)) = words::counter(r)
        && let Some(r) = r.strip_prefix("s on it")
    {
        return Some((ObjectFilter::HasCounter(kind), r));
    }
    for (prefix, kind) in [("power ", 0u8), ("toughness ", 1), ("mana value ", 2)] {
        let Some(r) = s.strip_prefix(prefix) else {
            continue;
        };
        let (n, r) = words::number(r)?;
        let v = Value::Fixed(n);
        if let Some(r) = r.strip_prefix(" or less") {
            return Some((
                match kind {
                    0 => ObjectFilter::PowerAtMost(v),
                    1 => ObjectFilter::ToughnessAtMost(v),
                    _ => ObjectFilter::ManaValueAtMost(v),
                },
                r,
            ));
        }
        if let Some(r) = r.strip_prefix(" or greater") {
            return Some((
                match kind {
                    0 => ObjectFilter::PowerAtLeast(v),
                    2 => ObjectFilter::ManaValueAtLeast(v),
                    // At least N is not at most N - 1.
                    _ => not(ObjectFilter::ToughnessAtMost(Value::Fixed(n - 1))),
                },
                r,
            ));
        }
        // Exactly: "with mana value 3", "with power 2".
        return match kind {
            0 => Some((
                ObjectFilter::And(vec![
                    ObjectFilter::PowerAtLeast(v.clone()),
                    ObjectFilter::PowerAtMost(v),
                ]),
                r,
            )),
            2 => Some((
                ObjectFilter::And(vec![
                    ObjectFilter::ManaValueAtLeast(v.clone()),
                    ObjectFilter::ManaValueAtMost(v),
                ]),
                r,
            )),
            _ => Some((
                ObjectFilter::And(vec![
                    not(ObjectFilter::ToughnessAtMost(Value::Fixed(n - 1))),
                    ObjectFilter::ToughnessAtMost(v),
                ]),
                r,
            )),
        };
    }
    None
}

/// Whether a phrase names many objects through a singular noun ("each creature", "each
/// other Elf"), which takes a singular verb.
pub fn each_singular(s: &str) -> bool {
    s.starts_with("each ")
}

/// A card of any permanent type (CR 110.4).
pub fn permanent_card() -> ObjectFilter {
    ObjectFilter::Or(
        [
            CardType::Artifact,
            CardType::Creature,
            CardType::Enchantment,
            CardType::Land,
            CardType::Planeswalker,
            CardType::Battle,
        ]
        .into_iter()
        .map(ObjectFilter::HasType)
        .collect(),
    )
}

/// An object selector. Returns the selector, whether it denotes several objects, and
/// the rest of the input.
pub fn object<'s>(s: &'s str, cx: &mut Cx) -> Option<(Selector, bool, &'s str)> {
    if let Some(r) = s.strip_prefix('~') {
        return Some((Selector::SelfSource, false, r));
    }
    if let Some(r) = s.strip_prefix("it").filter(|r| !r.starts_with(['s', '\''])) {
        return Some((cx.it.clone()?, false, r));
    }
    // "them": what an earlier step made or moved ("create two tokens … exile them").
    if let Some(r) = s
        .strip_prefix("them")
        .filter(|r| !r.starts_with(|c: char| c.is_alphanumeric()))
        && cx.it == Some(Selector::Bound(mtg_ir::selector::Binding::It))
    {
        return Some((cx.it.clone()?, true, r));
    }
    for (prefix, kind) in [
        ("that creature", CardType::Creature),
        ("that permanent", CardType::Creature),
    ] {
        if let Some(r) = s.strip_prefix(prefix)
            && let Some(it) = cx.that_creature.clone().or_else(|| cx.it.clone())
        {
            let _ = kind;
            return Some((it, false, r));
        }
    }
    for (prefix, t) in [
        ("enchanted creature", Some(CardType::Creature)),
        ("equipped creature", Some(CardType::Creature)),
        ("enchanted land", Some(CardType::Land)),
        ("enchanted artifact", Some(CardType::Artifact)),
        ("enchanted permanent", None),
    ] {
        if let Some(r) = s.strip_prefix(prefix) {
            let mut f = vec![ObjectFilter::AttachedToSelf];
            if let Some(t) = t {
                f.push(ObjectFilter::HasType(t));
            }
            return Some((
                Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(f),
                },
                false,
                r,
            ));
        }
    }

    // "target player's graveyard", "your graveyard", "each opponent's graveyard".
    for (prefix, whose) in [
        ("target player's graveyard", None),
        ("target opponent's graveyard", Some(Selector::Opponents)),
        ("your graveyard", Some(Selector::You)),
    ] {
        if let Some(r) = s.strip_prefix(prefix) {
            let who = match whose {
                Some(Selector::You) => Selector::You,
                players => cx.target(player_target(players)),
            };
            return Some((
                Selector::All {
                    zone: Zone::Graveyard,
                    filter: ObjectFilter::OwnedBy(Box::new(who)),
                },
                true,
                r,
            ));
        }
    }

    // "two target creatures", "up to two target creature cards": one slot each, and no
    // object chosen twice (CR 115.3).
    // "each of up to two other target creatures" (support) says the same thing.
    let listed = s.strip_prefix("each of ").unwrap_or(s);
    for (lead, up_to) in [("up to ", true), ("", false)] {
        if let Some(r) = listed.strip_prefix(lead)
            && let Some((n, r)) = words::number(r)
            && (2..=3).contains(&n)
            && let Some((other, r)) = r
                .strip_prefix(" target ")
                .map(|r| (false, r))
                .or_else(|| r.strip_prefix(" other target ").map(|r| (true, r)))
        {
            let (noun, rest) = noun(r, cx)?;
            if !noun.plural {
                return None;
            }
            let filter = if other {
                ObjectFilter::And(vec![noun.filter.clone(), not(ObjectFilter::IsSelf)])
            } else {
                noun.filter.clone()
            };
            let slots = (0..n)
                .map(|_| {
                    cx.target(TargetSpec {
                        zone: noun.zone,
                        filter: filter.clone(),
                        allows_players: false,
                        players: None,
                        mode: None,
                        count: Value::ONE,
                        up_to,
                        distinct_from_other_targets: true,
                    })
                })
                .collect();
            let sel = Selector::Union(slots);
            cx.it = Some(sel.clone());
            return Some((sel, true, rest));
        }
    }

    // Targets.
    let (up_to, another, r) = if let Some(r) = s.strip_prefix("up to one target ") {
        (true, false, r)
    } else if let Some(r) = s.strip_prefix("up to one other target ") {
        (true, true, r)
    } else if let Some(r) = s.strip_prefix("another target ") {
        (false, true, r)
    } else if let Some(r) = s.strip_prefix("target ") {
        (false, false, r)
    } else {
        return mass(s, cx);
    };
    let (n, rest) = noun(r, cx)?;
    if n.plural {
        return None;
    }
    let mut filter = n.filter;
    if another {
        filter = ObjectFilter::And(vec![filter, not(ObjectFilter::IsSelf)]);
    }
    let sel = cx.target(TargetSpec {
        zone: n.zone,
        filter,
        allows_players: false,
        players: None,
        mode: None,
        count: Value::ONE,
        up_to,
        distinct_from_other_targets: false,
    });
    cx.it = Some(sel.clone());
    Some((sel, false, rest))
}

/// "each creature", "all creatures", "creatures you control", "other Elves you control".
fn mass<'s>(s: &'s str, cx: &Cx) -> Option<(Selector, bool, &'s str)> {
    let (other, r) = match s.strip_prefix("each other ") {
        Some(r) => (true, r),
        None => match s.strip_prefix("all other ") {
            Some(r) => (true, r),
            None => match s.strip_prefix("other ") {
                Some(r) => (true, r),
                None => (false, s),
            },
        },
    };
    let (quantified, r) = match r.strip_prefix("each ") {
        Some(r) => (true, r),
        None => match r.strip_prefix("all ") {
            Some(r) => (true, r),
            None => (other, r),
        },
    };
    let (n, rest) = noun(r, cx)?;
    // A bare plural ("creatures you control") names everything that matches; a bare
    // singular ("creature") is not a selector.
    if !quantified && !n.plural {
        return None;
    }
    if n.zone != Zone::Battlefield {
        return None;
    }
    let filter = if other {
        ObjectFilter::And(vec![n.filter, not(ObjectFilter::IsSelf)])
    } else {
        n.filter
    };
    Some((
        Selector::All {
            zone: Zone::Battlefield,
            filter,
        },
        true,
        rest,
    ))
}

/// A player selector. The boolean says whether the verb that follows is plural
/// ("you draw", "each player draws" — only "you" takes the bare form).
pub fn player<'s>(s: &'s str, cx: &mut Cx) -> Option<(Selector, bool, &'s str)> {
    if let Some(r) = s
        .strip_prefix("you")
        .filter(|r| !r.starts_with(['r', '\'']))
    {
        return Some((Selector::You, true, r));
    }
    if let Some(r) = s.strip_prefix("each player") {
        return Some((Selector::EachPlayer, false, r));
    }
    if let Some(r) = s.strip_prefix("each opponent") {
        return Some((Selector::Opponents, false, r));
    }
    if let Some(r) = s.strip_prefix("defending player") {
        return Some((Selector::DefendingPlayer, false, r));
    }
    if let Some(r) = s.strip_prefix("enchanted player") {
        return Some((Selector::EnchantedPlayer, false, r));
    }
    // "enchanted creature's controller": whoever controls what the source is attached to.
    if let Some(r) = s
        .strip_prefix("enchanted creature's controller")
        .or_else(|| s.strip_prefix("equipped creature's controller"))
    {
        let attached = Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::AttachedToSelf,
        };
        return Some((Selector::ControllerOf(Box::new(attached)), false, r));
    }
    // Players named through an object or the triggering event.
    for (prefix, owner) in [
        ("its controller", false),
        ("that creature's controller", false),
        ("that permanent's controller", false),
        ("its owner", true),
        ("that creature's owner", true),
    ] {
        if let Some(r) = s.strip_prefix(prefix) {
            let it = Box::new(cx.it.clone()?);
            let sel = if owner {
                Selector::OwnerOf(it)
            } else {
                Selector::ControllerOf(it)
            };
            return Some((sel, false, r));
        }
    }
    if let Some(r) = s.strip_prefix("that player") {
        return Some((cx.that_player.clone()?, false, r));
    }
    if let Some(r) = s.strip_prefix("target player") {
        return Some((cx.target(player_target(None)), false, r));
    }
    if let Some(r) = s.strip_prefix("target opponent") {
        return Some((
            cx.target(player_target(Some(Selector::Opponents))),
            false,
            r,
        ));
    }
    None
}

/// A target slot that takes a player only.
pub fn player_target(players: Option<Selector>) -> TargetSpec {
    TargetSpec {
        zone: Zone::Battlefield,
        filter: not(ObjectFilter::Any),
        allows_players: true,
        players,
        mode: None,
        count: Value::ONE,
        up_to: false,
        distinct_from_other_targets: false,
    }
}

/// What damage can be dealt to: an object, a player, or "any target".
pub fn recipient<'s>(s: &'s str, cx: &mut Cx) -> Option<(Selector, &'s str)> {
    let creature_or_walker = ObjectFilter::Or(vec![
        ObjectFilter::HasType(CardType::Creature),
        ObjectFilter::HasType(CardType::Planeswalker),
    ]);
    for (prefix, filter, players) in [
        ("any target", creature_or_walker.clone(), None),
        ("any other target", creature_or_walker.clone(), None),
        (
            "target player or planeswalker",
            ObjectFilter::HasType(CardType::Planeswalker),
            None,
        ),
        (
            "target opponent or planeswalker",
            ObjectFilter::HasType(CardType::Planeswalker),
            Some(Selector::Opponents),
        ),
    ] {
        if let Some(r) = s.strip_prefix(prefix) {
            let filter = if prefix == "any other target" {
                ObjectFilter::And(vec![filter, not(ObjectFilter::IsSelf)])
            } else {
                filter
            };
            let sel = cx.target(TargetSpec {
                zone: Zone::Battlefield,
                filter,
                allows_players: true,
                players,
                mode: None,
                count: Value::ONE,
                up_to: false,
                // "… to any target and 1 damage to any other target" (CR 115.3).
                distinct_from_other_targets: prefix == "any other target",
            });
            cx.it = Some(sel.clone());
            return Some((sel, r));
        }
    }
    if let Some(r) = s.strip_prefix("each creature and each player") {
        return Some((
            Selector::Union(vec![
                Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::HasType(CardType::Creature),
                },
                Selector::EachPlayer,
            ]),
            r,
        ));
    }
    if let Some((p, _, r)) = player(s, cx) {
        return Some((p, r));
    }
    let (o, _, r) = object(s, cx)?;
    Some((o, r))
}
