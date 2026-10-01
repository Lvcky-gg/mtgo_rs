//! Driving the engine and narrating what happens.

use mtg_core::{CardId, Event, ObjectId, PlayerId, Step, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, Choice, ChoiceKind},
    state::GameState,
    view,
};
use mtg_policy::Policy;

use crate::cards::DemoCards;

pub struct Settings {
    /// Stop once this many turns have begun.
    pub turns: u32,
    /// Print every event, not just step transitions.
    pub verbose: bool,
    /// How much the client answers on the player's behalf.
    pub policy: Policy,
    /// Hard cap on engine steps, so a rules bug shows up as a stall rather than a
    /// hang.
    pub budget: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            turns: 3,
            verbose: false,
            policy: Policy::default(),
            budget: 200_000,
        }
    }
}

#[derive(Default)]
pub struct Report {
    pub turns_played: u32,
    pub events: usize,
    /// Choices the policy answered without involving the player. The whole point of
    /// `mtg-policy`, so it is counted rather than assumed.
    pub auto_answered: usize,
    /// Choices a human would actually have been shown.
    pub asked: usize,
    /// Ordering prompts specifically — the metric the trigger analysis exists to
    /// keep near zero.
    pub ordering_prompts: usize,
    pub winners: Vec<PlayerId>,
    pub stalled: bool,
    pub lines: Vec<String>,
}

/// Run a game, narrating it. Returns the finished engine so the caller can inspect
/// the final state without replaying.
pub fn run(state: GameState, cards: &DemoCards, settings: &Settings) -> (Engine, Report) {
    let mut engine = Engine::new(state);
    let mut report = Report::default();
    let mut last_step: Option<(u32, Step)> = None;
    let mut logged = 0usize;

    for _ in 0..settings.budget {
        // Narrate anything new in the log before deciding what happens next.
        while logged < engine.log.len() {
            let e = &engine.log[logged];
            if settings.verbose
                && let Some(line) = describe(&e.event, cards, &engine.state)
            {
                report.lines.push(format!("      {line}"));
            }
            logged += 1;
        }
        report.events = engine.log.len();

        let here = (engine.state.turn, engine.state.step);
        if last_step != Some(here) {
            last_step = Some(here);
            report.turns_played = engine.state.turn;
            report.lines.push(format!(
                "  T{} {:?} — {:?}",
                here.0, here.1, engine.state.active_player
            ));
        }

        if engine.state.turn > settings.turns {
            report.turns_played = settings.turns;
            return (engine, report);
        }

        match engine.advance(cards) {
            Progress::Continue => {}

            Progress::GameOver { winners } => {
                report.winners = winners.clone();
                report.lines.push(match winners.as_slice() {
                    [w] => format!("  game over — {w:?} wins"),
                    [] => "  game over — no survivors".to_string(),
                    many => format!("  game over — {many:?} still standing"),
                });
                return (engine, report);
            }

            Progress::NeedsChoice(c) => {
                let v = view::project(&engine.state, c.who);
                match settings.policy.may_auto_answer(&c, &v) {
                    // Handled without troubling the player.
                    Some(answer) => {
                        report.auto_answered += 1;
                        if engine.answer(cards, c.id, answer).is_err() {
                            report.stalled = true;
                            return (engine, report);
                        }
                    }
                    // A real decision. A GUI would render this; here it is logged and
                    // answered by a simple stand-in so the game keeps moving.
                    None => {
                        report.asked += 1;
                        if matches!(c.kind, ChoiceKind::OrderTriggers { .. }) {
                            report.ordering_prompts += 1;
                        }
                        report
                            .lines
                            .push(format!("  ? {}", prompt_line(&c, cards, &engine.state)));
                        let answer = stand_in(&c);
                        if engine.answer(cards, c.id, answer).is_err() {
                            report.stalled = true;
                            return (engine, report);
                        }
                    }
                }
            }
        }
    }

    report.stalled = true;
    report.lines.push("  stopped: step budget exhausted".into());
    (engine, report)
}

/// What an unattended run does when the policy declines to answer.
///
/// Deliberately simple and deliberately *not* in `mtg-policy`: this is a stand-in
/// for a human, not a rule, and pretending otherwise would hide how much the policy
/// is actually deciding.
fn stand_in(c: &Choice) -> Answer {
    match &c.kind {
        ChoiceKind::Priority { legal } => {
            // Prefer doing something over passing, so a demo run produces a board:
            // land first, then the most expensive spell affordable. Crude on purpose
            // — this stands in for a human, and dressing it up as strategy would
            // blur the line between the client and the policy layer.
            legal
                .actions
                .iter()
                .find(|a| matches!(a, Action::PlayLand { .. }))
                .or_else(|| {
                    legal
                        .actions
                        .iter()
                        .find(|a| matches!(a, Action::Cast { .. }))
                })
                .cloned()
                .map(Answer::Action)
                .unwrap_or(Answer::Pass)
        }
        // Attack with everything and block with everything: crude, but it exercises
        // combat, and a demo client is not the place for strategy.
        ChoiceKind::DeclareAttackers { eligible, .. } => Answer::Objects(eligible.clone()),
        ChoiceKind::DeclareBlockers { eligible } => Answer::Blocks(
            eligible
                .iter()
                .filter_map(|(blocker, can_block)| {
                    can_block.first().map(|attacker| (*blocker, *attacker))
                })
                .collect(),
        ),
        ChoiceKind::OrderBlockers { blockers, .. } => Answer::Order((0..blockers.len()).collect()),
        ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order(triggers.clone()),
        ChoiceKind::KeepOneLegend { candidates, .. } => {
            Answer::Objects(candidates.first().copied().into_iter().collect())
        }
        ChoiceKind::ChooseObjects { from, min, .. } => {
            Answer::Objects(from.iter().copied().take(*min as usize).collect())
        }
        ChoiceKind::Confirm => Answer::Bool(false),
        ChoiceKind::ChooseX { min, .. } => Answer::Number(*min),
        _ => c.default.clone().unwrap_or(Answer::Pass),
    }
}

fn prompt_line(c: &Choice, cards: &DemoCards, state: &GameState) -> String {
    match &c.kind {
        ChoiceKind::Priority { legal } => {
            // Two copies of a card are two distinct objects, so the raw list repeats
            // itself. Collapse to "play Training Field x4" for readability; the
            // engine's list is unchanged.
            let mut counts: Vec<(String, u32)> = Vec::new();
            for a in legal.meaningful() {
                let label = match a {
                    Action::PlayLand { object } => {
                        format!("play {}", name_of(*object, cards, state))
                    }
                    Action::Cast { object } => format!("cast {}", name_of(*object, cards, state)),
                    other => format!("{other:?}"),
                };
                match counts.iter_mut().find(|(l, _)| *l == label) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((label, 1)),
                }
            }
            let options: Vec<String> = counts
                .into_iter()
                .map(|(l, n)| if n > 1 { format!("{l} x{n}") } else { l })
                .collect();
            format!("{:?}: {} [{}]", c.who, c.because, options.join(", "))
        }
        // The explanation is the feature: an ordering prompt says which shared thing
        // makes the order matter.
        ChoiceKind::OrderTriggers {
            triggers,
            conflicts,
            ..
        } => {
            let over: Vec<String> = conflicts
                .iter()
                .flat_map(|x| x.over.iter().map(|r| format!("{r:?}")))
                .collect();
            format!(
                "{:?}: order {} triggers — they interact over {}",
                c.who,
                triggers.len(),
                if over.is_empty() {
                    "a choice each".into()
                } else {
                    over.join(", ")
                }
            )
        }
        ChoiceKind::KeepOneLegend { name, candidates } => {
            format!("{:?}: keep one of {} {}", c.who, candidates.len(), name)
        }
        ChoiceKind::DeclareAttackers { eligible, .. } => {
            format!("{:?}: declare attackers ({} able)", c.who, eligible.len())
        }
        ChoiceKind::DeclareBlockers { eligible } => {
            format!("{:?}: declare blockers ({} able)", c.who, eligible.len())
        }
        ChoiceKind::OrderBlockers { attacker, blockers } => format!(
            "{:?}: order {} blockers of {}",
            c.who,
            blockers.len(),
            name_of(*attacker, cards, state)
        ),
        ChoiceKind::AssignCombatDamage {
            attacker,
            among,
            total,
        } => format!(
            "{:?}: assign {} damage from {} among {} blockers",
            c.who,
            total,
            name_of(*attacker, cards, state),
            among.len()
        ),
        other => format!("{:?}: {:?}", c.who, std::mem::discriminant(other)),
    }
}

fn name_of(id: ObjectId, cards: &DemoCards, state: &GameState) -> String {
    state
        .objects
        .get(&id)
        .map(|o| cards.name(o.card).to_string())
        .unwrap_or_else(|| format!("{id:?}"))
}

fn card_name(card: CardId, cards: &DemoCards) -> &str {
    cards.name(card)
}

/// A one-line rendering of an event, or `None` for bookkeeping not worth printing.
fn describe(event: &Event, cards: &DemoCards, state: &GameState) -> Option<String> {
    Some(match event {
        Event::ZoneChange {
            new_object,
            from,
            to,
            ..
        } => {
            let what = state
                .objects
                .get(new_object)
                .map(|o| card_name(o.card, cards))
                .unwrap_or("something");
            format!("{what}: {:?} → {:?}", from.zone, to.zone)
        }
        Event::Drew { player, object } => {
            format!("{player:?} draws {}", name_of(*object, cards, state))
        }
        Event::AttemptedDrawFromEmptyLibrary { player } => {
            format!("{player:?} tried to draw from an empty library")
        }
        Event::LifeChanged { player, delta } => format!("{player:?} life {delta:+}"),
        Event::DamageMarked {
            object,
            amount,
            deathtouch,
            ..
        } => format!(
            "{} takes {amount}{}",
            name_of(*object, cards, state),
            if *deathtouch { " (deathtouch)" } else { "" }
        ),
        Event::DamageDealtToPlayer { player, amount, .. } => {
            format!("{player:?} takes {amount}")
        }
        Event::CountersChanged {
            object,
            kind,
            delta,
        } => {
            format!("{} {delta:+} {kind:?}", name_of(*object, cards, state))
        }
        Event::TapChanged { object, tapped } => format!(
            "{} {}",
            name_of(*object, cards, state),
            if *tapped { "taps" } else { "untaps" }
        ),
        Event::Lost { player, reason } => format!("{player:?} loses ({reason:?})"),
        Event::AbilityActivated { source, .. } => {
            format!("{} activated an ability", name_of(*source, cards, state))
        }
        Event::Attacked { attacker, .. } => {
            format!("{} attacks", name_of(*attacker, cards, state))
        }
        Event::Blocked { blocker, attacker } => format!(
            "{} blocks {}",
            name_of(*blocker, cards, state),
            name_of(*attacker, cards, state)
        ),
        Event::AbilityPutOnStack { source, .. } => {
            format!("{} triggers", name_of(*source, cards, state))
        }
        // By narration time the stack object is gone (it took a new identity in its
        // destination zone), and the zone change that follows already names it.
        Event::Resolved { .. } => return None,
        Event::SpellCast { object, controller } => {
            format!("{controller:?} casts {}", name_of(*object, cards, state))
        }
        // Step transitions and priority are printed by the driver, not here.
        Event::StepBegan { .. } | Event::StepEnded { .. } | Event::PriorityReceived { .. } => {
            return None;
        }
        _ => return None,
    })
}

/// A readable snapshot of the board, for the end of a run.
pub fn board_summary(engine: &Engine, cards: &DemoCards) -> Vec<String> {
    let mut out = Vec::new();
    for p in &engine.state.turn_order {
        let ps = engine.state.player(*p);
        out.push(format!(
            "{p:?} — {} life, {} in hand, {} in library, {} in graveyard{}",
            ps.life,
            engine.state.objects_in(ZoneRef::of(Zone::Hand, *p)).len(),
            engine
                .state
                .objects_in(ZoneRef::of(Zone::Library, *p))
                .len(),
            engine
                .state
                .objects_in(ZoneRef::of(Zone::Graveyard, *p))
                .len(),
            if ps.has_lost { " [lost]" } else { "" }
        ));
        for id in engine.state.battlefield() {
            let Some(o) = engine.state.objects.get(&id) else {
                continue;
            };
            if o.controller != *p {
                continue;
            }
            // Power and toughness come from the layer system, not the printed card.
            let ch = mtg_engine::layers::compute(&engine.state, cards, id);
            let pt = ch
                .as_ref()
                .and_then(|c| c.power.zip(c.toughness))
                .map(|(pw, t)| format!(" {pw}/{t}"))
                .unwrap_or_default();
            out.push(format!(
                "    {}{}{}",
                cards.name(o.card),
                pt,
                if o.tapped { " (tapped)" } else { "" }
            ));
        }
    }
    out
}
