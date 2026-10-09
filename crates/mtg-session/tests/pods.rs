//! Observable multiplayer acceptance checks, using invented cards and no catalog.
use mtg_core::{CardType, ManaCost, PlayerId, Subtype, Supertype, Zone};
use mtg_engine::{Choice, ChoiceKind, PlayerView, actions::Action, choice::Answer};
use mtg_ir::CardFace;
use mtg_net::{fairness::Seed, wire::MemoryChannel};
use mtg_session::{
    Decide,
    game::{
        CardKey, CardSource, DeckSpec, Format, GameCards, MatchSettings, SourcedCard,
        new_game_with_players,
    },
    matches::{
        GameStart, Seat, guest_match, host_match_with_players, play_local_match_with_players,
    },
};

struct Cards;
impl CardSource for Cards {
    fn card(&self, key: &CardKey) -> Option<SourcedCard> {
        let CardKey::Demo(n) = key else { return None };
        Some(SourcedCard {
            layout: mtg_ir::Layout::Normal,
            color_identity: String::new(),
            faces: vec![CardFace {
                name: format!("Pod Card {n}").into(),
                mana_cost: ManaCost::FREE,
                card_types: vec![if *n == 0 {
                    CardType::Land
                } else {
                    CardType::Creature
                }],
                supertypes: vec![if *n == 0 {
                    Supertype::Basic
                } else {
                    Supertype::Legendary
                }],
                subtypes: vec![],
                power: Some(1),
                toughness: Some(1),
                loyalty: None,
                abilities: vec![],
                oracle_text: None,
                colors: None,
            }],
        })
    }
    fn subtype_name(&self, _: Subtype) -> Option<String> {
        None
    }
}
fn deck(seat: usize) -> DeckSpec {
    DeckSpec {
        name: format!("Seat {seat}"),
        main: vec![(CardKey::Demo(0), 99)],
        commander: Some(CardKey::Demo(seat as u32 + 1)),
        ..Default::default()
    }
}
fn settings() -> MatchSettings {
    MatchSettings {
        format: Format::Commander,
        best_of: 1,
    }
}
#[derive(Default)]
struct Player {
    start: Option<GameStart>,
    seen: usize,
    survivor: u8,
}
impl Decide for Player {
    fn observe(&mut self, view: &PlayerView) {
        assert_eq!(view.viewer, self.start.as_ref().unwrap().you);
        // All hands are private in this fixture: no reveal effects or history.
        assert!(
            view.visible
                .values()
                .all(|object| object.zone.zone != Zone::Hand
                    || (object.zone.player == Some(view.viewer) || object.card.is_none()))
        );
        self.seen += 1;
    }
    fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
        assert_eq!(choice.who, view.viewer);
        assert_eq!(choice.who, self.start.as_ref().unwrap().you);
        if matches!(choice.kind, ChoiceKind::Priority { .. }) && choice.who.0 != self.survivor {
            Answer::Action(Action::Concede)
        } else {
            mtg_policy::well_formed(choice, view)
        }
    }
}
impl Seat for Player {
    fn begin_game(&mut self, start: &GameStart) {
        self.start = Some(start.clone());
    }
}

#[test]
fn four_commanders_open_in_distinct_seats_with_rotated_turn_order() {
    let decks: Vec<_> = (0..4).map(deck).collect();
    let cards = GameCards::build(decks.iter(), &Cards).unwrap();
    let state = new_game_with_players(
        Format::Commander,
        &cards,
        &decks.iter().collect::<Vec<_>>(),
        &Seed([7; 32]),
        1,
        PlayerId(2),
    );
    assert_eq!(
        state.turn_order,
        [PlayerId(2), PlayerId(3), PlayerId(0), PlayerId(1)]
    );
    assert_eq!(state.players.len(), 4);
    for i in 0..4 {
        let who = PlayerId(i);
        assert_eq!(state.players[&who].life, 40);
        assert_eq!(
            state
                .objects
                .values()
                .filter(|object| object.owner == who && object.zone.zone == Zone::Hand)
                .count(),
            7
        );
        assert_eq!(
            state
                .objects
                .values()
                .filter(|object| object.owner == who && object.zone.zone == Zone::Library)
                .count(),
            92
        );
        assert_eq!(
            state
                .objects
                .values()
                .filter(|object| object.owner == who && object.zone.zone == Zone::Command)
                .count(),
            1
        );
    }
}

#[test]
fn local_two_three_and_four_player_pods_finish_after_concessions() {
    for count in 2..=4 {
        let mut decks: Vec<_> = (0..count).map(deck).collect();
        let cards = GameCards::build(decks.iter(), &Cards).unwrap();
        let mut players: Vec<_> = (0..count)
            .map(|_| Player {
                survivor: count as u8 - 1,
                ..Default::default()
            })
            .collect();
        let mut seats: Vec<&mut dyn Seat> =
            players.iter_mut().map(|p| p as &mut dyn Seat).collect();
        let end = play_local_match_with_players(
            settings(),
            &cards,
            &mut decks,
            Seed([0; 32]),
            &mut seats,
        );
        assert_eq!(end.winner, Some(PlayerId(count as u8 - 1)));
        assert_eq!(end.score.len(), count);
        assert_eq!(end.score.iter().sum::<u8>(), 1);
        for (i, player) in players.iter().enumerate() {
            assert_eq!(player.start.as_ref().unwrap().you, PlayerId(i as u8));
        }
    }
}

#[test]
fn three_remote_guests_receive_private_views_and_agree_after_host_elimination() {
    let mut hosts = vec![];
    let mut guests = vec![];
    for i in 1..4 {
        let (host, mut guest) = MemoryChannel::pair();
        hosts.push(host);
        guests.push(std::thread::spawn(move || {
            let mut seat = Player {
                survivor: 3,
                ..Default::default()
            };
            let end = guest_match(&mut guest, deck(i), &mut seat).unwrap();
            assert_eq!(seat.start.as_ref().unwrap().you, PlayerId(i as u8));
            assert!(seat.seen > 0);
            end
        }));
    }
    let mut local = Player {
        survivor: 3,
        ..Default::default()
    };
    let end = host_match_with_players(
        &mut hosts,
        settings(),
        deck(0),
        &Cards,
        Seed([0; 32]),
        &mut local,
    )
    .unwrap();
    assert_eq!(end.winner, Some(PlayerId(3)));
    assert_eq!(end.score, [0, 0, 0, 1]);
    assert!(local.seen > 0, "eliminated host keeps observing");
    for guest in guests {
        assert_eq!(guest.join().unwrap(), end);
    }
}

#[test]
fn a_guest_refuses_questions_for_other_seats_or_with_mismatched_ids() {
    use mtg_net::{
        session::HostMessage,
        wire::{TypedChannel, encode},
    };
    for (who, message_id) in [(PlayerId(2), 41), (PlayerId(1), 42)] {
        let decks: Vec<_> = (0..4).map(deck).collect();
        let cards = GameCards::build(decks.iter(), &Cards).unwrap();
        let state = new_game_with_players(
            Format::Commander,
            &cards,
            &decks.iter().collect::<Vec<_>>(),
            &Seed([0; 32]),
            1,
            PlayerId(0),
        );
        let view = mtg_engine::Engine::new(state).view_for(PlayerId(1));
        let (mut host, guest) = MemoryChannel::pair();
        host.send_msg(&HostMessage::Snapshot {
            at_event: 0,
            view_bytes: encode(&view).unwrap(),
        })
        .unwrap();
        host.send_msg(&HostMessage::Ask {
            choice_id: message_id,
            choice_bytes: encode(&Choice {
                id: 41,
                who,
                kind: ChoiceKind::Confirm,
                because: "invalid routing".into(),
                default: None,
                undo: false,
            })
            .unwrap(),
        })
        .unwrap();
        let mut decider = |_: &Choice, _: &PlayerView| -> Answer {
            panic!("misrouted question must not reach the player")
        };
        assert!(matches!(
            mtg_session::GuestSession::new(guest).play(&mut decider),
            Err(mtg_session::SessionError::Unexpected(_))
        ));
    }
}

#[test]
fn a_disconnected_guest_aborts_the_pod_without_a_false_winner() {
    use mtg_net::wire::TypedChannel;
    use mtg_session::matches::{ToGuest, ToHost};
    let mut hosts = vec![];
    let mut guests = vec![];
    for i in 1..4 {
        let (host, mut guest) = MemoryChannel::pair();
        hosts.push(host);
        guests.push(std::thread::spawn(move || {
            if i == 1 {
                guest.send_msg(&ToHost::Deck(deck(i))).unwrap();
                assert!(matches!(
                    guest.recv_msg::<ToGuest>().unwrap(),
                    ToGuest::Welcome { .. }
                ));
                assert!(matches!(
                    guest.recv_msg::<ToGuest>().unwrap(),
                    ToGuest::Game(_)
                ));
                return None; // Close the connection just as play begins.
            }
            Some(
                guest_match(
                    &mut guest,
                    deck(i),
                    &mut Player {
                        survivor: 3,
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
        }));
    }
    let end = host_match_with_players(
        &mut hosts,
        settings(),
        deck(0),
        &Cards,
        Seed([0; 32]),
        &mut Player {
            survivor: 3,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(end.winner, None);
    assert_eq!(end.score, [0, 0, 0, 0]);
    assert!(end.reason.as_deref().unwrap().contains("disconnected"));
    for guest in guests {
        if let Some(result) = guest.join().unwrap() {
            assert_eq!(result, end);
        }
    }
}
