//! The menu's Host and Join paths, end to end over a real socket: invite link, encryption,
//! handshake, deck exchange, and a game played to its end by two bots.

use std::sync::{Arc, atomic::AtomicBool, mpsc};

use mtg_app::{
    decks::{LocalSource, demo_deck},
    net,
    seat::{BotSeat, MatchEvent},
};
use mtg_net::Identity;
use mtg_session::game::{Format, MatchSettings};

#[test]
fn a_hosted_match_is_joined_by_link_and_played_to_the_end() {
    let settings = MatchSettings {
        format: Format::Constructed,
        best_of: 1,
    };
    let (events_tx, events) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));

    let host = std::thread::spawn(move || {
        let identity = Identity::generate();
        let source = LocalSource::new(None);
        let config = net::HostConfig {
            settings,
            players: 2,
            deck: demo_deck(),
            port: 0,
            tunnel_token: None,
            tunnel_domain: None,
            name: "Test game".into(),
            public_address: None,
            identity: &identity,
            source: &source,
        };
        net::host(config, &mut BotSeat::default(), &events_tx, &cancel)
    });

    let link = loop {
        if let MatchEvent::Invite(link) = events.recv().expect("the host reports progress") {
            break link;
        }
    };

    let (guest_tx, _guest_events) = mpsc::channel();
    let guest = net::join(
        &link,
        demo_deck(),
        &Identity::generate(),
        &mut BotSeat::default(),
        &guest_tx,
    )
    .expect("joined and played");
    let host = host.join().unwrap().expect("hosted and played");

    assert_eq!(host.score, guest.score, "both ends agree on the result");
    assert_eq!(
        host.score.iter().sum::<u8>(),
        1,
        "one game, played to a result"
    );
    assert_eq!(host.winner, guest.winner);
}

#[test]
fn a_bad_link_is_refused_before_anything_is_sent() {
    let (tx, _rx) = mpsc::channel();
    let err = net::join(
        "not a link",
        demo_deck(),
        &Identity::generate(),
        &mut BotSeat::default(),
        &tx,
    )
    .unwrap_err();
    assert!(err.contains("invite"), "{err}");
}

mod commander_pods {
    use super::*;
    use mtg_core::{PlayerId, Zone};
    use mtg_engine::{Choice, ChoiceKind, PlayerView, actions::Action, choice::Answer};
    use mtg_session::{
        Decide,
        game::{CardKey, DeckSpec},
        matches::Seat,
    };
    use mtg_store::{FaceRow, Store, StoredCard};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    fn deck(i: u8) -> DeckSpec {
        DeckSpec {
            name: format!("Pod seat {i}"),
            main: vec![(CardKey::Oracle("alpha-pod-field".into(), None), 99)],
            commander: Some(CardKey::Oracle(format!("alpha-pod-commander-{i}"), None)),
            ..Default::default()
        }
    }
    fn source() -> LocalSource {
        let store = Store::in_memory().unwrap();
        for i in 0..5 {
            let land = i == 4;
            let card = StoredCard {
                oracle_uuid: if land {
                    "alpha-pod-field".into()
                } else {
                    format!("alpha-pod-commander-{i}")
                },
                name: format!("Invented Lobby Card {i}"),
                layout: "normal".into(),
                faces: vec![FaceRow {
                    name: format!("Invented Lobby Card {i}"),
                    type_line: if land {
                        "Basic Land".into()
                    } else {
                        "Legendary Creature".into()
                    },
                    power: (!land).then(|| "1".into()),
                    toughness: (!land).then(|| "1".into()),
                    ..Default::default()
                }],
            };
            let oracle = store.put_card(&card, &[]).unwrap();
            store.set_legal_in(oracle, &["commander"]).unwrap();
        }
        LocalSource::new(Some(store))
    }
    struct ConcedeUntilLast;
    impl Decide for ConcedeUntilLast {
        fn observe(&mut self, view: &PlayerView) {
            assert!(
                view.visible
                    .values()
                    .all(|object| object.zone.zone != Zone::Hand
                        || object.zone.player == Some(view.viewer)
                        || object.card.is_none())
            );
        }
        fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
            assert_eq!(choice.who, view.viewer);
            if let ChoiceKind::KeepOrMulligan { mulligans_taken } = choice.kind {
                assert_eq!(view.players[&view.viewer].hand_size, 7);
                return Answer::Bool(mulligans_taken > 0);
            }
            assert!(
                !choice.because.starts_with("London mulligan:"),
                "each seat takes exactly one free multiplayer mulligan"
            );
            if matches!(choice.kind, ChoiceKind::Priority { .. }) && choice.who != PlayerId(3) {
                Answer::Action(Action::Concede)
            } else {
                mtg_policy::well_formed(choice, view)
            }
        }
    }
    impl Seat for ConcedeUntilLast {}

    fn spawn_host(
        cancel: Arc<AtomicBool>,
        events_tx: mpsc::Sender<MatchEvent>,
    ) -> std::thread::JoinHandle<Result<mtg_session::matches::MatchEnd, String>> {
        std::thread::spawn(move || {
            let identity = Identity::generate();
            let source = source();
            net::host(
                net::HostConfig {
                    settings: MatchSettings {
                        format: Format::Commander,
                        best_of: 1,
                    },
                    players: 4,
                    deck: deck(0),
                    port: 0,
                    public_address: None,
                    tunnel_token: None,
                    tunnel_domain: None,
                    name: "Four-player alpha test".into(),
                    identity: &identity,
                    source: &source,
                },
                &mut ConcedeUntilLast,
                &events_tx,
                &cancel,
            )
        })
    }
    fn next_invite(events: &mpsc::Receiver<MatchEvent>) -> String {
        loop {
            if let MatchEvent::Invite(link) = events
                .recv_timeout(Duration::from_secs(15))
                .expect("lobby invite")
            {
                return link;
            }
        }
    }
    #[test]
    fn four_encrypted_clients_use_distinct_invites_and_agree_on_a_result() {
        let (tx, events) = mpsc::channel();
        let host = spawn_host(Arc::new(AtomicBool::new(false)), tx);
        let mut guests = vec![];
        let mut links = std::collections::BTreeSet::new();
        for i in 1..4 {
            let link = next_invite(&events);
            assert!(
                links.insert(link.clone()),
                "each seat has its own invite nonce"
            );
            guests.push(std::thread::spawn(move || {
                let (tx, _rx) = mpsc::channel();
                net::join(
                    &link,
                    deck(i),
                    &Identity::generate(),
                    &mut ConcedeUntilLast,
                    &tx,
                )
                .unwrap()
            }));
        }
        let end = host.join().unwrap().unwrap();
        assert_eq!(end.winner, Some(PlayerId(3)));
        assert_eq!(end.score, [0, 0, 0, 1]);
        for guest in guests {
            assert_eq!(guest.join().unwrap(), end);
        }
    }
    #[test]
    fn cancelling_a_partially_filled_pod_releases_waiting_guests() {
        let (tx, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let host = spawn_host(cancel.clone(), tx);
        let link = next_invite(&events);
        let guest = std::thread::spawn(move || {
            let (tx, _rx) = mpsc::channel();
            net::join(
                &link,
                deck(1),
                &Identity::generate(),
                &mut ConcedeUntilLast,
                &tx,
            )
        });
        next_invite(&events); // First guest authenticated; host is awaiting seat 3.
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(host.join().unwrap().unwrap_err(), "cancelled");
        assert!(guest.join().unwrap().is_err());
    }
}

mod standard_alpha {
    use super::*;
    use mtg_core::{PlayerId, Zone};
    use mtg_engine::{Choice, ChoiceKind, PlayerView, actions::Action, choice::Answer};
    use mtg_session::{
        Decide,
        game::{DeckSpec, GameCards},
        matches::{GameEnd, GameStart, PlayDrawChoice, Seat},
    };
    use mtg_store::{FaceRow, Store, StoredCard};
    use std::time::Duration;

    fn imported_source_and_deck() -> (LocalSource, DeckSpec) {
        let mut store = Store::in_memory().unwrap();
        for (id, name, type_line, legal) in [
            ("alpha-field", "Acceptance Field", "Basic Land", true),
            ("alpha-soldier", "Acceptance Soldier", "Creature", true),
            ("alpha-scout", "Acceptance Scout", "Creature", true),
            ("alpha-illegal", "Acceptance Illegal", "Creature", false),
        ] {
            let card = StoredCard {
                oracle_uuid: id.into(),
                name: name.into(),
                layout: "normal".into(),
                faces: vec![FaceRow {
                    name: name.into(),
                    type_line: type_line.into(),
                    power: Some("1".into()),
                    toughness: Some("1".into()),
                    ..Default::default()
                }],
            };
            let oracle = store
                .put_card(&card, &[mtg_deck::resolve::normalise(name)])
                .unwrap();
            store
                .set_legal_in(oracle, if legal { &["standard"] } else { &[] })
                .unwrap();
        }
        mtg_app::decks::import_deck(
            &mut store,
            "Standard acceptance",
            "56 Acceptance Field\n4 Acceptance Soldier\n\nSideboard\n4 Acceptance Scout",
        )
        .unwrap();
        let id = store.deck_list().unwrap()[0].0;
        let deck =
            mtg_app::decks::deck_spec(Some(&store), mtg_app::decks::DeckId::Stored(id)).unwrap();
        assert_eq!(deck.main_count(), 60);
        assert_eq!(deck.side.iter().map(|(_, n)| u32::from(*n)).sum::<u32>(), 4);
        (LocalSource::new(Some(store)), deck)
    }
    #[derive(Default)]
    struct TestSeat {
        starts: Vec<GameStart>,
        ends: Vec<GameEnd>,
        sideboards: usize,
        choices: Vec<PlayDrawChoice>,
    }
    impl Decide for TestSeat {
        fn observe(&mut self, view: &PlayerView) {
            assert_eq!(view.viewer, self.starts.last().unwrap().you);
            assert!(
                view.visible
                    .values()
                    .all(|object| object.zone.zone != Zone::Hand
                        || object.zone.player == Some(view.viewer)
                        || object.card.is_none())
            );
        }
        fn decide(&mut self, choice: &Choice, view: &PlayerView) -> Answer {
            let start = self.starts.last().unwrap();
            assert_eq!(choice.who, start.you);
            if matches!(choice.kind, ChoiceKind::Priority { .. })
                && choice.who == PlayerId(if start.number == 1 { 1 } else { 0 })
            {
                Answer::Action(Action::Concede)
            } else {
                mtg_policy::well_formed(choice, view)
            }
        }
    }
    impl Seat for TestSeat {
        fn choose_play_first(&mut self, choice: &PlayDrawChoice) -> bool {
            assert_eq!(self.starts.len() + 1, choice.number as usize);
            assert_eq!(self.sideboards + 1, choice.number as usize);
            self.choices.push(choice.clone());
            choice.number == 1 // Exercise drawing first after both sideboarding rounds.
        }
        fn begin_match(&mut self, settings: MatchSettings, _: &GameCards) {
            assert_eq!(settings.format, Format::Standard);
            assert_eq!(settings.best_of, 3);
        }
        fn begin_game(&mut self, start: &GameStart) {
            self.starts.push(start.clone());
        }
        fn end_game(&mut self, end: &GameEnd) {
            self.ends.push(end.clone());
        }
        fn sideboard(&mut self, deck: &DeckSpec, _: &GameCards) -> DeckSpec {
            let mut next = deck.clone();
            assert_eq!(next.main.len(), 2);
            assert_eq!(next.side.len(), 1);
            std::mem::swap(&mut next.main[1], &mut next.side[0]);
            assert!(next.same_cards_as(deck));
            self.sideboards += 1;
            next
        }
    }
    #[test]
    fn imported_standard_decks_sideboard_twice_over_encrypted_transport() {
        let (tx, events) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let identity = Identity::generate();
            let (source, deck) = imported_source_and_deck();
            let mut seat = TestSeat::default();
            let result = net::host(
                net::HostConfig {
                    settings: MatchSettings {
                        format: Format::Standard,
                        best_of: 3,
                    },
                    players: 2,
                    deck,
                    port: 0,
                    tunnel_token: None,
                    tunnel_domain: None,
                    name: "Standard acceptance".into(),
                    public_address: None,
                    identity: &identity,
                    source: &source,
                },
                &mut seat,
                &tx,
                &Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            (result, seat)
        });
        let link = loop {
            if let MatchEvent::Invite(link) = events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                break link;
            }
        };
        let (_, deck) = imported_source_and_deck();
        let mut seat = TestSeat::default();
        let (tx, _rx) = mpsc::channel();
        let result = net::join(&link, deck, &Identity::generate(), &mut seat, &tx).unwrap();
        let (host_result, host_seat) = host.join().unwrap();
        assert_eq!(result, host_result);
        assert_eq!(result.score, [1, 2]);
        assert_eq!(result.winner, Some(PlayerId(1)));
        assert_eq!(
            seat.starts,
            host_seat
                .starts
                .iter()
                .map(|start| GameStart {
                    you: PlayerId(1),
                    ..start.clone()
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(seat.ends, host_seat.ends);
        assert_eq!(seat.starts.len(), 3);
        assert_eq!((seat.sideboards, host_seat.sideboards), (2, 2));
        assert_eq!(seat.starts[1].first, PlayerId(0));
        assert_eq!(seat.starts[2].first, PlayerId(1));
        let mut choices = seat
            .choices
            .iter()
            .chain(&host_seat.choices)
            .collect::<Vec<_>>();
        choices.sort_by_key(|choice| choice.number);
        assert_eq!(choices.len(), 3);
        assert_eq!(choices[0].number, 1);
        assert_eq!(choices[1].chooser, PlayerId(1));
        assert_eq!(choices[1].score, [1, 0]);
        assert_eq!(choices[2].chooser, PlayerId(0));
        assert_eq!(choices[2].score, [1, 1]);
    }

    #[test]
    fn an_illegal_standard_card_is_refused_before_either_seat_begins() {
        let (tx, events) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let identity = Identity::generate();
            let (source, deck) = imported_source_and_deck();
            let mut seat = TestSeat::default();
            let result = net::host(
                net::HostConfig {
                    settings: MatchSettings {
                        format: Format::Standard,
                        best_of: 3,
                    },
                    players: 2,
                    deck,
                    port: 0,
                    tunnel_token: None,
                    tunnel_domain: None,
                    name: "Standard refusal".into(),
                    public_address: None,
                    identity: &identity,
                    source: &source,
                },
                &mut seat,
                &tx,
                &Arc::new(AtomicBool::new(false)),
            );
            assert!(seat.starts.is_empty());
            result.unwrap_err()
        });
        let link = loop {
            if let MatchEvent::Invite(link) = events.recv_timeout(Duration::from_secs(15)).unwrap()
            {
                break link;
            }
        };
        let (_, mut deck) = imported_source_and_deck();
        deck.main[1].0 = mtg_session::game::CardKey::Oracle("alpha-illegal".into(), None);
        let mut seat = TestSeat::default();
        let (tx, _rx) = mpsc::channel();
        let reason = net::join(&link, deck, &Identity::generate(), &mut seat, &tx).unwrap_err();
        assert!(seat.starts.is_empty());
        assert_eq!(reason, host.join().unwrap());
        assert!(
            reason.contains("Acceptance Illegal is not legal for Standard"),
            "{reason}"
        );
    }
}
