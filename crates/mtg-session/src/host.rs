//! The authoritative side.

use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress, actions::Action, choice::Answer, layers::PrintedCards, view};
use mtg_net::{
    session::{GuestMessage, HostMessage},
    wire::{Channel, TypedChannel},
};

use crate::{BUDGET, Decide, Outcome, SessionError};

/// How many illegal answers in a row before the peer is treated as broken.
///
/// Not zero, because a legitimate client can race a stale choice; not unbounded, because a
/// modified client must not be able to keep the host busy forever.
const ILLEGAL_TOLERANCE: u32 = 3;

/// Runs the engine and serves a remote guest.
pub struct HostSession<C: Channel> {
    pub engine: Engine,
    channel: C,
    guest_seat: PlayerId,
}

impl<C: Channel> HostSession<C> {
    pub fn new(engine: Engine, channel: C, guest_seat: PlayerId) -> Self {
        Self {
            engine,
            channel,
            guest_seat,
        }
    }

    /// The view the guest is entitled to, right now.
    pub fn guest_view(&self) -> view::PlayerView {
        self.engine.view_for(self.guest_seat)
    }

    /// Play until the game ends or the connection does.
    ///
    /// `local` answers questions put to the host's own player — a UI, a policy, or a bot; they
    /// are the same shape, which is why the engine hands out choices rather than calling into
    /// anything.
    pub fn play(
        &mut self,
        cards: &dyn PrintedCards,
        local: &mut dyn Decide,
    ) -> Result<Outcome, SessionError> {
        let mut illegal_in_a_row = 0u32;

        for _ in 0..BUDGET {
            match self.engine.advance(cards) {
                Progress::Continue => {}

                Progress::GameOver { winners } => {
                    // Best effort: the game is over locally whether or not this arrives.
                    let _ = self.channel.send_msg(&HostMessage::GameOver {
                        winner: winners.first().map(|p| p.0),
                    });
                    return Ok(Outcome::Over { winners });
                }

                Progress::NeedsChoice(choice) => {
                    let answer = if choice.who == self.guest_seat {
                        match self.ask_guest(&choice)? {
                            Some(answer) => answer,
                            // The guest conceded or vanished.
                            None => return Ok(Outcome::Disconnected),
                        }
                    } else {
                        let view = self.engine.view_for(choice.who);
                        local.decide(&choice, &view)
                    };

                    match self.engine.answer(cards, choice.id, answer) {
                        Ok(()) => illegal_in_a_row = 0,
                        Err(e) => {
                            // The engine leaves the choice pending on rejection, so the next
                            // pass re-asks. That is the right behaviour for a raced answer and
                            // the wrong one for a broken client, hence the tolerance.
                            illegal_in_a_row += 1;
                            if illegal_in_a_row > ILLEGAL_TOLERANCE {
                                return Err(SessionError::Illegal(e));
                            }
                        }
                    }
                }
            }
        }

        Err(SessionError::Stalled)
    }

    /// Send the guest its view and the question, and wait for the matching answer.
    ///
    /// Returns `None` when the guest conceded, which is not an error.
    fn ask_guest(&mut self, choice: &mtg_engine::Choice) -> Result<Option<Answer>, SessionError> {
        // The view goes first, so the guest is looking at the position the question is about.
        let view = self.engine.view_for(self.guest_seat);
        self.channel.send_msg(&HostMessage::Snapshot {
            at_event: self.engine.log.len() as u64,
            view_bytes: mtg_net::wire::encode(&view)?,
        })?;
        self.channel.send_msg(&HostMessage::Ask {
            choice_id: choice.id,
            choice_bytes: mtg_net::wire::encode(choice)?,
        })?;

        // Ignore answers to questions that are no longer outstanding rather than failing: a
        // guest that resent one after a hiccup should not lose the game over it.
        for _ in 0..64 {
            match self.channel.recv_msg::<GuestMessage>()? {
                GuestMessage::Answer {
                    choice_id,
                    answer_bytes,
                } if choice_id == choice.id => {
                    return Ok(Some(mtg_net::wire::decode(&answer_bytes)?));
                }
                GuestMessage::Answer { .. } => continue,

                // Conceding is always legal, so it is translated into the engine's own action
                // rather than handled as a special case out of band.
                GuestMessage::Concede => {
                    return Ok(Some(Answer::Action(Action::Concede)));
                }

                GuestMessage::RequestTakeback { to_event, .. } => {
                    // Rewinding needs the host's player to agree, and there is no one to ask in
                    // an unattended session, so it is declined explicitly rather than ignored.
                    self.channel.send_msg(&HostMessage::TakebackDecision {
                        to_event,
                        granted: false,
                    })?;
                }

                GuestMessage::Resume { last_seen, .. } => {
                    // Resync is a snapshot: see the crate docs on why events are not streamed.
                    let _ = last_seen;
                    let view = self.engine.view_for(self.guest_seat);
                    self.channel.send_msg(&HostMessage::Snapshot {
                        at_event: self.engine.log.len() as u64,
                        view_bytes: mtg_net::wire::encode(&view)?,
                    })?;
                    self.channel.send_msg(&HostMessage::Ask {
                        choice_id: choice.id,
                        choice_bytes: mtg_net::wire::encode(choice)?,
                    })?;
                }

                other => {
                    return Err(SessionError::Unexpected(format!("{other:?}")));
                }
            }
        }

        Err(SessionError::Stalled)
    }
}

/// Authoritative play for a host and one to three remote seats. Each connection
/// receives its own projection; only the connection assigned to a choice may answer.
pub fn play_pod<C: Channel>(
    mut engine: Engine,
    cards: &dyn PrintedCards,
    channels: &mut [C],
    local: &mut dyn Decide,
) -> Result<Outcome, SessionError> {
    if !(1..=3).contains(&channels.len()) || engine.state.players.len() != channels.len() + 1 {
        return Err(SessionError::Unexpected("invalid pod seating".into()));
    }
    let mut illegal = 0;
    for _ in 0..BUDGET {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                local.observe(&engine.view_for(PlayerId(0)));
                for (i, channel) in channels.iter_mut().enumerate() {
                    channel.send_msg(&HostMessage::Snapshot {
                        at_event: engine.log.len() as u64,
                        view_bytes: mtg_net::wire::encode(
                            &engine.view_for(PlayerId((i + 1) as u8)),
                        )?,
                    })?;
                    channel.send_msg(&HostMessage::GameOver {
                        winner: winners.first().map(|p| p.0),
                    })?;
                }
                return Ok(Outcome::Over { winners });
            }
            Progress::NeedsChoice(choice) => {
                local.observe(&engine.view_for(PlayerId(0)));
                for (i, channel) in channels.iter_mut().enumerate() {
                    channel.send_msg(&HostMessage::Snapshot {
                        at_event: engine.log.len() as u64,
                        view_bytes: mtg_net::wire::encode(
                            &engine.view_for(PlayerId((i + 1) as u8)),
                        )?,
                    })?;
                }
                let answer = if choice.who == PlayerId(0) {
                    local.decide(&choice, &engine.view_for(PlayerId(0)))
                } else {
                    let channel = channels.get_mut(choice.who.0 as usize - 1).ok_or_else(|| {
                        SessionError::Unexpected("choice for an unseated player".into())
                    })?;
                    channel.send_msg(&HostMessage::Ask {
                        choice_id: choice.id,
                        choice_bytes: mtg_net::wire::encode(&choice)?,
                    })?;
                    let mut answer = None;
                    for _ in 0..64 {
                        match channel.recv_msg::<GuestMessage>()? {
                            GuestMessage::Answer {
                                choice_id,
                                answer_bytes,
                            } if choice_id == choice.id => {
                                answer = Some(mtg_net::wire::decode(&answer_bytes)?);
                                break;
                            }
                            GuestMessage::Answer { .. } => {}
                            GuestMessage::Concede => {
                                answer = Some(Answer::Action(Action::Concede));
                                break;
                            }
                            GuestMessage::RequestTakeback { to_event, .. } => {
                                channel.send_msg(&HostMessage::TakebackDecision {
                                    to_event,
                                    granted: false,
                                })?;
                            }
                            GuestMessage::Resume { .. } => {
                                channel.send_msg(&HostMessage::Snapshot {
                                    at_event: engine.log.len() as u64,
                                    view_bytes: mtg_net::wire::encode(
                                        &engine.view_for(choice.who),
                                    )?,
                                })?;
                                channel.send_msg(&HostMessage::Ask {
                                    choice_id: choice.id,
                                    choice_bytes: mtg_net::wire::encode(&choice)?,
                                })?;
                            }
                            other => return Err(SessionError::Unexpected(format!("{other:?}"))),
                        }
                    }
                    answer.ok_or(SessionError::Stalled)?
                };
                if let Err(error) = engine.answer(cards, choice.id, answer) {
                    illegal += 1;
                    if illegal > ILLEGAL_TOLERANCE {
                        return Err(SessionError::Illegal(error));
                    }
                } else {
                    illegal = 0;
                }
            }
        }
    }
    Err(SessionError::Stalled)
}
