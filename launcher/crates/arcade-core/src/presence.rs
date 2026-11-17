//! Who plays what on the gaming VLAN, for the launcher's People list.
//!
//! Every launcher tells the gaming server's control service who sits at the
//! seat and what they play (`POST <gameservers>/api/presence`), and gets the
//! whole room back. The service files each entry under the seat's address
//! and drops it 30 s after the last heartbeat. A launcher reports:
//! - every few seconds while it is on screen, with the "On the LAN" refresh;
//! - every 10 s while a game runs, from the thread that watches the game.

use crate::browse::{self, Game};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(3);

/// What this seat reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Beat {
    pub seat: String,
    pub nickname: String,
    /// The running game's id; `None` in the launcher.
    pub game: Option<String>,
    /// play, host or join.
    pub mode: Option<String>,
    /// `ip:port` of the server joined from the launcher.
    pub address: Option<String>,
}

impl Beat {
    /// The same player, back in the launcher.
    pub fn idle(&self) -> Beat {
        Beat {
            game: None,
            mode: None,
            address: None,
            ..self.clone()
        }
    }
}

/// One entry of the control service's list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Entry {
    pub ip: String,
    pub seat: String,
    pub nickname: String,
    pub game: Option<String>,
    pub mode: Option<String>,
    pub address: Option<String>,
    /// How long they have been doing what they do now.
    pub seconds: u64,
}

/// A person as the People list shows them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Person {
    pub nickname: String,
    pub seat: String,
    pub game: Option<String>,
    /// play, host or join; `None` in the launcher.
    pub mode: Option<String>,
    pub seconds: u64,
    /// The server they are on, when this seat can join it.
    pub server: Option<Server>,
    /// This seat.
    pub you: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Server {
    pub name: String,
    /// `ip:port` to join.
    pub address: String,
}

fn url(base: &str) -> String {
    format!("{}/api/presence", base.trim_end_matches('/'))
}

pub fn parse(json: &str) -> Result<Vec<Entry>> {
    serde_json::from_str(json).context("reading the people list")
}

/// Sends this seat's heartbeat and returns everyone.
pub fn report(base: &str, beat: &Beat) -> Result<Vec<Entry>> {
    let body = serde_json::to_string(beat)?;
    parse(&browse::http("POST", &url(base), Some(&body), TIMEOUT)?)
}

/// Everyone, without reporting.
pub fn list(base: &str) -> Result<Vec<Entry>> {
    parse(&browse::http_get(&url(base), TIMEOUT)?)
}

/// Takes this seat off the list at once (End session).
pub fn leave(base: &str) -> Result<()> {
    browse::http("DELETE", &url(base), None, TIMEOUT).map(|_| ())
}

/// Adds the server each person can be joined on, from this seat's own
/// "On the LAN" list:
/// - joined from the launcher: the server with that address. A server that
///   is not in the list (yet) still shows, under its address.
/// - hosting: the game this person's seat hosts, once it answers the LAN.
/// - started from the game's own menu, or playing alone: no server.
pub fn resolve(
    entries: &[Entry],
    lan: &BTreeMap<String, Vec<Game>>,
    own: Option<IpAddr>,
) -> Vec<Person> {
    entries
        .iter()
        .map(|e| {
            let games = e.game.as_ref().and_then(|g| lan.get(g));
            let server = match (e.mode.as_deref(), games) {
                (Some("join"), _) => e.address.as_ref().map(|a| Server {
                    name: games
                        .and_then(|l| l.iter().find(|g| &g.address == a))
                        .map_or_else(|| a.clone(), |g| g.name.clone()),
                    address: a.clone(),
                }),
                (Some("host"), Some(l)) => l
                    .iter()
                    .find(|g| g.address.rsplit_once(':').is_some_and(|(ip, _)| ip == e.ip))
                    .map(|g| Server {
                        name: g.name.clone(),
                        address: g.address.clone(),
                    }),
                _ => None,
            };
            Person {
                nickname: e.nickname.clone(),
                seat: e.seat.clone(),
                game: e.game.clone(),
                mode: e.mode.clone(),
                seconds: e.seconds,
                server,
                you: own.is_some_and(|ip| ip.to_string() == e.ip),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn entry(
        ip: &str,
        nick: &str,
        game: Option<&str>,
        mode: Option<&str>,
        address: Option<&str>,
    ) -> Entry {
        Entry {
            ip: ip.into(),
            seat: "Seat 01".into(),
            nickname: nick.into(),
            game: game.map(Into::into),
            mode: mode.map(Into::into),
            address: address.map(Into::into),
            seconds: 60,
        }
    }

    fn game(name: &str, address: &str) -> Game {
        Game {
            host: "gaming-server".into(),
            name: name.into(),
            address: address.into(),
            map: "DM-Rankin".into(),
            players: 2,
            max_players: 16,
            ping_ms: Some(1),
            mine: false,
        }
    }

    fn lan() -> BTreeMap<String, Vec<Game>> {
        BTreeMap::from([
            (
                "ut2004".into(),
                vec![game("Nimtech UT2004", "192.0.2.10:7800")],
            ),
            (
                "quake3".into(),
                vec![game("Kaptein's game", "192.0.2.23:27960")],
            ),
        ])
    }

    #[test]
    fn joined_from_the_launcher_gets_the_servers_name() {
        let e = entry(
            "192.0.2.21",
            "xXx_Gamer_xXx",
            Some("ut2004"),
            Some("join"),
            Some("192.0.2.10:7800"),
        );
        let p = resolve(&[e], &lan(), None);
        assert_eq!(
            p[0].server,
            Some(Server {
                name: "Nimtech UT2004".into(),
                address: "192.0.2.10:7800".into()
            })
        );
    }

    #[test]
    fn unknown_server_shows_its_address() {
        let e = entry(
            "192.0.2.21",
            "Mus",
            Some("ut2004"),
            Some("join"),
            Some("192.0.2.30:7777"),
        );
        let p = resolve(&[e], &lan(), None);
        assert_eq!(p[0].server.as_ref().unwrap().name, "192.0.2.30:7777");
    }

    #[test]
    fn host_is_found_by_the_seats_address() {
        let e = entry("192.0.2.23", "Kaptein", Some("quake3"), Some("host"), None);
        let p = resolve(&[e], &lan(), None);
        assert_eq!(p[0].server.as_ref().unwrap().address, "192.0.2.23:27960");
        let not_yet = entry("192.0.2.24", "Lyn", Some("ut2004"), Some("host"), None);
        assert_eq!(resolve(&[not_yet], &lan(), None)[0].server, None);
    }

    #[test]
    fn in_game_or_launcher_has_no_server() {
        let e = [
            entry("192.0.2.25", "Lyn", Some("ut99"), Some("play"), None),
            entry("192.0.2.26", "Nima", None, None, None),
        ];
        let p = resolve(&e, &lan(), "192.0.2.26".parse().ok());
        assert_eq!((p[0].server.clone(), p[0].you), (None, false));
        assert_eq!((p[1].server.clone(), p[1].you), (None, true));
    }

    #[test]
    fn idle_keeps_the_player_and_drops_the_game() {
        let b = Beat {
            seat: "Seat 01".into(),
            nickname: "Kaptein".into(),
            game: Some("ut2004".into()),
            mode: Some("join".into()),
            address: Some("192.0.2.10:7800".into()),
        };
        let i = b.idle();
        assert_eq!(
            (i.nickname.as_str(), i.game, i.mode, i.address),
            ("Kaptein", None, None, None)
        );
    }

    #[test]
    fn report_posts_json_and_reads_the_list() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            // Headers, then exactly Content-Length bytes of body.
            let complete = |t: &str| {
                t.split_once("\r\n\r\n").is_some_and(|(head, body)| {
                    let n: usize = head
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length: "))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    body.len() >= n
                })
            };
            let mut text = String::new();
            let mut buf = [0u8; 1024];
            while !complete(&text) {
                let n = s.read(&mut buf).unwrap();
                text.push_str(std::str::from_utf8(&buf[..n]).unwrap());
            }
            assert!(text.starts_with("POST /api/presence HTTP/1.0\r\n"));
            assert!(text.contains("Content-Type: application/json\r\n"));
            assert!(
                text.ends_with(r#""nickname":"Kaptein","game":null,"mode":null,"address":null}"#)
            );
            s.write_all(
                b"HTTP/1.0 200 OK\r\n\r\n[{\"ip\":\"127.0.0.1\",\"seat\":\"Seat 01\",\"nickname\":\"Kaptein\",\"game\":null,\"mode\":null,\"address\":null,\"seconds\":0}]",
            )
            .unwrap();
        });
        let beat = Beat {
            seat: "Seat 01".into(),
            nickname: "Kaptein".into(),
            game: None,
            mode: None,
            address: None,
        };
        let list = report(&base, &beat).unwrap();
        server.join().unwrap();
        let want = Entry {
            seconds: 0,
            ..entry("127.0.0.1", "Kaptein", None, None, None)
        };
        assert_eq!(list, vec![want]);
    }
}
