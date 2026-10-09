//! Running a match — one game or best of three — locally or over a connection.
//!
//! The same loop serves every arrangement. A [`Seat`] is one player's side of the match: it
//! answers questions like any [`Decide`], and additionally hears when games start and end and is
//! asked for its deck between games. A person at the window, the bot, and the far end of a
//! connection are all seats, which is why a match against the bot and a match against a friend
//! run the same rules for who plays first, scoring and sideboarding.
//!
//! # Over a connection
//!
//! After the invite handshake the two ends exchange lobby messages, in a fixed order, on the
//! same channel the games then use:
//!
//! ```text
//!   guest ── Deck ─────────────────────────────►  host   (by CardKey: meaningful on both ends)
//!   guest ◄──────────── Welcome{settings,cards} ── host   (or Rejected, with the reason)
//!         ◄──────────── PlayDraw ────────────────        two players; chooser answers before hands
//!   guest ── PlayDraw{number,play_first} ────────►        only if the guest chooses
//!         ◄──────────── Game{number,first,score} ─        then one game, as `host`/`guest` run it
//!         ◄──────────── Sideboard ────────────────        between games, best-of-three only
//!   guest ── Sideboarded ──────────────────────►
//!         ◄──────────── MatchOver ────────────────
//! ```
//!
//! The host builds the match's card table from both decks and sends it whole, so the guest can
//! show any card without having imported it. Seat 0 is always the host.

use mtg_core::PlayerId;
use mtg_engine::{Engine, Progress};
use mtg_net::{
    fairness::Seed,
    wire::{Channel, TypedChannel},
};
use serde::{Deserialize, Serialize};

use crate::{
    Decide, GuestSession, Outcome, SessionError,
    game::{
        CardSource, DeckSpec, GameCards, MatchSettings, SEATS, deck_problems, new_game_with_players,
    },
};

/// A game about to begin, from one seat's point of view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GameStart {
    /// 1-based.
    pub number: u8,
    pub you: PlayerId,
    pub first: PlayerId,
    /// Games won so far, by seat.
    pub score: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameEnd {
    pub number: u8,
    pub winner: Option<PlayerId>,
    pub score: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MatchEnd {
    pub score: Vec<u8>,
    pub winner: Option<PlayerId>,
    /// Why it ended early, if it did.
    pub reason: Option<String>,
}

/// A two-player decision made before opening hands are dealt.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PlayDrawChoice {
    pub number: u8,
    pub chooser: PlayerId,
    pub score: Vec<u8>,
}

/// Bound repeated drawn games without mistaking that operational limit for a match result.
pub const MAX_MATCH_GAMES: u8 = 64;

/// One player's side of a match. See the module docs.
pub trait Seat: Decide {
    fn begin_match(&mut self, _settings: MatchSettings, _cards: &GameCards) {}
    fn begin_game(&mut self, _start: &GameStart) {}
    fn end_game(&mut self, _end: &GameEnd) {}
    /// True to play first; false to let the opponent start. Bots prefer to play.
    fn choose_play_first(&mut self, _choice: &PlayDrawChoice) -> bool {
        true
    }
    /// Between games of a best-of-three: the deck for the next game. Only cards moved between
    /// main deck and sideboard are accepted; anything else keeps the deck as it was.
    fn sideboard(&mut self, deck: &DeckSpec, _cards: &GameCards) -> DeckSpec {
        deck.clone()
    }
    /// Whether the player has left, so the match should stop after this game.
    fn has_left(&self) -> bool {
        false
    }
}

/// The running score, and who plays first next.
struct Score {
    settings: MatchSettings,
    wins: Vec<u8>,
    games: u8,
    next_first: PlayerId,
    chooser: PlayerId,
}

impl Score {
    /// The agreed seed selects the first chooser; that player may play or draw.
    fn new(settings: MatchSettings, seed: &Seed) -> Self {
        let next_first = SEATS[(seed.0[0] & 1) as usize];
        Self {
            settings,
            wins: vec![0, 0],
            games: 0,
            next_first,
            chooser: next_first,
        }
    }

    fn start(&mut self) -> (u8, PlayerId) {
        self.games += 1;
        (self.games, self.next_first)
    }

    /// CR 103.1: the loser chooses next; a draw retains the previous chooser.
    fn finish(&mut self, winner: Option<PlayerId>) {
        if let Some(w) = winner {
            self.wins[w.0 as usize] += 1;
            if self.wins.len() == 2 {
                self.chooser = if w == SEATS[0] { SEATS[1] } else { SEATS[0] };
            }
        }
    }

    fn play_draw_choice(&self) -> PlayDrawChoice {
        PlayDrawChoice {
            number: self.games + 1,
            chooser: self.chooser,
            score: self.wins.clone(),
        }
    }

    fn choose_first(&mut self, play_first: bool) {
        self.next_first = if play_first {
            self.chooser
        } else {
            SEATS[1 - self.chooser.0 as usize]
        };
    }

    fn match_winner(&self) -> Option<PlayerId> {
        let need = self.settings.wins_needed();
        self.wins
            .iter()
            .position(|wins| *wins >= need)
            .map(|i| PlayerId(i as u8))
    }

    fn over(&self) -> bool {
        self.match_winner().is_some()
            || (self.settings.best_of == 1 && self.games >= 1)
            || self.games >= MAX_MATCH_GAMES
    }

    fn end(&self, reason: Option<String>) -> MatchEnd {
        MatchEnd {
            score: self.wins.clone(),
            winner: self.match_winner(),
            reason: reason.or_else(|| {
                (self.games >= MAX_MATCH_GAMES && self.match_winner().is_none()).then(|| {
                    format!(
                        "match stopped at the {MAX_MATCH_GAMES}-game safety limit without a winner"
                    )
                })
            }),
        }
    }
}

/// Constructed size limits still apply after redistributing cards between games.
pub fn sideboard_sizes_valid(deck: &DeckSpec) -> bool {
    deck.main_count() >= 60 && deck.side.iter().map(|(_, n)| u32::from(*n)).sum::<u32>() <= 15
}

/// A seat's sideboarding, kept only if it moved cards and retained valid sizes.
fn sideboarded(seat: &mut dyn Seat, deck: &DeckSpec, cards: &GameCards) -> DeckSpec {
    let next = seat.sideboard(deck, cards);
    if next.same_cards_as(deck) && sideboard_sizes_valid(&next) {
        next
    } else {
        deck.clone()
    }
}

/// A whole match on this machine: both seats local, typically a person and the bot.
pub fn play_local_match(
    settings: MatchSettings,
    cards: &GameCards,
    mut decks: [DeckSpec; 2],
    seed: Seed,
    seats: [&mut dyn Seat; 2],
) -> MatchEnd {
    play_local_match_with_players(
        settings,
        cards,
        &mut decks,
        seed,
        &mut seats.into_iter().collect::<Vec<_>>(),
    )
}

/// Local Commander pods share the same rules loop as two-player matches.
pub fn play_local_match_with_players(
    settings: MatchSettings,
    cards: &GameCards,
    decks: &mut [DeckSpec],
    seed: Seed,
    seats: &mut [&mut dyn Seat],
) -> MatchEnd {
    assert!((2..=4).contains(&seats.len()) && seats.len() == decks.len());
    assert!(seats.len() == 2 || settings.format == crate::game::Format::Commander);
    assert!(seats.len() == 2 || settings.best_of == 1);
    let mut score = Score::new(settings, &seed);
    score.wins = vec![0; seats.len()];
    score.next_first = PlayerId((seed.0[0] as usize % seats.len()) as u8);
    score.chooser = score.next_first;
    for seat in seats.iter_mut() {
        seat.begin_match(settings, cards);
    }

    while !score.over() {
        if seats.iter().any(|seat| seat.has_left()) {
            return score.end(Some("a player left before the next game".into()));
        }
        if seats.len() == 2 {
            let choice = score.play_draw_choice();
            let play_first = seats[choice.chooser.0 as usize].choose_play_first(&choice);
            if seats.iter().any(|seat| seat.has_left()) {
                return score.end(Some("a player left before the next game".into()));
            }
            score.choose_first(play_first);
        }
        let (number, first) = score.start();
        for (i, seat) in seats.iter_mut().enumerate() {
            seat.begin_game(&GameStart {
                number,
                you: PlayerId(i as u8),
                first,
                score: score.wins.clone(),
            });
        }

        let state = new_game_with_players(
            settings.format,
            cards,
            &decks.iter().collect::<Vec<_>>(),
            &seed,
            number,
            first,
        );
        let winner = match play_local_game(Engine::new(state), cards, seats) {
            Ok(winner) => winner,
            Err(error) => return score.end(Some(error.to_string())),
        };
        score.finish(winner);
        let end = GameEnd {
            number,
            winner,
            score: score.wins.clone(),
        };
        for seat in seats.iter_mut() {
            seat.end_game(&end);
        }

        if seats.iter().any(|s| s.has_left()) {
            return score.end(Some("a player left".into()));
        }
        if !score.over() && settings.format.has_sideboard() {
            for (i, seat) in seats.iter_mut().enumerate() {
                decks[i] = sideboarded(&mut **seat, &decks[i], cards);
            }
        }
    }
    score.end(None)
}

/// A hard cap on engine steps per game, so a rules bug ends a game rather than spinning.
const GAME_BUDGET: usize = 2_000_000;

/// How many rejected answers to one question before a seat's answer is replaced with a
/// well-formed one. A person gets a few tries; a broken heuristic cannot stall the game.
const REJECTIONS: u32 = 3;

fn play_local_game(
    mut engine: Engine,
    cards: &GameCards,
    seats: &mut [&mut dyn Seat],
) -> Result<Option<PlayerId>, SessionError> {
    let mut rejected = 0;
    for _ in 0..GAME_BUDGET {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => {
                for (i, seat) in seats.iter_mut().enumerate() {
                    seat.observe(&engine.view_for(PlayerId(i as u8)));
                }
                return Ok(winners.first().copied());
            }
            Progress::NeedsChoice(choice) => {
                for (i, seat) in seats.iter_mut().enumerate() {
                    seat.observe(&engine.view_for(PlayerId(i as u8)));
                }
                let view = engine.view_for(choice.who);
                let seat = &mut seats[choice.who.0 as usize];
                let answer = if rejected >= REJECTIONS {
                    mtg_policy::well_formed(&choice, &view)
                } else {
                    seat.decide(&choice, &view)
                };
                if engine.answer(cards, choice.id, answer).is_err() {
                    rejected += 1;
                } else {
                    rejected = 0;
                }
            }
        }
    }
    Err(SessionError::Stalled)
}

// ---- over a connection ---------------------------------------------------

/// Lobby messages from host to guest. See the module docs for the order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToGuest {
    Welcome {
        settings: MatchSettings,
        cards: Box<GameCards>,
    },
    Rejected {
        reason: String,
    },
    Game(GameStart),
    Sideboard,
    MatchOver {
        score: Vec<u8>,
        reason: Option<String>,
    },
    PlayDraw(PlayDrawChoice),
}

/// Lobby messages from guest to host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToHost {
    Deck(DeckSpec),
    Sideboarded(DeckSpec),
    PlayDraw { number: u8, play_first: bool },
}

/// Host a match: receive the guest's deck, build and send the card table, then play the games.
/// `seat` is the host's own player, in seat 0.
pub fn host_match<C: Channel>(
    channel: &mut C,
    settings: MatchSettings,
    host_deck: DeckSpec,
    source: &dyn CardSource,
    seed: Seed,
    seat: &mut dyn Seat,
) -> Result<MatchEnd, SessionError> {
    host_match_with_players(&mut [channel], settings, host_deck, source, seed, seat)
}

/// Host a two-to-four-seat pod. Extra seats are supported for Commander best-of-one.
pub fn host_match_with_players<C: Channel>(
    channels: &mut [C],
    settings: MatchSettings,
    host_deck: DeckSpec,
    source: &dyn CardSource,
    seed: Seed,
    seat: &mut dyn Seat,
) -> Result<MatchEnd, SessionError> {
    if !(1..=3).contains(&channels.len())
        || ![1, 3].contains(&settings.best_of)
        || (channels.len() > 1
            && (settings.format != crate::game::Format::Commander || settings.best_of != 1))
    {
        return Err(SessionError::Unexpected(
            "use two players, or a two-to-four-player Commander game".into(),
        ));
    }
    let mut decks = vec![host_deck];
    for channel in channels.iter_mut() {
        let ToHost::Deck(deck) = channel.recv_msg::<ToHost>()? else {
            return Err(SessionError::Unexpected("expected the guest's deck".into()));
        };
        decks.push(deck);
    }
    let prepared = (|| {
        let cards = GameCards::build(decks.iter(), source).map_err(|missing| format!(
            "the host's card database is missing or cannot support {} cards; check deck support before playing", missing.len()))?;
        for (i, deck) in decks.iter().enumerate() {
            let problems = deck_problems(settings.format, deck, &cards);
            if !problems.is_empty() {
                return Err(format!(
                    "seat {} deck is not legal for {}: {}",
                    i + 1,
                    settings.format.name(),
                    problems.join("; ")
                ));
            }
        }
        Ok::<_, String>(cards)
    })();
    let cards = match prepared {
        Ok(cards) => cards,
        Err(reason) => {
            for channel in channels.iter_mut() {
                let _ = channel.send_msg(&ToGuest::Rejected {
                    reason: reason.clone(),
                });
            }
            return Err(SessionError::Unexpected(reason));
        }
    };
    for channel in channels.iter_mut() {
        channel.send_msg(&ToGuest::Welcome {
            settings,
            cards: Box::new(cards.clone()),
        })?;
    }
    seat.begin_match(settings, &cards);
    let mut score = Score::new(settings, &seed);
    score.wins = vec![0; decks.len()];
    score.next_first = PlayerId((seed.0[0] as usize % decks.len()) as u8);
    score.chooser = score.next_first;
    while !score.over() {
        if seat.has_left() {
            break;
        }
        if decks.len() == 2 {
            let choice = score.play_draw_choice();
            channels[0].send_msg(&ToGuest::PlayDraw(choice.clone()))?;
            let play_first = if choice.chooser == PlayerId(0) {
                seat.choose_play_first(&choice)
            } else {
                let ToHost::PlayDraw { number, play_first } = channels[0].recv_msg::<ToHost>()?
                else {
                    return Err(SessionError::Unexpected(
                        "expected a play/draw decision".into(),
                    ));
                };
                if number != choice.number {
                    return Err(SessionError::Unexpected(
                        "wrong game in play/draw decision".into(),
                    ));
                }
                play_first
            };
            if seat.has_left() {
                break;
            }
            score.choose_first(play_first);
        }
        let (number, first) = score.start();
        for (i, channel) in channels.iter_mut().enumerate() {
            channel.send_msg(&ToGuest::Game(GameStart {
                number,
                you: PlayerId((i + 1) as u8),
                first,
                score: score.wins.clone(),
            }))?;
        }
        seat.begin_game(&GameStart {
            number,
            you: PlayerId(0),
            first,
            score: score.wins.clone(),
        });
        let state = new_game_with_players(
            settings.format,
            &cards,
            &decks.iter().collect::<Vec<_>>(),
            &seed,
            number,
            first,
        );
        let outcome = crate::host::play_pod(Engine::new(state), &cards, channels, seat);
        let winner = match outcome {
            Ok(Outcome::Over { winners }) => winners.first().copied(),
            Ok(Outcome::Disconnected)
            | Err(SessionError::Wire(mtg_net::WireError::Closed | mtg_net::WireError::Io(_))) => {
                let reason = Some("a player disconnected; this match cannot resume".into());
                for channel in channels.iter_mut() {
                    let _ =
                        channel.send_msg(&mtg_net::session::HostMessage::GameOver { winner: None });
                    let _ = channel.send_msg(&ToGuest::MatchOver {
                        score: score.wins.clone(),
                        reason: reason.clone(),
                    });
                }
                return Ok(score.end(reason));
            }
            Err(error) => return Err(error),
        };
        score.finish(winner);
        seat.end_game(&GameEnd {
            number,
            winner,
            score: score.wins.clone(),
        });
        if seat.has_left() {
            break;
        }
        if !score.over() && settings.format.has_sideboard() {
            for channel in channels.iter_mut() {
                channel.send_msg(&ToGuest::Sideboard)?;
            }
            decks[0] = sideboarded(seat, &decks[0], &cards);
            for (i, channel) in channels.iter_mut().enumerate() {
                if let ToHost::Sideboarded(next) = channel.recv_msg::<ToHost>()?
                    && next.same_cards_as(&decks[i + 1])
                    && sideboard_sizes_valid(&next)
                {
                    decks[i + 1] = next;
                }
            }
        }
    }
    let end = score.end(seat.has_left().then(|| "the host left".to_string()));
    for channel in channels.iter_mut() {
        channel.send_msg(&ToGuest::MatchOver {
            score: score.wins.clone(),
            reason: end.reason.clone(),
        })?;
    }
    Ok(end)
}

/// Join a match: send the deck, then play whatever games the host starts. `seat` is the
/// guest's own player, in seat 1.
pub fn guest_match<C: Channel>(
    channel: &mut C,
    deck: DeckSpec,
    seat: &mut dyn Seat,
) -> Result<MatchEnd, SessionError> {
    channel.send_msg(&ToHost::Deck(deck.clone()))?;
    let (settings, cards) = match channel.recv_msg::<ToGuest>()? {
        ToGuest::Welcome { settings, cards } => {
            if ![1, 3].contains(&settings.best_of) {
                return Err(SessionError::Unexpected("invalid match length".into()));
            }
            seat.begin_match(settings, &cards);
            (settings, cards)
        }
        ToGuest::Rejected { reason } => return Err(SessionError::Unexpected(reason)),
        other => return Err(SessionError::Unexpected(format!("{other:?}"))),
    };

    let mut deck = deck;
    let mut last_score: Option<Vec<u8>> = None;
    let mut assigned_seat = None;
    let mut games_played = 0u8;
    let mut awaiting_sideboard = false;
    let mut expected_chooser = None;
    let mut pending_play_draw: Option<(PlayDrawChoice, Option<PlayerId>)> = None;
    loop {
        match channel.recv_msg::<ToGuest>()? {
            ToGuest::PlayDraw(choice) => {
                if choice.score.len() != 2
                    || choice.chooser.0 > 1
                    || choice.number != games_played + 1
                    || games_played >= MAX_MATCH_GAMES
                    || (settings.best_of == 1 && games_played >= 1)
                    || awaiting_sideboard
                    || pending_play_draw.is_some()
                    || assigned_seat.is_some_and(|seat| seat != PlayerId(1))
                    || expected_chooser.is_some_and(|chooser| chooser != choice.chooser)
                    || last_score.as_ref().map_or_else(
                        || choice.score != [0, 0],
                        |score| {
                            score != &choice.score
                                || score.iter().any(|wins| *wins >= settings.wins_needed())
                        },
                    )
                {
                    return Err(SessionError::Unexpected("invalid play/draw request".into()));
                }
                assigned_seat = Some(PlayerId(1));
                expected_chooser = Some(choice.chooser);
                let first = if choice.chooser == PlayerId(1) {
                    let play_first = seat.choose_play_first(&choice);
                    channel.send_msg(&ToHost::PlayDraw {
                        number: choice.number,
                        play_first,
                    })?;
                    Some(if play_first { PlayerId(1) } else { PlayerId(0) })
                } else {
                    None
                };
                pending_play_draw = Some((choice, first));
            }
            ToGuest::Game(start) => {
                if !(2..=4).contains(&start.score.len())
                    || start.you == PlayerId(0)
                    || start.you.0 as usize >= start.score.len()
                    || start.first.0 as usize >= start.score.len()
                    || (start.score.len() > 2
                        && (settings.format != crate::game::Format::Commander
                            || settings.best_of != 1))
                    || start.number != games_played + 1
                    || games_played >= MAX_MATCH_GAMES
                    || (settings.best_of == 1 && games_played >= 1)
                    || awaiting_sideboard
                    || if start.score.len() == 2 {
                        pending_play_draw.as_ref().is_none_or(|(choice, first)| {
                            choice.number != start.number
                                || choice.score != start.score
                                || first.is_some_and(|first| first != start.first)
                        })
                    } else {
                        pending_play_draw.is_some()
                    }
                    || assigned_seat.is_some_and(|seat| seat != start.you)
                    || last_score.as_ref().map_or_else(
                        || start.score.iter().any(|wins| *wins != 0),
                        |score| {
                            score != &start.score
                                || score.iter().any(|wins| *wins >= settings.wins_needed())
                        },
                    )
                {
                    return Err(SessionError::Unexpected("invalid game seating".into()));
                }
                pending_play_draw = None;
                let mut score = start.score.clone();
                assigned_seat = Some(start.you);
                games_played += 1;
                seat.begin_game(&start);
                let outcome = GuestSession::for_seat(&mut *channel, start.you).play(seat)?;
                let winner = match outcome {
                    Outcome::Over { winners } => winners.first().copied(),
                    Outcome::Disconnected => {
                        let reason = Some("the host disconnected".to_string());
                        return Ok(MatchEnd {
                            score,
                            winner: None,
                            reason,
                        });
                    }
                };
                if let Some(w) = winner {
                    let wins = score
                        .get_mut(w.0 as usize)
                        .ok_or_else(|| SessionError::Unexpected("winner is not seated".into()))?;
                    *wins = wins
                        .checked_add(1)
                        .ok_or_else(|| SessionError::Unexpected("invalid match score".into()))?;
                    if score.len() == 2 {
                        expected_chooser = Some(PlayerId(1 - w.0));
                    }
                }
                seat.end_game(&GameEnd {
                    number: start.number,
                    winner,
                    score: score.clone(),
                });
                awaiting_sideboard = settings.format.has_sideboard()
                    && settings.best_of == 3
                    && games_played < MAX_MATCH_GAMES
                    && !score.iter().any(|wins| *wins >= settings.wins_needed());
                last_score = Some(score);
            }
            ToGuest::Sideboard => {
                if !awaiting_sideboard {
                    return Err(SessionError::Unexpected("unexpected sideboarding".into()));
                }
                awaiting_sideboard = false;
                deck = sideboarded(seat, &deck, &cards);
                channel.send_msg(&ToHost::Sideboarded(deck.clone()))?;
            }
            ToGuest::MatchOver { score, reason } => {
                let early_abort =
                    games_played == 0 && reason.is_some() && score.iter().all(|wins| *wins == 0);
                if !(2..=4).contains(&score.len())
                    || (early_abort
                        && pending_play_draw
                            .as_ref()
                            .is_some_and(|(choice, _)| choice.score != score))
                    || (!early_abort && last_score.as_ref() != Some(&score))
                {
                    return Err(SessionError::Unexpected("invalid final seating".into()));
                }
                let winner = score
                    .iter()
                    .position(|wins| *wins >= settings.wins_needed())
                    .map(|seat| PlayerId(seat as u8));
                if reason.is_none() && winner.is_none() && settings.best_of != 1 {
                    return Err(SessionError::Unexpected(
                        "match ended before either player won two games".into(),
                    ));
                }
                return Ok(MatchEnd {
                    score,
                    winner,
                    reason,
                });
            }
            other => return Err(SessionError::Unexpected(format!("{other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{CardKey, Format, SourcedCard};
    use mtg_core::{CardType, ManaCost, Subtype, Supertype};
    use mtg_engine::{Choice, PlayerView, choice::Answer};
    use mtg_ir::CardFace;
    use mtg_net::wire::MemoryChannel;

    /// Two invented cards: a free land and a free 1/1.
    struct Invented;
    impl CardSource for Invented {
        fn card(&self, key: &CardKey) -> Option<SourcedCard> {
            let (name, types) = match key {
                CardKey::Demo(0) => ("Bare Field", vec![CardType::Land]),
                CardKey::Demo(1) => ("Small Soldier", vec![CardType::Creature]),
                CardKey::Demo(2) => ("Shared Defeat", vec![CardType::Instant]),
                _ => return None,
            };
            let face = CardFace {
                name: name.into(),
                mana_cost: ManaCost::FREE,
                card_types: types,
                subtypes: Vec::new(),
                supertypes: vec![Supertype::Basic],
                power: Some(1),
                toughness: Some(1),
                loyalty: None,
                abilities: if *key == CardKey::Demo(2) {
                    vec![mtg_ir::Ability {
                        id: mtg_core::AbilityId(0),
                        kind: mtg_ir::AbilityKind::SpellEffect(mtg_ir::Effect::LoseLife {
                            who: mtg_ir::Selector::EachPlayer,
                            amount: mtg_ir::Value::Fixed(20),
                        }),
                        targets: Vec::new(),
                        source_text: None,
                    }]
                } else {
                    Vec::new()
                },
                oracle_text: None,
                colors: None,
            };
            Some(SourcedCard {
                layout: mtg_ir::Layout::Normal,
                faces: vec![face],
                color_identity: String::new(),
            })
        }
        fn subtype_name(&self, _: Subtype) -> Option<String> {
            None
        }
    }

    fn deck() -> DeckSpec {
        DeckSpec {
            name: "tiny".into(),
            main: vec![(CardKey::Demo(0), 30), (CardKey::Demo(1), 30)],
            ..Default::default()
        }
    }

    #[test]
    fn sideboarding_preserves_size_limits_as_well_as_cards() {
        struct Redistribute(DeckSpec);
        impl Decide for Redistribute {
            fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
                mtg_policy::well_formed(choice, view)
            }
        }
        impl Seat for Redistribute {
            fn sideboard(&mut self, _: &DeckSpec, _: &GameCards) -> DeckSpec {
                self.0.clone()
            }
        }
        let mut original = deck();
        original.main[0].1 = 45; // 75 cards available for redistribution
        let cards = GameCards::build([&original], &Invented).unwrap();
        for (main_land, side_land, accepted) in [(30, 15, true), (29, 16, false), (14, 31, false)] {
            let mut next = original.clone();
            next.main[0].1 = main_land;
            next.side = vec![(CardKey::Demo(0), side_land)];
            assert!(next.same_cards_as(&original));
            let actual = sideboarded(&mut Redistribute(next.clone()), &original, &cards);
            assert_eq!(actual, if accepted { next } else { original.clone() });
        }
        let original = deck();
        let mut too_small = original.clone();
        too_small.main[0].1 = 29;
        too_small.side = vec![(CardKey::Demo(0), 1)];
        assert!(too_small.same_cards_as(&original));
        assert_eq!(
            sideboarded(&mut Redistribute(too_small), &original, &cards),
            original
        );
    }

    /// A seat that plays by the bot's rules and records what it heard.
    #[derive(Default)]
    struct Recorder {
        starts: Vec<GameStart>,
        ends: Vec<GameEnd>,
        sideboarded: u32,
        leave_after_one: bool,
        draw_first: bool,
        choose_draw: bool,
        choices: Vec<PlayDrawChoice>,
    }
    impl Decide for Recorder {
        fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
            let number = self.starts.last().map_or(0, |start| start.number);
            if let mtg_engine::ChoiceKind::Priority { ref legal } = choice.kind {
                if self.draw_first && number == 1 {
                    // A free invented spell makes both players lose simultaneously. Pass
                    // while it is on the stack so the fixture actually reaches a draw.
                    if view.stack.is_empty()
                        && let Some(action) = legal.actions.iter().find(|action| {
                            matches!(action, mtg_engine::actions::Action::Cast { .. })
                        })
                    {
                        return Answer::Action(action.clone());
                    }
                    return Answer::Pass;
                }
                let first_decisive = if self.draw_first { 2 } else { 1 };
                if (view.viewer == SEATS[1] && number == first_decisive)
                    || (view.viewer == SEATS[0] && number > first_decisive)
                {
                    return Answer::Action(mtg_engine::actions::Action::Concede);
                }
            }
            mtg_policy::well_formed(choice, view)
        }
    }
    impl Seat for Recorder {
        fn choose_play_first(&mut self, choice: &PlayDrawChoice) -> bool {
            assert_eq!(self.starts.len() + 1, choice.number as usize);
            assert_eq!(self.sideboarded + 1, u32::from(choice.number));
            self.choices.push(choice.clone());
            !self.choose_draw
        }
        fn has_left(&self) -> bool {
            self.leave_after_one && !self.ends.is_empty()
        }
        fn begin_game(&mut self, start: &GameStart) {
            self.starts.push(start.clone());
        }
        fn end_game(&mut self, end: &GameEnd) {
            self.ends.push(end.clone());
        }
        fn sideboard(&mut self, deck: &DeckSpec, _: &GameCards) -> DeckSpec {
            self.sideboarded += 1;
            deck.clone()
        }
    }

    fn settings(best_of: u8) -> MatchSettings {
        MatchSettings {
            format: Format::Constructed,
            best_of,
        }
    }

    #[test]
    fn a_best_of_three_ends_when_someone_has_two_wins() {
        let cards = GameCards::build([&deck()], &Invented).unwrap();
        let (mut a, mut b) = (Recorder::default(), Recorder::default());
        // Game 1: seat 1 concedes. Games 2+: seat 0 concedes. So 1–2.
        let end = play_local_match(
            settings(3),
            &cards,
            [deck(), deck()],
            Seed([0; 32]),
            [&mut a, &mut b],
        );
        assert_eq!(end.score, [1, 2]);
        assert_eq!(end.winner, Some(SEATS[1]));
        assert_eq!(a.starts.len(), 3);
        assert_eq!(
            a.sideboarded, 2,
            "asked to sideboard before games two and three"
        );
    }

    fn draw_deck() -> DeckSpec {
        // Invented Basic fixture cards bypass the copy limit to guarantee a castable
        // simultaneous-loss spell in the opening hand. This is not a legal real deck.
        DeckSpec {
            main: vec![(CardKey::Demo(2), 60)],
            ..Default::default()
        }
    }

    fn assert_draw_match(end: &MatchEnd, seat: &Recorder) {
        assert_eq!(end.score, [1, 2]);
        assert_eq!(end.winner, Some(SEATS[1]));
        assert_eq!(end.reason, None);
        assert_eq!(
            seat.starts.len(),
            4,
            "a draw does not consume a decisive game"
        );
        assert_eq!(seat.sideboarded, 3);
        assert_eq!(seat.ends[0].winner, None);
        assert_eq!(seat.ends[0].score, [0, 0]);
        assert_eq!(seat.starts[1].first, seat.starts[0].first);
        assert_eq!(seat.starts[3].score, [1, 1]);
    }

    #[test]
    fn a_local_draw_continues_through_game_four_and_sideboarding() {
        let deck = draw_deck();
        let cards = GameCards::build([&deck], &Invented).unwrap();
        let (mut a, mut b) = (
            Recorder {
                draw_first: true,
                ..Default::default()
            },
            Recorder {
                draw_first: true,
                ..Default::default()
            },
        );
        let end = play_local_match(
            settings(3),
            &cards,
            [deck.clone(), deck],
            Seed([0; 32]),
            [&mut a, &mut b],
        );
        assert_draw_match(&end, &a);
        assert_draw_match(&end, &b);
    }

    #[test]
    fn a_hosted_draw_keeps_both_clients_in_sync_through_game_four() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            let mut seat = Recorder {
                draw_first: true,
                ..Default::default()
            };
            let end = guest_match(&mut guest_end, draw_deck(), &mut seat).unwrap();
            (end, seat)
        });
        let mut seat = Recorder {
            draw_first: true,
            ..Default::default()
        };
        let end = host_match(
            &mut host_end,
            settings(3),
            draw_deck(),
            &Invented,
            Seed([0; 32]),
            &mut seat,
        )
        .unwrap();
        let (remote, remote_seat) = guest.join().unwrap();
        assert_eq!(end, remote);
        assert_draw_match(&end, &seat);
        assert_draw_match(&remote, &remote_seat);
    }

    #[test]
    fn drawing_first_retains_the_chooser_after_a_draw_and_then_passes_to_the_loser() {
        for seed in [0, 1] {
            let deck = draw_deck();
            let cards = GameCards::build([&deck], &Invented).unwrap();
            let (mut a, mut b) = (
                Recorder {
                    draw_first: true,
                    choose_draw: true,
                    ..Default::default()
                },
                Recorder {
                    draw_first: true,
                    choose_draw: true,
                    ..Default::default()
                },
            );
            let end = play_local_match(
                settings(3),
                &cards,
                [deck.clone(), deck],
                Seed([seed; 32]),
                [&mut a, &mut b],
            );
            assert_draw_match(&end, &a);
            let mut choices = a.choices.iter().chain(&b.choices).collect::<Vec<_>>();
            choices.sort_by_key(|choice| choice.number);
            assert_eq!(choices.len(), 4);
            assert_eq!(choices[0].chooser, PlayerId(seed));
            assert_eq!(
                choices[1].chooser,
                PlayerId(seed),
                "draw retains chooser, not starter"
            );
            assert_eq!(choices[2].chooser, PlayerId(1));
            assert_eq!(choices[3].chooser, PlayerId(0));
            assert_eq!(a.starts[0].first, PlayerId(1 - seed));
            assert_eq!(a.starts[2].first, PlayerId(0));
            assert_eq!(a.starts[3].first, PlayerId(1));
        }
    }

    #[test]
    fn a_hosted_draw_first_choice_is_honored_before_each_opening_hand() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            let mut seat = Recorder {
                draw_first: true,
                choose_draw: true,
                ..Default::default()
            };
            let end = guest_match(&mut guest_end, draw_deck(), &mut seat).unwrap();
            (end, seat)
        });
        let mut seat = Recorder {
            draw_first: true,
            choose_draw: true,
            ..Default::default()
        };
        let end = host_match(
            &mut host_end,
            settings(3),
            draw_deck(),
            &Invented,
            Seed([1; 32]),
            &mut seat,
        )
        .unwrap();
        let (remote, remote_seat) = guest.join().unwrap();
        assert_eq!(end, remote);
        assert_draw_match(&end, &seat);
        assert_eq!(
            remote_seat
                .choices
                .iter()
                .map(|choice| choice.number)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            seat.choices
                .iter()
                .map(|choice| choice.number)
                .collect::<Vec<_>>(),
            [4]
        );
        assert_eq!(
            seat.starts
                .iter()
                .map(|start| start.first)
                .collect::<Vec<_>>(),
            [PlayerId(0), PlayerId(0), PlayerId(0), PlayerId(1)]
        );
    }

    #[test]
    fn repeated_draws_stop_at_a_bounded_limit_without_a_false_winner() {
        let mut score = Score::new(settings(3), &Seed([0; 32]));
        for number in 1..=MAX_MATCH_GAMES {
            assert!(!score.over());
            assert_eq!(score.start().0, number);
            score.finish(None);
        }
        assert!(score.over());
        let end = score.end(None);
        assert_eq!(end.winner, None);
        assert_eq!(end.score, [0, 0]);
        assert!(end.reason.unwrap().contains("safety limit"));
        let mut single = Score::new(settings(1), &Seed([0; 32]));
        single.start();
        single.finish(None);
        assert!(single.over(), "best-of-one still means exactly one game");
        assert_eq!(single.end(None).reason, None);
    }

    #[test]
    fn the_loser_plays_first_next_game() {
        let cards = GameCards::build([&deck()], &Invented).unwrap();
        let (mut a, mut b) = (Recorder::default(), Recorder::default());
        play_local_match(
            settings(3),
            &cards,
            [deck(), deck()],
            Seed([0; 32]),
            [&mut a, &mut b],
        );
        assert_eq!(a.starts[1].first, SEATS[1], "seat 1 lost game one");
        assert_eq!(a.starts[2].first, SEATS[0], "seat 0 lost game two");
    }

    #[test]
    fn a_best_of_one_is_one_game() {
        let cards = GameCards::build([&deck()], &Invented).unwrap();
        let (mut a, mut b) = (Recorder::default(), Recorder::default());
        let end = play_local_match(
            settings(1),
            &cards,
            [deck(), deck()],
            Seed([0; 32]),
            [&mut a, &mut b],
        );
        assert_eq!(end.score, [1, 0]);
        assert_eq!(a.sideboarded, 0);
    }

    #[test]
    fn a_whole_match_over_a_channel_agrees_on_the_result() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            let mut seat = Recorder::default();
            let end = guest_match(&mut guest_end, deck(), &mut seat).expect("guest");
            (end, seat)
        });
        let mut seat = Recorder::default();
        let end = host_match(
            &mut host_end,
            settings(3),
            deck(),
            &Invented,
            Seed([0; 32]),
            &mut seat,
        )
        .expect("host");
        let (guest_result, guest_seat) = guest.join().unwrap();

        assert_eq!(end.score, [1, 2]);
        assert_eq!(
            guest_result.score, end.score,
            "both ends agree on the score"
        );
        assert_eq!(guest_result.winner, end.winner);
        assert_eq!(guest_seat.starts.len(), 3);
        assert!(guest_seat.starts.iter().all(|s| s.you == SEATS[1]));
        assert_eq!(guest_seat.sideboarded, 2);
    }

    #[test]
    fn leaving_after_game_one_preserves_host_guest_agreement_without_a_match_winner() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            guest_match(&mut guest_end, deck(), &mut Recorder::default()).unwrap()
        });
        let end = host_match(
            &mut host_end,
            settings(3),
            deck(),
            &Invented,
            Seed([0; 32]),
            &mut Recorder {
                leave_after_one: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(end.score, [1, 0]);
        assert_eq!(end.winner, None);
        assert_eq!(end.reason.as_deref(), Some("the host left"));
        assert_eq!(guest.join().unwrap(), end);
    }

    #[test]
    fn the_host_refuses_a_play_draw_reply_for_another_game() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            guest_end.send_msg(&ToHost::Deck(deck())).unwrap();
            assert!(matches!(
                guest_end.recv_msg::<ToGuest>().unwrap(),
                ToGuest::Welcome { .. }
            ));
            let ToGuest::PlayDraw(choice) = guest_end.recv_msg::<ToGuest>().unwrap() else {
                panic!("choice");
            };
            assert_eq!(choice.chooser, PlayerId(1));
            guest_end
                .send_msg(&ToHost::PlayDraw {
                    number: 2,
                    play_first: false,
                })
                .unwrap();
        });
        let mut seat = Recorder::default();
        let result = host_match(
            &mut host_end,
            settings(3),
            deck(),
            &Invented,
            Seed([1; 32]),
            &mut seat,
        );
        assert!(matches!(result, Err(SessionError::Unexpected(_))));
        assert!(seat.starts.is_empty());
        guest.join().unwrap();
    }

    #[test]
    fn leaving_during_the_initial_choice_starts_no_game_and_agrees_remotely() {
        struct Leave(Recorder);
        impl Decide for Leave {
            fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
                self.0.decide(choice, view)
            }
        }
        impl Seat for Leave {
            fn choose_play_first(&mut self, _: &PlayDrawChoice) -> bool {
                self.0.leave_after_one = true;
                false
            }
            fn has_left(&self) -> bool {
                self.0.leave_after_one
            }
            fn begin_game(&mut self, _: &GameStart) {
                panic!("no hand may be dealt after leaving");
            }
        }
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            guest_match(&mut guest_end, deck(), &mut Recorder::default()).unwrap()
        });
        let end = host_match(
            &mut host_end,
            settings(3),
            deck(),
            &Invented,
            Seed([0; 32]),
            &mut Leave(Recorder::default()),
        )
        .unwrap();
        assert_eq!(end.score, [0, 0]);
        assert_eq!(end.winner, None);
        assert_eq!(end.reason.as_deref(), Some("the host left"));
        assert_eq!(guest.join().unwrap(), end);
        let cards = GameCards::build([&deck()], &Invented).unwrap();
        let local = play_local_match(
            settings(3),
            &cards,
            [deck(), deck()],
            Seed([0; 32]),
            [&mut Leave(Recorder::default()), &mut Recorder::default()],
        );
        assert_eq!(local.winner, None);
        assert_eq!(local.score, [0, 0]);
        assert!(local.reason.is_some());
    }

    #[test]
    fn a_deck_the_host_cannot_resolve_is_rejected_with_a_reason() {
        let (mut host_end, mut guest_end) = MemoryChannel::pair();
        let guest = std::thread::spawn(move || {
            let unknown = DeckSpec {
                main: vec![(CardKey::Oracle("not-here".into(), None), 60)],
                ..Default::default()
            };
            guest_match(&mut guest_end, unknown, &mut Recorder::default())
        });
        let host = host_match(
            &mut host_end,
            settings(1),
            deck(),
            &Invented,
            Seed([0; 32]),
            &mut Recorder::default(),
        );
        assert!(host.is_err());
        let Err(SessionError::Unexpected(reason)) = guest.join().unwrap() else {
            panic!("rejected")
        };
        assert!(reason.contains("missing"), "{reason}");
    }
}

#[cfg(test)]
mod alpha_protocol_tests {
    use super::*;
    use crate::game::Format;
    use mtg_engine::{Choice, PlayerView, choice::Answer};
    use mtg_net::{session::HostMessage, wire::MemoryChannel};

    #[derive(Default)]
    struct Client {
        starts: usize,
        observations: usize,
        sideboards: usize,
        choices: usize,
    }
    impl Decide for Client {
        fn observe(&mut self, _: &PlayerView) {
            self.observations += 1;
        }
        fn decide(&mut self, _: &Choice, _: &PlayerView) -> Answer {
            panic!("no decision expected")
        }
    }
    impl Seat for Client {
        fn choose_play_first(&mut self, _: &PlayDrawChoice) -> bool {
            self.choices += 1;
            true
        }
        fn begin_game(&mut self, _: &GameStart) {
            self.starts += 1;
        }
        fn sideboard(&mut self, deck: &DeckSpec, _: &GameCards) -> DeckSpec {
            self.sideboards += 1;
            deck.clone()
        }
    }
    fn welcome(channel: &mut MemoryChannel, best_of: u8) {
        channel
            .send_msg(&ToGuest::Welcome {
                settings: MatchSettings {
                    format: Format::Standard,
                    best_of,
                },
                cards: Box::default(),
            })
            .unwrap();
    }
    fn start(channel: &mut MemoryChannel, number: u8, you: u8, score: Vec<u8>) {
        channel
            .send_msg(&ToGuest::PlayDraw(PlayDrawChoice {
                number,
                chooser: PlayerId(if score.first() > score.get(1) { 1 } else { 0 }),
                score: score.clone(),
            }))
            .unwrap();
        channel
            .send_msg(&ToGuest::Game(GameStart {
                number,
                you: PlayerId(you),
                first: PlayerId(0),
                score,
            }))
            .unwrap();
    }
    fn game_over(channel: &mut MemoryChannel, winner: Option<u8>) {
        channel.send_msg(&HostMessage::GameOver { winner }).unwrap();
    }
    #[test]
    fn malformed_or_duplicate_play_draw_requests_do_not_reach_the_ui() {
        for (number, chooser, score, duplicate) in [
            (0, 1, vec![0, 0], false),
            (1, 2, vec![0, 0], false),
            (1, 1, vec![1, 0], false),
            (1, 1, vec![0, 0, 0], false),
            (1, 0, vec![0, 0], true),
        ] {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, 3);
            let request = ToGuest::PlayDraw(PlayDrawChoice {
                number,
                chooser: PlayerId(chooser),
                score,
            });
            host.send_msg(&request).unwrap();
            if duplicate {
                host.send_msg(&request).unwrap();
            }
            let mut client = Client::default();
            assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
            assert_eq!(client.choices, 0);
            assert_eq!(client.starts, 0);
        }
    }

    #[test]
    fn play_draw_requires_sideboarding_the_previous_loser_and_the_selected_starter() {
        for case in 0..3 {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, 3);
            start(&mut host, 1, 1, vec![0, 0]);
            game_over(&mut host, Some(0));
            if case != 0 {
                host.send_msg(&ToGuest::Sideboard).unwrap();
            }
            host.send_msg(&ToGuest::PlayDraw(PlayDrawChoice {
                number: 2,
                chooser: PlayerId(if case == 1 { 0 } else { 1 }),
                score: vec![1, 0],
            }))
            .unwrap();
            if case == 2 {
                // Guest chose play; the host must not start the opponent instead.
                host.send_msg(&ToGuest::Game(GameStart {
                    number: 2,
                    you: PlayerId(1),
                    first: PlayerId(0),
                    score: vec![1, 0],
                }))
                .unwrap();
            }
            let mut client = Client::default();
            assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
            assert_eq!(client.starts, 1);
            assert_eq!(client.choices, usize::from(case == 2));
        }
    }

    #[test]
    fn a_two_player_game_without_play_draw_agreement_is_rejected() {
        let (mut host, mut guest) = MemoryChannel::pair();
        welcome(&mut host, 1);
        host.send_msg(&ToGuest::Game(GameStart {
            number: 1,
            you: PlayerId(1),
            first: PlayerId(0),
            score: vec![0, 0],
        }))
        .unwrap();
        let mut client = Client::default();
        assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
        assert_eq!(client.starts, 0);
    }

    #[test]
    fn an_early_best_of_three_lead_is_not_a_match_win() {
        let (mut host, mut guest) = MemoryChannel::pair();
        welcome(&mut host, 3);
        start(&mut host, 1, 1, vec![0, 0]);
        game_over(&mut host, Some(0));
        host.send_msg(&ToGuest::MatchOver {
            score: vec![1, 0],
            reason: Some("the host left".into()),
        })
        .unwrap();
        let result = guest_match(&mut guest, DeckSpec::default(), &mut Client::default()).unwrap();
        assert_eq!(result.score, [1, 0]);
        assert_eq!(result.winner, None);
        assert_eq!(result.reason.as_deref(), Some("the host left"));
    }
    #[test]
    fn an_unexplained_early_best_of_three_end_is_rejected() {
        for winner in [None, Some(0)] {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, 3);
            start(&mut host, 1, 1, vec![0, 0]);
            game_over(&mut host, winner);
            host.send_msg(&ToGuest::MatchOver {
                score: if winner.is_some() {
                    vec![1, 0]
                } else {
                    vec![0, 0]
                },
                reason: None,
            })
            .unwrap();
            assert!(guest_match(&mut guest, DeckSpec::default(), &mut Client::default()).is_err());
        }
    }

    #[test]
    fn the_guest_accepts_the_draw_limit_but_refuses_game_sixty_five() {
        for extra_game in [false, true] {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, 3);
            for number in 1..=MAX_MATCH_GAMES {
                start(&mut host, number, 1, vec![0, 0]);
                game_over(&mut host, None);
                if number < MAX_MATCH_GAMES {
                    host.send_msg(&ToGuest::Sideboard).unwrap();
                }
            }
            if extra_game {
                start(&mut host, MAX_MATCH_GAMES + 1, 1, vec![0, 0]);
            } else {
                host.send_msg(&ToGuest::MatchOver {
                    score: vec![0, 0],
                    reason: Some(
                        "match stopped at the 64-game safety limit without a winner".into(),
                    ),
                })
                .unwrap();
            }
            let mut client = Client::default();
            let result = guest_match(&mut guest, DeckSpec::default(), &mut client);
            assert_eq!(client.starts, MAX_MATCH_GAMES as usize);
            assert_eq!(client.sideboards, (MAX_MATCH_GAMES - 1) as usize);
            if extra_game {
                assert!(result.is_err());
            } else {
                let end = result.unwrap();
                assert_eq!(end.winner, None);
                assert_eq!(end.score, [0, 0]);
                assert!(end.reason.unwrap().contains("safety limit"));
            }
        }
    }

    #[test]
    fn a_best_of_one_draw_is_an_ordinary_completed_match() {
        let (mut host, mut guest) = MemoryChannel::pair();
        welcome(&mut host, 1);
        start(&mut host, 1, 1, vec![0, 0]);
        game_over(&mut host, None);
        host.send_msg(&ToGuest::MatchOver {
            score: vec![0, 0],
            reason: None,
        })
        .unwrap();
        let end = guest_match(&mut guest, DeckSpec::default(), &mut Client::default()).unwrap();
        assert_eq!(end.winner, None);
        assert_eq!(end.reason, None);
    }

    #[test]
    fn a_changed_final_score_is_rejected() {
        let (mut host, mut guest) = MemoryChannel::pair();
        welcome(&mut host, 1);
        start(&mut host, 1, 1, vec![0, 0]);
        game_over(&mut host, Some(1));
        host.send_msg(&ToGuest::MatchOver {
            score: vec![1, 0],
            reason: None,
        })
        .unwrap();
        assert!(guest_match(&mut guest, DeckSpec::default(), &mut Client::default()).is_err());
    }
    #[test]
    fn changed_seating_scores_game_numbers_and_omitted_sideboarding_are_rejected() {
        for (number, you, score, sideboard) in [
            (2, 0, vec![1, 0], true),
            (2, 1, vec![0, 1], true),
            (3, 1, vec![1, 0], true),
            (2, 1, vec![1, 0], false),
            (2, 2, vec![1, 0, 0], true),
        ] {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, 3);
            start(&mut host, 1, 1, vec![0, 0]);
            game_over(&mut host, Some(0));
            if sideboard {
                host.send_msg(&ToGuest::Sideboard).unwrap();
            }
            start(&mut host, number, you, score);
            let mut client = Client::default();
            assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
            assert_eq!(
                client.starts, 1,
                "bad next game is rejected before reaching the UI"
            );
        }
    }
    #[test]
    fn unrequested_sideboarding_and_invalid_settings_do_not_reach_the_ui() {
        for best_of in [0, 1, 2, 255] {
            let (mut host, mut guest) = MemoryChannel::pair();
            welcome(&mut host, best_of);
            host.send_msg(&ToGuest::Sideboard).unwrap();
            let mut client = Client::default();
            assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
            assert_eq!(client.sideboards, 0);
        }
    }
    #[test]
    fn a_snapshot_must_match_the_assigned_seat_before_observation() {
        let (mut host, mut guest) = MemoryChannel::pair();
        welcome(&mut host, 1);
        start(&mut host, 1, 1, vec![0, 0]);
        let engine = Engine::new(mtg_engine::state::GameState::new(&SEATS, 20));
        host.send_msg(&HostMessage::Snapshot {
            at_event: 0,
            view_bytes: mtg_net::wire::encode(&engine.view_for(PlayerId(0))).unwrap(),
        })
        .unwrap();
        let mut client = Client::default();
        assert!(guest_match(&mut guest, DeckSpec::default(), &mut client).is_err());
        assert_eq!(client.observations, 0);
    }
}
