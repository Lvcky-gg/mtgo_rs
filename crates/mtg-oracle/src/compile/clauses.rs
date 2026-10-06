//! Effect clauses: the verbs.
//!
//! A sentence is a sequence of clauses joined by ", then", " and", or a full stop. Each
//! clause is either imperative ("draw a card", "destroy target creature") — performed by
//! the ability's controller — or has a subject ("target player draws two cards",
//! "~ deals 3 damage to any target").

use mtg_core::{CardType, Color, Zone};
use mtg_ir::{
    Ability, AbilityKind, Effect, Selector, Value,
    effect::{Duration, Modification, Restriction, TokenSpec, ZonePosition},
};

use super::{Cx, nouns, words};

/// A whole effect: one or more sentences. `None` unless every word is accounted for.
pub fn effect(text: &str, cx: &mut Cx) -> Option<Effect> {
    let text = text.trim().strip_suffix('.').unwrap_or(text.trim());
    // "…, where X is the number of creatures you control."
    if let Some((body, what)) = text.split_once(", where x is ") {
        let (v, rest) = value_phrase(what, cx)?;
        if !rest.is_empty() || cx.x {
            return None;
        }
        cx.x = true;
        let mut e = effect(body, cx);
        cx.x = false;
        if let Some(e) = &mut e {
            replace_x(e, &v)?;
        }
        return e;
    }
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        // "… If this spell was kicked, that creature gets -5/-5 until end of turn instead.":
        // the previous effect, or this one when the condition holds.
        if let Some(prev) = out.last()
            && let Some((alt, r)) = instead(rest, prev, cx)
        {
            *out.last_mut()? = alt;
            if r.is_empty() {
                break;
            }
            rest = r.strip_prefix(". ")?;
            continue;
        }
        let (mut e, mut r) = clause(rest, cx)?;
        // "… at the beginning of the next end step": later, once.
        if let Some((on, after)) = delayed_suffix(r) {
            e = delayed(on, e)?;
            r = after;
        }
        // "… for each creature you control": the clause's number, that many times over.
        if let Some(after) = r.strip_prefix(" for each ") {
            let (by, after) = per(after, cx)?;
            if !scale(&mut e, by) {
                return None;
            }
            r = after;
        }
        out.push(e);
        if r.is_empty() {
            break;
        }
        rest = r
            .strip_prefix(". ")
            .or_else(|| r.strip_prefix(", then "))
            .or_else(|| r.strip_prefix(" and "))
            .or_else(|| r.strip_prefix(", and "))
            .or_else(|| r.strip_prefix(". then "))?;
    }
    Some(if out.len() == 1 {
        out.pop()?
    } else {
        Effect::Sequence(out)
    })
}

/// The object an effect acts on, for "it" in a sentence that follows.
fn acted_on(e: &Effect) -> Option<Selector> {
    match e {
        Effect::DealDamage { to, .. } => Some(to.clone()),
        Effect::AddCounters { what, .. }
        | Effect::Continuous { what, .. }
        | Effect::MoveZone { what, .. }
        | Effect::Destroy { what }
        | Effect::Tap { what }
        | Effect::Untap { what } => Some(what.clone()),
        Effect::Sequence(items) => items.iter().find_map(acted_on),
        _ => None,
    }
}

/// "if <condition>, <effect> instead", after `prev`: `prev` unless the condition holds.
/// The alternative may not choose targets of its own — it names the earlier ones — and
/// "it deals N damage instead" repeats the earlier damage with another amount.
fn instead<'s>(s: &'s str, prev: &Effect, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let saved = cx.clone();
    let r = s.strip_prefix("if ")?;
    let (cond, r) = super::conditions::condition(r, cx)?;
    let r = r.strip_prefix(", ")?;
    // "it" and "that creature" in the alternative are what the previous effect acted on.
    let (it, that) = (cx.it.clone(), cx.that_creature.clone());
    if let Some(subject) = acted_on(prev) {
        cx.it = Some(subject);
        cx.that_creature = None;
    }
    let alt = if let Effect::DealDamage { source, to, .. } = prev
        && let Some(r2) = r
            .strip_prefix("it deals ")
            .or_else(|| r.strip_prefix("~ deals "))
        && let Some((amount, r2)) = amount(r2, cx)
        && let Some(r2) = r2.strip_prefix(" damage instead")
    {
        Some((
            Effect::DealDamage {
                source: source.clone(),
                to: to.clone(),
                amount,
            },
            r2,
        ))
    } else {
        let targets = cx.targets.len();
        clause(r, cx)
            .filter(|_| cx.targets.len() == targets)
            .and_then(|(e, r)| Some((e, r.strip_prefix(" instead")?)))
    };
    let Some((then, r)) = alt else {
        *cx = saved;
        return None;
    };
    (cx.it, cx.that_creature) = (it, that);
    Some((
        Effect::If {
            cond,
            then: Box::new(then),
            otherwise: Box::new(prev.clone()),
        },
        r,
    ))
}

/// One clause, trying each shape in turn. A failed attempt is rolled back, so a target
/// slot allocated by a shape that did not match is not left behind.
fn clause<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    type Shape = for<'a> fn(&'a str, &mut Cx) -> Option<(Effect, &'a str)>;
    const SHAPES: &[Shape] = &[
        prevent_combat_damage,
        prevent_target_damage,
        prevent_damage,
        transform,
        becomes_creature,
        add_mana,
        damage_to_itself,
        reorder_top,
        get_energy,
        monstrosity,
        reveal_hand_choose,
        proliferate,
        exile_if_dies,
        adapt_populate,
        dig,
        grant_play,
        amass,
        goad,
        monarch,
        support,
        bolster,
        extra_land,
        gain_control,
        doesnt_untap,
        set_or_switch_pt,
        put_into_library,
        copy_spell,
        explores,
        delayed_clause,
        conditional,
        fight,
        sacrifice_you,
        search,
        attach,
        may,
        equal_to,
        create_token,
        draw,
        regenerate,
        discard_you,
        destroy,
        exile,
        bounce,
        put_onto_battlefield,
        tap_untap,
        counter_spell,
        put_counters,
        scry_surveil,
        mill_you,
        sacrifice_self,
        player_does,
        deals_damage,
        pump,
        cant_block,
        all_must_block,
        assign_unblocked,
        have_block,
        cant_cast_this_turn,
        shuffle_into_library,
    ];
    for shape in SHAPES {
        let saved = cx.clone();
        if let Some(found) = shape(s, cx) {
            return Some(found);
        }
        *cx = saved;
    }
    None
}

fn prevent_combat_damage<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    let rest = s.strip_prefix("prevent all combat damage that would be dealt this turn")?;
    Some((Effect::PreventAllCombatDamage, rest))
}

fn prevent_target_damage<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let rest = s.strip_prefix("prevent all damage that would be dealt to ")?;
    let (to, rest) = nouns::recipient(rest, cx)?;
    // Targeted recipients are selected on announcement. Do not approximate
    // continuously selected groups or source-specific prevention.
    if !matches!(to, Selector::Target { .. }) {
        return None;
    }
    let rest = rest.strip_prefix(" this turn")?;
    Some((Effect::PreventDamage { to }, rest))
}

/// "Prevent the next 3 damage that would be dealt to any target this turn.", "Prevent all
/// combat damage that would be dealt by target attacking creature this turn.", "Prevent
/// all damage that would be dealt to you and creatures you control this turn." (CR 615)
fn prevent_damage<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (amount, r) = prevention_head(s, cx)?;
    let (combat_only, r) = match r.strip_prefix("combat ") {
        Some(r) => (true, r),
        None => (false, r),
    };
    let r = r.strip_prefix("damage that would be dealt")?;
    let (to, r) = match r.strip_prefix(" to ") {
        Some(r) => {
            let (to, r) = shielded(r, cx)?;
            (Some(to), r)
        }
        None => (None, r),
    };
    let (by, r) = match r.strip_prefix(" by ") {
        Some(r) => {
            let (by, plural, r) = nouns::object(r, cx)?;
            battlefield_only(&by, cx)?;
            if plural {
                return None;
            }
            (Some(by), r)
        }
        None => (None, r),
    };
    let r = r.strip_prefix(" this turn")?;
    Some((
        Effect::PreventDamageShield {
            to,
            by,
            combat_only,
            amount,
        },
        r,
    ))
}

/// "~ explores", "it explores", "target creature you control explores" (CR 701.44).
fn explores<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (what, plural, r) = nouns::object(s, cx)?;
    let verb = |r: &'s str, v: &str| {
        if plural {
            r.strip_prefix(&format!(" {v}") as &str)
        } else {
            r.strip_prefix(&format!(" {v}s") as &str)
        }
    };
    battlefield_only(&what, cx)?;
    // "~ connives" (CR 701.50).
    if let Some(r) = verb(r, "connive") {
        return Some((Effect::Connive { what }, r));
    }
    let r = verb(r, "explore")?;
    Some((Effect::Explore { what }, r))
}

/// "copy target instant or sorcery spell. you may choose new targets for the copy." (CR
/// 707.10)
fn copy_spell<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("copy ")?;
    // "Whenever you cast an instant or sorcery spell, copy that spell": the cast one.
    let subject = Some(Selector::Bound(mtg_ir::selector::Binding::EventSubject));
    let (what, plural, r) = match r
        .strip_prefix("that spell")
        .or_else(|| r.strip_prefix("it"))
    {
        Some(rest) if cx.it == subject => (cx.it.clone()?, false, rest),
        _ => nouns::object(r, cx)?,
    };
    if plural {
        return None;
    }
    // Only a spell: the noun must have been about the stack.
    let on_stack = what == Selector::Bound(mtg_ir::selector::Binding::EventSubject)
        || match &what {
            Selector::Target { index } => cx
                .targets
                .get(*index as usize)
                .is_some_and(|t| t.zone == Zone::Stack),
            _ => false,
        };
    if !on_stack {
        return None;
    }
    let (may_change_targets, r) = match r.strip_prefix(". you may choose new targets for the copy")
    {
        Some(r) => (true, r),
        None => (false, r),
    };
    Some((
        Effect::CopySpell {
            what,
            may_change_targets,
        },
        r,
    ))
}

/// "prevent all " or "prevent the next N ": how much a prevention effect stops.
pub fn prevention_head<'s>(s: &'s str, cx: &Cx) -> Option<(Option<Value>, &'s str)> {
    let r = s.strip_prefix("prevent ")?;
    if let Some(r) = r.strip_prefix("all ") {
        return Some((None, r));
    }
    let (n, r) = amount(r.strip_prefix("the next ")?, cx)?;
    Some((Some(n), r.strip_prefix(' ')?))
}

/// Who a prevention shield protects. A shield fixes its recipients as it resolves, so a
/// group of permanents ("creatures you control") is refused: the effect should also
/// cover one that arrives later.
fn shielded<'s>(s: &'s str, cx: &mut Cx) -> Option<(Selector, &'s str)> {
    if let Some(r) = s.strip_prefix("players") {
        return Some((Selector::EachPlayer, r));
    }
    if let Some(r) = s.strip_prefix("you") {
        return Some((Selector::You, r));
    }
    let (to, r) = nouns::recipient(s, cx)?;
    matches!(to, Selector::Target { .. } | Selector::SelfSource).then_some((to, r))
}

/// "you may <clause>".
fn may<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("you may ")?;
    // "You may pay {E}{E}. If you do, …"
    if let Some((n, after)) = r.strip_prefix("pay ").and_then(words::energy) {
        let after = after.strip_prefix(". if you do, ")?;
        let (then, rest) = if_you_do_body(after, cx)?;
        return Some((
            Effect::MayPay {
                cost: mtg_ir::Cost {
                    additional: vec![mtg_ir::AdditionalCost::PayEnergy {
                        amount: Value::Fixed(n),
                    }],
                    ..mtg_ir::Cost::free()
                },
                then: Box::new(then),
            },
            rest,
        ));
    }
    // "You may pay {2}. When you do, …" — a reflexive trigger (CR 603.12).
    if let Some(after) = r.strip_prefix("pay ")
        && let Some((mana, after)) = super::mana_cost(after)
        && let Some(after) = after.strip_prefix(". when you do, ")
    {
        let (then, rest) = reflexive(after, cx)?;
        return Some((
            Effect::MayPay {
                cost: mtg_ir::Cost {
                    mana,
                    ..mtg_ir::Cost::free()
                },
                then: Box::new(then),
            },
            rest,
        ));
    }
    // "You may pay {2}. If you do, …" — an optional payment the rest depends on.
    if let Some(after) = r.strip_prefix("pay ")
        && let Some((mana, after)) = super::mana_cost(after)
    {
        let after = after.strip_prefix(". if you do, ")?;
        let (then, rest) = if_you_do_body(after, cx)?;
        return Some((
            Effect::MayPay {
                cost: mtg_ir::Cost {
                    mana,
                    ..mtg_ir::Cost::free()
                },
                then: Box::new(then),
            },
            rest,
        ));
    }
    let (first, rest) = clause(r, cx)?;
    let prompt = r[..r.len() - rest.len()].to_string();
    // "You may sacrifice a creature. When you do, …" — a reflexive trigger.
    if let Some(after) = rest.strip_prefix(". when you do, ") {
        let possible = possible(&first)?;
        let (then, rest) = reflexive(after, cx)?;
        return Some((
            Effect::If {
                cond: possible,
                then: Box::new(Effect::May {
                    prompt: prompt.into(),
                    then: Box::new(Effect::Sequence(vec![first, then])),
                    otherwise: None,
                }),
                otherwise: Box::new(Effect::Nothing),
            },
            rest,
        ));
    }
    // "You may discard a card. If you do, draw a card." — the rest happens only if the
    // first part did, so it is offered only when the first part is possible.
    if let Some(after) = rest.strip_prefix(". if you do, ") {
        let possible = possible(&first)?;
        let (then, rest) = if_you_do_body(after, cx)?;
        return Some((
            Effect::If {
                cond: possible,
                then: Box::new(Effect::May {
                    prompt: prompt.into(),
                    then: Box::new(Effect::Sequence(vec![first, then])),
                    otherwise: None,
                }),
                otherwise: Box::new(Effect::Nothing),
            },
            rest,
        ));
    }
    Some((
        Effect::May {
            prompt: prompt.into(),
            then: Box::new(first),
            otherwise: None,
        },
        rest,
    ))
}

/// The ability after "when you do,": its own effect and targets (CR 603.12), the rest of
/// that sentence and any that follow it.
fn reflexive<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let mut inner = cx.fresh();
    inner.it = cx.it.clone();
    inner.that_player = cx.that_player.clone();
    let (effect, rest) = if_you_do_body(s, &mut inner)?;
    Some((
        Effect::Reflexive {
            effect: Box::new(effect),
            targets: inner.targets,
        },
        rest,
    ))
}

/// The effect after "if you do,": the rest of that sentence.
fn if_you_do_body<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        let (e, r) = clause(rest, cx)?;
        parts.push(e);
        match r
            .strip_prefix(", then ")
            .or_else(|| r.strip_prefix(" and "))
            .or_else(|| r.strip_prefix(", and "))
        {
            Some(next) => rest = next,
            None => {
                rest = r;
                break;
            }
        }
    }
    let e = if parts.len() == 1 {
        parts.pop()?
    } else {
        Effect::Sequence(parts)
    };
    Some((e, rest))
}

/// When a "you may" action that "if you do" depends on can actually be done. Only
/// actions whose success is exactly this condition are accepted.
fn possible(e: &Effect) -> Option<mtg_ir::trigger::Condition> {
    use mtg_ir::trigger::Condition as C;
    Some(match e {
        Effect::Discard {
            who: Selector::You,
            count,
            at_random: false,
        } => C::CountAtLeast {
            what: Selector::All {
                zone: Zone::Hand,
                filter: mtg_ir::ObjectFilter::OwnedBy(Box::new(Selector::You)),
            },
            at_least: count.clone(),
        },
        Effect::Sacrifice {
            who: Selector::You,
            what: Selector::SelfSource,
        } => C::Exists(Selector::All {
            zone: Zone::Battlefield,
            filter: mtg_ir::ObjectFilter::IsSelf,
        }),
        Effect::Sacrifice {
            who: Selector::You,
            what: Selector::All { filter, .. },
        } => C::CountAtLeast {
            what: Selector::All {
                zone: Zone::Battlefield,
                filter: mtg_ir::ObjectFilter::And(vec![
                    filter.clone(),
                    mtg_ir::ObjectFilter::ControlledBy(Box::new(Selector::You)),
                ]),
            },
            at_least: Value::ONE,
        },
        // Drawing and gaining life always happen when chosen.
        Effect::Draw { .. } | Effect::GainLife { .. } => C::Always,
        _ => return None,
    })
}

/// "draw a card", "draw two cards".
fn draw<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("draw ")?;
    let (n, r) = cards_drawn(r, cx)?;
    Some((
        Effect::Draw {
            who: Selector::You,
            count: n,
        },
        r,
    ))
}

/// What is drawn: "a card", "two cards", or — on top of the normal draw — "an additional
/// card", "two additional cards".
fn cards_drawn<'s>(s: &'s str, cx: &Cx) -> Option<(Value, &'s str)> {
    if let Some(r) = s.strip_prefix("an additional card") {
        return Some((Value::ONE, r));
    }
    if let Some((n, r)) = amount(s, cx)
        && n != Value::ONE
        && let Some(r) = r.strip_prefix(" additional cards")
    {
        return Some((n, r));
    }
    count_of(s, "card", cx)
}

/// "a card" / "two cards" / "X cards"; the noun given in the singular.
fn count_of<'s>(s: &'s str, noun: &str, cx: &Cx) -> Option<(Value, &'s str)> {
    let (n, r) = amount(s, cx)?;
    let r = r.strip_prefix(' ')?;
    let r = if n == Value::ONE {
        r.strip_prefix(noun)?
    } else {
        r.strip_prefix(noun)?.strip_prefix('s')?
    };
    Some((n, r))
}

/// A number, or `X` where the cost being paid defines it (CR 107.3).
pub fn amount<'s>(s: &'s str, cx: &Cx) -> Option<(Value, &'s str)> {
    if cx.x
        && let Some(r) = s.strip_prefix('x')
        && (r.is_empty() || r.starts_with([' ', ',', '.']))
    {
        return Some((Value::X, r));
    }
    if cx.that_much
        && let Some(r) = s
            .strip_prefix("that much")
            .or_else(|| s.strip_prefix("that many"))
    {
        return Some((Value::EventAmount, r));
    }
    let (n, r) = words::number(s)?;
    Some((Value::Fixed(n), r))
}

fn destroy<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("destroy ")?;
    let (what, plural, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    // "Destroy target creature. It can't be regenerated." (CR 701.15c)
    let no_regen = if plural {
        [". they can't be regenerated", ". it can't be regenerated"]
    } else {
        [
            ". it can't be regenerated",
            ". that creature can't be regenerated",
        ]
    };
    for tail in no_regen {
        if let Some(rest) = r.strip_prefix(tail) {
            return Some((
                Effect::Sequence(vec![
                    Effect::Continuous {
                        what: what.clone(),
                        modification: Modification::Restriction(Restriction::CantBeRegenerated),
                        duration: Duration::UntilEndOfTurn,
                    },
                    Effect::Destroy { what },
                ]),
                rest,
            ));
        }
    }
    Some((Effect::Destroy { what }, r))
}

fn exile<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("exile ")?;
    // "exile the top card of your library", "exile the top two cards of your library":
    // what is exiled is "that card" / "them" afterwards.
    if let Some(after) = r.strip_prefix("the top ") {
        let (n, after) = match after.strip_prefix("card of your library") {
            Some(a) => (Value::ONE, a),
            None => {
                let (n, a) = amount(after, cx)?;
                (n, a.strip_prefix(" cards of your library")?)
            }
        };
        cx.moved = true;
        cx.it = Some(Selector::Bound(mtg_ir::selector::Binding::It));
        return Some((
            move_zone(
                Selector::TopOfLibrary {
                    player: Box::new(Selector::You),
                    count: n,
                },
                Zone::Exile,
                ZonePosition::Natural,
            ),
            after,
        ));
    }
    let (what, plural, r) = nouns::object(r, cx)?;
    // "… until ~ leaves the battlefield" (CR 610.3): the card comes back when it does.
    if let Some(r) = r.strip_prefix(" until ~ leaves the battlefield") {
        battlefield_only(&what, cx)?;
        return Some((Effect::ExileUntilSourceLeaves { what }, r));
    }
    if !plural {
        cx.moved = true;
    }
    Some((move_zone(what, Zone::Exile, ZonePosition::Natural), r))
}

/// "a land you control", "a creature you control": one permanent the controller chooses
/// as the effect resolves — not a target.
fn chosen_permanent_you_control<'s>(s: &'s str, cx: &Cx) -> Option<(Selector, &'s str)> {
    let r = s.strip_prefix("a ").or_else(|| s.strip_prefix("an "))?;
    // The noun reads "you control" itself.
    let (n, r) = nouns::noun(r, cx)?;
    let yours = mtg_ir::ObjectFilter::ControlledBy(Box::new(Selector::You));
    let controlled = matches!(&n.filter, mtg_ir::ObjectFilter::And(fs) if fs.contains(&yours));
    if n.plural || n.zone != Zone::Battlefield || !controlled {
        return None;
    }
    Some((
        Selector::ChosenBy {
            chooser: Box::new(Selector::You),
            zone: Zone::Battlefield,
            filter: n.filter,
            count: Value::ONE,
            up_to: false,
        },
        r,
    ))
}

/// "return target creature to its owner's hand", "return target creature card from your
/// graveyard to your hand", "return ~ to its owner's hand".
fn bounce<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("return ")?;
    // After "exile …", "return it" is the card in exile: the new object the move made.
    let moved = cx.moved
        && ["it ", "that card ", "the exiled card ", "~ "]
            .iter()
            .any(|p| r.starts_with(p));
    // "return ~ from your graveyard to …": this card, where it is.
    let r_self = r.strip_prefix("~ from your graveyard");
    let (what, plural, r) = if let Some(r) = r_self {
        (Selector::SelfSource, false, r)
    } else if moved {
        let (_, _, r) = nouns::object(r, cx).or_else(|| {
            r.strip_prefix("that card")
                .or_else(|| r.strip_prefix("the exiled card"))
                .map(|r| (Selector::SelfSource, false, r))
        })?;
        (Selector::Bound(mtg_ir::selector::Binding::It), false, r)
    } else if let Some((what, r)) = chosen_permanent_you_control(r, cx) {
        // "return a land you control to its owner's hand": chosen as it resolves.
        (what, false, r)
    } else {
        nouns::object(r, cx)?
    };
    // "to the battlefield under your control" (reanimating anyone's card).
    if let Some(r) = r.strip_prefix(" to the battlefield") {
        let (tapped, r) = match r.strip_prefix(" tapped") {
            Some(r) => (true, r),
            None => (false, r),
        };
        if let Some(r) = r.strip_prefix(" under your control") {
            return Some((under_your_control(what, tapped), r));
        }
    }
    // "to the battlefield" — under its owner's control, which is where a card goes back
    // to unless the text says otherwise.
    if let Some(r) = r.strip_prefix(" to the battlefield tapped") {
        let mut e = move_zone(what, Zone::Battlefield, ZonePosition::Natural);
        if let Effect::MoveZone { tapped, .. } = &mut e {
            *tapped = true;
        }
        return Some((e, r));
    }
    if let Some(r) = r
        .strip_prefix(" to the battlefield under its owner's control")
        .or_else(|| r.strip_prefix(" to the battlefield"))
    {
        return Some((move_zone(what, Zone::Battlefield, ZonePosition::Natural), r));
    }
    let r = if plural {
        // "to your hand" only for cards from your own graveyard, which are yours.
        let from_graveyard = cx.targets.last().is_some_and(|t| t.zone == Zone::Graveyard);
        r.strip_prefix(" to their owners' hands")
            .or_else(|| r.strip_prefix(" to your hand").filter(|_| from_graveyard))?
    } else {
        r.strip_prefix(" to its owner's hand")
            .or_else(|| r.strip_prefix(" to your hand"))?
    };
    Some((move_zone(what, Zone::Hand, ZonePosition::Natural), r))
}

/// "target opponent reveals their hand. you choose a nonland card from it. that player
/// discards that card." — or "… and exile that card", "… exile that card."
fn reveal_hand_choose<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (who, _, r) = nouns::player(s, cx)?;
    let r = r.strip_prefix(" reveals their hand. you choose ")?;
    let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
    let (head, r) = match r.strip_prefix("card from it") {
        Some(r) => ("", r),
        None => {
            let at = r.find(" card from it")?;
            (&r[..at], &r[at + " card from it".len()..])
        }
    };
    // A bare "nonland", "noncreature": every card but that type.
    let negated = head
        .strip_prefix("non")
        .and_then(|t| words::card_type(t).map(|(t, _)| t));
    let filter = if head.is_empty() {
        mtg_ir::ObjectFilter::Any
    } else if let Some(t) = negated {
        mtg_ir::ObjectFilter::Not(Box::new(mtg_ir::ObjectFilter::HasType(t)))
    } else {
        let (noun, left) = nouns::noun(head, cx)?;
        if !left.is_empty() {
            return None;
        }
        noun.filter
    };
    for (tail, exile) in [
        (". that player discards that card", false),
        (" and exile that card", true),
        (". exile that card", true),
    ] {
        if let Some(r) = r.strip_prefix(tail) {
            return Some((Effect::RevealHandChoose { who, filter, exile }, r));
        }
    }
    None
}

/// "transform ~" (CR 701.28), and the Saga's "exile ~, then return it to the battlefield
/// transformed under your control".
fn transform<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    if let Some(r) =
        s.strip_prefix("exile ~, then return it to the battlefield transformed under your control")
    {
        return Some((
            Effect::ExileReturnTransformed {
                what: Selector::SelfSource,
            },
            r,
        ));
    }
    let r = s.strip_prefix("transform ~")?;
    Some((
        Effect::Transform {
            what: Selector::SelfSource,
        },
        r,
    ))
}

/// "add {b}{b}{b}" (a ritual).
fn add_mana<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    super::add_mana_effect(s)
}

/// "target creature deals damage to itself equal to its power".
fn damage_to_itself<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (what, plural, r) = nouns::object(s, cx)?;
    if plural {
        return None;
    }
    battlefield_only(&what, cx)?;
    let r = r.strip_prefix(" deals damage to itself equal to its power")?;
    Some((
        Effect::DealDamage {
            source: what.clone(),
            to: what.clone(),
            amount: Value::Power(Box::new(what)),
        },
        r,
    ))
}

/// "look at the top four cards of your library, then put them back in any order".
fn reorder_top<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("look at the top ")?;
    let (n, r) = words::number(r)?;
    let r = r
        .strip_prefix(" cards of your library, then put them back in any order")
        .or_else(|| r.strip_prefix(" cards of your library and put them back in any order"))?;
    Some((
        Effect::ReorderLibraryTop {
            who: Selector::You,
            count: Value::Fixed(n),
        },
        r,
    ))
}

/// "you get {e}{e}" (CR 107.14).
fn get_energy<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    let (n, r) = words::energy(s.strip_prefix("you get ")?)?;
    Some((
        Effect::GainEnergy {
            who: Selector::You,
            amount: Value::Fixed(n),
        },
        r,
    ))
}

/// "monstrosity 3", "monstrosity x" (CR 701.37): if this isn't monstrous, put that many
/// +1/+1 counters on it and it becomes monstrous.
fn monstrosity<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("monstrosity ")?;
    let (n, r) = amount(r, cx)?;
    Some((
        Effect::If {
            cond: mtg_ir::trigger::Condition::Not(Box::new(mtg_ir::trigger::Condition::Monstrous)),
            then: Box::new(Effect::Sequence(vec![
                Effect::AddCounters {
                    what: Selector::SelfSource,
                    kind: mtg_core::CounterKind::PlusOnePlusOne,
                    amount: n,
                },
                Effect::BecomeMonstrous {
                    what: Selector::SelfSource,
                },
            ])),
            otherwise: Box::new(Effect::Nothing),
        },
        r,
    ))
}

/// "proliferate" (CR 701.27).
/// CR 701.47 — "amass Zombies 2": if you control no Army, create a 0/0 black Zombie Army
/// creature token first; then put two +1/+1 counters on an Army you control. ("Amass 2",
/// the older wording, is Zombies.) Not modelled: an Army of another kind gaining the
/// amassed type, which matters only when two kinds of amass meet.
fn amass<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("amass ")?;
    let (word, r) = match words::number(r) {
        Some(_) => ("zombies", r),
        None => {
            let (w, r) = words::first_word(r);
            (w, r.strip_prefix(' ')?)
        }
    };
    let (n, r) = words::number(r)?;
    let kind = cx.subtype(word)?;
    let army = cx.subtype("army")?;
    let name = capitalise(word.strip_suffix('s').unwrap_or(word));
    let armies = Selector::All {
        zone: Zone::Battlefield,
        filter: mtg_ir::ObjectFilter::And(vec![
            mtg_ir::ObjectFilter::HasSubtype(army),
            mtg_ir::ObjectFilter::ControlledBy(Box::new(Selector::You)),
        ]),
    };
    let token = TokenSpec {
        card: None,
        name: format!("{name} Army").into(),
        types: vec![CardType::Creature],
        subtypes: vec![kind, army],
        colors: vec![mtg_core::Color::Black],
        power: Value::ZERO,
        toughness: Value::ZERO,
        abilities: Vec::new(),
    };
    let Selector::All { filter, .. } = armies.clone() else {
        return None;
    };
    Some((
        Effect::Sequence(vec![
            Effect::If {
                cond: mtg_ir::trigger::Condition::Exists(armies),
                then: Box::new(Effect::Nothing),
                otherwise: Box::new(Effect::CreateToken {
                    token,
                    count: Value::ONE,
                    controller: Selector::You,
                }),
            },
            Effect::Let {
                slot: mtg_ir::selector::Binding::It,
                what: Selector::ChosenBy {
                    chooser: Box::new(Selector::You),
                    zone: Zone::Battlefield,
                    filter,
                    count: Value::ONE,
                    up_to: false,
                },
                body: Box::new(Effect::AddCounters {
                    what: Selector::Bound(mtg_ir::selector::Binding::It),
                    kind: mtg_core::CounterKind::PlusOnePlusOne,
                    amount: Value::Fixed(n),
                }),
            },
        ]),
        r,
    ))
}

/// "Look at the top four cards of your library. You may reveal a creature card from among
/// them and put it into your hand. Put the rest on the bottom of your library in a random
/// order." — and "put one of them into your hand and the rest into your graveyard".
fn dig<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("look at the top ")?;
    let (n, r) = amount(r, cx)?;
    let r = r.strip_prefix(" cards of your library. ")?;
    // Where the rest go: (zone, random order).
    let rest_of = |r: &'s str| -> Option<((Zone, bool), &'s str)> {
        if let Some(r) = r.strip_prefix(" on the bottom of your library in a random order") {
            Some(((Zone::Library, true), r))
        } else if let Some(r) = r.strip_prefix(" on the bottom of your library in any order") {
            Some(((Zone::Library, false), r))
        } else {
            Some((
                (Zone::Graveyard, false),
                r.strip_prefix(" into your graveyard")?,
            ))
        }
    };
    // "a creature card", "a creature or land card", "a card".
    let card = |r: &'s str| -> Option<(mtg_ir::ObjectFilter, &'s str)> {
        let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
        if let Some(r) = r.strip_prefix("card ") {
            return Some((mtg_ir::ObjectFilter::Any, r));
        }
        let at = r.find(" card ")?;
        let (noun, left) = nouns::noun(&r[..at], cx)?;
        if !left.is_empty() || noun.plural || noun.zone != Zone::Battlefield {
            return None;
        }
        Some((noun.filter, &r[at + " card ".len()..]))
    };
    let dig = |take: i32, up_to, filter, take_to, reveal, (rest_to, rest_random)| Effect::Dig {
        count: n.clone(),
        take: Value::Fixed(take),
        up_to,
        filter,
        take_to,
        reveal,
        rest_to,
        rest_random,
    };
    // "put one of them into your hand and the rest …"
    if let Some(r) = r.strip_prefix("put ")
        && let Some((k, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(" of them into your hand and the ")
        && let Some(r) = r.strip_prefix("rest").or_else(|| r.strip_prefix("other"))
        && let Some((rest, r)) = rest_of(r)
    {
        return Some((
            dig(k, false, mtg_ir::ObjectFilter::Any, Zone::Hand, false, rest),
            r,
        ));
    }
    // "you may reveal a creature card from among them and put it into your hand", "you may
    // put a land card from among them onto the battlefield", then where the rest go.
    let r = r.strip_prefix("you may ")?;
    let (reveal, r) = match r.strip_prefix("reveal ") {
        Some(r) => (true, r),
        None => (false, r.strip_prefix("put ")?),
    };
    let (filter, r) = card(r)?;
    let r = r.strip_prefix("from among them ")?;
    let r = if reveal {
        r.strip_prefix("and put it ")
            .or_else(|| r.strip_prefix("and put that card "))?
    } else {
        r
    };
    let (take_to, r) = if let Some(r) = r.strip_prefix("into your hand") {
        (Zone::Hand, r)
    } else {
        (Zone::Battlefield, r.strip_prefix("onto the battlefield")?)
    };
    let r = r.strip_prefix(". put the rest")?;
    let (rest, r) = rest_of(r)?;
    Some((dig(1, true, filter, take_to, reveal, rest), r))
}

/// "You may play that card this turn", "until the end of your next turn, you may play
/// those cards", "you may cast it this turn": a permission for what was just exiled.
fn grant_play<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    use mtg_ir::effect::PlayUntil;
    let it = cx.it.clone().filter(|_| cx.moved)?;
    let (lead, r) = if let Some(r) = s.strip_prefix("until the end of your next turn, ") {
        (Some(PlayUntil::EndOfYourNextTurn), r)
    } else if let Some(r) = s.strip_prefix("until end of turn, ") {
        (Some(PlayUntil::ThisTurn), r)
    } else {
        (None, s)
    };
    let (cast_only, r) = if let Some(r) = r.strip_prefix("you may play ") {
        (false, r)
    } else {
        (true, r.strip_prefix("you may cast ")?)
    };
    let r = [
        "that card",
        "those cards",
        "them",
        "it",
        "the exiled card",
        "the exiled cards",
    ]
    .iter()
    .find_map(|p| r.strip_prefix(p))?;
    let (until, r) = match lead {
        Some(u) => (u, r),
        None => {
            if let Some(r) = r.strip_prefix(" this turn") {
                (PlayUntil::ThisTurn, r)
            } else {
                (
                    PlayUntil::EndOfYourNextTurn,
                    r.strip_prefix(" until the end of your next turn")?,
                )
            }
        }
    };
    Some((
        Effect::GrantPlay {
            what: it,
            until,
            cast_only,
        },
        r,
    ))
}

/// CR 724 — "you become the monarch", "target player becomes the monarch".
/// "bolster 2" (CR 701.39a): of the creatures you control, one with the least toughness
/// — the controller's choice among ties — gets that many +1/+1 counters.
fn bolster<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    use mtg_ir::ObjectFilter as F;
    let r = s.strip_prefix("bolster ")?;
    let (n, r) = amount(r, cx)?;
    let yours = F::And(vec![
        F::HasType(mtg_core::CardType::Creature),
        F::ControlledBy(Box::new(Selector::You)),
    ]);
    let least = Value::LeastToughness(Box::new(Selector::All {
        zone: Zone::Battlefield,
        filter: yours.clone(),
    }));
    let it = mtg_ir::selector::Binding::It;
    Some((
        Effect::Let {
            slot: it,
            what: Selector::ChosenBy {
                chooser: Box::new(Selector::You),
                zone: Zone::Battlefield,
                filter: F::And(vec![yours, F::ToughnessAtMost(least)]),
                count: Value::ONE,
                up_to: false,
            },
            body: Box::new(Effect::AddCounters {
                what: Selector::Bound(it),
                kind: mtg_core::CounterKind::PlusOnePlusOne,
                amount: n,
            }),
        },
        r,
    ))
}

/// "support 2" (CR 701.41): a +1/+1 counter on each of up to that many other target
/// creatures — "other" only matters on a creature; a spell is never one of them.
fn support<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("support ")?;
    let (n, r) = words::number(r)?;
    if !(2..=3).contains(&n) {
        return None;
    }
    let phrase = format!("up to {n} other target creatures");
    let (what, _, "") = nouns::object(&phrase, cx)? else {
        return None;
    };
    Some((
        Effect::AddCounters {
            what,
            kind: mtg_core::CounterKind::PlusOnePlusOne,
            amount: Value::ONE,
        },
        r,
    ))
}

/// "You may play an additional land this turn" (CR 305.2): one more land play for the
/// spell's or ability's controller, until the turn ends.
fn extra_land<'s>(s: &'s str, _cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("you may play an additional land this turn")?;
    Some((
        continuous(
            Selector::SelfSource,
            vec![Modification::Restriction(
                mtg_ir::effect::Restriction::AdditionalLandPlay,
            )],
            Duration::UntilEndOfTurn,
        ),
        r,
    ))
}

fn monarch<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (who, r) = if let Some(r) = s.strip_prefix("you become the monarch") {
        (Selector::You, r)
    } else {
        let (who, bare, r) = nouns::player(s, cx)?;
        if bare {
            return None;
        }
        (who, r.strip_prefix(" becomes the monarch")?)
    };
    Some((
        Effect::BecomeMonarch {
            who,
            emblem: Some(monarch_emblem()),
        },
        r,
    ))
}

/// The monarch's two triggered abilities (CR 724.2), on an object in the command zone:
/// "At the beginning of your end step, draw a card." and "Whenever a creature deals combat
/// damage to you, its controller becomes the monarch."
fn monarch_emblem() -> TokenSpec {
    use mtg_ir::trigger::{DamageRecipient, EventPattern, Trigger, TriggerTiming};
    let trigger = |on| Trigger {
        on,
        functions_from: Zone::Command,
        intervening_if: None,
        optional: false,
        limit: None,
        timing: TriggerTiming::Normal,
    };
    let ability = |id, kind, text: &str| Ability {
        id: mtg_core::AbilityId(id),
        kind,
        targets: Vec::new(),
        source_text: Some(text.into()),
    };
    TokenSpec {
        card: None,
        name: "The Monarch".into(),
        types: Vec::new(),
        subtypes: Vec::new(),
        colors: Vec::new(),
        power: Value::ZERO,
        toughness: Value::ZERO,
        abilities: vec![
            ability(
                0,
                AbilityKind::Triggered {
                    trigger: trigger(EventPattern::StepBegins {
                        step: mtg_core::Step::End,
                        whose: Selector::You,
                    }),
                    effect: Effect::Draw {
                        who: Selector::You,
                        count: Value::ONE,
                    },
                },
                "At the beginning of your end step, draw a card.",
            ),
            ability(
                1,
                AbilityKind::Triggered {
                    trigger: trigger(EventPattern::DealsDamage {
                        source: mtg_ir::ObjectFilter::HasType(CardType::Creature),
                        to: DamageRecipient::Player(Selector::You),
                        combat_only: true,
                    }),
                    effect: Effect::BecomeMonarch {
                        who: Selector::ControllerOf(Box::new(Selector::Bound(
                            mtg_ir::selector::Binding::EventSubject,
                        ))),
                        emblem: None,
                    },
                },
                "Whenever a creature deals combat damage to you, its controller becomes the \
                 monarch.",
            ),
        ],
    }
}

/// CR 701.38 — "goad target creature": until your next turn it attacks each combat if
/// able (and attacks another player than you if able).
fn goad<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("goad ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    Some((
        Effect::Continuous {
            what,
            modification: Modification::Restriction(Restriction::Goaded),
            duration: Duration::UntilYourNextTurn,
        },
        r,
    ))
}

/// "if that creature would die this turn, exile it instead".
fn exile_if_dies<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("if ")?;
    // After damage to a target: "if a creature dealt damage this way would die this turn",
    // which only a creature can be — not a player, not a planeswalker that was the target.
    let damaged = cx
        .it
        .clone()
        .filter(|it| matches!(it, Selector::Target { .. }));
    let (what, r) = if let Some(it) = damaged.clone()
        && let Some(r) = r.strip_prefix("a creature dealt damage this way")
    {
        let noncreature = Selector::All {
            zone: Zone::Battlefield,
            filter: mtg_ir::ObjectFilter::Not(Box::new(mtg_ir::ObjectFilter::HasType(
                mtg_core::CardType::Creature,
            ))),
        };
        (Selector::Except(Box::new(it), Box::new(noncreature)), r)
    } else if let Some(it) = damaged
        && let Some(r) = r
            .strip_prefix("a permanent dealt damage this way")
            .or_else(|| r.strip_prefix("that creature or planeswalker"))
            .or_else(|| r.strip_prefix("that permanent"))
    {
        (it, r)
    } else {
        let (what, _, r) = nouns::object(r, cx)?;
        (what, r)
    };
    battlefield_only(&what, cx)?;
    let r = r.strip_prefix(" would die this turn, exile ")?;
    let r = r
        .strip_prefix("it instead")
        .or_else(|| r.strip_prefix("that creature instead"))
        .or_else(|| r.strip_prefix("them instead"))?;
    Some((Effect::ExileIfDiesThisTurn { what }, r))
}

/// CR 701.46 — "adapt 2": if this has no +1/+1 counters, put two on it. CR 701.36 —
/// "populate": create a token that's a copy of a creature token you control.
fn adapt_populate<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    use mtg_ir::ObjectFilter as F;
    // The imperative: "Take an extra turn after this one."
    if let Some(r) = s.strip_prefix("take an extra turn after this one") {
        return Some((Effect::ExtraTurn { who: Selector::You }, r));
    }
    if let Some(r) = s.strip_prefix("populate") {
        return Some((
            Effect::Let {
                slot: mtg_ir::selector::Binding::It,
                what: Selector::ChosenBy {
                    chooser: Box::new(Selector::You),
                    zone: Zone::Battlefield,
                    filter: F::And(vec![
                        F::Token,
                        F::HasType(CardType::Creature),
                        F::ControlledBy(Box::new(Selector::You)),
                    ]),
                    count: Value::ONE,
                    up_to: false,
                },
                body: Box::new(Effect::CreateTokenCopy {
                    of: Selector::Bound(mtg_ir::selector::Binding::It),
                    count: Value::ONE,
                    controller: Selector::You,
                }),
            },
            r,
        ));
    }
    let (n, r) = words::number(s.strip_prefix("adapt ")?)?;
    let pluses = mtg_core::CounterKind::PlusOnePlusOne;
    Some((
        Effect::If {
            cond: mtg_ir::trigger::Condition::Exists(Selector::All {
                zone: Zone::Battlefield,
                filter: F::And(vec![F::IsSelf, F::HasCounter(pluses)]),
            }),
            then: Box::new(Effect::Nothing),
            otherwise: Box::new(Effect::AddCounters {
                what: Selector::SelfSource,
                kind: pluses,
                amount: Value::Fixed(n),
            }),
        },
        r,
    ))
}

fn proliferate<'s>(s: &'s str, _: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("proliferate")?;
    Some((Effect::Proliferate, r))
}

/// "gain control of target creature until end of turn", "gain control of target
/// artifact" (layer 2).
fn gain_control<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("gain control of ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    let (duration, r) = match r.strip_prefix(" until end of turn") {
        Some(r) => (Duration::UntilEndOfTurn, r),
        None => (Duration::Permanent, r),
    };
    // "… for as long as you control ~" and other durations are not read.
    if r.starts_with(" for as long") || r.starts_with(" until ") {
        return None;
    }
    Some((
        Effect::GainControl {
            what,
            who: Selector::You,
            duration,
        },
        r,
    ))
}

/// "switch target creature's power and toughness until end of turn", "target creature has
/// base power and toughness 1/1 until end of turn".
fn set_or_switch_pt<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    if let Some(r) = s.strip_prefix("switch ") {
        const TAIL: &str = "'s power and toughness until end of turn";
        let at = r.find(TAIL)?;
        let (what, plural, left) = nouns::object(&r[..at], cx)?;
        if plural || !left.is_empty() {
            return None;
        }
        battlefield_only(&what, cx)?;
        let r = &r[at + TAIL.len()..];
        return Some((
            Effect::Continuous {
                what,
                modification: Modification::SwitchPowerToughness,
                duration: Duration::UntilEndOfTurn,
            },
            r,
        ));
    }
    let (what, plural, r) = nouns::object(s, cx)?;
    battlefield_only(&what, cx)?;
    let r = if plural {
        r.strip_prefix(" have base power and toughness ")?
    } else {
        r.strip_prefix(" has base power and toughness ")?
    };
    let ((p, t), r) = words::pt(r)?;
    let r = r.strip_prefix(" until end of turn")?;
    Some((
        Effect::Continuous {
            what,
            modification: Modification::SetBasePowerToughness {
                power: Value::Fixed(p),
                toughness: Value::Fixed(t),
            },
            duration: Duration::UntilEndOfTurn,
        },
        r,
    ))
}

/// "that creature doesn't untap during its controller's next untap step", "those
/// creatures don't untap during their controller's next untap step".
fn doesnt_untap<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (what, r) = if let Some(r) = s
        .strip_prefix("those creatures")
        .or_else(|| s.strip_prefix("those permanents"))
    {
        (cx.it.clone()?, r)
    } else {
        let (what, _, r) = nouns::object(s, cx)?;
        (what, r)
    };
    battlefield_only(&what, cx)?;
    let r = r
        .strip_prefix(" doesn't untap during its controller's next untap step")
        .or_else(|| r.strip_prefix(" don't untap during their controller's next untap step"))
        .or_else(|| r.strip_prefix(" don't untap during their controllers' next untap steps"))?;
    Some((
        Effect::Continuous {
            what,
            modification: Modification::Restriction(Restriction::CantUntapDuringUntapStep),
            duration: Duration::ThroughNextUntapStep,
        },
        r,
    ))
}

/// "put target creature on top of its owner's library", "put target nonland permanent on
/// the bottom of its owner's library".
fn put_into_library<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("put ")?;
    let (what, plural, r) = nouns::object(r, cx)?;
    // A permanent, or a card in a graveyard: "put target card from a graveyard on the
    // bottom of its owner's library".
    let from_graveyard = matches!(&what, Selector::Target { index }
        if cx.targets.get(*index as usize).is_some_and(|t| t.zone == Zone::Graveyard));
    if !from_graveyard {
        battlefield_only(&what, cx)?;
    }
    // "your library" only where the card can only be yours.
    let yours = from_graveyard
        && matches!(&what, Selector::Target { index }
        if cx.targets.get(*index as usize).is_some_and(|t| {
            matches!(&t.filter, mtg_ir::ObjectFilter::And(parts)
                if parts.iter().any(|p| matches!(p, mtg_ir::ObjectFilter::OwnedBy(w)
                    if **w == Selector::You)))
                || matches!(&t.filter, mtg_ir::ObjectFilter::OwnedBy(w) if **w == Selector::You)
        }));
    let library = |r: &'s str, at: &str| {
        r.strip_prefix(&*format!(" {at} its owner's library"))
            .or_else(|| {
                r.strip_prefix(&*format!(" {at} your library"))
                    .filter(|_| yours)
            })
    };
    let (position, r) = if plural {
        return None;
    } else if let Some(r) = library(r, "on top of") {
        (ZonePosition::Top, r)
    } else if let Some(r) = library(r, "into") {
        let (position, r) = if let Some(r) = r.strip_prefix(" second from the top") {
            (ZonePosition::FromTop(1), r)
        } else {
            (
                ZonePosition::FromTop(2),
                r.strip_prefix(" third from the top")?,
            )
        };
        (position, r)
    } else {
        (ZonePosition::Bottom, library(r, "on the bottom of")?)
    };
    Some((move_zone(what, Zone::Library, position), r))
}

/// Onto the battlefield under the controller's control.
fn under_your_control(what: Selector, tapped: bool) -> Effect {
    Effect::MoveZone {
        what,
        to: Zone::Battlefield,
        owner_relative_to: None,
        position: ZonePosition::Natural,
        tapped,
        face_down: false,
        under_control_of: Some(Selector::You),
    }
}

/// "put target creature card from a graveyard onto the battlefield under your control".
fn put_onto_battlefield<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("put ")?;
    // "put a creature card from your hand onto the battlefield [tapped]": one the
    // controller chooses as this resolves.
    if let Some(r) = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))
        && let Some(at) = r.find(" card from your hand onto the battlefield")
    {
        let head = &r[..at];
        let filter = if head.is_empty() {
            mtg_ir::ObjectFilter::Any
        } else {
            let (noun, left) = nouns::noun(head, cx)?;
            if !left.is_empty() || noun.zone != Zone::Battlefield {
                return None;
            }
            noun.filter
        };
        let rest = &r[at + " card from your hand onto the battlefield".len()..];
        let (tapped, rest) = match rest.strip_prefix(" tapped") {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        let mut e = move_zone(
            Selector::ChosenBy {
                chooser: Box::new(Selector::You),
                zone: Zone::Hand,
                filter: mtg_ir::ObjectFilter::And(vec![
                    filter,
                    mtg_ir::ObjectFilter::OwnedBy(Box::new(Selector::You)),
                ]),
                count: Value::ONE,
                up_to: false,
            },
            Zone::Battlefield,
            ZonePosition::Natural,
        );
        if let Effect::MoveZone { tapped: t, .. } = &mut e {
            *t = tapped;
        }
        return Some((e, rest));
    }
    let (what, _, r) = nouns::object(r, cx)?;
    if matches!(what, Selector::SelfSource) {
        return None;
    }
    let r = r.strip_prefix(" onto the battlefield")?;
    let (tapped, r) = match r.strip_prefix(" tapped") {
        Some(r) => (true, r),
        None => (false, r),
    };
    if let Some(r) = r.strip_prefix(" under your control") {
        return Some((under_your_control(what, tapped), r));
    }
    let r = r.strip_prefix(" under its owner's control").unwrap_or(r);
    let mut e = move_zone(what, Zone::Battlefield, ZonePosition::Natural);
    if let Effect::MoveZone { tapped: t, .. } = &mut e {
        *t = tapped;
    }
    Some((e, r))
}

fn move_zone(what: Selector, to: Zone, position: ZonePosition) -> Effect {
    Effect::MoveZone {
        what,
        to,
        owner_relative_to: None,
        position,
        tapped: false,
        face_down: false,
        under_control_of: None,
    }
}

fn tap_untap<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    if let Some(r) = s.strip_prefix("tap ") {
        let (what, _, r) = nouns::object(r, cx)?;
        battlefield_only(&what, cx)?;
        return Some((Effect::Tap { what }, r));
    }
    let r = s.strip_prefix("untap ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    Some((Effect::Untap { what }, r))
}

fn counter_spell<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("counter ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    // Only a targeted spell: the stack is where it must be.
    let Selector::Target { index } = what else {
        return None;
    };
    if cx.targets.get(index as usize)?.zone != Zone::Stack {
        return None;
    }
    // "If that spell is countered this way, exile it instead of putting it into its owner's
    // graveyard."
    let exiled = |r: &'s str| {
        r.strip_prefix(
            ". if that spell is countered this way, exile it instead of putting it into its \
             owner's graveyard",
        )
        .map_or((false, r), |r| (true, r))
    };
    if let Some(after) = r.strip_prefix(" unless its controller pays ") {
        let (mana, rest) = super::mana_cost(after)?;
        let (exile, rest) = exiled(rest);
        return Some((
            Effect::CounterUnlessPays {
                what,
                mana,
                life: None,
                discard: false,
                exile,
            },
            rest,
        ));
    }
    let (exile, r) = exiled(r);
    Some((Effect::CounterSpell { what, exile }, r))
}

/// "put a +1/+1 counter on target creature", "put two +1/+1 counters on ~".
fn put_counters<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("put ")?;
    let (n, r) = amount(r, cx)?;
    let r = r.strip_prefix(' ')?;
    let (kind, r) = words::counter(r)?;
    let r = if n == Value::ONE {
        r
    } else {
        r.strip_prefix('s')?
    };
    let r = r.strip_prefix(" on ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    Some((
        Effect::AddCounters {
            what,
            kind,
            amount: n,
        },
        r,
    ))
}

/// "scry 2", "surveil 1" (CR 701.22, 701.25).
fn scry_surveil<'s>(s: &'s str, _cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (other_zone, r) = if let Some(r) = s.strip_prefix("scry ") {
        (Zone::Library, r)
    } else {
        let r = s.strip_prefix("surveil ")?;
        (Zone::Graveyard, r)
    };
    let (n, r) = words::number(r)?;
    Some((
        Effect::LookAndSort {
            who: Selector::You,
            count: Value::Fixed(n),
            keep_zone: Zone::Library,
            other_zone,
        },
        r,
    ))
}

/// "mill two cards" — the imperative, for the controller.
fn mill_you<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("mill ")?;
    let (n, r) = count_of(r, "card", cx)?;
    Some((mill(Selector::You, n), r))
}

fn mill(who: Selector, n: Value) -> Effect {
    move_zone(
        Selector::TopOfLibrary {
            player: Box::new(who),
            count: n,
        },
        Zone::Graveyard,
        ZonePosition::Natural,
    )
}

/// "sacrifice ~".
fn sacrifice_self<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    // "sacrifice them", "sacrifice it": what an earlier step made (tokens), each of them.
    let made = Some(Selector::Bound(mtg_ir::selector::Binding::It));
    if cx.it == made
        && let Some(r) = s
            .strip_prefix("sacrifice them")
            .or_else(|| s.strip_prefix("sacrifice it"))
            .or_else(|| s.strip_prefix("sacrifice those tokens"))
            .or_else(|| s.strip_prefix("sacrifice that token"))
            .filter(|r| !r.starts_with(|c: char| c.is_alphanumeric()))
    {
        let it = Selector::Bound(mtg_ir::selector::Binding::It);
        return Some((
            Effect::ForEach {
                what: it.clone(),
                body: Box::new(Effect::Sacrifice {
                    who: Selector::You,
                    what: it,
                }),
            },
            r,
        ));
    }
    // "sacrifice it", where "it" is this permanent ("when ~ attacks, sacrifice it").
    let r = s.strip_prefix("sacrifice ~").or_else(|| {
        s.strip_prefix("sacrifice it")
            .filter(|r| !r.starts_with(|c: char| c.is_alphanumeric()))
            .filter(|_| cx.it == Some(Selector::SelfSource))
    })?;
    let sacrifice = Effect::Sacrifice {
        who: Selector::You,
        what: Selector::SelfSource,
    };
    // "sacrifice ~ unless it escaped", "… unless an opponent was dealt damage this turn"
    if let Some(after) = r.strip_prefix(" unless ")
        && let Some((cond, r)) = super::conditions::condition(after, cx)
    {
        return Some((
            Effect::If {
                cond,
                then: Box::new(Effect::Nothing),
                otherwise: Box::new(sacrifice),
            },
            r,
        ));
    }
    // "sacrifice ~ unless you discard a land card": the controller may do it, or else
    // sacrifices this. Offered only when it is possible.
    if let Some(after) = r.strip_prefix(" unless you ")
        && !after.starts_with("pay ")
        && let Some((first, r)) = clause(after, cx)
        && let Some(possible) = possible(&first)
    {
        let prompt = after[..after.len() - r.len()].to_string();
        return Some((
            Effect::If {
                cond: possible,
                then: Box::new(Effect::May {
                    prompt: prompt.into(),
                    then: Box::new(first),
                    otherwise: Some(Box::new(sacrifice.clone())),
                }),
                otherwise: Box::new(sacrifice),
            },
            r,
        ));
    }
    // "sacrifice ~ unless you pay {1}{G}"
    if let Some(cost) = r.strip_prefix(" unless you pay ")
        && let Some((mana, r)) = super::mana_cost(cost)
    {
        return Some((
            Effect::UnlessPays {
                cost: mtg_ir::Cost {
                    mana,
                    ..mtg_ir::Cost::free()
                },
                times: Value::ONE,
                otherwise: Box::new(sacrifice),
            },
            r,
        ));
    }
    Some((sacrifice, r))
}

/// A player doing something: draws, gains or loses life, discards, mills, sacrifices.
fn player_does<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (who, bare, r) = nouns::player(s, cx)?;
    let (first, mut r) = player_verb(who.clone(), bare, r, cx)?;
    // "target player draws two cards and loses 2 life": one subject, several verbs.
    let mut effects = vec![first];
    while let Some(rest) = r.strip_prefix(" and") {
        let saved = cx.clone();
        match player_verb(who.clone(), bare, rest, cx) {
            Some((e, rest)) => {
                effects.push(e);
                r = rest;
            }
            None => {
                *cx = saved;
                break;
            }
        }
    }
    let effect = if effects.len() == 1 {
        effects.pop()?
    } else {
        Effect::Sequence(effects)
    };
    Some((effect, r))
}

/// One verb phrase of [`player_does`], after its subject: " draws two cards".
fn player_verb<'s>(
    who: Selector,
    bare: bool,
    r: &'s str,
    cx: &mut Cx,
) -> Option<(Effect, &'s str)> {
    let verb = |r: &'s str, v: &str| -> Option<&'s str> {
        let r = r.strip_prefix(' ')?.strip_prefix(v)?;
        if bare {
            Some(r)
        } else {
            // "draws", "loses", "discards"; "sacrifices".
            r.strip_prefix('s')
        }
    };
    if let Some(r) = verb(r, "draw") {
        let (n, r) = cards_drawn(r.strip_prefix(' ')?, cx)?;
        return Some((Effect::Draw { who, count: n }, r));
    }
    if let Some(r) = verb(r, "gain")
        .filter(|_| !bare)
        .or_else(|| verb(r, "gain"))
    {
        let (n, r) = amount(r.strip_prefix(' ')?, cx)?;
        let r = r.strip_prefix(" life")?;
        return Some((Effect::GainLife { who, amount: n }, r));
    }
    // "defending player gets a poison counter", "each opponent gets two poison counters"
    if let Some(r) = verb(r, "get")
        && let Some(r) = r.strip_prefix(' ')
        && let Some((n, r)) = r
            .strip_prefix("a poison counter")
            .map(|r| (Value::ONE, r))
            .or_else(|| {
                let (n, r) = amount(r, cx)?;
                Some((n, r.strip_prefix(" poison counters")?))
            })
    {
        return Some((Effect::GivePoison { who, amount: n }, r));
    }
    if let Some(r) = verb(r, "lose") {
        let (n, r) = amount(r.strip_prefix(' ')?, cx)?;
        let r = r.strip_prefix(" life")?;
        return Some((Effect::LoseLife { who, amount: n }, r));
    }
    if let Some(r) = verb(r, "discard") {
        let r = r.strip_prefix(' ')?;
        // "At random" is not compiled: the engine's random discard is not yet drawn from
        // the game's seeded generator.
        if r.contains("at random") {
            return None;
        }
        let (n, r) = count_of(r, "card", cx)?;
        let at_random = false;
        return Some((
            Effect::Discard {
                who,
                count: n,
                at_random,
            },
            r,
        ));
    }
    if let Some(r) = verb(r, "mill") {
        let (n, r) = count_of(r.strip_prefix(' ')?, "card", cx)?;
        return Some((mill(who, n), r));
    }
    // "take an extra turn after this one" (CR 500.7)
    if let Some(r) = verb(r, "take")
        && let Some(r) = r.strip_prefix(" an extra turn after this one")
    {
        return Some((Effect::ExtraTurn { who }, r));
    }
    // "that player exiles the top card of their library"
    if let Some(r) = verb(r, "exile")
        && let Some(r) = r.strip_prefix(" the top ")
    {
        let (n, r) = match r.strip_prefix("card of ") {
            Some(r) => (Value::ONE, r),
            None => {
                let (n, r) = amount(r, cx)?;
                (n, r.strip_prefix(" cards of ")?)
            }
        };
        let r = r
            .strip_prefix("their library")
            .or_else(|| r.strip_prefix("your library").filter(|_| bare))?;
        return Some((
            move_zone(
                Selector::TopOfLibrary {
                    player: Box::new(who),
                    count: n,
                },
                Zone::Exile,
                ZonePosition::Natural,
            ),
            r,
        ));
    }
    if let Some(r) = verb(r, "sacrifice") {
        // "sacrifices a creature": the player chooses one they control. "Sacrifice another
        // creature": one other than this.
        let r = r.strip_prefix(' ')?;
        // "sacrifices two permanents": one at a time, each chosen as it is sacrificed.
        if let Some((k, rest)) = words::number(r)
            && k > 1
            && let Some(rest) = rest.strip_prefix(' ')
            && let Some((n, rest)) = nouns::noun(rest, cx)
            && n.plural
            && n.zone == Zone::Battlefield
        {
            return Some((
                Effect::Repeat {
                    times: Value::Fixed(k),
                    body: Box::new(Effect::Sacrifice {
                        who,
                        what: Selector::All {
                            zone: Zone::Battlefield,
                            filter: n.filter,
                        },
                    }),
                },
                rest,
            ));
        }
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
        // "… a creature of their choice": the sacrificing player chooses, as always.
        let r = r.strip_prefix(" of their choice").unwrap_or(r);
        let filter = if other {
            mtg_ir::ObjectFilter::And(vec![
                n.filter,
                mtg_ir::ObjectFilter::Not(Box::new(mtg_ir::ObjectFilter::IsSelf)),
            ])
        } else {
            n.filter
        };
        return Some((
            Effect::Sacrifice {
                who,
                what: Selector::All {
                    zone: Zone::Battlefield,
                    filter,
                },
            },
            r,
        ));
    }
    None
}

/// "~ deals 3 damage to any target", "~ deals 1 damage to each opponent".
fn deals_damage<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (source, _, r) = nouns::object(s, cx)?;
    let r = r.strip_prefix(" deals ")?;
    let (n, r) = amount(r, cx)?;
    if let Some(among) = r.strip_prefix(" damage divided as you choose among ") {
        let Value::Fixed(total) = n else {
            return None;
        };
        return divided(source, total, among, cx);
    }
    let r = r.strip_prefix(" damage to ")?;
    let (to, r) = nouns::recipient(r, cx)?;
    let first = Effect::DealDamage {
        source: source.clone(),
        to,
        amount: n,
    };
    // "~ deals 2 damage to target creature and 2 damage to target player", "… and 1
    // damage to you": a second recipient, from the same source.
    let saved = cx.clone();
    if let Some(after) = r.strip_prefix(" and ")
        && let Some((m, after)) = amount(after, cx)
        && let Some(after) = after.strip_prefix(" damage to ")
        && let Some((to, after)) = nouns::recipient(after, cx)
    {
        let second = Effect::DealDamage {
            source,
            to,
            amount: m,
        };
        return Some((Effect::Sequence(vec![first, second]), after));
    }
    *cx = saved;
    Some((first, r))
}

/// "… divided as you choose among one, two, or three targets" (CR 601.2d): one target slot
/// per point of damage, any of which may name the same target, so the division is announced
/// with the targets. Slots are mandatory: all the damage is divided.
fn divided<'s>(
    source: Selector,
    total: i32,
    among: &'s str,
    cx: &mut Cx,
) -> Option<(Effect, &'s str)> {
    let (most, r) = if let Some(r) = among.strip_prefix("one or two ") {
        (2, r)
    } else if let Some(r) = among.strip_prefix("one, two, or three ") {
        (3, r)
    } else if let Some(r) = among.strip_prefix("any number of ") {
        (total, r)
    } else {
        let r = among.strip_prefix("up to ")?;
        let (n, r) = words::number(r)?;
        (n, r.strip_prefix(' ')?)
    };
    if !(1..=10).contains(&total) || total > most {
        return None;
    }
    let (spec, rest) = if let Some(rest) = r.strip_prefix("targets") {
        (
            mtg_ir::selector::TargetSpec {
                zone: Zone::Battlefield,
                filter: mtg_ir::ObjectFilter::Or(vec![
                    mtg_ir::ObjectFilter::HasType(CardType::Creature),
                    mtg_ir::ObjectFilter::HasType(CardType::Planeswalker),
                ]),
                allows_players: true,
                players: None,
                mode: None,
                count: Value::ONE,
                up_to: false,
                distinct_from_other_targets: false,
            },
            rest,
        )
    } else {
        let r = r.strip_prefix("target ")?;
        let (noun, rest) = nouns::noun(r, cx)?;
        if !noun.plural || noun.zone != Zone::Battlefield {
            return None;
        }
        (
            mtg_ir::selector::TargetSpec {
                zone: noun.zone,
                filter: noun.filter,
                allows_players: false,
                players: None,
                mode: None,
                count: Value::ONE,
                up_to: false,
                distinct_from_other_targets: false,
            },
            rest,
        )
    };
    let shares: Vec<Selector> = (0..total).map(|_| cx.target(spec.clone())).collect();
    cx.it = Some(Selector::Union(shares.clone()));
    Some((Effect::DealDamageDivided { source, shares }, rest))
}

/// "target creature gets +2/+2 until end of turn", "creatures you control get +1/+0 and
/// gain haste until end of turn", "~ gains flying until end of turn".
fn pump<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    // "Until end of turn, target creature gets +1/+1 and gains trample": the duration said
    // first instead of last.
    let (leading, s) = match s.strip_prefix("until end of turn, ") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (what, plural, r) = nouns::object(s, cx)?;
    battlefield_only(&what, cx)?;
    // "each creature you control gets": many objects, a singular verb.
    let plural = plural && !nouns::each_singular(s);
    // "two target creatures each get +1/+1": the same verb, with "each" between.
    let r = if plural {
        r.strip_prefix(" each").unwrap_or(r)
    } else {
        r
    };
    let (mut mods, mut r) = grants(r, plural, "gain", cx.x, cx)?;
    // "gets +1/+1 for each creature you control until end of turn", or "… until end of
    // turn for each creature blocking it".
    let mut scaled = false;
    if let Some(after) = r.strip_prefix(" for each ") {
        let (by, after) = per(after, cx)?;
        scale_mods(&mut mods, by);
        r = after;
        scaled = true;
    }
    let mut r = if leading {
        r
    } else {
        r.strip_prefix(" until end of turn")?
    };
    if !scaled
        && let Some(after) = r.strip_prefix(" for each ")
        && let Some((by, after)) = per(after, cx)
    {
        scale_mods(&mut mods, by);
        r = after;
    }
    // "… gets +2/+2 until end of turn and must be blocked this turn if able"
    if let Some(after) = r.strip_prefix(" and must be blocked this turn if able") {
        mods.push(Modification::Restriction(Restriction::MustBeBlocked));
        r = after;
    }
    Some((continuous(what, mods, Duration::UntilEndOfTurn), r))
}

/// "gets +N/+N", "gains K", "gets +N/+N and gains K" — the predicate of a pump or an
/// anthem. `verb` is "gain" for one-shot effects and "have" for static abilities.
pub fn grants<'s>(
    s: &'s str,
    plural: bool,
    verb: &str,
    x_ok: bool,
    cx: &Cx,
) -> Option<(Vec<Modification>, &'s str)> {
    let get = if plural { " get " } else { " gets " };
    let has = match (verb, plural) {
        ("have", false) => " has ".to_string(),
        ("have", true) => " have ".to_string(),
        (v, false) => format!(" {v}s "),
        (v, true) => format!(" {v} "),
    };
    let and_has = format!(" and{}", has);
    let mut out = Vec::new();
    let mut r = s;
    if let Some(after) = r.strip_prefix(get) {
        let ((power, toughness), after) = pt_delta_value(after, x_ok)?;
        out.push(Modification::ModifyPowerToughness { power, toughness });
        r = after;
        if let Some(after) = r.strip_prefix(and_has.as_str()) {
            let (ms, after) = granted(after, verb == "have", cx)?;
            out.extend(ms);
            r = after;
        }
    } else {
        let after = r.strip_prefix(has.as_str())?;
        let (ms, after) = granted(after, verb == "have", cx)?;
        out.extend(ms);
        r = after;
    }
    Some((out, r))
}

/// What "gains …" / "has …" gives: keywords, or protection from a quality ("protection
/// from red", "protection from the color of your choice" — not for a static ability,
/// which has no moment to choose).
fn granted<'s>(s: &'s str, is_static: bool, cx: &Cx) -> Option<(Vec<Modification>, &'s str)> {
    if let Some(q) = s.strip_prefix("protection from ") {
        let end = [" until end of turn", ".", ", ", " and "]
            .iter()
            .filter_map(|stop| q.find(stop))
            .min()
            .unwrap_or(q.len());
        // "protection from red and from black" keeps its " and from ".
        let end = if q[end..].starts_with(" and from ") {
            [" until end of turn", ".", ", "]
                .iter()
                .filter_map(|stop| q.find(stop))
                .min()
                .unwrap_or(q.len())
        } else {
            end
        };
        let (quality, rest) = q.split_at(end);
        let (from, chosen_color) = if quality == "the color of your choice" {
            if is_static {
                return None;
            }
            (mtg_ir::ObjectFilter::Any, true)
        } else {
            (super::protection_quality(quality, cx)?, false)
        };
        return Some((
            vec![Modification::Restriction(
                mtg_ir::effect::Restriction::Protection { from, chosen_color },
            )],
            rest,
        ));
    }
    let (ks, r) = words::keyword_list(s)?;
    Some((ks.into_iter().map(grant).collect(), r))
}

/// A granted keyword, as a layer 6 modification.
pub fn grant(k: mtg_core::Keyword) -> Modification {
    Modification::GrantAbility(Box::new(Ability {
        id: mtg_core::AbilityId(0),
        kind: AbilityKind::Keyword(k),
        targets: Vec::new(),
        source_text: None,
    }))
}

fn continuous(what: Selector, mods: Vec<Modification>, duration: Duration) -> Effect {
    let mut effects: Vec<Effect> = mods
        .into_iter()
        .map(|modification| Effect::Continuous {
            what: what.clone(),
            modification,
            duration,
        })
        .collect();
    if effects.len() == 1 {
        effects.pop().unwrap_or(Effect::Nothing)
    } else {
        Effect::Sequence(effects)
    }
}

/// "target creature can't block this turn".
fn cant_block<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    // Before the subject is read: naming a target makes it the new "it".
    let it_is_self = cx.it == Some(Selector::SelfSource);
    let (what, _, r) = nouns::object(s, cx)?;
    battlefield_only(&what, cx)?;
    // "blocks ~ this combat if able", "blocks it this turn if able" — "it" being this
    // permanent, the source of the requirement.
    for (lead, duration) in [
        (" blocks ~ this turn if able", Duration::UntilEndOfTurn),
        (" blocks ~ this combat if able", Duration::UntilEndOfCombat),
        (" blocks it this turn if able", Duration::UntilEndOfTurn),
        (" blocks it this combat if able", Duration::UntilEndOfCombat),
    ] {
        if let Some(r) = r.strip_prefix(lead)
            && (lead.contains('~') || it_is_self)
        {
            return Some((
                continuous(
                    what,
                    vec![Modification::Restriction(Restriction::MustBlockSource)],
                    duration,
                ),
                r,
            ));
        }
    }
    let (rs, r) = if let Some(r) = r.strip_prefix(" can't attack or block this turn") {
        (vec![Restriction::CantAttack, Restriction::CantBlock], r)
    } else if let Some(r) = r.strip_prefix(" can't block this turn") {
        (vec![Restriction::CantBlock], r)
    } else if let Some(r) = r.strip_prefix(" can't attack this turn") {
        (vec![Restriction::CantAttack], r)
    } else if let Some(r) = r.strip_prefix(" must be blocked this turn if able") {
        (vec![Restriction::MustBeBlocked], r)
    } else if let Some(r) = r
        .strip_prefix(" blocks this turn if able")
        .or_else(|| r.strip_prefix(" block this turn if able"))
    {
        (vec![Restriction::MustBlock], r)
    } else {
        let r = r.strip_prefix(" can't be blocked this turn")?;
        (
            vec![Restriction::CantBeBlockedExceptBy(
                mtg_ir::ObjectFilter::Not(Box::new(mtg_ir::ObjectFilter::Any)),
            )],
            r,
        )
    };
    Some((
        continuous(
            what,
            rs.into_iter().map(Modification::Restriction).collect(),
            Duration::UntilEndOfTurn,
        ),
        r,
    ))
}

/// "you may have ~ assign its combat damage this turn as though it weren't blocked",
/// "you may have creatures you control assign their combat damage this turn as though they
/// weren't blocked" (CR 510.1c).
fn assign_unblocked<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let rest = s.strip_prefix("you may have ")?;
    let (what, plural, rest) = nouns::object(rest, cx)?;
    battlefield_only(&what, cx)?;
    let rest = if plural {
        rest.strip_prefix(" assign their combat damage this turn as though they weren't blocked")?
    } else {
        rest.strip_prefix(" assign its combat damage this turn as though it weren't blocked")?
    };
    Some((
        continuous(
            what,
            vec![Modification::Restriction(
                Restriction::AssignAsThoughUnblocked,
            )],
            Duration::UntilEndOfTurn,
        ),
        rest,
    ))
}

/// "target player shuffles their graveyard into their library", "each player shuffles their
/// hand and graveyard into their library".
fn shuffle_into_library<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (who, _, rest) = nouns::player(s, cx)?;
    let (zones, rest) =
        if let Some(r) = rest.strip_prefix(" shuffles their graveyard into their library") {
            (vec![Zone::Graveyard], r)
        } else {
            let r = rest.strip_prefix(" shuffles their hand and graveyard into their library")?;
            (vec![Zone::Hand, Zone::Graveyard], r)
        };
    let mut steps: Vec<Effect> = zones
        .into_iter()
        .map(|zone| Effect::MoveZone {
            what: Selector::All {
                zone,
                filter: mtg_ir::ObjectFilter::OwnedBy(Box::new(who.clone())),
            },
            to: Zone::Library,
            owner_relative_to: None,
            position: ZonePosition::Top,
            tapped: false,
            face_down: false,
            under_control_of: None,
        })
        .collect();
    steps.push(Effect::Shuffle { who: who.clone() });
    // "…, then draws seven cards."
    let rest = if let Some(r) = rest.strip_prefix(", then draws ")
        && let Some((n, r)) = words::number(r)
        && let Some(r) = r.strip_prefix(if n == 1 { " card" } else { " cards" })
    {
        steps.push(Effect::Draw {
            who,
            count: Value::Fixed(n),
        });
        r
    } else {
        rest
    };
    Some((Effect::Sequence(steps), rest))
}

/// "your opponents can't cast spells this turn", "players can't cast noncreature spells this
/// turn".
fn cant_cast_this_turn<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (who, rest) = super::cant_cast_who(s)?;
    let rest = rest.strip_prefix(" can't cast ")?;
    let (what, rest) = rest.split_once(" this turn")?;
    let modification = super::cant_cast(who, what, cx)?;
    Some((
        Effect::Continuous {
            what: Selector::SelfSource,
            modification,
            duration: Duration::UntilEndOfTurn,
        },
        rest,
    ))
}

/// "you may have target creature block ~ this turn if able", and provoke's "you may have
/// target creature defending player controls untap and block it this combat if able".
fn have_block<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let rest = s.strip_prefix("you may have ")?;
    let it_is_self = cx.it == Some(Selector::SelfSource);
    let (what, false, rest) = nouns::object(rest, cx)? else {
        return None;
    };
    battlefield_only(&what, cx)?;
    let (untap, rest) = match rest.strip_prefix(" untap and") {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let (duration, rest) = [
        (" block ~ this turn if able", Duration::UntilEndOfTurn),
        (" block ~ this combat if able", Duration::UntilEndOfCombat),
        (" block it this turn if able", Duration::UntilEndOfTurn),
        (" block it this combat if able", Duration::UntilEndOfCombat),
    ]
    .into_iter()
    .find_map(|(lead, d)| {
        rest.strip_prefix(lead)
            .filter(|_| lead.contains('~') || it_is_self)
            .map(|r| (d, r))
    })?;
    let block = Effect::Continuous {
        what: what.clone(),
        modification: Modification::Restriction(Restriction::MustBlockSource),
        duration,
    };
    let then = if untap {
        Effect::Sequence(vec![Effect::Untap { what }, block])
    } else {
        block
    };
    Some((
        Effect::May {
            prompt: "have it block".into(),
            then: Box::new(then),
            otherwise: None,
        },
        rest,
    ))
}

/// "all creatures able to block target creature this turn do so" (CR 509.1c).
fn all_must_block<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let rest = s.strip_prefix("all ")?;
    let (blockers, rest) = nouns::noun(rest, cx)?;
    if !blockers.plural || blockers.zone != Zone::Battlefield {
        return None;
    }
    let rest = rest.strip_prefix(" able to block ")?;
    let (what, false, rest) = nouns::object(rest, cx)? else {
        return None;
    };
    battlefield_only(&what, cx)?;
    let rest = rest.strip_prefix(" this turn do so")?;
    Some((
        continuous(
            what,
            vec![Modification::Restriction(Restriction::MustBeBlockedByAll(
                blockers.filter,
            ))],
            Duration::UntilEndOfTurn,
        ),
        rest,
    ))
}

/// Effects that act on permanents must not be handed a card in another zone.
fn battlefield_only(what: &Selector, cx: &Cx) -> Option<()> {
    match what {
        Selector::Target { index } => {
            (cx.targets.get(*index as usize)?.zone == Zone::Battlefield).then_some(())
        }
        _ => Some(()),
    }
}

/// "create a 1/1 white Soldier creature token", "create two 2/2 black Zombie creature
/// tokens with deathtouch", "create a tapped 1/1 colorless Thopter artifact creature token
/// with flying", "create a Food token", "investigate".
fn create_token<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    if let Some(r) = s.strip_prefix("investigate") {
        return Some((make_token(predefined("clue", cx)?, 1, false), r));
    }
    let r = s.strip_prefix("create ")?;
    let (n, r) = words::number(r)?;
    let r = r.strip_prefix(' ')?;
    let (tapped, r) = match r.strip_prefix("tapped ") {
        Some(r) => (true, r),
        None => (false, r),
    };
    // "create two tapped and attacking 1/1 red Warrior creature tokens"
    let (mut attacking, r) = match r.strip_prefix("and attacking ").filter(|_| tapped) {
        Some(r) => (true, r),
        None => (false, r),
    };
    let plural = n > 1;
    let token_word = if plural { " tokens" } else { " token" };

    // A predefined token: "a Food token", "two Clue tokens".
    let (w, rest) = words::first_word(r);
    if let Some(spec) = predefined(w, cx)
        && let Some(rest) = rest.strip_prefix(token_word)
    {
        return Some((make_token(spec, n, tapped), rest));
    }

    // "a token that's a copy of target creature you control" (CR 707.2).
    for lead in ["token that's a copy of ", "tokens that are copies of "] {
        if let Some(r) = r.strip_prefix(lead) {
            if tapped || (lead.starts_with("token ") == plural) {
                return None;
            }
            let (of, of_plural, rest) = nouns::object(r, cx)?;
            if of != Selector::SelfSource {
                battlefield_only(&of, cx)?;
            }
            if of_plural {
                return None;
            }
            let create = Effect::CreateTokenCopy {
                of,
                count: Value::Fixed(n),
                controller: Selector::You,
            };
            // "…, except it's a 4/4 black Zombie with no mana cost" (CR 707.9b): the
            // exceptions hold for as long as the token exists.
            if let Some(r) = rest.strip_prefix(", except ") {
                let (mods, rest) = copy_exceptions(r, cx)?;
                let made = Selector::Bound(mtg_ir::selector::Binding::It);
                let mut steps = vec![create];
                steps.extend(mods.into_iter().map(|modification| Effect::Continuous {
                    what: made.clone(),
                    modification,
                    duration: Duration::Permanent,
                }));
                cx.it = Some(made);
                return Some((Effect::Sequence(steps), rest));
            }
            return Some((create, rest));
        }
    }
    let ((p, t), r) = words::pt(r)?;
    let r = r.strip_prefix(' ')?;
    let (colors, r) = token_colors(r)?;
    // Subtypes, then card types, then "token(s)".
    let mut subtypes = Vec::new();
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut rest = r;
    loop {
        let (w, after) = words::first_word(rest);
        if let Some((ty, false)) = words::card_type(w) {
            types.push(ty);
        } else if let Some(st) = cx.subtype(w).filter(|_| types.is_empty()) {
            subtypes.push(st);
            names.push(capitalise(w));
        } else {
            break;
        }
        rest = after.strip_prefix(' ').unwrap_or(after);
        if after.starts_with(token_word) {
            rest = after;
            break;
        }
    }
    if !types.contains(&CardType::Creature) || subtypes.is_empty() {
        return None;
    }
    let mut rest = rest.strip_prefix(token_word)?;
    let mut abilities = Vec::new();
    if let Some(r) = rest.strip_prefix(" with ") {
        let mut r = r;
        // Keywords, a quoted ability, or keywords "and" a quoted ability.
        if let Some((ks, after)) = words::keyword_list(r) {
            abilities.extend(ks.into_iter().map(|k| Ability {
                id: mtg_core::AbilityId(0),
                kind: AbilityKind::Keyword(k),
                targets: Vec::new(),
                source_text: None,
            }));
            r = after;
            if let Some(after) = r.strip_prefix(" and ").filter(|a| a.starts_with('"')) {
                r = after;
            }
        }
        if let Some(q) = r.strip_prefix('"') {
            let end = q.find('"')?;
            let inner = q[..end].replace("this token", "~");
            abilities.extend(super::quoted_abilities(&inner, cx)?);
            r = &q[end + 1..];
        }
        rest = r;
    }
    // "… creature token. It has \"Sacrifice this token: Add {C}.\"" (Eldrazi Scions).
    for lead in [". it has \"", ". they have \""] {
        if let Some(q) = rest.strip_prefix(lead) {
            let end = q.find('"')?;
            let inner = q[..end].replace("this token", "~");
            abilities.extend(super::quoted_abilities(&inner, cx)?);
            rest = &q[end + 1..];
        }
    }
    let mut tapped = tapped;
    // "… creature token that's tapped and attacking"
    if let Some(r) = rest
        .strip_prefix(" that's tapped and attacking")
        .or_else(|| rest.strip_prefix(" that are tapped and attacking"))
    {
        tapped = true;
        attacking = true;
        rest = r;
    }
    let spec = TokenSpec {
        card: None,
        name: names.join(" ").into(),
        types,
        subtypes,
        colors,
        power: Value::Fixed(p),
        toughness: Value::Fixed(t),
        abilities,
    };
    let made = make_token(spec, n, tapped);
    // "Sacrifice it at the beginning of the next end step": the tokens just made.
    cx.it = Some(Selector::Bound(mtg_ir::selector::Binding::It));
    if attacking {
        return Some((
            Effect::Sequence(vec![
                made,
                Effect::PutAttacking {
                    what: Selector::Bound(mtg_ir::selector::Binding::It),
                },
            ]),
            rest,
        ));
    }
    Some((made, rest))
}

/// "~ becomes a 2/2 green Bear creature until end of turn. It's still a land.", "until
/// end of turn, target land you control becomes a 3/3 Elemental creature with haste" —
/// layer 4 types and subtypes, layer 5 color, layer 6 keywords, layer 7b base P/T. Without
/// "it's still a …" or "in addition to its other types", the new card types replace the old
/// (CR 205.1a).
fn becomes_creature<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (lead, s) = match s.strip_prefix("until end of turn, ") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (what, plural, r) = nouns::object(s, cx)?;
    if plural {
        return None;
    }
    battlefield_only(&what, cx)?;
    let r = r
        .strip_prefix(" becomes a ")
        .or_else(|| r.strip_prefix(" becomes an "))?;
    let ((p, t), r) = words::pt(r)?;
    let r = r.strip_prefix(' ')?;
    let (colors, r) = match token_colors(r) {
        Some((cs, r)) => (Some(cs), r),
        None => (None, r),
    };
    let mut subtypes = Vec::new();
    let mut types = Vec::new();
    let mut rest = r;
    loop {
        let (w, after) = words::first_word(rest);
        if let Some((ty, false)) = words::card_type(w) {
            types.push(ty);
        } else if let Some(st) = cx.subtype(w).filter(|_| types.is_empty()) {
            subtypes.push(st);
        } else {
            break;
        }
        rest = after;
        if types.contains(&CardType::Creature) {
            break;
        }
        rest = rest.strip_prefix(' ')?;
    }
    if !types.contains(&CardType::Creature) {
        return None;
    }
    let mut r = rest;
    let mut keywords = Vec::new();
    if let Some(after) = r.strip_prefix(" with ") {
        let (ks, after) = words::keyword_list(after)?;
        keywords = ks;
        r = after;
    }
    let mut retain = false;
    if let Some(after) = r.strip_prefix(" in addition to its other types") {
        retain = true;
        r = after;
    }
    let (duration, r) = if lead {
        (Duration::UntilEndOfTurn, r)
    } else if let Some(r) = r.strip_prefix(" until end of turn") {
        (Duration::UntilEndOfTurn, r)
    } else {
        (
            Duration::UntilYourNextTurn,
            r.strip_prefix(" until your next turn")?,
        )
    };
    let r = match r
        .strip_prefix(". it's still a land")
        .or_else(|| r.strip_prefix(" that's still a land"))
    {
        Some(r) => {
            retain = true;
            r
        }
        None => r,
    };
    let mut mods = vec![if retain {
        Modification::AddTypes(types)
    } else {
        Modification::SetTypes(types)
    }];
    if !subtypes.is_empty() {
        mods.push(Modification::AddSubtypes(subtypes));
    }
    if let Some(cs) = colors {
        mods.push(Modification::SetColors(cs));
    }
    mods.extend(keywords.into_iter().map(grant));
    mods.push(Modification::SetBasePowerToughness {
        power: Value::Fixed(p),
        toughness: Value::Fixed(t),
    });
    Some((continuous(what, mods, duration), r))
}

/// A token copy's exceptions: "it isn't legendary", "it's a 4/4 black Zombie", "it has
/// haste", "it's an artifact in addition to its other types", "… with no mana cost", joined
/// by commas and "and".
fn copy_exceptions<'s>(s: &'s str, cx: &Cx) -> Option<(Vec<Modification>, &'s str)> {
    let mut mods = Vec::new();
    let mut r = s;
    loop {
        r = r.strip_prefix("and ").unwrap_or(r);
        let r0 = r;
        if let Some(rest) = [
            "it isn't legendary",
            "the copy isn't legendary",
            "the token isn't legendary",
            "it's not legendary",
        ]
        .iter()
        .find_map(|p| r.strip_prefix(p))
        {
            mods.push(Modification::RemoveSupertype(
                mtg_core::Supertype::Legendary,
            ));
            r = rest;
        } else if let Some(rest) = r.strip_prefix("it has ") {
            let (ks, rest) = words::keyword_list(rest)?;
            mods.extend(ks.into_iter().map(grant));
            r = rest;
        } else if let Some(rest) = r.strip_prefix("it's ") {
            let rest = rest
                .strip_prefix("a ")
                .or_else(|| rest.strip_prefix("an "))
                .unwrap_or(rest);
            // "4/4", then colors, then subtypes and types.
            let rest = match words::pt(rest) {
                Some(((p, t), after)) => {
                    mods.push(Modification::SetBasePowerToughness {
                        power: Value::Fixed(p),
                        toughness: Value::Fixed(t),
                    });
                    after.strip_prefix(' ').unwrap_or(after)
                }
                None => rest,
            };
            let rest = match token_colors(rest) {
                Some((cs, after)) => {
                    mods.push(Modification::SetColors(cs));
                    after
                }
                None => rest,
            };
            let mut subtypes = Vec::new();
            let mut types = Vec::new();
            let mut rest = rest;
            loop {
                let (w, after) = words::first_word(rest);
                if w.is_empty() {
                    break;
                }
                if let Some((ty, false)) = words::card_type(w) {
                    types.push(ty);
                } else if let Some(st) = cx.subtype(w) {
                    subtypes.push(st);
                } else {
                    break;
                }
                rest = after.strip_prefix(' ').unwrap_or(after);
                if !after.starts_with(' ') {
                    rest = after;
                    break;
                }
            }
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            let rest = rest
                .strip_prefix("in addition to its other types")
                .unwrap_or(rest);
            if !types.is_empty() {
                mods.push(Modification::AddTypes(types));
            }
            if !subtypes.is_empty() {
                mods.push(Modification::AddSubtypes(subtypes));
            }
            r = rest;
        }
        if let Some(rest) = r.trim_start().strip_prefix("with no mana cost") {
            mods.push(Modification::NoManaCost);
            r = rest;
        }
        if r == r0 {
            return None;
        }
        match r.strip_prefix(", ") {
            Some(rest) => r = rest,
            None => break,
        }
    }
    (!mods.is_empty()).then_some((mods, r))
}

fn make_token(spec: TokenSpec, n: i32, tapped: bool) -> Effect {
    let create = Effect::CreateToken {
        token: spec,
        count: Value::Fixed(n),
        controller: Selector::You,
    };
    if tapped {
        // The tokens just created are bound as "it" (see the engine's `CreateToken`).
        Effect::Sequence(vec![
            create,
            Effect::Tap {
                what: Selector::Bound(mtg_ir::selector::Binding::It),
            },
        ])
    } else {
        create
    }
}

/// "white", "green and white", "colorless", "white, blue, and black".
fn token_colors(s: &str) -> Option<(Vec<Color>, &str)> {
    if let Some(r) = s.strip_prefix("colorless ") {
        return Some((Vec::new(), r));
    }
    let (w, r) = words::first_word(s);
    let mut out = vec![words::color(w)?];
    let mut rest = r;
    loop {
        let next = rest
            .strip_prefix(" and ")
            .or_else(|| rest.strip_prefix(", and "))
            .or_else(|| rest.strip_prefix(", "));
        match next.map(words::first_word) {
            Some((w, r)) if words::color(w).is_some() => {
                out.push(words::color(w)?);
                rest = r;
            }
            _ => break,
        }
    }
    Some((out, rest.strip_prefix(' ')?))
}

/// Tokens whose rules text is defined by the game (CR 111.10).
fn predefined(word: &str, cx: &mut Cx) -> Option<TokenSpec> {
    let (name, text) = match word {
        "food" => ("Food", "{2}, {t}, sacrifice ~: you gain 3 life."),
        "clue" => ("Clue", "{2}, sacrifice ~: draw a card."),
        "treasure" => ("Treasure", "{t}, sacrifice ~: add one mana of any color."),
        "powerstone" => (
            "Powerstone",
            "{t}: add {c}. this mana can't be spent to cast a nonartifact spell.",
        ),
        "blood" => (
            "Blood",
            "{1}, {t}, discard a card, sacrifice ~: draw a card.",
        ),
        _ => return None,
    };
    let subtype = cx.subtype(word)?;
    let kind = super::quoted_abilities(text, cx)?.pop()?.kind;
    Some(TokenSpec {
        card: None,
        name: name.into(),
        types: vec![CardType::Artifact],
        subtypes: vec![subtype],
        colors: Vec::new(),
        power: Value::ZERO,
        toughness: Value::ZERO,
        abilities: vec![Ability {
            id: mtg_core::AbilityId(0),
            kind,
            targets: Vec::new(),
            source_text: Some(text.into()),
        }],
    })
}

fn capitalise(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// "discard a card", "discard two cards" — the imperative, for the controller.
fn discard_you<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("discard ")?;
    // Snapshot the hand before discarding so "draw that many" still counts
    // those cards after they have moved to new objects in another zone.
    if let Some(r) = r
        .strip_prefix("your hand")
        .or_else(|| r.strip_prefix("all the cards in your hand"))
    {
        let hand = Selector::All {
            zone: Zone::Hand,
            filter: mtg_ir::ObjectFilter::OwnedBy(Box::new(Selector::You)),
        };
        let discard = Effect::Discard {
            who: Selector::You,
            count: Value::Count(Box::new(hand.clone())),
            at_random: false,
        };
        if let Some(r) = r
            .strip_prefix(", then draw that many cards")
            .or_else(|| r.strip_prefix(". then draw that many cards"))
        {
            let slot = mtg_ir::selector::Binding::Named(u16::MAX - 5);
            let count = Value::Count(Box::new(Selector::Bound(slot)));
            let (count, r) = if let Some(r) = r.strip_prefix(" plus ") {
                let (extra, r) = words::number(r)?;
                (Value::Sum(vec![count, Value::Fixed(extra)]), r)
            } else {
                (count, r)
            };
            return Some((
                Effect::Let {
                    slot,
                    what: hand,
                    body: Box::new(Effect::Sequence(vec![
                        discard,
                        Effect::Draw {
                            who: Selector::You,
                            count,
                        },
                    ])),
                },
                r,
            ));
        }
        return Some((discard, r));
    }
    let (n, r) = count_of(r, "card", cx)?;
    if r.starts_with(" at random") {
        return None;
    }
    Some((
        Effect::Discard {
            who: Selector::You,
            count: n,
            at_random: false,
        },
        r,
    ))
}

/// Multiply a clause's number by `by`: "draw a card for each …", "deals 1 damage to …
/// for each …". Only clauses with one obvious number can be scaled.
fn scale(e: &mut Effect, by: Value) -> bool {
    let times = |v: &mut Value| {
        *v = match std::mem::replace(v, Value::ZERO) {
            Value::Fixed(1) => by.clone(),
            other => Value::Product(vec![other, by.clone()]),
        };
    };
    match e {
        Effect::Draw { count, .. } | Effect::Discard { count, .. } => times(count),
        Effect::GainLife { amount, .. }
        | Effect::LoseLife { amount, .. }
        | Effect::DealDamage { amount, .. }
        | Effect::AddCounters { amount, .. } => times(amount),
        Effect::Continuous {
            modification: Modification::ModifyPowerToughness { power, toughness },
            ..
        } => {
            times(power);
            times(toughness);
        }
        _ => return false,
    }
    true
}

/// "if you control a Swamp, draw a card" — checked as the effect resolves.
fn conditional<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("if ")?;
    let (cond, r) = super::conditions::condition(r, cx)?;
    let r = r.strip_prefix(", ")?;
    let (then, r) = clause(r, cx)?;
    Some((
        Effect::If {
            cond,
            then: Box::new(then),
            otherwise: Box::new(Effect::Nothing),
        },
        r,
    ))
}

/// "~ deals damage equal to its power to target creature", "you gain life equal to the
/// number of creatures you control".
fn equal_to<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    if let Some((source, _, r)) = nouns::object(s, cx)
        && let Some(r) = r.strip_prefix(" deals damage equal to ")
    {
        let (amount, r) = value_phrase(r, cx)?;
        let r = r.strip_prefix(" to ")?;
        let (to, r) = nouns::recipient(r, cx)?;
        return Some((Effect::DealDamage { source, to, amount }, r));
    }
    // "~ deals damage to target creature equal to the number of lands you control"
    let saved = cx.clone();
    if let Some((source, _, r)) = nouns::object(s, cx)
        && let Some(r) = r.strip_prefix(" deals damage to ")
        && let Some((to, r)) = nouns::recipient(r, cx)
        && let Some(r) = r.strip_prefix(" equal to ")
        && let Some((amount, r)) = value_phrase(r, cx)
    {
        return Some((Effect::DealDamage { source, to, amount }, r));
    }
    *cx = saved;
    let (who, bare, r) = nouns::player(s, cx)?;
    let verb = |v: &str| {
        if bare {
            format!(" {v} ")
        } else {
            format!(" {v}s ")
        }
    };
    if let Some(r) = r
        .strip_prefix(verb("gain").as_str())
        .and_then(|r| r.strip_prefix("life equal to "))
    {
        let (amount, r) = value_phrase(r, cx)?;
        return Some((Effect::GainLife { who, amount }, r));
    }
    if let Some(r) = r
        .strip_prefix(verb("lose").as_str())
        .and_then(|r| r.strip_prefix("life equal to "))
    {
        let (amount, r) = value_phrase(r, cx)?;
        return Some((Effect::LoseLife { who, amount }, r));
    }
    if let Some(r) = r
        .strip_prefix(verb("draw").as_str())
        .and_then(|r| r.strip_prefix("cards equal to "))
    {
        let (count, r) = value_phrase(r, cx)?;
        return Some((Effect::Draw { who, count }, r));
    }
    None
}

/// "the number of creatures you control", "its power", "~'s toughness".
pub fn value_phrase<'s>(s: &'s str, cx: &Cx) -> Option<(Value, &'s str)> {
    if let Some(r) = s.strip_prefix("the number of colors of mana spent to cast ")
        && let Some(r) = spent_on_self(r)
    {
        return Some((Value::ColorsSpent, r));
    }
    if let Some(r) = s.strip_prefix("the number of ") {
        let (what, r) = super::conditions::counted(r, cx)?;
        return Some((Value::Count(Box::new(what)), r));
    }
    let (who, r) = if let Some(r) = s.strip_prefix("its ") {
        (cx.it.clone()?, r)
    } else if let Some(r) = s.strip_prefix("~'s ") {
        (Selector::SelfSource, r)
    } else {
        let r = s.strip_prefix("that creature's ")?;
        (cx.it.clone()?, r)
    };
    if let Some(r) = r.strip_prefix("power") {
        return Some((Value::Power(Box::new(who)), r));
    }
    if let Some(r) = r.strip_prefix("toughness") {
        return Some((Value::Toughness(Box::new(who)), r));
    }
    if let Some(r) = r.strip_prefix("mana value") {
        return Some((Value::ManaValue(Box::new(who)), r));
    }
    None
}

/// Multiply the power/toughness changes among `mods` by `by`.
pub fn scale_mods(mods: &mut [Modification], by: Value) {
    for m in mods {
        if let Modification::ModifyPowerToughness { power, toughness } = m {
            for v in [power, toughness] {
                *v = match std::mem::replace(v, Value::ZERO) {
                    Value::Fixed(0) => Value::ZERO,
                    Value::Fixed(1) => by.clone(),
                    other => Value::Product(vec![other, by.clone()]),
                };
            }
        }
    }
}

/// "search your library for a basic land card, put it onto the battlefield tapped, then
/// shuffle" (CR 701.19): the controller chooses among matching cards in their library, and
/// may find none — a searched-for quality can always fail to be found (CR 701.19b).
fn search<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("search your library for ")?;
    let (n, r) = if let Some(r) = r.strip_prefix("up to ") {
        let (n, r) = words::number(r)?;
        (n, r.strip_prefix(' ')?)
    } else if let Some(r) = r.strip_prefix("any number of ") {
        (i32::from(u8::MAX), r)
    } else {
        let r = r.strip_prefix("a ").or_else(|| r.strip_prefix("an "))?;
        (1, r)
    };
    // The noun runs up to " card" / " cards".
    let marker = if n == 1 { " card" } else { " cards" };
    let (filter, r) = if let Some(r) = r.strip_prefix(marker.trim_start()) {
        // "a card named ~"
        match r.strip_prefix(" named ~") {
            Some(r) => (mtg_ir::ObjectFilter::NamedLikeSource, r),
            None => (mtg_ir::ObjectFilter::Any, r),
        }
    } else {
        let at = r.find(marker)?;
        let (head, rest) = r.split_at(at);
        // "a Mercenary permanent card", "a permanent card".
        let filter = if head == "permanent" {
            nouns::permanent_card()
        } else if let Some(head) = head.strip_suffix(" permanent") {
            let (noun, left) = nouns::noun(head, cx)?;
            if !left.is_empty() || noun.zone != Zone::Battlefield {
                return None;
            }
            mtg_ir::ObjectFilter::And(vec![noun.filter, nouns::permanent_card()])
        } else {
            let (noun, left) = nouns::noun(head, cx)?;
            if !left.is_empty() || noun.zone != Zone::Battlefield {
                return None;
            }
            noun.filter
        };
        (filter, &rest[marker.len()..])
    };
    // "… card with mana value 3 or less"
    let (filter, r) = match r.strip_prefix(" with ").and_then(nouns::with_clause) {
        Some((quality, r)) => (mtg_ir::ObjectFilter::And(vec![filter, quality]), r),
        None => (filter, r),
    };
    let found = Selector::ChosenBy {
        chooser: Box::new(Selector::You),
        zone: Zone::Library,
        filter,
        count: Value::Fixed(n),
        up_to: true,
    };
    let r = r.strip_prefix(", ")?;
    let r = r
        .strip_prefix("reveal it, ")
        .or_else(|| r.strip_prefix("reveal them, "))
        .or_else(|| r.strip_prefix("reveal that card, "))
        .or_else(|| r.strip_prefix("reveal those cards, "))
        .unwrap_or(r);
    // "…, then shuffle and put that card on top": it is set aside, and goes back on top.
    if n == 1
        && let Some(r) = r.strip_prefix("then shuffle and put that card on top")
    {
        return Some((
            Effect::Let {
                slot: mtg_ir::selector::Binding::It,
                what: found,
                body: Box::new(Effect::Sequence(vec![
                    Effect::Shuffle { who: Selector::You },
                    Effect::MoveZone {
                        what: Selector::Bound(mtg_ir::selector::Binding::It),
                        to: Zone::Library,
                        owner_relative_to: None,
                        position: ZonePosition::Top,
                        tapped: false,
                        face_down: false,
                        under_control_of: None,
                    },
                ])),
            },
            r,
        ));
    }
    let it = if n == 1 { "it" } else { "them" };
    let that = if n == 1 { "that card" } else { "those cards" };
    let (to, tapped, r) = if let Some(r) = r
        .strip_prefix(&format!("put {it} onto the battlefield tapped") as &str)
        .or_else(|| r.strip_prefix(&format!("put {that} onto the battlefield tapped") as &str))
    {
        (Zone::Battlefield, true, r)
    } else if let Some(r) = r
        .strip_prefix(&format!("put {it} onto the battlefield") as &str)
        .or_else(|| r.strip_prefix(&format!("put {that} onto the battlefield") as &str))
    {
        (Zone::Battlefield, false, r)
    } else if let Some(r) = r
        .strip_prefix(&format!("put {it} into your graveyard") as &str)
        .or_else(|| r.strip_prefix(&format!("put {that} into your graveyard") as &str))
    {
        (Zone::Graveyard, false, r)
    } else if let Some(r) = r
        .strip_prefix(&format!("exile {it}") as &str)
        .or_else(|| r.strip_prefix(&format!("exile {that}") as &str))
    {
        (Zone::Exile, false, r)
    } else {
        let r = r
            .strip_prefix(&format!("put {it} into your hand") as &str)
            .or_else(|| {
                r.strip_prefix("put that card into your hand")
                    .filter(|_| n == 1)
            })?;
        (Zone::Hand, false, r)
    };
    let r = r
        .strip_prefix(", then shuffle")
        .or_else(|| r.strip_prefix(". then shuffle"))?;
    Some((
        Effect::Sequence(vec![
            Effect::MoveZone {
                what: found,
                to,
                owner_relative_to: None,
                position: ZonePosition::Natural,
                tapped,
                face_down: false,
                under_control_of: None,
            },
            Effect::Shuffle { who: Selector::You },
        ]),
        r,
    ))
}

/// What "for each …" counts: objects ("creature you control") or counters ("charge counter
/// on ~").
/// "this spell", "~", "it": the source, after "… spent to cast ".
fn spent_on_self(s: &str) -> Option<&str> {
    s.strip_prefix("~")
        .or_else(|| s.strip_prefix("it"))
        .or_else(|| s.strip_prefix("this spell"))
}

pub fn per<'s>(s: &'s str, cx: &Cx) -> Option<(Value, &'s str)> {
    // Converge: "for each color of mana spent to cast this spell".
    if let Some(r) = s.strip_prefix("color of mana spent to cast ")
        && let Some(r) = spent_on_self(r)
    {
        return Some((Value::ColorsSpent, r));
    }
    // "for each creature in your party" (CR 700.8).
    if let Some(r) = s.strip_prefix("creature in your party") {
        return Some((
            Value::PartySize(Box::new(Selector::All {
                zone: Zone::Battlefield,
                filter: mtg_ir::ObjectFilter::And(vec![
                    mtg_ir::ObjectFilter::HasType(CardType::Creature),
                    mtg_ir::ObjectFilter::ControlledBy(Box::new(Selector::You)),
                ]),
            })),
            r,
        ));
    }
    // Domain: "for each basic land type among lands you control".
    if let Some(r) = s.strip_prefix("basic land type among lands you control") {
        return Some((
            Value::BasicLandTypesAmong(Box::new(Selector::All {
                zone: Zone::Battlefield,
                filter: mtg_ir::ObjectFilter::And(vec![
                    mtg_ir::ObjectFilter::HasType(CardType::Land),
                    mtg_ir::ObjectFilter::ControlledBy(Box::new(Selector::You)),
                ]),
            })),
            r,
        ));
    }
    // Undaunted: "for each opponent you have".
    if let Some(r) = s.strip_prefix("opponent you have") {
        return Some((Value::Count(Box::new(Selector::Opponents)), r));
    }
    if let Some((kind, r)) = words::counter(s) {
        let (what, _, r) = nouns::object(r.strip_prefix(" on ")?, &mut cx.clone())?;
        return Some((Value::Counters(Box::new(what), kind), r));
    }
    let (what, r) = super::conditions::counted(s, cx)?;
    let count = Value::Count(Box::new(what));
    // Rampage: "for each creature blocking it beyond the first".
    if let Some(r) = r.strip_prefix(" beyond the first") {
        let less = Value::Sum(vec![count, Value::Fixed(-1)]);
        return Some((Value::Max(Box::new(less), Box::new(Value::ZERO)), r));
    }
    Some((count, r))
}

/// "attach ~ to target creature you control", "attach it to target creature you control"
/// (an Equipment's own trigger), "attach this equipment to it" (a token just created).
fn attach<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("attach ")?;
    let (what, r) = if let Some(r) = r.strip_prefix("~ to ") {
        (Selector::SelfSource, r)
    } else {
        let r = r
            .strip_prefix("it to ")
            .filter(|_| cx.it == Some(Selector::SelfSource))?;
        (Selector::SelfSource, r)
    };
    if let Some(r) = r.strip_prefix("it") {
        // The token the previous clause created.
        return Some((
            Effect::Attach {
                what,
                to: Selector::Bound(mtg_ir::selector::Binding::It),
            },
            r,
        ));
    }
    let (to, plural, r) = nouns::object(r, cx)?;
    if plural {
        return None;
    }
    battlefield_only(&to, cx)?;
    Some((Effect::Attach { what, to }, r))
}

/// "regenerate ~", "regenerate target creature" (CR 701.15).
fn regenerate<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("regenerate ")?;
    let (what, _, r) = nouns::object(r, cx)?;
    battlefield_only(&what, cx)?;
    Some((Effect::Regenerate { what }, r))
}

/// "sacrifice a creature" — the controller chooses one they control.
fn sacrifice_you<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("sacrifice ")?;
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
    let filter = if other {
        mtg_ir::ObjectFilter::And(vec![
            n.filter,
            mtg_ir::ObjectFilter::Not(Box::new(mtg_ir::ObjectFilter::IsSelf)),
        ])
    } else {
        n.filter
    };
    Some((
        Effect::Sacrifice {
            who: Selector::You,
            what: Selector::All {
                zone: Zone::Battlefield,
                filter,
            },
        },
        r,
    ))
}

/// Put `v` wherever `X` was written. Only the clause shapes that take a number are
/// walked; an `X` anywhere else fails the line rather than being left meaningless.
fn replace_x(e: &mut Effect, v: &Value) -> Option<()> {
    let sub = |x: &mut Value| {
        if *x == Value::X {
            *x = v.clone();
        } else if *x == Value::Negate(Box::new(Value::X)) {
            *x = Value::Negate(Box::new(v.clone()));
        } else if let Value::Product(parts) = x {
            for p in parts {
                if *p == Value::X {
                    *p = v.clone();
                }
            }
        }
    };
    match e {
        Effect::Sequence(es) => {
            for e in es {
                replace_x(e, v)?;
            }
        }
        Effect::If {
            then, otherwise, ..
        } => {
            replace_x(then, v)?;
            replace_x(otherwise, v)?;
        }
        Effect::May { then, .. } | Effect::MayPay { then, .. } => replace_x(then, v)?,
        Effect::Draw { count, .. } | Effect::Discard { count, .. } => sub(count),
        Effect::GainLife { amount, .. }
        | Effect::LoseLife { amount, .. }
        | Effect::DealDamage { amount, .. }
        | Effect::AddCounters { amount, .. } => sub(amount),
        Effect::Continuous {
            modification: Modification::ModifyPowerToughness { power, toughness },
            ..
        } => {
            sub(power);
            sub(toughness);
        }
        Effect::MoveZone {
            what: Selector::TopOfLibrary { count, .. },
            ..
        } => sub(count),
        _ => {}
    }
    Some(())
}

/// "+2/+2", or with `X` allowed "+X/+X", "+X/+0", "-X/-X".
fn pt_delta_value(s: &str, x_ok: bool) -> Option<((Value, Value), &str)> {
    if let Some(((p, t), r)) = words::pt_delta(s) {
        return Some(((Value::Fixed(p), Value::Fixed(t)), r));
    }
    if !x_ok {
        return None;
    }
    fn one(s: &str) -> Option<(Value, &str)> {
        let (neg, r) = match s.as_bytes().first()? {
            b'+' => (false, &s[1..]),
            b'-' => (true, &s[1..]),
            _ => return None,
        };
        if let Some(r) = r.strip_prefix('x') {
            let v = if neg {
                Value::Negate(Box::new(Value::X))
            } else {
                Value::X
            };
            return Some((v, r));
        }
        let digits = r.chars().take_while(char::is_ascii_digit).count();
        let n: i32 = r.get(..digits)?.parse().ok()?;
        Some((Value::Fixed(if neg { -n } else { n }), &r[digits..]))
    }
    let (p, r) = one(s)?;
    let (t, r) = one(r.strip_prefix('/')?)?;
    Some(((p, t), r))
}

/// Where a delayed trigger waits: "at the beginning of the next end step" and friends.
fn delayed_when(s: &str) -> Option<(mtg_ir::EventPattern, &str)> {
    use mtg_core::Step;
    for (phrase, step, whose) in [
        ("the next end step", Step::End, Selector::EachPlayer),
        (
            "the beginning of the next end step",
            Step::End,
            Selector::EachPlayer,
        ),
        ("the next turn's upkeep", Step::Upkeep, Selector::EachPlayer),
        (
            "the beginning of the next turn's upkeep",
            Step::Upkeep,
            Selector::EachPlayer,
        ),
        ("your next upkeep", Step::Upkeep, Selector::You),
        (
            "the beginning of your next upkeep",
            Step::Upkeep,
            Selector::You,
        ),
        ("end of combat", Step::EndCombat, Selector::EachPlayer),
    ] {
        if let Some(r) = s.strip_prefix(phrase) {
            return Some((mtg_ir::EventPattern::StepBegins { step, whose }, r));
        }
    }
    None
}

/// "… at the beginning of the next end step" after a clause.
fn delayed_suffix(s: &str) -> Option<(mtg_ir::EventPattern, &str)> {
    delayed_when(s.strip_prefix(" at ")?)
}

/// "at the beginning of the next end step, sacrifice it" as a clause of its own.
fn delayed_clause<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let r = s.strip_prefix("at ")?;
    let (on, r) = delayed_when(r)?;
    let r = r.strip_prefix(", ")?;
    let (e, r) = clause(r, cx)?;
    Some((delayed(on, e)?, r))
}

/// A delayed trigger's effect. It has no targets of its own, so an effect that names a
/// target of the spell that created it is not compiled.
fn delayed(on: mtg_ir::EventPattern, e: Effect) -> Option<Effect> {
    if format!("{e:?}").contains("Target {") {
        return None;
    }
    Some(Effect::Delayed {
        on,
        effect: Box::new(e),
    })
}

/// "target creature you control fights target creature you don't control", "~ fights up
/// to one target creature" (CR 701.14).
fn fight<'s>(s: &'s str, cx: &mut Cx) -> Option<(Effect, &'s str)> {
    let (a, plural, r) = nouns::object(s, cx)?;
    if plural {
        return None;
    }
    let r = r.strip_prefix(" fights ")?;
    let (b, plural, r) = nouns::object(r, cx)?;
    if plural {
        return None;
    }
    battlefield_only(&a, cx)?;
    battlefield_only(&b, cx)?;
    Some((Effect::Fight { a, b }, r))
}
