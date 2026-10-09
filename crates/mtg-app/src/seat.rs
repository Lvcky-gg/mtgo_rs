//! The two kinds of player a match on this machine can have: the person at the window, and the
//! bot.
//!
//! Both are [`Seat`]s, so a match loop cannot tell them apart. The window's seat runs on the
//! match's worker thread and talks to the window over channels: questions go through
//! [`PolicyThenUi`] exactly as before, and everything else about the match — who plays first,
//! the score, a request to sideboard — goes as a [`MatchEvent`].

use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, Sender},
};

use mtg_engine::{Choice, ChoiceKind, PlayerView, actions::Action, choice::Answer};
use mtg_ir::PrintedCards;
use mtg_policy::bot::Bot;
use mtg_session::{
    Decide,
    game::{CardKey, DeckSpec, GameCards, MatchSettings},
    matches::{GameEnd, GameStart, MatchEnd, PlayDrawChoice, Seat},
};

use crate::{PolicyThenUi, cards_text::CardTexts};

/// What the window hears from a running match, besides questions.
pub enum MatchEvent {
    /// Connection progress, for the waiting screen.
    Status(String),
    /// A host's invite link, to be shared.
    Invite(String),
    MatchStarted {
        settings: MatchSettings,
        texts: CardTexts,
    },
    PlayDraw(PlayDrawChoice),
    GameStarted(GameStart),
    GameEnded(GameEnd),
    /// Between games: the current deck, and card names for showing it. Answered with the
    /// deck to play next.
    Sideboard {
        deck: DeckSpec,
        names: BTreeMap<CardKey, String>,
    },
    MatchEnded(Result<MatchEnd, String>),
}

/// The person at the window.
pub struct UiSeat {
    pub inner: PolicyThenUi,
    pub events: Sender<MatchEvent>,
    pub sideboards: Receiver<DeckSpec>,
    pub play_draws: Receiver<bool>,
    /// Set by the window when the player leaves the match.
    pub left: Arc<AtomicBool>,
}

impl Decide for UiSeat {
    fn observe(&mut self, view: &PlayerView) {
        self.inner.observe(view);
    }
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        // A player who has left concedes at the next opportunity rather than leaving the game
        // to play itself out.
        if self.left.load(Ordering::Relaxed) {
            if matches!(choice.kind, ChoiceKind::Priority { .. }) {
                return Answer::Action(Action::Concede);
            }
            return mtg_policy::well_formed(choice, view);
        }
        self.inner.decide(choice, view)
    }
}

impl Seat for UiSeat {
    fn begin_match(&mut self, settings: MatchSettings, cards: &GameCards) {
        let texts = CardTexts::for_match(cards);
        let _ = self
            .events
            .send(MatchEvent::MatchStarted { settings, texts });
    }

    fn choose_play_first(&mut self, choice: &PlayDrawChoice) -> bool {
        if self.left.load(Ordering::Relaxed)
            || self
                .events
                .send(MatchEvent::PlayDraw(choice.clone()))
                .is_err()
        {
            return true;
        }
        loop {
            if self.left.load(Ordering::Relaxed) {
                return true;
            }
            match self
                .play_draws
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(play_first) => return play_first,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
            }
        }
    }

    fn begin_game(&mut self, start: &GameStart) {
        let _ = self.events.send(MatchEvent::GameStarted(start.clone()));
    }

    fn end_game(&mut self, end: &GameEnd) {
        let _ = self.events.send(MatchEvent::GameEnded(end.clone()));
    }

    fn sideboard(&mut self, deck: &DeckSpec, cards: &GameCards) -> DeckSpec {
        if self.left.load(Ordering::Relaxed) {
            return deck.clone();
        }
        let names = deck
            .keys()
            .filter_map(|k| {
                let name = cards
                    .id(k)
                    .and_then(|id| cards.face(id, 0))?
                    .name
                    .to_string();
                Some((k.clone(), name))
            })
            .collect();
        let _ = self.events.send(MatchEvent::Sideboard {
            deck: deck.clone(),
            names,
        });
        // The window always answers; if it has gone, keep the deck as it was.
        self.sideboards.recv().unwrap_or_else(|_| deck.clone())
    }

    fn has_left(&self) -> bool {
        self.left.load(Ordering::Relaxed)
    }
}

/// An opponent that does nothing at all — never plays a land, never blocks — so a deck can be
/// tested on its own: how it draws, how it curves, how fast it can win.
#[derive(Default)]
pub struct GoldfishSeat;

impl Decide for GoldfishSeat {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        mtg_policy::well_formed(choice, view)
    }
}

impl Seat for GoldfishSeat {}

/// The bot, for a match against the computer.
#[derive(Default)]
pub struct BotSeat {
    bot: Option<Bot>,
}

impl Decide for BotSeat {
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        match &self.bot {
            Some(bot) => bot.decide(choice, view),
            None => mtg_policy::well_formed(choice, view),
        }
    }
}

impl Seat for BotSeat {
    fn begin_match(&mut self, _settings: MatchSettings, cards: &GameCards) {
        self.bot = Some(Bot::new(cards, cards.ids()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn play_draw_waits_for_the_ui_and_releases_on_answer_leave_or_closed_channel() {
        for ending in 0..3 {
            let (decider, _questions, _answers) = crate::UiDecider::new();
            let (events, event_rx) = mpsc::channel();
            let (play_tx, play_rx) = mpsc::channel();
            let (_side_tx, side_rx) = mpsc::channel();
            let left = Arc::new(AtomicBool::new(false));
            let mut seat = UiSeat {
                inner: PolicyThenUi::new(mtg_policy::Policy::default(), decider),
                events,
                sideboards: side_rx,
                play_draws: play_rx,
                left: Arc::clone(&left),
            };
            let choice = PlayDrawChoice {
                number: 1,
                chooser: mtg_core::PlayerId(0),
                score: vec![0, 0],
            };
            let expected = choice.clone();
            let (result_tx, result_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                result_tx.send(seat.choose_play_first(&choice)).unwrap();
            });
            let MatchEvent::PlayDraw(actual) =
                event_rx.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!("prompt");
            };
            assert_eq!(actual, expected);
            assert!(matches!(
                result_rx.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            match ending {
                0 => play_tx.send(false).unwrap(),
                1 => left.store(true, Ordering::Relaxed),
                _ => drop(play_tx),
            }
            assert_eq!(
                result_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                ending != 0
            );
            worker.join().unwrap();
        }
    }
}
