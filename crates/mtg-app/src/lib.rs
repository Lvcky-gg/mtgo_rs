//! The native application.
//!
//! A renderer over [`PlayerView`] and a [`Decide`] implementation. It holds **no rules**: the
//! engine publishes the legal actions and the questions, and this draws them. Nothing is
//! clickable that the engine did not offer, which is what makes the view layer unable to contain
//! a rules bug.
//!
//! # Why the game runs on a worker thread
//!
//! `Decide::decide` blocks until a player answers. egui is a per-frame loop, so it cannot block —
//! a UI that waited inside `decide` would stop drawing and stop responding.
//!
//! So the game runs on its own thread and the two talk over channels: the game thread sends a
//! question and blocks on the answer, the UI thread renders and sends an answer when the player
//! clicks. That also means the game logic is untouched by the UI's existence, which is why
//! [`UiDecider`] is the only piece either side needs to know about.
//!
//! # Card images
//!
//! A card with a real printing is drawn as its image, fetched from Scryfall's CDN the first time
//! it is seen and cached on disk (see [`art`]). Nothing is bundled. Anything without an image — an
//! invented card, a failed download, a card still arriving — is drawn as a plain panel of name,
//! cost, types, power/toughness and rules text. See ADR-010, amending ADR-009.

pub mod app;
pub mod art;
pub mod board;
pub mod builder;
pub mod cards_text;
pub mod deck_profile;
pub mod decks;
pub mod format;
pub mod mana_text;
pub mod narrate;
pub mod net;
pub mod seat;
pub mod theme;
mod tournaments;
pub mod ui;
pub mod widgets;

use std::sync::mpsc::{Receiver, Sender};

use mtg_engine::{Choice, PlayerView, choice::Answer};
use mtg_session::Decide;

/// A question waiting to be answered, with the position it concerns.
#[derive(Clone, Debug)]
pub struct Question {
    pub choice: Choice,
    pub view: PlayerView,
}

/// A [`Decide`] that defers to a user interface on another thread.
///
/// Sends the question across and blocks on the answer. If the UI has gone away, it falls back to
/// the choice's own default rather than hanging — a closed window should end the game, not wedge
/// the thread holding it.
pub struct UiDecider {
    to_ui: Sender<Question>,
    from_ui: Receiver<Answer>,
    /// Every position the game passes through, answered by a person or not — so the window
    /// can keep the board current and tell the story while the policy is doing the passing.
    observers: Option<Sender<PlayerView>>,
}

impl UiDecider {
    /// Create a decider and the two ends the UI holds.
    pub fn new() -> (Self, Receiver<Question>, Sender<Answer>) {
        let (to_ui, questions) = std::sync::mpsc::channel();
        let (answers, from_ui) = std::sync::mpsc::channel();
        (
            Self {
                to_ui,
                from_ui,
                observers: None,
            },
            questions,
            answers,
        )
    }

    /// Also send every view the game shows this player, including ones the policy answers
    /// without asking. Returns the receiving end, for the window.
    pub fn watch(&mut self) -> Receiver<PlayerView> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.observers = Some(tx);
        rx
    }

    fn observe(&self, view: &PlayerView) {
        if let Some(tx) = &self.observers {
            let _ = tx.send(view.clone());
        }
    }
}

impl Decide for UiDecider {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        let question = Question {
            choice: choice.clone(),
            view: view.clone(),
        };
        if self.to_ui.send(question).is_err() {
            // The window is gone. Take the rules' default so the game ends tidily.
            return mtg_policy::well_formed(choice, view);
        }
        self.from_ui
            .recv()
            .unwrap_or_else(|_| mtg_policy::well_formed(choice, view))
    }
}

/// A decider that consults a policy first and a human second.
///
/// This is the arrangement the whole prompt-eliding design exists for: the policy answers what it
/// can prove is not a decision, and only what is left reaches the player. Without it a human
/// would be asked to pass priority hundreds of times a game.
pub struct PolicyThenUi {
    pub policy: mtg_policy::Policy,
    pub ui: UiDecider,
    /// How many questions the policy handled, for the status line. Seeing this number is how a
    /// player learns the client is working for them.
    pub auto_answered: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl PolicyThenUi {
    pub fn new(policy: mtg_policy::Policy, ui: UiDecider) -> Self {
        Self {
            policy,
            ui,
            auto_answered: Default::default(),
        }
    }
}

impl Decide for PolicyThenUi {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        self.ui.observe(view);
        if let Some(answer) = self.policy.may_auto_answer(choice, view) {
            self.auto_answered
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return answer;
        }
        self.ui.decide(choice, view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_engine::ChoiceKind;

    fn a_choice() -> Choice {
        Choice {
            id: 1,
            who: mtg_core::PlayerId(0),
            kind: ChoiceKind::Confirm,
            because: "test".into(),
            default: Some(Answer::Bool(false)),
            undo: false,
        }
    }

    fn a_view() -> PlayerView {
        PlayerView {
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer: mtg_core::PlayerId(0),
            turn: 1,
            active_player: mtg_core::PlayerId(0),
            step: mtg_core::Step::PrecombatMain,
            priority: Some(mtg_core::PlayerId(0)),
            players: Default::default(),
            visible: Default::default(),
            stack: Vec::new(),
        }
    }

    #[test]
    fn a_question_reaches_the_ui_and_the_answer_comes_back() {
        let (mut decider, questions, answers) = UiDecider::new();

        let ui = std::thread::spawn(move || {
            let q = questions.recv().expect("a question arrived");
            assert_eq!(q.choice.id, 1);
            answers.send(Answer::Bool(true)).expect("answer sent");
        });

        let answer = decider.decide(&a_choice(), &a_view());
        ui.join().unwrap();
        assert!(matches!(answer, Answer::Bool(true)));
    }

    #[test]
    fn a_closed_window_falls_back_to_the_default_rather_than_hanging() {
        let (mut decider, questions, answers) = UiDecider::new();
        drop(questions);
        drop(answers);

        // Must return, and must return something legal.
        let answer = decider.decide(&a_choice(), &a_view());
        assert!(
            matches!(answer, Answer::Bool(false)),
            "the choice's own default"
        );
    }

    #[test]
    fn a_ui_that_stops_answering_does_not_wedge_the_game() {
        let (mut decider, questions, answers) = UiDecider::new();
        // The UI takes the question and then dies without answering.
        let ui = std::thread::spawn(move || {
            let _ = questions.recv();
            drop(answers);
        });
        let answer = decider.decide(&a_choice(), &a_view());
        ui.join().unwrap();
        assert!(matches!(answer, Answer::Bool(false)));
    }

    #[test]
    fn the_policy_answers_first_and_the_player_is_not_asked() {
        // The point of the prompt-eliding design: a `Confirm` is a real decision, but a priority
        // choice with nothing to do is not, and the player should never see it.
        let (ui, questions, _answers) = UiDecider::new();
        let mut decider = PolicyThenUi::new(mtg_policy::Policy::default(), ui);

        let choice = Choice {
            id: 2,
            who: mtg_core::PlayerId(0),
            kind: ChoiceKind::Priority {
                legal: Default::default(),
            },
            because: "nothing to do".into(),
            default: Some(Answer::Pass),
            undo: false,
        };

        let answer = decider.decide(&choice, &a_view());
        assert!(matches!(answer, Answer::Pass));
        assert_eq!(
            decider
                .auto_answered
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "handled without troubling the player"
        );
        assert!(questions.try_recv().is_err(), "nothing was sent to the UI");
    }
}

pub mod printings;

pub mod sound;

mod tunnel;
