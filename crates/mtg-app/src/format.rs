//! Turning game state into text a person can read.
//!
//! Pure functions, separated from the renderer so they can be tested without a window — which
//! matters more than usual here, because the thing most likely to be wrong in a UI is the label,
//! not the rectangle.
//!
//! Card text here is what the text panel shows, and what hovering over a card image shows. See
//! ADR-010 for when a card is drawn as its image instead.

use mtg_core::{Characteristics, ManaCost, ManaSymbol};
use mtg_engine::{Choice, ChoiceKind, PlayerView, actions::Action, view::ObjectView};

/// Render a mana cost the way it is printed: `{2}{W}{U}`.
pub fn mana_cost(cost: &ManaCost) -> String {
    if cost.symbols.is_empty() {
        return String::new();
    }
    cost.symbols.iter().map(symbol).collect()
}

fn symbol(s: &ManaSymbol) -> String {
    match s {
        ManaSymbol::Generic(n) => format!("{{{n}}}"),
        ManaSymbol::Variable => "{X}".into(),
        ManaSymbol::Colored(c) => format!("{{{}}}", color_letter(*c)),
        ManaSymbol::Colorless => "{C}".into(),
        ManaSymbol::Snow => "{S}".into(),
        ManaSymbol::Hybrid(a, b) => {
            format!("{{{}/{}}}", color_letter(*a), color_letter(*b))
        }
        ManaSymbol::MonoHybrid(n, c) => format!("{{{n}/{}}}", color_letter(*c)),
        ManaSymbol::Phyrexian(c) => format!("{{{}/P}}", color_letter(*c)),
    }
}

fn color_letter(c: mtg_core::Color) -> char {
    match c {
        mtg_core::Color::White => 'W',
        mtg_core::Color::Blue => 'U',
        mtg_core::Color::Black => 'B',
        mtg_core::Color::Red => 'R',
        mtg_core::Color::Green => 'G',
    }
}

/// The type line, as printed: `Legendary Creature — Human Wizard`.
pub fn type_line(
    ch: &Characteristics,
    subtype_name: impl Fn(mtg_core::Subtype) -> String,
) -> String {
    let mut front: Vec<String> = ch.supertypes.iter().map(|s| format!("{s:?}")).collect();
    front.extend(ch.card_types.iter().map(|t| format!("{t:?}")));

    let subs: Vec<String> = ch
        .subtypes
        .iter()
        .map(|s| subtype_name(*s))
        .filter(|s| !s.is_empty())
        .collect();
    if subs.is_empty() {
        front.join(" ")
    } else {
        // An em dash, as printed.
        format!("{} — {}", front.join(" "), subs.join(" "))
    }
}

/// Power and toughness, or nothing for a card that has none.
pub fn power_toughness(ch: &Characteristics) -> Option<String> {
    let (p, t) = (ch.power?, ch.toughness?);
    Some(format!("{p}/{t}"))
}

/// A one-line description of a permanent on the battlefield.
pub fn permanent_line(obj: &ObjectView, ch: Option<&Characteristics>, name: &str) -> String {
    let mut parts = vec![name.to_string()];
    if let Some(ch) = ch
        && let Some(pt) = power_toughness(ch)
    {
        parts.push(pt);
    }
    if obj.tapped {
        parts.push("(tapped)".into());
    }
    if obj.damage > 0 {
        parts.push(format!("({} damage)", obj.damage));
    }
    for (kind, n) in &obj.counters {
        if *n != 0 {
            parts.push(format!("({})", counter_total(*kind, *n)));
        }
    }
    parts.join(" ")
}

/// Readable names for public counter kinds. Custom counters have no names in the view yet.
pub fn counter_name(kind: mtg_core::CounterKind) -> &'static str {
    use mtg_core::CounterKind;
    match kind {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
        CounterKind::Loyalty => "loyalty",
        CounterKind::Defense => "defense",
        CounterKind::Poison => "poison",
        CounterKind::Stun => "stun",
        CounterKind::Shield => "shield",
        CounterKind::Other(_) => "other",
    }
}

pub fn counter_total(kind: mtg_core::CounterKind, count: i32) -> String {
    let noun = if count == 1 { "counter" } else { "counters" };
    format!("{count} {} {noun}", counter_name(kind))
}

/// What a button for this action should say.
pub fn action_label(action: &Action, name_of: impl Fn(mtg_core::ObjectId) -> String) -> String {
    match action {
        Action::Pass => "Pass".into(),
        Action::Concede => "Concede".into(),
        Action::PlayLand { object } | Action::PlayLandFace { object, .. } => {
            format!("Play {}", name_of(*object))
        }
        Action::Cast { object } | Action::CastFace { object, .. } => {
            format!("Cast {}", name_of(*object))
        }
        Action::CastFaceDown { object } => format!("Cast {} face down", name_of(*object)),
        Action::CastAlternative { object, .. } => {
            format!("Cast {} for its alternative cost", name_of(*object))
        }
        Action::ActivateAbility { source, .. } => format!("Activate {}", name_of(*source)),
        Action::SpecialAction { source, .. } => format!("Use {}", name_of(*source)),
        Action::ActivateManaAbility { source, color, .. } => match color {
            Some(c) => format!("Tap {} for {{{}}}", name_of(*source), color_letter(*c)),
            None => format!("Tap {} for mana", name_of(*source)),
        },
    }
}

/// Floating mana as symbols, `{W}{W}{G}`, in pool slot order. Empty when nothing floats.
pub fn mana_pool(amounts: &[u16; 6]) -> String {
    let letters = ["W", "U", "B", "R", "G", "C"];
    amounts
        .iter()
        .zip(letters)
        .flat_map(|(n, l)| std::iter::repeat_n(format!("{{{l}}}"), *n as usize))
        .collect()
}

/// The heading for a question.
///
/// The engine already supplies a `because` string naming the rules reason; this adds the shape of
/// the question, so a player is never asked something without being told what kind of thing it is.
pub fn choice_heading(choice: &Choice) -> String {
    let kind = match &choice.kind {
        ChoiceKind::Priority { .. } => "Your turn to act",
        ChoiceKind::OrderTriggers { .. } => "Order your triggers",
        ChoiceKind::OrderReplacements { .. } => "Order replacement effects",
        ChoiceKind::ChooseTargets { .. } => "Choose targets",
        ChoiceKind::ChooseModes { .. } => "Choose a mode",
        ChoiceKind::ChooseX { .. } => "Choose a number",
        ChoiceKind::Confirm => "Confirm",
        ChoiceKind::ChooseObjects { .. } => "Choose",
        ChoiceKind::KeepOneLegend { .. } => "The legend rule",
        ChoiceKind::PayMana { .. } => "Pay mana",
        ChoiceKind::DeclareAttackers { .. } => "Declare attackers",
        ChoiceKind::DeclareBlockers { .. } => "Declare blockers",
        ChoiceKind::OrderBlockers { .. } => "Order blockers",
        ChoiceKind::AssignCombatDamage { .. } => "Assign combat damage",
        ChoiceKind::DiscardToHandSize { .. } => "Discard to hand size",
        ChoiceKind::KeepOrMulligan { .. } => {
            "Your opening hand — keep it, or mulligan for a new one"
        }
    };
    // The engine's reason is shown only when it adds something to the heading.
    if kind.to_lowercase().contains(&choice.because.to_lowercase()) {
        kind.to_string()
    } else {
        format!("{kind} — {}", choice.because)
    }
}

/// Why an ordering prompt is being shown.
///
/// The engine only asks when it could not prove the order irrelevant, and it says which shared
/// state made the difference. Surfacing that is the difference between a dialog a player resents
/// and one they learn from.
pub fn conflict_explanation(choice: &Choice) -> Option<String> {
    let ChoiceKind::OrderTriggers { conflicts, .. } = &choice.kind else {
        return None;
    };
    if conflicts.is_empty() {
        return None;
    }
    let mut reasons: Vec<String> = Vec::new();
    for c in conflicts {
        let over: Vec<String> = c.over.iter().map(describe_resource).collect();
        reasons.push(if over.is_empty() {
            format!("{:?}", c.kind)
        } else {
            format!("they both affect {}", over.join(" and "))
        });
    }
    reasons.dedup();
    Some(format!("The order matters here: {}.", reasons.join("; ")))
}

fn describe_resource(r: &mtg_ir::footprint::Resource) -> String {
    use mtg_ir::footprint::Resource as R;
    match r {
        R::Object(_) => "the same permanent".into(),
        R::ObjectClass => "an overlapping set of permanents".into(),
        R::Life(p) => format!("{p:?}'s life total"),
        R::ManaPool(p) => format!("{p:?}'s mana"),
        R::ZoneOf(p, z) => format!("{p:?}'s {z:?}"),
        R::SharedZone(z) => format!("the {z:?}"),
        R::Counters(_, kind) => format!("{} counters", counter_name(*kind)),
        R::ContinuousEffects => "continuous effects".into(),
        R::Stack => "the stack".into(),
        R::TurnState => "the turn".into(),
        R::Unanalysable => "something this build cannot analyse".into(),
    }
}

/// How a player is named on screen: "You" for the viewer, otherwise their seat.
///
/// A seat number rather than a debug print — `PlayerId(1)` is an internal handle, not a name.
pub fn player_name(view: &PlayerView, p: mtg_core::PlayerId) -> String {
    if p == view.viewer {
        "You".into()
    } else if view.players.len() <= 2 {
        "Opponent".into()
    } else {
        format!("Opponent {}", p.0)
    }
}

/// A short status line: whose turn, which step, life totals.
pub fn status_line(view: &PlayerView) -> String {
    let mine = view.players.get(&view.viewer).map(|p| p.life).unwrap_or(0);
    let theirs: Vec<String> = view
        .players
        .values()
        .filter(|p| p.id != view.viewer)
        .map(|p| format!("{} {}", player_name(view, p.id), p.life))
        .collect();
    let whose = if view.active_player == view.viewer {
        "your turn"
    } else {
        "their turn"
    };
    format!(
        "Turn {} · {} · {} · you {} · {}",
        view.turn,
        whose,
        step_name(view.step),
        mine,
        theirs.join(", ")
    )
}

/// A step as a player says it.
pub fn step_name(step: mtg_core::Step) -> &'static str {
    use mtg_core::Step as S;
    match step {
        S::Untap => "Untap",
        S::Upkeep => "Upkeep",
        S::Draw => "Draw",
        S::PrecombatMain => "Main phase",
        S::BeginCombat => "Beginning of combat",
        S::DeclareAttackers => "Declare attackers",
        S::DeclareBlockers => "Declare blockers",
        S::FirstStrikeCombatDamage => "First-strike damage",
        S::CombatDamage => "Combat damage",
        S::EndCombat => "End of combat",
        S::PostcombatMain => "Second main phase",
        S::End => "End step",
        S::Cleanup => "Cleanup",
    }
}

/// The steps shown in the phase bar, with a short label each, in turn order.
pub const PHASE_BAR: [(mtg_core::Step, &str); 11] = {
    use mtg_core::Step as S;
    [
        (S::Untap, "Untap"),
        (S::Upkeep, "Upkeep"),
        (S::Draw, "Draw"),
        (S::PrecombatMain, "Main 1"),
        (S::BeginCombat, "Combat"),
        (S::DeclareAttackers, "Attack"),
        (S::DeclareBlockers, "Block"),
        (S::CombatDamage, "Damage"),
        (S::EndCombat, "End combat"),
        (S::PostcombatMain, "Main 2"),
        (S::End, "End"),
    ]
};

/// The heading for a priority question: whose moment it is, and what can be done with it.
pub fn priority_heading(view: &PlayerView, top_of_stack: Option<&str>) -> String {
    use mtg_core::Step as S;
    let mine = view.active_player == view.viewer;
    if !view.stack.is_empty() {
        return format!(
            "{} is on the stack — respond, or pass priority",
            top_of_stack.unwrap_or("An item")
        );
    }
    match (mine, view.step) {
        (true, S::PrecombatMain) => "Your main phase — act, or pass toward combat".into(),
        (true, S::PostcombatMain) => {
            "Your second main phase — act, or pass toward the end step".into()
        }
        (true, _) => format!("Your turn · {}", step_name(view.step)),
        (false, _) => format!(
            "Opponent's turn · {} — you may respond",
            step_name(view.step)
        ),
    }
}

/// The current-step intent of passing. This sends one priority pass; it never
/// commits to skipping other players' responses or future decisions.
pub fn pass_label(view: &PlayerView) -> String {
    use mtg_core::Step as S;
    if !view.stack.is_empty() || view.active_player != view.viewer {
        return "Pass priority".into();
    }
    match view.step {
        S::Untap => "Pass priority",
        S::Upkeep => "Finish upkeep",
        S::Draw => "Finish draw step",
        S::PrecombatMain => "Go to combat",
        // Attackers, blocks and first-strike damage can change which decisions
        // follow. The projected view is not an oracle for that continuation.
        S::BeginCombat
        | S::DeclareAttackers
        | S::DeclareBlockers
        | S::FirstStrikeCombatDamage
        | S::CombatDamage => "Continue combat",
        S::EndCombat => "Go to second main",
        S::PostcombatMain => "Go to end step",
        S::End => "Finish end step",
        S::Cleanup => "Continue cleanup",
    }
    .into()
}

/// Explain the exact action behind contextual button labels and the Space shortcut.
pub fn pass_hint(view: &PlayerView) -> &'static str {
    if !view.stack.is_empty() {
        "Space: pass priority. The top stack item resolves only after everyone passes."
    } else {
        "Space: pass priority. Other players may respond before the step changes."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{CardType, Color, Supertype};

    fn chars(name: &str) -> Characteristics {
        Characteristics {
            name: name.into(),
            mana_cost: ManaCost::FREE,
            colors: mtg_core::ColorSet::COLORLESS,
            card_types: vec![CardType::Creature],
            subtypes: Vec::new(),
            supertypes: Vec::new(),
            abilities: Vec::new(),
            granted_keywords: Vec::new(),
            every_creature_type: false,
            granted_abilities: Vec::new(),
            power: Some(2),
            toughness: Some(3),
            loyalty: None,
            defense: None,
        }
    }

    #[test]
    fn a_mana_cost_reads_as_printed() {
        let cost = ManaCost {
            symbols: vec![
                ManaSymbol::Generic(2),
                ManaSymbol::Colored(Color::White),
                ManaSymbol::Colored(Color::Blue),
            ],
        };
        assert_eq!(mana_cost(&cost), "{2}{W}{U}");
    }

    #[test]
    fn a_free_cost_renders_as_nothing() {
        assert_eq!(mana_cost(&ManaCost::FREE), "");
    }

    #[test]
    fn the_three_slash_forms_are_visually_distinct() {
        // A player must be able to tell hybrid, mono-hybrid and phyrexian apart at a glance.
        let cost = ManaCost {
            symbols: vec![
                ManaSymbol::Hybrid(Color::White, Color::Blue),
                ManaSymbol::MonoHybrid(2, Color::White),
                ManaSymbol::Phyrexian(Color::White),
            ],
        };
        assert_eq!(mana_cost(&cost), "{W/U}{2/W}{W/P}");
    }

    #[test]
    fn variable_and_colorless_costs_render() {
        let cost = ManaCost {
            symbols: vec![
                ManaSymbol::Variable,
                ManaSymbol::Colorless,
                ManaSymbol::Snow,
            ],
        };
        assert_eq!(mana_cost(&cost), "{X}{C}{S}");
    }

    #[test]
    fn a_type_line_without_subtypes_has_no_dash() {
        let mut c = chars("Thing");
        c.card_types = vec![CardType::Instant];
        assert_eq!(type_line(&c, |_| String::new()), "Instant");
    }

    #[test]
    fn a_type_line_with_subtypes_uses_an_em_dash() {
        let mut c = chars("Thing");
        c.subtypes = vec![mtg_core::Subtype(1), mtg_core::Subtype(2)];
        let names = |s: mtg_core::Subtype| match s.0 {
            1 => "Human".to_string(),
            _ => "Wizard".to_string(),
        };
        assert_eq!(type_line(&c, names), "Creature — Human Wizard");
    }

    #[test]
    fn supertypes_come_before_card_types() {
        let mut c = chars("Thing");
        c.supertypes = vec![Supertype::Legendary];
        assert_eq!(type_line(&c, |_| String::new()), "Legendary Creature");
    }

    #[test]
    fn power_and_toughness_is_absent_for_a_noncreature() {
        let mut c = chars("Thing");
        assert_eq!(power_toughness(&c), Some("2/3".into()));
        c.power = None;
        assert_eq!(power_toughness(&c), None);
    }

    #[test]
    fn action_labels_name_the_card_rather_than_an_id() {
        let name = |_id: mtg_core::ObjectId| "Quiet Field".to_string();
        assert_eq!(
            action_label(
                &Action::PlayLand {
                    object: mtg_core::ObjectId(1)
                },
                name
            ),
            "Play Quiet Field"
        );
        let name = |_id: mtg_core::ObjectId| "Spark Jolt".to_string();
        assert_eq!(
            action_label(
                &Action::Cast {
                    object: mtg_core::ObjectId(2)
                },
                name
            ),
            "Cast Spark Jolt"
        );
        assert_eq!(action_label(&Action::Pass, |_| String::new()), "Pass");
    }

    #[test]
    fn tapping_for_mana_names_the_colour_when_there_was_a_choice() {
        let name = |_id: mtg_core::ObjectId| "Quiet Grove".to_string();
        let tap = |color| Action::ActivateManaAbility {
            source: mtg_core::ObjectId(1),
            ability: mtg_core::AbilityId(0),
            color,
        };
        assert_eq!(
            action_label(&tap(Some(mtg_core::Color::Green)), name),
            "Tap Quiet Grove for {G}"
        );
        assert_eq!(action_label(&tap(None), name), "Tap Quiet Grove for mana");
    }

    #[test]
    fn a_mana_pool_reads_as_symbols() {
        assert_eq!(mana_pool(&[0; 6]), "");
        assert_eq!(mana_pool(&[2, 0, 0, 0, 1, 1]), "{W}{W}{G}{C}");
    }

    #[test]
    fn a_permanent_line_shows_state_a_player_needs() {
        let obj = mtg_engine::view::ObjectView {
            id: mtg_core::ObjectId(1),
            zone: mtg_core::ZoneRef::shared(mtg_core::Zone::Battlefield),
            controller: mtg_core::PlayerId(0),
            card: Some(mtg_core::CardId(0)),
            face: 0,
            adventure_player: None,
            tapped: true,
            damage: 2,
            counters: Default::default(),
            attached_to: None,
            targets: Vec::new(),
            is_ability: false,
            ability: None,
            attacking: false,
            attacking_target: None,
            blocking: None,
        };
        let line = permanent_line(&obj, Some(&chars("Bear")), "Bear");
        assert!(line.contains("Bear"));
        assert!(line.contains("2/3"), "power and toughness: {line}");
        assert!(line.contains("tapped"), "tapped state: {line}");
        assert!(line.contains("2 damage"), "marked damage: {line}");
    }
}
