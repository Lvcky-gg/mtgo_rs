//! The side that answers.
//!
//! A guest holds no game state of its own — only the latest redacted view. It cannot assert a
//! change; it can only answer a question it was asked. That is what makes a modified guest
//! client harmless: there is no message by which it could do anything the rules do not already
//! permit.

use mtg_engine::{Choice, PlayerView};
use mtg_net::{
    session::{GuestMessage, HostMessage},
    wire::{Channel, TypedChannel},
};

use crate::{BUDGET, Decide, Outcome, SessionError};

/// Answers a remote host's questions.
pub struct GuestSession<C: Channel> {
    channel: C,
    view: Option<PlayerView>,
    /// The last question, kept so a client can redraw while it is outstanding.
    pending: Option<Choice>,
}

impl<C: Channel> GuestSession<C> {
    pub fn new(channel: C) -> Self {
        Self {
            channel,
            view: None,
            pending: None,
        }
    }

    /// The most recent view of the game, if one has arrived.
    pub fn view(&self) -> Option<&PlayerView> {
        self.view.as_ref()
    }

    /// The outstanding question, if any.
    pub fn pending(&self) -> Option<&Choice> {
        self.pending.as_ref()
    }

    /// Ask to rewind to a point in the game. The host's player decides.
    pub fn request_takeback(&mut self, to_event: u64, reason: &str) -> Result<(), SessionError> {
        self.channel.send_msg(&GuestMessage::RequestTakeback {
            to_event,
            reason: reason.into(),
        })?;
        Ok(())
    }

    /// Play until the game ends or the connection does.
    pub fn play(&mut self, decide: &mut dyn Decide) -> Result<Outcome, SessionError> {
        for _ in 0..BUDGET {
            let msg = match self.channel.recv_msg::<HostMessage>() {
                Ok(m) => m,
                // A closed connection mid-game is a disconnect, not a crash. The host keeps the
                // log, so this is resumable in principle.
                Err(mtg_net::WireError::Closed) => return Ok(Outcome::Disconnected),
                Err(e) => return Err(e.into()),
            };

            match msg {
                HostMessage::Snapshot { view_bytes, .. } => {
                    self.view = Some(mtg_net::wire::decode(&view_bytes)?);
                }

                HostMessage::Ask {
                    choice_id,
                    choice_bytes,
                } => {
                    let choice: Choice = mtg_net::wire::decode(&choice_bytes)?;

                    // The host always sends the view before the question. Without one there is
                    // nothing to decide against, and guessing would be worse than saying so.
                    let Some(view) = self.view.as_ref() else {
                        return Err(SessionError::Unexpected(
                            "a question arrived before any view".into(),
                        ));
                    };

                    self.pending = Some(choice.clone());
                    let answer = decide.decide(&choice, view);
                    self.pending = None;

                    self.channel.send_msg(&GuestMessage::Answer {
                        choice_id,
                        answer_bytes: mtg_net::wire::encode(&answer)?,
                    })?;
                }

                HostMessage::GameOver { winner } => {
                    return Ok(Outcome::Over {
                        winners: winner.map(mtg_core::PlayerId).into_iter().collect(),
                    });
                }

                HostMessage::TakebackDecision { .. } => {
                    // Informational; a client would surface it. Nothing to answer.
                }

                // Events are not streamed yet — see the crate docs on redaction.
                HostMessage::Events { .. } => {}

                other => {
                    return Err(SessionError::Unexpected(format!("{other:?}")));
                }
            }
        }

        Err(SessionError::Stalled)
    }
}
