//! Conditions: "if you control a Swamp", "as long as you have 10 or less life", "during
//! your turn". The whole condition must be understood; nothing is approximated.

use mtg_core::{CardType, Zone};
use mtg_ir::{ObjectFilter, Selector, Value, trigger::Condition};

use super::{Cx, nouns, words};

/// The cards in your graveyard.
fn your_graveyard() -> Selector {
    Selector::All {
        zone: Zone::Graveyard,
        filter: ObjectFilter::OwnedBy(Box::new(Selector::You)),
    }
}

/// A condition, and the rest of the input.
pub fn condition<'s>(s: &'s str, cx: &Cx) -> Option<(Condition, &'s str)> {
    condition_shapes(s, cx)
        .filter(|(_, r)| !r.starts_with(" and a"))
        .or_else(|| control_both(s, cx))
        .or_else(|| condition_shapes(s, cx))
        .or_else(|| comparison(s, cx))
}

/// "you control an artifact and an enchantment" (Soul Transfer): both.
fn control_both<'s>(s: &'s str, cx: &Cx) -> Option<(Condition, &'s str)> {
    let r = s.strip_prefix("you control ")?;
    for (at, _) in r.match_indices(" and a") {
        let (first, second) = (&r[..at], &r[at + " and ".len()..]);
        let Some((a, "")) = condition_shapes(&format!("you control {first}"), cx) else {
            continue;
        };
        let owned = format!("you control {second}");
        let Some((b, rest)) = condition_shapes(&owned, cx) else {
            continue;
        };
        let consumed = owned.len() - rest.len() - "you control ".len();
        return Some((Condition::And(vec![a, b]), &second[consumed..]));
    }
    None
}

/// "X is greater than or equal to the number of cards in your library" (Thassa's Oracle),
/// "your devotion to blue is less than …": two numbers compared, when nothing more
/// particular reads the condition.
fn comparison<'s>(s: &'s str, cx: &Cx) -> Option<(Condition, &'s str)> {
    let number = |s: &'s str| -> Option<(Value, &'s str)> {
        super::clauses::value_phrase(s, cx)
            .or_else(|| words::number(s).map(|(n, r)| (Value::Fixed(n), r)))
    };
    let (lhs, r) = number(s)?;
    let r = r.strip_prefix(" is ")?;
    for (op, at_least, strictly) in [
        ("greater than or equal to ", true, false),
        ("less than or equal to ", false, false),
        ("greater than ", true, true),
        ("less than ", false, true),
    ] {
        let Some(r) = r.strip_prefix(op) else {
            continue;
        };
        let (rhs, r) = number(r)?;
        let plus_one = |v: Value| Value::Sum(vec![v, Value::ONE]);
        let cond = match (at_least, strictly) {
            (true, false) => Condition::ValueAtLeast { lhs, rhs },
            (false, false) => Condition::ValueAtLeast { lhs: rhs, rhs: lhs },
            (true, true) => Condition::ValueAtLeast {
                lhs,
                rhs: plus_one(rhs),
            },
            (false, true) => Condition::ValueAtLeast {
                lhs: rhs,
                rhs: plus_one(lhs),
            },
        };
        return Some((cond, r));
    }
    None
}

fn condition_shapes<'s>(s: &'s str, cx: &Cx) -> Option<(Condition, &'s str)> {
    if let Some(rest) = s.strip_prefix("you have the city's blessing") {
        return Some((Condition::HasCityBlessing, rest));
    }
    // "It" is this object unless the line has bound it to another ("whenever you cast a
    // spell, if it was bargained" is about that spell).
    let it_is_self = matches!(cx.it, None | Some(Selector::SelfSource));
    if let Some(r) = s
        .strip_prefix("~ was kicked")
        .or_else(|| s.strip_prefix("it was kicked").filter(|_| it_is_self))
        // Bargain is read as a kicker (see `compile_line`).
        .or_else(|| s.strip_prefix("~ was bargained"))
        .or_else(|| s.strip_prefix("it was bargained").filter(|_| it_is_self))
    {
        return Some((Condition::Kicked, r));
    }
    // "if you cast it from your hand", "if you cast it".
    if let Some(r) = s.strip_prefix("you cast it from your hand") {
        return Some((Condition::WasCast(Some(Zone::Hand)), r));
    }
    if let Some(r) = s.strip_prefix("you cast it") {
        return Some((Condition::WasCast(None), r));
    }
    // "as long as it's attacking", "~ is equipped".
    let me = |f: ObjectFilter| {
        Condition::Exists(Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::And(vec![ObjectFilter::IsSelf, f]),
        })
    };
    for (prefix, f) in [
        ("it's attacking", ObjectFilter::Attacking),
        ("~ is attacking", ObjectFilter::Attacking),
        ("it's blocking", ObjectFilter::Blocking),
        ("~ is blocking", ObjectFilter::Blocking),
        ("it's untapped", ObjectFilter::Tapped(false)),
        ("~ is untapped", ObjectFilter::Tapped(false)),
        ("it's tapped", ObjectFilter::Tapped(true)),
        ("~ is tapped", ObjectFilter::Tapped(true)),
        ("~ entered this turn", ObjectFilter::EnteredThisTurn),
        (
            "~ is an enchantment",
            ObjectFilter::HasType(CardType::Enchantment),
        ),
        ("~ is on the battlefield", ObjectFilter::Any),
    ] {
        if let Some(r) = s.strip_prefix(prefix) {
            return Some((me(f), r));
        }
    }
    for (prefix, sub) in [("~ is equipped", "equipment"), ("~ is enchanted", "aura")] {
        if let Some(r) = s.strip_prefix(prefix)
            && let Some(st) = cx.subtype(sub)
        {
            return Some((
                Condition::Exists(Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(vec![
                        ObjectFilter::HasSubtype(st),
                        ObjectFilter::AttachedToSource,
                    ]),
                }),
                r,
            ));
        }
    }
    if let Some(r) = s
        .strip_prefix("you're the monarch")
        .or_else(|| s.strip_prefix("you are the monarch"))
    {
        return Some((Condition::YouAreMonarch, r));
    }
    // "if {R} was spent to cast it", adamant: "if at least three white mana was spent to
    // cast this spell".
    let spent = |r: &'s str| {
        r.strip_prefix(" was spent to cast ").and_then(|r| {
            r.strip_prefix("~")
                .or_else(|| r.strip_prefix("it"))
                .or_else(|| r.strip_prefix("this spell"))
        })
    };
    if let Some(sym) = s.strip_prefix('{').and_then(|r| r.get(..1))
        && let Some(color) = words::color_symbol(sym)
        && let Some(r) = s.get(3..).and_then(spent)
    {
        return Some((
            Condition::ValueAtLeast {
                lhs: mtg_ir::Value::ManaSpentOfColor(color),
                rhs: mtg_ir::Value::ONE,
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("at least ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(' ')
        && let (w, r) = words::first_word(r)
        && let Some(color) = words::color(w)
        && let Some(r) = r.strip_prefix(" mana").and_then(spent)
    {
        return Some((
            Condition::ValueAtLeast {
                lhs: mtg_ir::Value::ManaSpentOfColor(color),
                rhs: mtg_ir::Value::Fixed(n),
            },
            r,
        ));
    }
    // "if no creatures are on the battlefield", "if there are no creatures
    // on the battlefield": the absence is global unless the noun says otherwise.
    if let Some(r) = s
        .strip_prefix("no ")
        .or_else(|| s.strip_prefix("there are no "))
        && let Some((noun, r)) = nouns::noun(r, cx)
        && noun.zone == Zone::Battlefield
        && noun.plural
        && let Some(r) = r
            .strip_prefix(" are on the battlefield")
            .or_else(|| r.strip_prefix(" on the battlefield"))
    {
        return Some((
            Condition::CountAtMost {
                what: Selector::All {
                    zone: Zone::Battlefield,
                    filter: noun.filter,
                },
                at_most: Value::ZERO,
            },
            r,
        ));
    }
    // Morbid.
    if let Some(r) = s.strip_prefix("an opponent lost life this turn") {
        return Some((Condition::OpponentLostLifeThisTurn, r));
    }
    if let Some(r) = s.strip_prefix("you gained life this turn") {
        return Some((Condition::YouGainedLifeThisTurn, r));
    }
    if let Some(r) = s.strip_prefix("a creature died this turn") {
        return Some((Condition::CreatureDiedThisTurn, r));
    }
    // "unless it escaped" (CR 702.138c): escape is the only way to cast these from a
    // graveyard.
    if let Some(r) = s.strip_prefix("it escaped") {
        return Some((Condition::WasCast(Some(Zone::Graveyard)), r));
    }
    // Delirium: "there are four or more card types among cards in your graveyard".
    if let Some(r) = s.strip_prefix("there are ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more card types among cards in your graveyard")
    {
        return Some((
            Condition::ValueAtLeast {
                lhs: Value::CardTypesAmong(Box::new(your_graveyard())),
                rhs: Value::Fixed(n),
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("no spells were cast last turn") {
        return Some((Condition::NoSpellsLastTurn, r));
    }
    if let Some(r) = s.strip_prefix("a player cast two or more spells last turn") {
        return Some((Condition::PlayerCastTwoLastTurn, r));
    }
    if let Some(r) = s.strip_prefix("you attacked this turn") {
        return Some((Condition::YouAttackedThisTurn, r));
    }
    if let Some(r) = s.strip_prefix("it's your turn") {
        return Some((Condition::YourTurn, r));
    }
    if let Some(r) = s.strip_prefix("it's not your turn") {
        return Some((Condition::Not(Box::new(Condition::YourTurn)), r));
    }
    // Formidable: "creatures you control have total power 8 or greater".
    if let Some(r) = s.strip_prefix("creatures you control have total power ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or greater")
    {
        return Some((
            Condition::ValueAtLeast {
                lhs: Value::Power(Box::new(Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(vec![
                        ObjectFilter::HasType(CardType::Creature),
                        ObjectFilter::ControlledBy(Box::new(Selector::You)),
                    ]),
                })),
                rhs: Value::Fixed(n),
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("you control ")
        && let Some(found) = controls(r, Selector::You, cx)
    {
        return Some(found);
    }
    // "its surge cost was paid", "this spell's spectacle cost was paid".
    for lead in ["its ", "~'s ", "this spell's "] {
        if let Some(r) = s.strip_prefix(lead) {
            use mtg_ir::ability::AltCost;
            for (word, alt) in [
                ("surge", AltCost::Surge),
                ("spectacle", AltCost::Spectacle),
                ("blitz", AltCost::Blitz),
                ("dash", AltCost::Dash),
                ("evoke", AltCost::Evoke),
            ] {
                if let Some(r) = r
                    .strip_prefix(word)
                    .and_then(|r| r.strip_prefix(" cost was paid"))
                {
                    return Some((Condition::CastFor(alt), r));
                }
            }
        }
    }
    if let Some(r) = s.strip_prefix("an opponent controls ") {
        return controls(r, Selector::Opponents, cx);
    }
    // "as long as you've cast a spell this turn". ("Another spell" depends on when it is
    // read, so only `cast_only` accepts it.)
    // "you've drawn two or more cards this turn"
    if let Some(r) = s.strip_prefix("you've drawn ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more cards this turn")
    {
        return Some((
            at_least(Value::CardsDrawnThisTurn(Box::new(Selector::You)), n),
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("you've cast a spell this turn") {
        return Some((
            Condition::ValueAtLeast {
                lhs: Value::SpellsCastThisTurn(Box::new(Selector::You)),
                rhs: Value::ONE,
            },
            r,
        ));
    }
    // "This creature can't be blocked as long as defending player controls an artifact":
    // read during combat, when there is a defending player.
    if let Some(r) = s.strip_prefix("defending player controls ") {
        return controls(r, Selector::DefendingPlayer, cx);
    }
    // Counted across all of them: "your opponents control eight or more lands".
    if let Some(r) = s.strip_prefix("your opponents control ") {
        return controls(r, Selector::Opponents, cx);
    }
    if let Some(r) = s.strip_prefix("you have two or more opponents") {
        return Some((
            Condition::CountAtLeast {
                what: Selector::Opponents,
                at_least: Value::Fixed(2),
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("a player has ") {
        // "a player has 5 or less life"
        let (n, r) = words::number(r)?;
        let r = r.strip_prefix(" or less life")?;
        return Some((
            Condition::ValueAtLeast {
                lhs: Value::Fixed(n),
                rhs: Value::Min(
                    Box::new(Value::LifeTotal(Box::new(Selector::You))),
                    Box::new(Value::LifeTotal(Box::new(Selector::Opponents))),
                ),
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("you have ") {
        // "you have 10 or less life", "you have no cards in hand".
        if let Some(r) = r.strip_prefix("no cards in hand") {
            return Some((
                Condition::CountAtMost {
                    what: hand(),
                    at_most: Value::ZERO,
                },
                r,
            ));
        }
        if let Some((n, r)) = words::number(r) {
            // "you have seven or more cards in hand", "one or fewer cards in hand".
            if let Some(r) = r.strip_prefix(" or more cards in hand") {
                return Some((
                    Condition::CountAtLeast {
                        what: hand(),
                        at_least: Value::Fixed(n),
                    },
                    r,
                ));
            }
            if let Some(r) = r.strip_prefix(" or fewer cards in hand") {
                return Some((
                    Condition::CountAtMost {
                        what: hand(),
                        at_most: Value::Fixed(n),
                    },
                    r,
                ));
            }
            let life = Value::LifeTotal(Box::new(Selector::You));
            if let Some(r) = r.strip_prefix(" or more life") {
                return Some((at_least(life, n), r));
            }
            if let Some(r) = r.strip_prefix(" or less life") {
                return Some((
                    Condition::ValueAtLeast {
                        lhs: Value::Fixed(n),
                        rhs: life,
                    },
                    r,
                ));
            }
        }
    }
    // "this spell was cast from a graveyard" (flashback and friends).
    if let Some(r) = s
        .strip_prefix("this spell was cast from a graveyard")
        .or_else(|| s.strip_prefix("~ was cast from a graveyard"))
    {
        return Some((Condition::WasCast(Some(Zone::Graveyard)), r));
    }
    // Coven: "you control three or more creatures with different powers".
    if let Some(r) = s.strip_prefix("you control ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more creatures with different powers")
    {
        let creatures = Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::And(vec![
                ObjectFilter::HasType(CardType::Creature),
                ObjectFilter::ControlledBy(Box::new(Selector::You)),
            ]),
        };
        return Some((at_least(Value::DistinctPowers(Box::new(creatures)), n), r));
    }
    // "you gained 3 or more life this turn".
    if let Some(r) = s.strip_prefix("you gained ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more life this turn")
    {
        return Some((
            at_least(Value::LifeGainedThisTurn(Box::new(Selector::You)), n),
            r,
        ));
    }
    // "you have a full party" (CR 700.8): one each of Cleric, Rogue, Warrior and Wizard.
    if let Some(r) = s.strip_prefix("you have a full party") {
        let creatures = Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::And(vec![
                ObjectFilter::HasType(CardType::Creature),
                ObjectFilter::ControlledBy(Box::new(Selector::You)),
            ]),
        };
        return Some((at_least(Value::PartySize(Box::new(creatures)), 4), r));
    }
    // "if X is 5 or more", while X means something.
    if cx.x
        && let Some(r) = s.strip_prefix("x is ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more")
    {
        return Some((at_least(Value::X, n), r));
    }
    // "there are two or more instant and/or sorcery cards in your graveyard".
    if let Some(r) = s.strip_prefix("there are ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more ")
        && let Some((noun, r)) = nouns::noun(r, cx)
        && noun.zone == Zone::Graveyard
        && noun.plural
    {
        return Some((
            Condition::CountAtLeast {
                what: Selector::All {
                    zone: Zone::Graveyard,
                    filter: noun.filter,
                },
                at_least: Value::Fixed(n),
            },
            r,
        ));
    }
    // "there are seven or more cards in your graveyard".
    if let Some(r) = s.strip_prefix("there are ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more cards in your graveyard")
    {
        return Some((
            Condition::CountAtLeast {
                what: Selector::All {
                    zone: Zone::Graveyard,
                    filter: ObjectFilter::OwnedBy(Box::new(Selector::You)),
                },
                at_least: Value::Fixed(n),
            },
            r,
        ));
    }
    // "if you control a commander" (Fierce Guardianship): a commander permanent, anyone's.
    if let Some(r) = s.strip_prefix("you control a commander") {
        return Some((
            Condition::CountAtLeast {
                what: Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(vec![
                        ObjectFilter::ControlledBy(Box::new(Selector::You)),
                        ObjectFilter::IsCommander,
                    ]),
                },
                at_least: Value::ONE,
            },
            r,
        ));
    }
    // "if an opponent cast three or more spells this turn": one opponent, not their total.
    if let Some(r) = s
        .strip_prefix("an opponent cast ")
        .or_else(|| s.strip_prefix("an opponent has cast "))
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more spells this turn")
    {
        return Some((
            at_least(
                Value::MostSpellsCastThisTurn(Box::new(Selector::Opponents)),
                n,
            ),
            r,
        ));
    }
    // "an opponent has eight or more cards in their graveyard" (two players: the one).
    if let Some(r) = s.strip_prefix("an opponent has ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" or more cards in their graveyard")
    {
        return Some((
            Condition::CountAtLeast {
                what: Selector::All {
                    zone: Zone::Graveyard,
                    filter: ObjectFilter::OwnedBy(Box::new(Selector::Opponents)),
                },
                at_least: Value::Fixed(n),
            },
            r,
        ));
    }
    if let Some(r) = s.strip_prefix("~ is monstrous") {
        return Some((Condition::Monstrous, r));
    }
    // "~ has three or more +1/+1 counters on it", "it has a divinity counter on it" ("it"
    // only while it means this object).
    let it_is_self = matches!(cx.it, None | Some(Selector::SelfSource));
    if let Some(r) = s
        .strip_prefix("~ has ")
        .or_else(|| s.strip_prefix("it has ").filter(|_| it_is_self))
    {
        if let Some((n, r)) = words::number(r)
            && let Some(r) = r.strip_prefix(" or more ")
            && let Some((kind, r)) = words::counter(r)
            && let Some(r) = r.strip_prefix("s on it")
        {
            return Some((
                at_least(Value::Counters(Box::new(Selector::SelfSource), kind), n),
                r,
            ));
        }
        if let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
            && let Some((kind, r)) = words::counter(r)
            && let Some(r) = r.strip_prefix(" on it")
        {
            return Some((
                at_least(Value::Counters(Box::new(Selector::SelfSource), kind), 1),
                r,
            ));
        }
    }
    // "there are no depletion counters on ~", "~ has no time counters on it".
    let none_on = s
        .strip_prefix("there are no ")
        .and_then(|r| {
            let (kind, r) = words::counter(r)?;
            Some((kind, r.strip_prefix("s on ~")?))
        })
        .or_else(|| {
            let r = s
                .strip_prefix("~ has no ")
                .or_else(|| s.strip_prefix("it has no ").filter(|_| it_is_self))?;
            let (kind, r) = words::counter(r)?;
            Some((kind, r.strip_prefix("s on it")?))
        });
    if let Some((kind, r)) = none_on {
        return Some((
            Condition::Not(Box::new(at_least(
                Value::Counters(Box::new(Selector::SelfSource), kind),
                1,
            ))),
            r,
        ));
    }
    // "enchanted creature is red", "equipped creature is a Human", "… is legendary".
    for host in [
        "enchanted creature is ",
        "equipped creature is ",
        "enchanted permanent is ",
    ] {
        let Some(r) = s.strip_prefix(host) else {
            continue;
        };
        let (w, rest) = words::first_word(r);
        let (quality, r) = if let Some(c) = words::color(w) {
            (ObjectFilter::HasColor(c), rest)
        } else if w == "legendary" {
            (
                ObjectFilter::HasSupertype(mtg_core::Supertype::Legendary),
                rest,
            )
        } else if let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
            && let Some((noun, r)) = nouns::noun(r, cx)
            && !noun.plural
        {
            (noun.filter, r)
        } else {
            continue;
        };
        return Some((
            Condition::Exists(Selector::All {
                zone: Zone::Battlefield,
                filter: ObjectFilter::And(vec![ObjectFilter::AttachedToSelf, quality]),
            }),
            r,
        ));
    }
    // "~ is attacking", "~ is tapped", "~ is untapped".
    if let Some(r) = s.strip_prefix("~ is ") {
        for (w, f) in [
            ("attacking", ObjectFilter::Attacking),
            ("blocking", ObjectFilter::Blocking),
            ("untapped", ObjectFilter::Tapped(false)),
            ("tapped", ObjectFilter::Tapped(true)),
        ] {
            if let Some(r) = r.strip_prefix(w) {
                return Some((
                    Condition::Exists(Selector::All {
                        zone: Zone::Battlefield,
                        filter: ObjectFilter::And(vec![ObjectFilter::IsSelf, f]),
                    }),
                    r,
                ));
            }
        }
    }
    None
}

fn at_least(lhs: Value, rhs: i32) -> Condition {
    Condition::ValueAtLeast {
        lhs,
        rhs: Value::Fixed(rhs),
    }
}

fn hand() -> Selector {
    Selector::All {
        zone: Zone::Hand,
        filter: ObjectFilter::OwnedBy(Box::new(Selector::You)),
    }
}

/// After "you control ": "a Swamp", "another Elf", "three or more artifacts", "no
/// creatures".
fn controls<'s>(s: &'s str, who: Selector, cx: &Cx) -> Option<(Condition, &'s str)> {
    // "exactly one creature", "exactly two lands": at least and at most.
    if let Some(r) = s.strip_prefix("exactly ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(' ')
        && let Some((noun, r)) = nouns::noun(r, cx)
        && noun.zone == Zone::Battlefield
    {
        let what = Selector::All {
            zone: Zone::Battlefield,
            filter: ObjectFilter::And(vec![noun.filter, ObjectFilter::ControlledBy(Box::new(who))]),
        };
        return Some((
            Condition::And(vec![
                Condition::CountAtLeast {
                    what: what.clone(),
                    at_least: Value::Fixed(n),
                },
                Condition::CountAtMost {
                    what,
                    at_most: Value::Fixed(n),
                },
            ]),
            r,
        ));
    }
    // "N or fewer": at most.
    if let Some((n, r)) = words::number(s)
        && let Some(r) = r.strip_prefix(" or fewer ")
    {
        let (other, r) = match r.strip_prefix("other ") {
            Some(r) => (true, r),
            None => (false, r),
        };
        let (noun, r) = nouns::noun(r, cx)?;
        if !noun.plural || noun.zone != Zone::Battlefield {
            return None;
        }
        let mut filter = vec![noun.filter, ObjectFilter::ControlledBy(Box::new(who))];
        if other {
            filter.push(ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)));
        }
        return Some((
            Condition::CountAtMost {
                what: Selector::All {
                    zone: Zone::Battlefield,
                    filter: ObjectFilter::And(filter),
                },
                at_most: Value::Fixed(n),
            },
            r,
        ));
    }
    let (count, other, r) = if let Some(r) = s.strip_prefix("no ") {
        (None, false, r)
    } else if let Some(r) = s.strip_prefix("another ") {
        (Some(1), true, r)
    } else if let Some(r) = s.strip_prefix("a ").or_else(|| s.strip_prefix("an ")) {
        // "a Forest or a Plains": either. The second article is dropped for parsing; what
        // is left over is the same text either way, so it maps back onto `r`.
        if let Some((a, b)) = r.split_once(" or a ").or_else(|| r.split_once(" or an ")) {
            let joined = format!("{a} or {b}");
            if let Some((noun, rest)) = nouns::noun(&joined, cx)
                && !noun.plural
                && noun.zone == Zone::Battlefield
                && rest.len() <= b.len()
            {
                let rest = &r[r.len() - rest.len()..];
                return Some((
                    Condition::CountAtLeast {
                        what: Selector::All {
                            zone: Zone::Battlefield,
                            filter: ObjectFilter::And(vec![
                                noun.filter,
                                ObjectFilter::ControlledBy(Box::new(who)),
                            ]),
                        },
                        at_least: Value::ONE,
                    },
                    rest,
                ));
            }
        }
        (Some(1), false, r)
    } else {
        let (n, r) = words::number(s)?;
        let r = r.strip_prefix(" or more ")?;
        let (other, r) = match r.strip_prefix("other ") {
            Some(r) => (true, r),
            None => (false, r),
        };
        (Some(n), other, r)
    };
    let (noun, r) = nouns::noun(r, cx)?;
    if noun.zone != Zone::Battlefield {
        return None;
    }
    // Singular after "a"/"another", plural after "no"/"N or more".
    if noun.plural != (count != Some(1)) {
        return None;
    }
    let mut filter = vec![noun.filter, ObjectFilter::ControlledBy(Box::new(who))];
    if other {
        filter.push(ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)));
    }
    let what = Selector::All {
        zone: Zone::Battlefield,
        filter: ObjectFilter::And(filter),
    };
    Some((
        match count {
            None => Condition::CountAtMost {
                what,
                at_most: Value::ZERO,
            },
            Some(n) => Condition::CountAtLeast {
                what,
                at_least: Value::Fixed(n),
            },
        },
        r,
    ))
}

/// What "for each …" and "the number of …" count: objects matching a noun.
pub fn counted<'s>(s: &'s str, cx: &Cx) -> Option<(Selector, &'s str)> {
    let (other, s) = match s.strip_prefix("other ") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (noun, r) = nouns::noun(s, cx)?;
    let mut filter = if other {
        ObjectFilter::And(vec![
            noun.filter,
            ObjectFilter::Not(Box::new(ObjectFilter::IsSelf)),
        ])
    } else {
        noun.filter
    };
    // "for each Equipment attached to it"
    let r = match r
        .strip_prefix(" attached to it")
        .or_else(|| r.strip_prefix(" attached to ~"))
    {
        Some(r) if noun.zone == Zone::Battlefield => {
            filter = ObjectFilter::And(vec![filter, ObjectFilter::AttachedToSource]);
            r
        }
        _ => r,
    };
    // "for each creature blocking it" (rampage)
    let r = match r
        .strip_prefix(" blocking it")
        .or_else(|| r.strip_prefix(" blocking ~"))
    {
        Some(r) if noun.zone == Zone::Battlefield => {
            filter = ObjectFilter::And(vec![filter, ObjectFilter::BlockingSource]);
            r
        }
        _ => r,
    };
    // "creature cards in all graveyards": every graveyard, not just yours.
    let r = match r
        .strip_prefix(" in all graveyards")
        .or_else(|| r.strip_prefix(" in each graveyard"))
    {
        Some(r) if noun.zone == Zone::Graveyard => r,
        _ => r,
    };
    // "for each creature on the battlefield": the default zone, said out loud.
    let r = match r.strip_prefix(" on the battlefield") {
        Some(r) if noun.zone == Zone::Battlefield => r,
        _ => r,
    };
    // A card in a hand or graveyard is only counted where the noun said.
    Some((
        Selector::All {
            zone: noun.zone,
            filter,
        },
        r,
    ))
}
