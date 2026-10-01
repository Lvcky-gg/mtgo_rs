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
use mtg_engine::{Engine, Progress, view};
use mtg_net::{
    fairness::Seed,
    wire::{Channel, TypedChannel},
};
use serde::{Deserialize, Serialize};

use crate::{
    Decide, GuestSession, HostSession, Outcome, SessionError,
    game::{CardSource, DeckSpec, GameCards, MatchSettings, SEATS, deck_problems, new_game},
};

/// A game about to begin, from one seat's point of view.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GameStart {
    /// 1-based.
    pub number: u8,
    pub you: PlayerId,
    pub first: PlayerId,
    /// Games won so far, by seat.
    pub score: [u8; 2],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GameEnd {
    pub number: u8,
    pub winner: Option<PlayerId>,
    pub score: [u8; 2],
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MatchEnd {
    pub score: [u8; 2],
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
    wins: [u8; 2],
    games: u8,
    next_first: PlayerId,
}

impl Score {
    /// The first game's starting player comes from the agreed seed, so neither side chose it.
    fn new(settings: MatchSettings, seed: &Seed) -> Self {
        let next_first = SEATS[(seed.0[0] & 1) as usize];
        Self {
            settings,
            wins: [0, 0],
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
        SEATS.into_iter().find(|p| self.wins[p.0 as usize] >= need)
    }

    fn over(&self) -> bool {
        self.match_winner().is_some() || self.games >= self.settings.best_of
    }

    fn end(&self, reason: Option<String>) -> MatchEnd {
        MatchEnd {
            score: self.wins,
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
    let [a, b] = seats;
    let mut seats: [&mut dyn Seat; 2] = [a, b];
    let mut score = Score::new(settings, &seed);
    for seat in seats.iter_mut() {
        seat.begin_match(settings, cards);
    }

    while !score.over() {
        let (number, first) = score.start();
        for (i, seat) in seats.iter_mut().enumerate() {
            seat.begin_game(&GameStart {
                number,
                you: SEATS[i],
                first,
                score: score.wins,
            });
        }

        let state = new_game(
            settings.format,
            cards,
            [&decks[0], &decks[1]],
            &seed,
            number,
            first,
        );
        let winner = play_local_game(Engine::new(state), cards, &mut seats);
        score.finish(winner);
        let end = GameEnd {
            number,
            winner,
            score: score.wins,
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
    seats: &mut [&mut dyn Seat; 2],
) -> Option<PlayerId> {
    let mut rejected = 0;
    for _ in 0..GAME_BUDGET {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::GameOver { winners } => return winners.first().copied(),
            Progress::NeedsChoice(choice) => {
                let view = view::project(&engine.state, choice.who);
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
    None
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
        score: [u8; 2],
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
    let ToHost::Deck(guest_deck) = channel.recv_msg::<ToHost>()? else {
        return Err(SessionError::Unexpected("expected the guest's deck".into()));
    };

    let cards = match GameCards::build([&host_deck, &guest_deck], source) {
        Ok(cards) => cards,
        Err(missing) => {
            let reason = format!(
                "the host's card database is missing {} of your cards; both players need a recent import",
                missing.len()
            );
            channel.send_msg(&ToGuest::Rejected {
                reason: reason.clone(),
            })?;
            return Err(SessionError::Unexpected(reason));
        }
    };
    let problems = deck_problems(settings.format, &guest_deck, &cards);
    if !problems.is_empty() {
        let reason = format!(
            "your deck is not legal for {}: {}",
            settings.format.name(),
            problems.join("; ")
        );
        channel.send_msg(&ToGuest::Rejected {
            reason: reason.clone(),
        })?;
        return Err(SessionError::Unexpected(reason));
    }

    channel.send_msg(&ToGuest::Welcome {
        settings,
        cards: Box::new(cards.clone()),
    })?;
    seat.begin_match(settings, &cards);

    let mut decks = [host_deck, guest_deck];
    let mut score = Score::new(settings, &seed);
    while !score.over() {
        let (number, first) = score.start();
        let start = GameStart {
            number,
            you: SEATS[1],
            first,
            score: score.wins,
        };
        channel.send_msg(&ToGuest::Game(start))?;
        seat.begin_game(&GameStart {
            you: SEATS[0],
            ..start
        });

        let state = new_game(
            settings.format,
            &cards,
            [&decks[0], &decks[1]],
            &seed,
            number,
            first,
        );
        let mut session = HostSession::new(Engine::new(state), &mut *channel, SEATS[1]);
        let outcome = session.play(&cards, seat)?;
        let winner = match outcome {
            Outcome::Over { winners } => winners.first().copied(),
            Outcome::Disconnected => {
                seat.end_game(&GameEnd {
                    number,
                    winner: None,
                    score: score.wins,
                });
                return Ok(score.end(Some("your opponent disconnected".into())));
            }
        };
        score.finish(winner);
        seat.end_game(&GameEnd {
            number,
            winner,
            score: score.wins,
        });

        if seat.has_left() {
            let reason = Some("the host left".to_string());
            let _ = channel.send_msg(&ToGuest::MatchOver {
                score: score.wins,
                reason: reason.clone(),
            });
            return Ok(score.end(reason));
        }
        if !score.over() && settings.format.has_sideboard() {
            channel.send_msg(&ToGuest::Sideboard)?;
            decks[0] = sideboarded(seat, &decks[0], &cards);
            if let ToHost::Sideboarded(next) = channel.recv_msg::<ToHost>()?
                && next.same_cards_as(&decks[1])
                && sideboard_sizes_valid(&next)
            {
                decks[1] = next;
            }
        }
    }

    channel.send_msg(&ToGuest::MatchOver {
        score: score.wins,
        reason: None,
    })?;
    Ok(score.end(None))
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
                let mut score = start.score;
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
                    score[w.0 as usize] += 1;
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
                let winner = match score[0].cmp(&score[1]) {
                    std::cmp::Ordering::Greater => Some(SEATS[0]),
                    std::cmp::Ordering::Less => Some(SEATS[1]),
                    std::cmp::Ordering::Equal => None,
                };
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
            self.starts.push(*start);
        }
        fn end_game(&mut self, end: &GameEnd) {
            self.ends.push(*end);
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
