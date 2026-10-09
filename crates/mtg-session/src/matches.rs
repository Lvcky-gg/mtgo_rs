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

/// One player's side of a match. See the module docs.
pub trait Seat: Decide {
    fn begin_match(&mut self, _settings: MatchSettings, _cards: &GameCards) {}
    fn begin_game(&mut self, _start: &GameStart) {}
    fn end_game(&mut self, _end: &GameEnd) {}
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
}

impl Score {
    /// The first game's starting player comes from the agreed seed, so neither side chose it.
    fn new(settings: MatchSettings, seed: &Seed) -> Self {
        let next_first = SEATS[(seed.0[0] & 1) as usize];
        Self {
            settings,
            wins: vec![0, 0],
            games: 0,
            next_first,
        }
    }

    fn start(&mut self) -> (u8, PlayerId) {
        self.games += 1;
        (self.games, self.next_first)
    }

    /// Record a result. The loser plays first next game (the loser's choice, CR 103.1c, taken
    /// as the choice nearly everyone makes); after a draw the same player goes first again.
    fn finish(&mut self, winner: Option<PlayerId>) {
        if let Some(w) = winner {
            self.wins[w.0 as usize] += 1;
            self.next_first = if w == SEATS[0] { SEATS[1] } else { SEATS[0] };
        }
    }

    fn match_winner(&self) -> Option<PlayerId> {
        let need = self.settings.wins_needed();
        self.wins
            .iter()
            .position(|wins| *wins >= need)
            .map(|i| PlayerId(i as u8))
    }

    fn over(&self) -> bool {
        self.match_winner().is_some() || self.games >= self.settings.best_of
    }

    fn end(&self, reason: Option<String>) -> MatchEnd {
        MatchEnd {
            score: self.wins.clone(),
            winner: self.match_winner(),
            reason,
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
    for seat in seats.iter_mut() {
        seat.begin_match(settings, cards);
    }

    while !score.over() {
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
}

/// Lobby messages from guest to host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToHost {
    Deck(DeckSpec),
    Sideboarded(DeckSpec),
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
    while !score.over() {
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
    let reason = seat.has_left().then(|| "the host left".to_string());
    for channel in channels.iter_mut() {
        channel.send_msg(&ToGuest::MatchOver {
            score: score.wins.clone(),
            reason: reason.clone(),
        })?;
    }
    Ok(score.end(reason))
}

/// Join a match: send the deck, then play whatever games the host starts. `seat` is the
/// guest's own player, in seat 1.
pub fn guest_match<C: Channel>(
    channel: &mut C,
    deck: DeckSpec,
    seat: &mut dyn Seat,
) -> Result<MatchEnd, SessionError> {
    channel.send_msg(&ToHost::Deck(deck.clone()))?;
    let cards = match channel.recv_msg::<ToGuest>()? {
        ToGuest::Welcome { settings, cards } => {
            seat.begin_match(settings, &cards);
            cards
        }
        ToGuest::Rejected { reason } => return Err(SessionError::Unexpected(reason)),
        other => return Err(SessionError::Unexpected(format!("{other:?}"))),
    };

    let mut deck = deck;
    loop {
        match channel.recv_msg::<ToGuest>()? {
            ToGuest::Game(start) => {
                if !(2..=4).contains(&start.score.len())
                    || start.you.0 as usize >= start.score.len()
                    || start.first.0 as usize >= start.score.len()
                {
                    return Err(SessionError::Unexpected("invalid game seating".into()));
                }
                let mut score = start.score.clone();
                seat.begin_game(&start);
                let outcome = GuestSession::new(&mut *channel).play(seat)?;
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
                }
                seat.end_game(&GameEnd {
                    number: start.number,
                    winner,
                    score,
                });
            }
            ToGuest::Sideboard => {
                deck = sideboarded(seat, &deck, &cards);
                channel.send_msg(&ToHost::Sideboarded(deck.clone()))?;
            }
            ToGuest::MatchOver { score, reason } => {
                if !(2..=4).contains(&score.len()) {
                    return Err(SessionError::Unexpected("invalid final seating".into()));
                }
                let maximum = score.iter().copied().max().unwrap_or(0);
                let leaders: Vec<_> = score
                    .iter()
                    .enumerate()
                    .filter(|(_, wins)| **wins == maximum && maximum > 0)
                    .collect();
                let winner = (leaders.len() == 1).then(|| PlayerId(leaders[0].0 as u8));
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
                abilities: Vec::new(),
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
    }
    impl Decide for Recorder {
        fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
            // Concede at once in the first game only, so games are short and results known.
            if let mtg_engine::ChoiceKind::Priority { .. } = choice.kind
                && view.viewer == SEATS[1]
                && self.starts.last().is_some_and(|s| s.number == 1)
            {
                return Answer::Action(mtg_engine::actions::Action::Concede);
            }
            if let mtg_engine::ChoiceKind::Priority { .. } = choice.kind
                && view.viewer == SEATS[0]
                && self.starts.last().is_some_and(|s| s.number >= 2)
            {
                return Answer::Action(mtg_engine::actions::Action::Concede);
            }
            mtg_policy::well_formed(choice, view)
        }
    }
    impl Seat for Recorder {
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
