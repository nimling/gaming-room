//! The IP addresses of the seats and the server, so players can copy
//! another seat's address into a game's Join box.

use crate::Config;
use serde::Serialize;
use std::net::{IpAddr, UdpSocket};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub name: String,
    pub address: String,
    pub this_seat: bool,
}

/// This seat's own address on the network that reaches `peer`. Connecting
/// a UDP socket picks the route; it sends nothing.
pub fn own_address(peer: &str) -> Option<IpAddr> {
    let host = peer.rsplit_once(':').map_or(peer, |(h, _)| h);
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect((host, 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// The hosts from the settings, with this seat marked. If this seat is
/// not in the list, it comes first under `seat_name`.
pub fn entries(config: &Config, seat_name: &str) -> Vec<Entry> {
    let peer = config
        .voice
        .as_ref()
        .map(|v| v.server.clone())
        .or_else(|| config.hosts.first().map(|h| h.address.clone()));
    let own = peer
        .as_deref()
        .and_then(own_address)
        .map(|ip| ip.to_string());
    mark(config, seat_name, own.as_deref())
}

fn mark(config: &Config, seat_name: &str, own: Option<&str>) -> Vec<Entry> {
    let mut list: Vec<Entry> = config
        .hosts
        .iter()
        .map(|h| Entry {
            name: h.name.clone(),
            address: h.address.clone(),
            this_seat: Some(h.address.as_str()) == own || h.name == seat_name,
        })
        .collect();
    if let Some(own) = own {
        if !list.iter().any(|e| e.this_seat) {
            list.insert(
                0,
                Entry {
                    name: seat_name.to_string(),
                    address: own.to_string(),
                    this_seat: true,
                },
            );
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        serde_yaml::from_str(
            "hosts:\n  - { name: gaming-server, address: 192.0.2.10 }\n  - { name: seat-01, address: 192.0.2.21 }\n  - { name: seat-02, address: 192.0.2.22 }\n",
        )
        .unwrap()
    }

    #[test]
    fn marks_this_seat_by_address_or_name() {
        let by_ip = mark(&config(), "unknown", Some("192.0.2.22"));
        assert_eq!(by_ip.iter().filter(|e| e.this_seat).count(), 1);
        assert!(by_ip[2].this_seat);
        let by_name = mark(&config(), "seat-01", None);
        assert!(by_name[1].this_seat);
    }

    #[test]
    fn adds_this_seat_when_it_is_not_listed() {
        let list = mark(&Config::default(), "seat-07", Some("192.0.2.27"));
        assert_eq!(
            list,
            vec![Entry {
                name: "seat-07".into(),
                address: "192.0.2.27".into(),
                this_seat: true
            }]
        );
        assert!(mark(&Config::default(), "seat-07", None).is_empty());
    }

    #[test]
    fn finds_an_own_address_without_sending() {
        // Loopback is filtered out, so asking for a route to it gives None.
        assert!(own_address("127.0.0.1").is_none());
    }
}
