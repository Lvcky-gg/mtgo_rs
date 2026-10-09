//! Walking an ability tree to rewrite parts of it in place.
//!
//! Two things need this, and both are about ids that are only meaningful relative to a
//! table: a match's card table re-interns subtypes, so every `HasSubtype` inside an
//! ability must be renumbered along with the face's own subtypes; and token specs are
//! registered as card faces, so each `CreateToken` learns which card it makes.
//!
//! The walk is exhaustive by construction — every `match` below names every variant — so
//! a new IR node that can hold a filter or a token cannot be forgotten silently.

use crate::{
    Ability, AbilityKind, AdditionalCost, Cost, Effect, EventPattern, ObjectFilter, Selector,
    TargetSpec, Trigger, Value,
    effect::{Modification, Replacement, ReplacementKind, Restriction, TokenSpec},
    trigger::{Condition, DamageRecipient},
};

/// Callbacks for the nodes a rewrite cares about. Each is called on every such node,
/// before its children are visited.
pub struct Visitor<'a> {
    pub filter: &'a mut dyn FnMut(&mut ObjectFilter),
    pub token: &'a mut dyn FnMut(&mut TokenSpec),
    pub value: &'a mut dyn FnMut(&mut Value),
}

impl Visitor<'_> {
    pub fn ability(&mut self, a: &mut Ability) {
        for t in &mut a.targets {
            self.target(t);
        }
        match &mut a.kind {
            AbilityKind::SpellEffect(e) | AbilityKind::ExertAsAttacks { effect: e } => {
                self.effect(e)
            }
            AbilityKind::Activated { cost, effect, .. } => {
                self.cost(cost);
                self.effect(effect);
            }
            AbilityKind::Triggered { trigger, effect } => {
                self.trigger(trigger);
                self.effect(effect);
            }
            AbilityKind::Static {
                what,
                modification,
                condition,
            } => {
                self.selector(what);
                self.modification(modification);
                if let Some(c) = condition {
                    self.condition(c);
                }
            }
            AbilityKind::ReplacementEffect(r) => self.replacement(r),
            AbilityKind::Protection { from } => self.filter(from),
            AbilityKind::CastOnlyIf { condition } => self.condition(condition),
            AbilityKind::AlternativeCost { cost, instead, .. } => {
                self.cost(cost);
                if let Some(e) = instead {
                    self.effect(e);
                }
            }
            AbilityKind::CastFrom { cost, .. }
            | AbilityKind::Kicker { cost, .. }
            | AbilityKind::Morph { cost, .. }
            | AbilityKind::Suspend { cost, .. }
            | AbilityKind::ExileToCastLater { cost, .. }
            | AbilityKind::AdditionalCastCost { cost } => self.cost(cost),
            AbilityKind::AdditionalCastCostChoice { options } => {
                for (_, cost) in options {
                    self.cost(cost);
                }
            }
            AbilityKind::Aftermath
            | AbilityKind::DeckRule(_)
            | AbilityKind::Dredge(_)
            | AbilityKind::Saga { .. }
            | AbilityKind::Enchant
            | AbilityKind::Strive { .. }
            | AbilityKind::Escalate { .. }
            | AbilityKind::FlashSurcharge { .. }
            | AbilityKind::SacrificeIfFlashed
            | AbilityKind::BeginOnBattlefield
            | AbilityKind::Spree { .. }
            | AbilityKind::Keyword(_)
            | AbilityKind::Native { .. } => {}
        }
    }

    fn target(&mut self, t: &mut TargetSpec) {
        self.filter(&mut t.filter);
        if let Some(p) = &mut t.players {
            self.selector(p);
        }
        self.value(&mut t.count);
    }

    fn cost(&mut self, c: &mut Cost) {
        for a in &mut c.additional {
            match a {
                AdditionalCost::Cycling | AdditionalCost::DiscardHand => {}
                AdditionalCost::Tap { what } | AdditionalCost::Untap { what } => {
                    self.selector(what)
                }
                AdditionalCost::Sacrifice { what, count } => {
                    self.selector(what);
                    self.value(count);
                }
                AdditionalCost::Discard { count, filter, .. }
                | AdditionalCost::ExileFrom { count, filter, .. }
                | AdditionalCost::Reveal { count, filter }
                | AdditionalCost::ReturnToHand { count, filter }
                | AdditionalCost::TapUntapped { filter, count } => {
                    self.value(count);
                    self.filter(filter);
                }
                AdditionalCost::PayLife { amount } | AdditionalCost::PayEnergy { amount } => {
                    self.value(amount)
                }
                AdditionalCost::RemoveCounters { what, amount, .. } => {
                    self.selector(what);
                    self.value(amount);
                }
                AdditionalCost::TapCreaturesWithPower { power } => self.value(power),
                AdditionalCost::ChooseMode
                | AdditionalCost::ReturnUnblockedAttacker
                | AdditionalCost::Exert
                | AdditionalCost::Mill { .. }
                | AdditionalCost::PutCounters { .. }
                | AdditionalCost::Loyalty { .. }
                | AdditionalCost::Native { .. } => {}
            }
        }
        for t in &mut c.timing {
            self.condition(t);
        }
    }

    fn trigger(&mut self, t: &mut Trigger) {
        self.pattern(&mut t.on);
        if let Some(c) = &mut t.intervening_if {
            self.condition(c);
        }
    }

    fn pattern(&mut self, p: &mut EventPattern) {
        match p {
            EventPattern::Enters { who }
            | EventPattern::Leaves { who }
            | EventPattern::Dies { who }
            | EventPattern::ZoneChange { who, .. }
            | EventPattern::Attacks { who }
            | EventPattern::Blocks { who }
            | EventPattern::BecomesBlocked { who }
            | EventPattern::AttacksUnblocked { who }
            | EventPattern::TakesDamage { who, .. }
            | EventPattern::BecomesTapped { who }
            | EventPattern::BecomesUntapped { who }
            | EventPattern::ExiledForMadness { who }
            | EventPattern::BecomesMonstrous { who }
            | EventPattern::TurnedFaceUp { who }
            | EventPattern::AttacksMostLife { who } => self.filter(who),
            EventPattern::AttacksPlayer { who, player, .. } => {
                self.filter(who);
                self.selector(player);
            }
            EventPattern::Scried { whose, .. } => self.selector(whose),
            EventPattern::Cycled { who, by } | EventPattern::Sacrificed { who, by } => {
                self.filter(who);
                self.selector(by);
            }
            EventPattern::ChapterReached { .. }
            | EventPattern::BecomesClassLevel { .. }
            | EventPattern::Reflexive => {}
            EventPattern::NthDraw { whose, .. } => self.selector(whose),
            EventPattern::NthSpellCast { by, .. } => self.selector(by),
            EventPattern::PlaysLand { who, .. } | EventPattern::SearchesLibrary { who } => {
                self.selector(who)
            }
            EventPattern::CastTargeting { by, target, spell } => {
                self.selector(by);
                self.filter(target);
                self.filter(spell);
            }
            EventPattern::Copied { who, by } => {
                self.filter(who);
                self.selector(by);
            }
            EventPattern::BlockedBy { attacker, blocker } => {
                self.filter(attacker);
                self.filter(blocker);
            }
            EventPattern::CounterPlaced { on, .. }
            | EventPattern::LastCounterRemoved { from: on, .. } => self.filter(on),
            EventPattern::StepBegins { whose, .. } | EventPattern::StepEnds { whose, .. } => {
                self.selector(whose)
            }
            EventPattern::Cast { who, by } | EventPattern::BecomesTarget { who, by } => {
                self.filter(who);
                self.selector(by);
            }
            EventPattern::AbilityActivated { by } => self.selector(by),
            EventPattern::DealsDamage { source, to, .. } => {
                self.filter(source);
                match to {
                    DamageRecipient::Player(s) => self.selector(s),
                    DamageRecipient::Object(f) => self.filter(f),
                    DamageRecipient::Any => {}
                }
            }
            EventPattern::LifeGained { whose }
            | EventPattern::LifeLost { whose }
            | EventPattern::Draws { whose }
            | EventPattern::Discards { whose } => self.selector(whose),
            EventPattern::StateIs(c) => self.condition(c),
            EventPattern::AnyOf(ps) => {
                for p in ps {
                    self.pattern(p);
                }
            }
        }
    }

    fn replacement(&mut self, r: &mut Replacement) {
        self.pattern(&mut r.matches);
        match &mut r.kind {
            ReplacementKind::Prevent
            | ReplacementKind::EntersTapped
            | ReplacementKind::EntersTappedUnlessPaysLife { .. }
            | ReplacementKind::EntersWithCounterIfChosen
            | ReplacementKind::EntersWithCounterOrHaste
            | ReplacementKind::Devour(_)
            | ReplacementKind::EntersChoosing(_)
            | ReplacementKind::EntersChoosingColorExcept(_) => {}
            ReplacementKind::EntersIfDiscards(f) => self.filter(f),
            ReplacementKind::EntersTappedUnless { condition } => self.condition(condition),
            ReplacementKind::Reduce { amount } => self.value(amount),
            ReplacementKind::Redirect { to } => self.selector(to),
            ReplacementKind::RedirectZoneChange { .. } => {}
            ReplacementKind::Multiply { factor } => self.value(factor),
            ReplacementKind::EntersModified { modification } => self.modification(modification),
            ReplacementKind::EntersWithCounters {
                amount, condition, ..
            } => {
                self.value(amount);
                if let Some(c) = condition {
                    self.condition(c);
                }
            }
            ReplacementKind::EntersAsCopy { of, .. } => self.filter(of),
            ReplacementKind::Instead { effect } | ReplacementKind::InAddition { effect } => {
                self.effect(effect)
            }
        }
    }

    fn modification(&mut self, m: &mut Modification) {
        match m {
            Modification::CopyOf(s) | Modification::Control(s) => self.selector(s),
            Modification::GrantAbility(a) => self.ability(a),
            Modification::SetBasePower(v) => self.value(v),
            Modification::SetBasePowerToughness { power, toughness }
            | Modification::ModifyPowerToughness { power, toughness } => {
                self.value(power);
                self.value(toughness);
            }
            Modification::Restriction(r) => match r {
                Restriction::CantBeBlockedExceptBy(f)
                | Restriction::CanBlockOnly(f)
                | Restriction::CantBeTargetedBy(f)
                | Restriction::PreventDamage { from: f, .. }
                | Restriction::CantAttackUnlessDefenderControls(f)
                | Restriction::Protection { from: f, .. }
                | Restriction::FlashFor(f) => self.filter(f),
                Restriction::CantGainLife(who) | Restriction::LifeGainBoost { who, .. } => {
                    self.selector(who)
                }
                Restriction::PlayFromTopOfLibrary { spells, .. }
                | Restriction::MustBeBlockedByAll(spells) => self.filter(spells),
                Restriction::CantCast { who, spells, .. } => {
                    self.selector(who);
                    self.filter(spells);
                }
                Restriction::CostModifier { what, delta } => {
                    self.filter(what);
                    self.value(delta);
                }
                Restriction::CantAttack
                | Restriction::AttackDespiteDefender
                | Restriction::AssignDamageByToughness
                | Restriction::AssignsNoCombatDamage
                | Restriction::CantBlockSource
                | Restriction::CantBlock
                | Restriction::MustAttackIfAble
                | Restriction::Goaded
                | Restriction::Indestructible
                | Restriction::CantBeCountered
                | Restriction::CantUntapDuringUntapStep
                | Restriction::CantBeRegenerated
                | Restriction::PlayerHexproof
                | Restriction::CantActivateAbilities
                | Restriction::LookAtTopOfLibrary
                | Restriction::SkipDrawStep
                | Restriction::Saddled
                | Restriction::TopOfLibraryRevealed
                | Restriction::NoMaximumHandSize
                | Restriction::UntapDuringOthersUntap
                | Restriction::CastOnlyAsSorcery { .. }
                | Restriction::AddsAdditionalMana { .. }
                | Restriction::AttackTax { .. }
                | Restriction::PlayerProtectionFromEverything
                | Restriction::LifeCantChange
                | Restriction::CantBeBlockedByMoreThanOne
                | Restriction::MinimumBlockers(_)
                | Restriction::CantAttackAlone
                | Restriction::CantBlockAlone
                | Restriction::BlockAdditional(_)
                | Restriction::MustBlock
                | Restriction::MustBlockSource
                | Restriction::MustBeBlocked
                | Restriction::AssignAsThoughUnblocked
                | Restriction::PreventDamageRemoveCounter
                | Restriction::AdditionalLandPlay
                | Restriction::MayChooseNotToUntap
                | Restriction::PlayLandsFromGraveyard => {}
            },
            Modification::ChangeText { .. }
            | Modification::AddTypes(_)
            | Modification::RemoveTypes(_)
            | Modification::SetTypes(_)
            | Modification::AddSubtypes(_)
            | Modification::SetCreatureTypes(_)
            | Modification::BecomesChosen(_)
            | Modification::RemoveSupertype(_)
            | Modification::NoManaCost
            | Modification::AddColors(_)
            | Modification::SetColors(_)
            | Modification::LoseAllAbilities
            | Modification::LoseKeyword(_)
            | Modification::SwitchPowerToughness => {}
        }
    }

    pub fn effect(&mut self, e: &mut Effect) {
        match e {
            Effect::Nothing
            | Effect::Ascend
            | Effect::PreventAllCombatDamage
            | Effect::DamageCantBePrevented
            | Effect::Cascade
            | Effect::Proliferate
            | Effect::GainClassLevel { .. }
            | Effect::Madness { .. }
            | Effect::Native { .. } => {}
            Effect::PreventDamage { to } => self.selector(to),
            Effect::PreventDamageShield { to, by, amount, .. } => {
                for s in [to, by].into_iter().flatten() {
                    self.selector(s);
                }
                if let Some(v) = amount {
                    self.value(v);
                }
            }
            Effect::Sequence(es) => {
                for e in es {
                    self.effect(e);
                }
            }
            Effect::If {
                cond,
                then,
                otherwise,
            } => {
                self.condition(cond);
                self.effect(then);
                self.effect(otherwise);
            }
            Effect::Modal { choose, modes, .. } => {
                self.value(choose);
                for (_, e) in modes {
                    self.effect(e);
                }
            }
            Effect::Let { what, body, .. } | Effect::ForEach { what, body } => {
                self.selector(what);
                self.effect(body);
            }
            Effect::Repeat { times, body } => {
                self.value(times);
                self.effect(body);
            }
            Effect::UnlessPays {
                payer,
                cost,
                times,
                otherwise,
            } => {
                self.selector(payer);
                self.cost(cost);
                self.value(times);
                self.effect(otherwise);
            }
            Effect::MayPay { cost, then } => {
                self.cost(cost);
                self.effect(then);
            }
            Effect::May {
                then, otherwise, ..
            } => {
                self.effect(then);
                if let Some(o) = otherwise {
                    self.effect(o);
                }
            }
            Effect::Delayed { on, effect } => {
                self.pattern(on);
                self.effect(effect);
            }
            Effect::AddMana { who, produces } => {
                self.selector(who);
                for p in produces {
                    self.mana(p);
                }
            }
            Effect::GainLife { who, amount }
            | Effect::LoseLife { who, amount }
            | Effect::GainEnergy { who, amount }
            | Effect::GivePoison { who, amount }
            | Effect::ReorderLibraryTop { who, count: amount } => {
                self.selector(who);
                self.value(amount);
            }
            Effect::Draw { who, count } | Effect::Discard { who, count, .. } => {
                self.selector(who);
                self.value(count);
            }
            Effect::MoveZone {
                what,
                owner_relative_to,
                under_control_of,
                ..
            } => {
                self.selector(what);
                if let Some(s) = owner_relative_to {
                    self.selector(s);
                }
                if let Some(s) = under_control_of {
                    self.selector(s);
                }
            }
            Effect::Reveal { what } => self.selector(what),
            Effect::LookAtHand { whose } => self.selector(whose),
            Effect::RemoveFromCombat { what } => self.selector(what),
            Effect::Shuffle { who } => self.selector(who),
            Effect::LookAndSort { who, count, .. } => {
                self.selector(who);
                self.value(count);
            }
            Effect::Destroy { what }
            | Effect::Regenerate { what }
            | Effect::Tap { what }
            | Effect::Untap { what }
            | Effect::CounterSpell { what, .. }
            | Effect::CopySpell { what, .. }
            | Effect::ChangeTargets { what, .. }
            | Effect::LoseGame { who: what }
            | Effect::PhaseOut { what }
            | Effect::WinGame { who: what }
            | Effect::CastWithoutPaying { what, .. } => self.selector(what),
            Effect::CounterUnlessPays { what, times, .. } => {
                self.selector(what);
                if let Some(times) = times {
                    self.value(times);
                }
            }
            Effect::Sacrifice { who, what } => {
                self.selector(who);
                self.selector(what);
            }
            Effect::DealDamage { source, to, amount } => {
                self.selector(source);
                self.selector(to);
                self.value(amount);
            }
            Effect::DealDamageDivided { source, shares, .. } => {
                self.selector(source);
                for to in shares {
                    self.selector(to);
                }
            }
            Effect::CopyCounters { from, to } => {
                self.selector(from);
                self.selector(to);
            }
            Effect::AddCounters { what, amount, .. }
            | Effect::RemoveCounters { what, amount, .. } => {
                self.selector(what);
                self.value(amount);
            }
            Effect::Fight { a, b } => {
                self.selector(a);
                self.selector(b);
            }
            Effect::Attach { what, to } => {
                self.selector(what);
                self.selector(to);
            }
            Effect::CreateToken {
                token,
                count,
                controller,
            } => {
                (self.token)(token);
                self.value(&mut token.power);
                self.value(&mut token.toughness);
                for a in &mut token.abilities {
                    self.ability(a);
                }
                self.value(count);
                self.selector(controller);
            }
            Effect::RevealHandChoose { who, filter, .. } => {
                self.selector(who);
                self.filter(filter);
            }
            Effect::ExtraTurn { who } | Effect::SkipNextTurn { who } => self.selector(who),
            Effect::AdditionalCombat => {}
            Effect::SpendOnly { only, effect, .. } => {
                self.filter(only);
                self.effect(effect);
            }
            Effect::PutAttacking { what } | Effect::ExileIfDiesThisTurn { what } => {
                self.selector(what)
            }
            Effect::Choose { then, .. } => self.effect(then),
            Effect::ExileSelfWithCounters { .. } | Effect::MarkOnceEachTurn => {}
            Effect::OnceEachTurn { body } => self.effect(body),
            Effect::AddManaAnyCombination { amount } => self.value(amount),
            Effect::RevealRandom { who } => self.selector(who),
            Effect::SearchLibraryAndGraveyard { filter } => self.filter(filter),
            Effect::ExileLinked { what } => self.selector(what),
            Effect::ReturnExiledWith { .. } => {}
            Effect::AsPlayer { who, body } => {
                self.selector(who);
                self.effect(body);
            }
            Effect::RollDie { outcomes, then, .. } => {
                for (_, _, e) in outcomes {
                    self.effect(e);
                }
                self.effect(then);
            }
            Effect::FlipCoin { win, lose } | Effect::Clash { win, lose } => {
                self.effect(win);
                self.effect(lose);
            }
            Effect::Reflexive { effect, targets } => {
                for t in targets {
                    self.filter(&mut t.filter);
                }
                self.effect(effect);
            }
            Effect::GrantPlay { what, .. } | Effect::GrantCastLater { what } => self.selector(what),
            Effect::Dig {
                count,
                take,
                filter,
                additional_filter,
                ..
            } => {
                self.value(count);
                self.value(take);
                self.filter(filter);
                if let Some(filter) = additional_filter {
                    self.filter(filter);
                }
            }
            Effect::BecomeMonarch { who, emblem } => {
                self.selector(who);
                if let Some(token) = emblem {
                    (self.token)(token);
                    for a in &mut token.abilities {
                        self.ability(a);
                    }
                }
            }
            Effect::ExileIfLeaves { what }
            | Effect::Explore { what }
            | Effect::ExileUntilSourceLeaves { what }
            | Effect::BecomeRenowned { what }
            | Effect::BecomeMonstrous { what }
            | Effect::Connive { what }
            | Effect::Transform { what }
            | Effect::ExileReturnTransformed { what }
            | Effect::EnterAttacking { what, .. } => self.selector(what),
            Effect::CreateTokenCopy {
                of,
                count,
                controller,
            } => {
                self.selector(of);
                self.value(count);
                self.selector(controller);
            }
            Effect::ExchangeControl { a, b } => {
                self.selector(a);
                self.selector(b);
            }
            Effect::GainControl { what, who, .. } => {
                self.selector(what);
                self.selector(who);
            }
            Effect::Continuous {
                what, modification, ..
            } => {
                self.selector(what);
                self.modification(modification);
            }
        }
    }

    fn mana(&mut self, m: &mut crate::ManaOutput) {
        if let crate::ManaOutput::Repeated { amount, output } = m {
            self.value(amount);
            self.mana(output);
        }
    }

    fn selector(&mut self, s: &mut Selector) {
        match s {
            Selector::SelfSource
            | Selector::You
            | Selector::Opponents
            | Selector::EachPlayer
            | Selector::ActivePlayer
            | Selector::DefendingPlayer
            | Selector::EnchantedPlayer
            | Selector::Player(_)
            | Selector::Target { .. }
            | Selector::Bound(_) => {}
            Selector::All { filter, .. } => self.filter(filter),
            Selector::ChosenBy {
                chooser,
                filter,
                count,
                ..
            } => {
                self.selector(chooser);
                self.filter(filter);
                self.value(count);
            }
            Selector::TopOfLibrary { player, count } => {
                self.selector(player);
                self.value(count);
            }
            Selector::Union(ss) => {
                for s in ss {
                    self.selector(s);
                }
            }
            Selector::Except(a, b) => {
                self.selector(a);
                self.selector(b);
            }
            Selector::ControllerOf(s) | Selector::OwnerOf(s) => self.selector(s),
        }
    }

    fn filter(&mut self, f: &mut ObjectFilter) {
        (self.filter)(f);
        match f {
            ObjectFilter::ControlledBy(s)
            | ObjectFilter::OwnedBy(s)
            | ObjectFilter::SharesColorWith(s)
            | ObjectFilter::SharesCreatureTypeWith(s) => self.selector(s),
            ObjectFilter::PowerAtMost(v)
            | ObjectFilter::PowerAtLeast(v)
            | ObjectFilter::ManaValueAtMost(v)
            | ObjectFilter::ManaValueAtLeast(v)
            | ObjectFilter::ToughnessAtMost(v) => self.value(v),
            ObjectFilter::Not(inner) | ObjectFilter::TargetsObject(inner) => self.filter(inner),
            ObjectFilter::And(fs) | ObjectFilter::Or(fs) => {
                for f in fs {
                    self.filter(f);
                }
            }
            ObjectFilter::Any
            | ObjectFilter::IsSelf
            | ObjectFilter::InBinding(_)
            | ObjectFilter::HasType(_)
            | ObjectFilter::HasSubtype(_)
            | ObjectFilter::HasSupertype(_)
            | ObjectFilter::HasColor(_)
            | ObjectFilter::Colorless
            | ObjectFilter::Tapped(_)
            | ObjectFilter::AttackingOrBlocking
            | ObjectFilter::Attacking
            | ObjectFilter::AttackingAlone
            | ObjectFilter::Blocking
            | ObjectFilter::AttachedToSelf
            | ObjectFilter::DealtDamageBySelfThisTurn
            | ObjectFilter::AttackedThisTurn
            | ObjectFilter::HasKeyword(_)
            | ObjectFilter::HasCounter(_)
            | ObjectFilter::HasAnyCounter
            | ObjectFilter::DealtDamageThisTurn
            | ObjectFilter::EnteredThisTurn
            | ObjectFilter::Token
            | ObjectFilter::AttachedToSource
            | ObjectFilter::BlockingSource
            | ObjectFilter::Named(_)
            | ObjectFilter::NamedLikeSource
            | ObjectFilter::Multicolored
            | ObjectFilter::HasChosenSubtype
            | ObjectFilter::HasChosenColor
            | ObjectFilter::Targetable
            | ObjectFilter::IsSpell
            | ObjectFilter::CastFromZone(_)
            | ObjectFilter::Kicked
            | ObjectFilter::HasXInCost
            | ObjectFilter::IsCommander
            | ObjectFilter::InZone(_)
            | ObjectFilter::SingleTarget
            | ObjectFilter::IsAbility => {}
        }
    }

    fn value(&mut self, v: &mut Value) {
        (self.value)(v);
        match v {
            Value::Fixed(_)
            | Value::X
            | Value::EventAmount
            | Value::ColorsSpent
            | Value::TimesKicked
            | Value::CastX
            | Value::OpponentsAttacked
            | Value::ManaSpentOfColor(_)
            | Value::SpellsCastBefore
            | Value::RollResult
            | Value::Devotion(_)
            | Value::StartingLife => {}
            Value::LifeGainedThisTurn(s) | Value::ColorsAmong(s) | Value::DistinctPowers(s) => {
                self.selector(s)
            }
            Value::Speed => {}
            Value::DiedThisTurn(f) => self.filter(f),
            Value::Half { value, .. } => self.value(value),
            Value::Count(s)
            | Value::LifeTotal(s)
            | Value::Counters(s, _)
            | Value::Power(s)
            | Value::Toughness(s)
            | Value::LeastToughness(s)
            | Value::GreatestPower(s)
            | Value::GreatestToughness(s)
            | Value::GreatestManaValue(s)
            | Value::ManaValue(s)
            | Value::CardTypesAmong(s)
            | Value::PartySize(s)
            | Value::BasicLandTypesAmong(s)
            | Value::SpellsCastThisTurn(s)
            | Value::MostSpellsCastThisTurn(s)
            | Value::CardsDrawnThisTurn(s) => self.selector(s),
            Value::Sum(vs) | Value::Product(vs) => {
                for v in vs {
                    self.value(v);
                }
            }
            Value::Negate(v) => self.value(v),
            Value::Max(a, b) | Value::Min(a, b) => {
                self.value(a);
                self.value(b);
            }
            Value::ChosenByController { min, max } => {
                self.value(min);
                self.value(max);
            }
            Value::If {
                cond,
                then,
                otherwise,
            } => {
                self.condition(cond);
                self.value(then);
                self.value(otherwise);
            }
        }
    }

    fn condition(&mut self, c: &mut Condition) {
        match c {
            Condition::Always
            | Condition::DuringStep(_)
            | Condition::YourTurn
            | Condition::Kicked
            | Condition::ControlledSinceLastUpkeep
            | Condition::OpponentDamagedThisTurn
            | Condition::YouAreAttacked
            | Condition::OpponentLostLifeThisTurn
            | Condition::YouGainedLifeThisTurn
            | Condition::CastFor(_)
            | Condition::Renowned
            | Condition::YouAttackedThisTurn
            | Condition::Monstrous
            | Condition::ClassLevelAtLeast(_)
            | Condition::WasCast(_)
            | Condition::CreatureDiedThisTurn
            | Condition::YouAreMonarch
            | Condition::MaxSpeed
            | Condition::HasCityBlessing
            | Condition::Saddled
            | Condition::NoSpellsLastTurn
            | Condition::PlayerCastTwoLastTurn => {}
            Condition::TargetsMatching(f) => self.filter(f),
            Condition::CountAtLeast { what, at_least: v }
            | Condition::CountAtMost { what, at_most: v } => {
                self.selector(what);
                self.value(v);
            }
            Condition::ValueAtLeast { lhs, rhs } | Condition::ValueEquals { lhs, rhs } => {
                self.value(lhs);
                self.value(rhs);
            }
            Condition::Exists(s) => self.selector(s),
            Condition::Not(c) => self.condition(c),
            Condition::And(cs) | Condition::Or(cs) => {
                for c in cs {
                    self.condition(c);
                }
            }
        }
    }
}

/// Renumber every subtype inside an ability, token specs included.
pub fn map_subtypes(a: &mut Ability, map: &mut dyn FnMut(mtg_core::Subtype) -> mtg_core::Subtype) {
    let map = std::cell::RefCell::new(map);
    Visitor {
        filter: &mut |f| {
            if let ObjectFilter::HasSubtype(s) = f {
                *s = (map.borrow_mut())(*s);
            }
        },
        token: &mut |t| {
            for s in &mut t.subtypes {
                *s = (map.borrow_mut())(*s);
            }
        },
        value: &mut |_| {},
    }
    .ability(a);
}

/// Replace every `from` value inside an effect with `to` ("the result" with the number
/// rolled).
pub fn substitute_value(e: &mut Effect, from: &Value, to: &Value) {
    Visitor {
        filter: &mut |_| {},
        token: &mut |_| {},
        value: &mut |v| {
            if v == from {
                *v = to.clone();
            }
        },
    }
    .effect(e);
}

/// Card ids from here up are token faces, so a table can grow without renumbering them.
pub const TOKEN_CARD_BASE: u32 = 1 << 24;

/// Give every token an ability can create a card face of its own.
///
/// Walks `faces`, and for each `CreateToken` without a card assigns one: identical specs
/// share a face. New faces are numbered from `first_id` and returned, in order, for the
/// caller to append to its table. The new faces are walked too, so a token that makes
/// tokens works.
pub fn register_tokens<'a>(
    faces: impl IntoIterator<Item = &'a mut crate::CardFace>,
    first_id: u32,
) -> Vec<crate::CardFace> {
    let mut specs: Vec<TokenSpec> = Vec::new();
    let mut made: Vec<crate::CardFace> = Vec::new();
    let assign =
        |t: &mut TokenSpec, specs: &mut Vec<TokenSpec>, made: &mut Vec<crate::CardFace>| {
            if t.card.is_some() {
                return;
            }
            let mut key = t.clone();
            key.card = None;
            let index = match specs.iter().position(|s| *s == key) {
                Some(i) => i,
                None => {
                    specs.push(key);
                    made.push(token_face(t));
                    specs.len() - 1
                }
            };
            t.card = Some(mtg_core::CardId(first_id + index as u32));
        };
    let walk = |abilities: &mut Vec<Ability>,
                specs: &mut Vec<TokenSpec>,
                made: &mut Vec<crate::CardFace>| {
        let cell = std::cell::RefCell::new((specs, made));
        for a in abilities {
            Visitor {
                filter: &mut |_| {},
                token: &mut |t| {
                    let mut g = cell.borrow_mut();
                    let (s, m) = &mut *g;
                    assign(t, s, m);
                },
                value: &mut |_| {},
            }
            .ability(a);
        }
    };
    for face in faces {
        walk(&mut face.abilities, &mut specs, &mut made);
    }
    // Faces made for tokens can themselves make tokens.
    let mut done = 0;
    while done < made.len() {
        let mut abilities = std::mem::take(&mut made[done].abilities);
        walk(&mut abilities, &mut specs, &mut made);
        made[done].abilities = abilities;
        done += 1;
    }
    made
}

/// The card face a token spec describes (CR 111.4: no mana cost, colours as stated).
fn token_face(t: &TokenSpec) -> crate::CardFace {
    let fixed = |v: &Value| match v {
        Value::Fixed(n) => Some(*n),
        _ => None,
    };
    let is_creature = t.types.contains(&mtg_core::CardType::Creature);
    crate::CardFace {
        name: t.name.clone(),
        mana_cost: mtg_core::ManaCost::FREE,
        card_types: t.types.clone(),
        subtypes: t.subtypes.clone(),
        supertypes: Vec::new(),
        power: if is_creature { fixed(&t.power) } else { None },
        toughness: if is_creature {
            fixed(&t.toughness)
        } else {
            None
        },
        loyalty: None,
        abilities: t
            .abilities
            .iter()
            .enumerate()
            .map(|(i, a)| Ability {
                id: mtg_core::AbilityId(i as u16),
                ..a.clone()
            })
            .collect(),
        oracle_text: None,
        colors: Some(t.colors.clone()),
    }
}
