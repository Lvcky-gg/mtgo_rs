//! Nearby lobbies advertised with signed, expiring invites over mDNS.

use crate::{Invite, SessionId, invite::Endpoint};
use mdns_sd::{Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
    collections::BTreeMap,
    net::Ipv4Addr,
    time::{SystemTime, UNIX_EPOCH},
};

const SERVICE: &str = "_mtgors._tcp.local.";
const CHUNK: usize = 200;
const MAX_PARTS: usize = 48;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn instance(session: SessionId) -> String {
    format!("{}.{SERVICE}", data_encoding::HEXLOWER.encode(&session.0))
}

pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertisement {
    pub fn publish(invite: &Invite, port: u16, name: &str) -> Result<Self, String> {
        let info = service_info(invite, port, name)?;
        let fullname = info.get_fullname().to_owned();
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        if let Err(error) = daemon.register(info) {
            let _ = daemon.shutdown();
            return Err(error.to_string());
        }
        Ok(Self { daemon, fullname })
    }
}

fn service_info(invite: &Invite, port: u16, name: &str) -> Result<ServiceInfo, String> {
    let link = invite.to_url();
    let chunks: Vec<_> = link.as_bytes().chunks(CHUNK).collect();
    if chunks.len() > MAX_PARTS {
        return Err("invite is too large to advertise".into());
    }
    let mut properties = vec![
        ("version".to_owned(), "1".to_owned()),
        ("parts".to_owned(), chunks.len().to_string()),
        ("name".to_owned(), name.chars().take(60).collect::<String>()),
    ];
    for (i, part) in chunks.iter().enumerate() {
        properties.push((
            format!("invite{i}"),
            String::from_utf8(part.to_vec()).map_err(|e| e.to_string())?,
        ));
    }
    let id = data_encoding::HEXLOWER.encode(&invite.session.0);
    ServiceInfo::new(
        SERVICE,
        &id,
        &format!("mtgors-{id}.local."),
        "",
        port,
        properties
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>()
            .as_slice(),
    )
    .map(|info| info.enable_addr_auto())
    .map_err(|e| e.to_string())
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        // Await the queued goodbye before stopping the daemon, bounded for shutdown.
        if let Ok(done) = self.daemon.unregister(&self.fullname) {
            let _ = done.recv_timeout(std::time::Duration::from_millis(200));
        }
        let _ = self.daemon.shutdown();
    }
}

#[derive(Clone, Debug)]
pub struct NearbyGame {
    pub name: String,
    pub link: String,
    pub invite: Invite,
    pub addresses: Vec<Ipv4Addr>,
    pub port: u16,
}

fn decode(service: &ResolvedService, time: u64) -> Option<NearbyGame> {
    if service.ty_domain != SERVICE || service.get_property_val_str("version")? != "1" {
        return None;
    }
    let count: usize = service.get_property_val_str("parts")?.parse().ok()?;
    if !(1..=MAX_PARTS).contains(&count) {
        return None;
    }
    let mut link = String::new();
    for i in 0..count {
        let part = service.get_property_val_str(&format!("invite{i}"))?;
        if part.len() > CHUNK {
            return None;
        }
        link.push_str(part);
    }
    let invite = Invite::from_url(&link).ok()?;
    invite.verify(time).ok()?;
    if !invite
        .endpoints
        .iter()
        .any(|e| matches!(e, Endpoint::Mdns { instance } if instance.as_ref() == service.fullname))
    {
        return None;
    }
    let mut addresses: Vec<_> = service.get_addresses_v4().into_iter().collect();
    addresses.sort();
    let name: String = service
        .get_property_val_str("name")
        .unwrap_or("Nearby game")
        .chars()
        .take(60)
        .collect();
    Some(NearbyGame {
        name,
        link,
        invite,
        addresses,
        port: service.port,
    })
}

pub struct Browser {
    daemon: ServiceDaemon,
    events: Receiver<ServiceEvent>,
    games: BTreeMap<String, NearbyGame>,
}

impl Browser {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let events = match daemon.browse(SERVICE) {
            Ok(events) => events,
            Err(e) => {
                let _ = daemon.shutdown();
                return Err(e.to_string());
            }
        };
        Ok(Self {
            daemon,
            events,
            games: BTreeMap::new(),
        })
    }

    pub fn poll(&mut self) {
        let time = now();
        for event in self.events.try_iter().take(64) {
            match event {
                ServiceEvent::ServiceResolved(service) => {
                    if let Some(game) = decode(&service, time) {
                        if self.games.len() < 128 || self.games.contains_key(&service.fullname) {
                            self.games.insert(service.fullname.clone(), game);
                        }
                    } else {
                        self.games.remove(&service.fullname);
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    self.games.remove(&fullname);
                }
                _ => {}
            }
        }
        self.games
            .retain(|_, game| game.invite.verify(time).is_ok());
    }

    pub fn games(&self) -> impl Iterator<Item = &NearbyGame> {
        self.games.values()
    }
    pub fn find(&self, instance: &str) -> Option<&NearbyGame> {
        self.games.get(instance)
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.daemon.stop_browse(SERVICE);
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    fn fixture() -> (Invite, ResolvedService) {
        let session = SessionId::random();
        let invite = Invite::issue(
            &Identity::generate(),
            session,
            vec![Endpoint::Mdns {
                instance: instance(session).into(),
            }],
            now() + 60,
        );
        let info = service_info(&invite, 12345, "Kitchen game")
            .unwrap()
            .as_resolved_service();
        (invite, info)
    }

    #[test]
    fn signed_invite_roundtrips_across_txt_chunks() {
        let (invite, info) = fixture();
        assert!(
            info.get_property_val_str("parts")
                .unwrap()
                .parse::<usize>()
                .unwrap()
                > 1
        );
        let game = decode(&info, now()).unwrap();
        assert_eq!(game.invite, invite);
        assert_eq!(game.name, "Kitchen game");
        assert_eq!(game.port, 12345);
    }

    #[test]
    fn expired_or_misnamed_services_are_ignored() {
        let (invite, mut info) = fixture();
        assert!(decode(&info, invite.not_after + 1).is_none());
        info.fullname = "another._mtgors._tcp.local.".into();
        assert!(decode(&info, now()).is_none());
    }

    #[test]
    #[ignore = "requires a network interface supporting local multicast"]
    fn a_lobby_is_discovered_and_removed_over_mdns() {
        let (invite, _) = fixture();
        let mut browser = Browser::new().unwrap();
        let advertisement = Advertisement::publish(&invite, 12345, "Discovery smoke test").unwrap();
        let fullname = instance(invite.session);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while browser.find(&fullname).is_none() && std::time::Instant::now() < deadline {
            browser.poll();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let game = browser
            .find(&fullname)
            .expect("a local lobby was discovered");
        assert_eq!(game.invite, invite);
        assert!(!game.addresses.is_empty());
        drop(advertisement);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while browser.find(&fullname).is_some() && std::time::Instant::now() < deadline {
            browser.poll();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            browser.find(&fullname).is_none(),
            "closed lobbies disappear"
        );
    }
}
