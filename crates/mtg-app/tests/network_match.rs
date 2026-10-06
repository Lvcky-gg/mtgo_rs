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
