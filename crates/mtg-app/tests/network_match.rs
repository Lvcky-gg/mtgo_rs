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
