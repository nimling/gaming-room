//! Games running on the gaming VLAN, for the launcher's "On the LAN" lists.
//!
//! Two sources, merged per game id:
//! - The gaming server's control service (`GET <gameservers>/api/servers`):
//!   its on-demand presets. Only running ones are listed.
//! - LAN queries, the way each game's own LAN browser finds servers: seats
//!   that host a match, and servers the control service does not run (the
//!   always-on UT99 server).
//!
//! One `scan` takes about half a second and a few dozen small packets. The
//! screen calls it every few seconds while it is awake; nothing runs while a
//! game or another window has the screen.

use crate::config::{Config, Host};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// One game on the LAN, as a card in "On the LAN now".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Game {
    /// Seat or server name from the settings, else the IP address.
    pub host: String,
    pub name: String,
    /// `ip:port` to join.
    pub address: String,
    pub map: String,
    #[serde(rename = "n")]
    pub players: u32,
    /// 0 when the server did not say.
    #[serde(rename = "max")]
    pub max_players: u32,
    /// Round trip of the LAN query; `None` for a server only the control
    /// service reported.
    #[serde(rename = "ping")]
    pub ping_ms: Option<u32>,
    /// Hosted by this seat.
    pub mine: bool,
}

/// A preset from the control service. Stopped ones are kept for a later
/// Start button; the lists show running ones only.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// The game's manifest id, for example `ut2004`.
    pub game: String,
    /// `ip:port` to join.
    pub address: String,
    #[serde(default)]
    pub always_on: bool,
    /// running, starting, failing, stopping, failed or stopped.
    pub state: String,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub map: Option<String>,
}

/// What a server said about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub name: String,
    pub map: String,
    pub players: u32,
    pub max_players: u32,
    /// Game port, when the reply carries it.
    pub port: Option<u16>,
}

/// A server that answered a LAN query.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub game: &'static str,
    pub ip: IpAddr,
    pub info: Info,
    pub ping: Duration,
}

// --- Parsers ------------------------------------------------------------------

/// Text in the Latin-1 the old engines use.
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn pairs(fields: &str) -> HashMap<&str, &str> {
    let mut parts = fields.split('\\');
    parts.next(); // text before the first backslash
    let mut out = HashMap::new();
    while let (Some(k), Some(v)) = (parts.next(), parts.next()) {
        out.entry(k).or_insert(v);
    }
    out
}

/// UT99's GameSpy `\status\` reply: `\key\value` pairs, ending in `\final\`.
pub fn parse_gamespy(blob: &[u8]) -> Option<Info> {
    let text = latin1(blob);
    let f = pairs(&text);
    Some(Info {
        name: f.get("hostname").copied().unwrap_or("").to_string(),
        map: f.get("mapname").copied().unwrap_or("").to_string(),
        players: f.get("numplayers")?.trim().parse().ok()?,
        max_players: f
            .get("maxplayers")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
        port: f.get("hostport").and_then(|v| v.trim().parse().ok()),
    })
}

/// UT99's LAN beacon answer to `REPORTQUERY`: `ut <query port>`.
pub fn parse_beacon(data: &[u8]) -> Option<u16> {
    let text = latin1(data);
    let mut words = text.trim_end_matches(['\0', '\n', ' ']).split(' ');
    let product = words.next()?;
    if product.is_empty() || product.starts_with('\\') {
        return None;
    }
    words.next()?.parse().ok()
}

/// A UT2004 query string: a length byte (counting the trailing NUL), then
/// the text. A length with the top bit set means UCS-2 text of
/// `length & 0x7f` characters.
fn ut2_string(data: &[u8], pos: &mut usize) -> Option<String> {
    let n = *data.get(*pos)? as usize;
    *pos += 1;
    let text = if n & 0x80 != 0 {
        let raw = data.get(*pos..*pos + (n & 0x7f) * 2)?;
        *pos += raw.len();
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        let raw = data.get(*pos..*pos + n)?;
        *pos += n;
        latin1(raw)
    };
    Some(text.trim_end_matches('\0').to_string())
}

fn ut2_int(data: &[u8], pos: &mut usize) -> Option<i32> {
    let raw = data.get(*pos..*pos + 4)?;
    *pos += 4;
    Some(i32::from_le_bytes(raw.try_into().ok()?))
}

/// UT2004's own server-info reply (to the request `80 00 00 00 00`).
pub fn parse_ut2004(data: &[u8]) -> Option<Info> {
    if data.len() <= 5 || data[..5] != [0x80, 0, 0, 0, 0] {
        return None;
    }
    let mut pos = 5;
    ut2_int(data, &mut pos)?; // server id
    ut2_string(data, &mut pos)?; // ip
    let port = ut2_int(data, &mut pos)?;
    ut2_int(data, &mut pos)?; // query port
    let name = ut2_string(data, &mut pos)?;
    let map = ut2_string(data, &mut pos)?;
    ut2_string(data, &mut pos)?; // game type
    let players = ut2_int(data, &mut pos)?;
    let max_players = ut2_int(data, &mut pos)?;
    Some(Info {
        name,
        map,
        players: u32::try_from(players).ok()?,
        max_players: u32::try_from(max_players).unwrap_or(0),
        port: u16::try_from(port).ok().filter(|p| *p > 0),
    })
}

/// Quake 3 colour codes (`^1Red`) out of a name.
fn strip_colours(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// A Quake 3 engine `getstatus` reply (Quake 3, Jedi Academy): the info
/// string, then one line per player.
pub fn parse_quake3(data: &[u8]) -> Option<Info> {
    let text = latin1(data.strip_prefix(b"\xff\xff\xff\xff")?);
    let mut lines = text.split('\n');
    if lines.next()? != "statusResponse" {
        return None;
    }
    let info = lines.next().unwrap_or("");
    let f = pairs(info);
    Some(Info {
        name: strip_colours(f.get("sv_hostname").copied().unwrap_or("")),
        map: f.get("mapname").copied().unwrap_or("").to_string(),
        players: lines.filter(|l| !l.trim().is_empty()).count() as u32,
        max_players: f
            .get("sv_maxclients")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
        port: None,
    })
}

/// The control service's list.
pub fn parse_presets(json: &str) -> Result<Vec<Preset>> {
    serde_json::from_str(json).context("reading the game server list")
}

// --- Control service ------------------------------------------------------------

/// A plain HTTP/1.0 GET; enough for the control service on the LAN.
pub fn http_get(url: &str, timeout: Duration) -> Result<String> {
    let rest = url
        .strip_prefix("http://")
        .with_context(|| format!("{url}: only http:// is supported"))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let target = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let addr = target
        .to_socket_addrs()
        .with_context(|| format!("resolving {authority}"))?
        .next()
        .with_context(|| format!("no address for {authority}"))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("connecting to {authority}"))?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    // One write: write! on an unbuffered stream sends each piece as its own
    // packet, and a server that reads once sees half a request.
    let req = format!("GET /{path} HTTP/1.0\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes())?;
    let mut raw = Vec::new();
    s.take(1 << 20).read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .context("no end of the HTTP headers")?;
    let status = head.split(' ').nth(1).unwrap_or("");
    if status != "200" {
        bail!("{url}: HTTP {status}");
    }
    Ok(body.to_string())
}

fn presets(base: &str) -> Result<Vec<Preset>> {
    let url = format!("{}/api/servers", base.trim_end_matches('/'));
    parse_presets(&http_get(&url, Duration::from_secs(3))?)
}

// --- LAN queries ----------------------------------------------------------------

/// How long one scan listens for answers.
const WINDOW: Duration = Duration::from_millis(500);

const UT99_BEACON: &[u8] = b"REPORTQUERY";
const UT99_STATUS: &[u8] = b"\\status\\";
const UT2004_INFO: &[u8] = b"\x80\x00\x00\x00\x00";
const Q3_STATUS: &[u8] = b"\xff\xff\xff\xffgetstatus\n";

/// The ports each game's LAN browser asks.
const UT99_BEACON_PORTS: std::ops::RangeInclusive<u16> = 8777..=8786;
/// UT2004: the default query port (game port 7777 + 1) and the LAN ports
/// servers listen on (10777, or the next free one).
const UT2004_PORTS: [u16; 11] = [
    7778, 10777, 10778, 10779, 10780, 10781, 10782, 10783, 10784, 10785, 10786,
];
const QUAKE3_PORTS: std::ops::RangeInclusive<u16> = 27960..=27963;
const JKA_PORTS: std::ops::RangeInclusive<u16> = 29070..=29073;

/// The queries one scan sends: (where, what).
pub fn probes(to: Ipv4Addr) -> Vec<(SocketAddr, &'static [u8])> {
    let at = |p: u16| SocketAddr::from((to, p));
    let mut out = Vec::new();
    out.extend(UT99_BEACON_PORTS.map(|p| (at(p), UT99_BEACON)));
    out.extend(UT2004_PORTS.map(|p| (at(p), UT2004_INFO)));
    out.extend(QUAKE3_PORTS.chain(JKA_PORTS).map(|p| (at(p), Q3_STATUS)));
    out
}

/// Sends `probes` from `socket` and gathers the answers for `window`. A UT99
/// beacon answer names the query port; that port is then asked for status.
pub fn collect(socket: &UdpSocket, probes: &[(SocketAddr, &[u8])], window: Duration) -> Vec<Found> {
    let start = Instant::now();
    for (to, what) in probes {
        // A seat without a route (or a test container) just finds nothing.
        let _ = socket.send_to(what, to);
    }
    let mut status_sent: HashMap<SocketAddr, Instant> = HashMap::new();
    let mut gamespy: HashMap<SocketAddr, Vec<u8>> = HashMap::new();
    let mut found = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = start + window;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
            break;
        }
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(r) => r,
            Err(_) => break, // timeout: the window is over
        };
        let data = &buf[..n];
        let ping = |since: Instant| since.elapsed();
        if data.first() == Some(&b'\\') {
            let blob = gamespy.entry(from).or_default();
            blob.extend_from_slice(data);
            if contains(blob, b"\\final\\") {
                let blob = gamespy.remove(&from).unwrap_or_default();
                let sent = status_sent.get(&from).copied().unwrap_or(start);
                push_gamespy(&mut found, from, &blob, ping(sent));
            }
        } else if let Some(info) = parse_ut2004(data) {
            found.push(Found {
                game: "ut2004",
                ip: from.ip(),
                info,
                ping: ping(start),
            });
        } else if let Some(info) = parse_quake3(data) {
            let game = if JKA_PORTS.contains(&from.port()) {
                "jka"
            } else {
                "quake3"
            };
            let info = Info {
                port: Some(from.port()),
                ..info
            };
            found.push(Found {
                game,
                ip: from.ip(),
                info,
                ping: ping(start),
            });
        } else if let Some(port) = parse_beacon(data) {
            let to = SocketAddr::new(from.ip(), port);
            if !status_sent.contains_key(&to) && socket.send_to(UT99_STATUS, to).is_ok() {
                status_sent.insert(to, Instant::now());
            }
        }
    }
    // A reply that lost its last packet still says enough.
    for (from, blob) in gamespy {
        let sent = status_sent.get(&from).copied().unwrap_or(start);
        push_gamespy(&mut found, from, &blob, sent.elapsed());
    }
    found
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn push_gamespy(found: &mut Vec<Found>, from: SocketAddr, blob: &[u8], ping: Duration) {
    if let Some(mut info) = parse_gamespy(blob) {
        // Without hostport, the game port is the query port - 1.
        info.port = info.port.or(from.port().checked_sub(1));
        found.push(Found {
            game: "ut99",
            ip: from.ip(),
            info,
            ping,
        });
    }
}

/// Every game that answers a broadcast on the seat's network.
pub fn query_lan() -> Vec<Found> {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    if socket.set_broadcast(true).is_err() {
        return Vec::new();
    }
    collect(&socket, &probes(Ipv4Addr::BROADCAST), WINDOW)
}

// --- Merge ----------------------------------------------------------------------

fn ms(d: Duration) -> u32 {
    // Round up: a LAN answer in 0.3 ms shows as 1 ms, not 0.
    u32::try_from(d.as_micros().div_ceil(1000)).unwrap_or(u32::MAX)
}

/// Joins both sources into one list per game id. A server both sources
/// know is listed once, from the LAN answer (it has the ping). A server
/// that answered twice (two query ports) is listed once.
pub fn merge(
    presets: &[Preset],
    found: &[Found],
    own: Option<IpAddr>,
    hosts: &[Host],
) -> BTreeMap<String, Vec<Game>> {
    let host_of = |ip: &str| {
        hosts
            .iter()
            .find(|h| h.address == ip)
            .map_or_else(|| ip.to_string(), |h| h.name.clone())
    };
    let is_mine = |ip: IpAddr| ip.is_loopback() || Some(ip) == own;
    let mut out: BTreeMap<String, Vec<Game>> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for f in found {
        let Some(port) = f.info.port else { continue };
        let address = SocketAddr::new(f.ip, port).to_string();
        if !seen.insert(address.clone()) {
            continue;
        }
        let ip = f.ip.to_string();
        out.entry(f.game.to_string()).or_default().push(Game {
            host: host_of(&ip),
            name: if f.info.name.trim().is_empty() {
                format!("{}'s game", host_of(&ip))
            } else {
                f.info.name.trim().to_string()
            },
            address,
            map: f.info.map.clone(),
            players: f.info.players,
            max_players: f.info.max_players,
            ping_ms: Some(ms(f.ping)),
            mine: is_mine(f.ip),
        });
    }
    for p in presets.iter().filter(|p| p.state == "running") {
        if !seen.insert(p.address.clone()) {
            continue;
        }
        let ip = p
            .address
            .rsplit_once(':')
            .map_or(p.address.as_str(), |(h, _)| h);
        out.entry(p.game.clone()).or_default().push(Game {
            host: host_of(ip),
            name: p.name.clone(),
            address: p.address.clone(),
            map: p.map.clone().unwrap_or_default(),
            players: p.players.unwrap_or(0),
            max_players: p.max_players.unwrap_or(0),
            ping_ms: None,
            mine: ip.parse().is_ok_and(is_mine),
        });
    }
    out
}

/// Asks the control service and the LAN at the same time. Never fails: a
/// source that does not answer adds nothing.
pub fn scan(config: &Config) -> BTreeMap<String, Vec<Game>> {
    let (list, found) = std::thread::scope(|s| {
        let api = s.spawn(|| match config.gameservers.as_deref() {
            Some(base) => presets(base).unwrap_or_else(|e| {
                eprintln!("arcade: game server list: {e:#}");
                Vec::new()
            }),
            None => Vec::new(),
        });
        let found = query_lan();
        (api.join().unwrap_or_default(), found)
    });
    let own = own_peer(config).and_then(|p| crate::lan::own_address(&p));
    merge(&list, &found, own, &config.hosts)
}

/// A host on the gaming VLAN, to find this seat's own address by route.
fn own_peer(config: &Config) -> Option<String> {
    let from_url = config.gameservers.as_deref().and_then(|u| {
        let rest = u.strip_prefix("http://")?;
        let authority = rest.split('/').next()?;
        Some(authority.to_string())
    });
    from_url
        .or_else(|| config.voice.as_ref().map(|v| v.server.clone()))
        .or_else(|| config.hosts.first().map(|h| h.address.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte fixtures from tests/gameservers/test_gameservers_api.py.
    fn ut2_str(text: &str) -> Vec<u8> {
        let mut raw = text.as_bytes().to_vec();
        raw.push(0);
        let mut out = vec![raw.len() as u8];
        out.extend(raw);
        out
    }

    fn ut2004_reply(name: &str, map: &str, players: i32, max: i32) -> Vec<u8> {
        let mut d = b"\x80\x00\x00\x00\x00".to_vec();
        d.extend(1i32.to_le_bytes());
        d.extend(ut2_str(""));
        d.extend(7800i32.to_le_bytes());
        d.extend(7801i32.to_le_bytes());
        d.extend(ut2_str(name));
        d.extend(ut2_str(map));
        d.extend(ut2_str("xDeathMatch"));
        d.extend(players.to_le_bytes());
        d.extend(max.to_le_bytes());
        d.extend(0i32.to_le_bytes());
        d.extend(0i32.to_le_bytes());
        d
    }

    const GAMESPY: &[u8] = b"\\hostname\\Nimtech\\mapname\\MH-NaliVillage][\\numplayers\\3\\maxplayers\\12\\queryid\\1.1\\final\\";
    const QUAKE3: &[u8] = b"\xff\xff\xff\xffstatusResponse\n\\sv_maxclients\\16\\mapname\\q3dm17\n0 50 \"Kaptein\"\n3 40 \"Lyn\"\n";

    #[test]
    fn gamespy() {
        let i = parse_gamespy(GAMESPY).unwrap();
        assert_eq!((i.players, i.max_players), (3, 12));
        assert_eq!(i.map, "MH-NaliVillage][");
        assert_eq!(i.name, "Nimtech");
        assert_eq!(i.port, None);
        let with_port = parse_gamespy(b"\\hostname\\X\\hostport\\7790\\numplayers\\0\\final\\");
        assert_eq!(with_port.unwrap().port, Some(7790));
        assert!(parse_gamespy(b"\\nothing\\here\\").is_none());
    }

    #[test]
    fn ut2004() {
        let i = parse_ut2004(&ut2004_reply("Nimtech InstaGib", "DM-Rankin", 2, 16)).unwrap();
        assert_eq!(
            i,
            Info {
                name: "Nimtech InstaGib".into(),
                map: "DM-Rankin".into(),
                players: 2,
                max_players: 16,
                port: Some(7800),
            }
        );
        // Cut short, or not a server-info reply.
        let full = ut2004_reply("A", "B", 1, 2);
        assert!(parse_ut2004(&full[..20]).is_none());
        assert!(parse_ut2004(b"\x80\x00\x00\x00\x00").is_none());
        assert!(parse_ut2004(QUAKE3).is_none());
    }

    #[test]
    fn ut2004_ucs2_name() {
        let mut d = ut2004_reply("", "DM-Rankin", 0, 8);
        // Swap the empty name (2 bytes: len 1, NUL) for "Ø" in UCS-2.
        let name_at = 5 + 4 + 1 + 1 + 8;
        d.splice(name_at..name_at + 2, [0x82, 0xd8, 0x00, 0x00, 0x00]);
        assert_eq!(parse_ut2004(&d).unwrap().name, "Ø");
    }

    #[test]
    fn quake3() {
        let i = parse_quake3(QUAKE3).unwrap();
        assert_eq!((i.players, i.max_players), (2, 16));
        assert_eq!(i.map, "q3dm17");
        let named = parse_quake3(b"\xff\xff\xff\xffstatusResponse\n\\sv_hostname\\^1Red ^7Room\n");
        assert_eq!(named.unwrap().name, "Red Room");
        assert!(parse_quake3(b"\xff\xff\xff\xffinfoResponse\n").is_none());
    }

    #[test]
    fn beacon() {
        assert_eq!(parse_beacon(b"ut 7778"), Some(7778));
        assert_eq!(parse_beacon(b"ut 7791\0"), Some(7791));
        assert_eq!(parse_beacon(b"ut"), None);
        assert_eq!(parse_beacon(GAMESPY), None);
    }

    #[test]
    fn presets_from_the_control_service() {
        let json = r#"[
          {"id": "ut2004-instagib", "name": "UT2004 InstaGib", "game": "ut2004",
           "address": "192.0.2.10:7800", "always_on": false, "state": "running",
           "players": 2, "max_players": 16, "map": "DM-Rankin"},
          {"id": "ut99-monsterhunt", "name": "UT99 Monster Hunt", "game": "ut99",
           "address": "192.0.2.10:7790", "always_on": false, "state": "stopped",
           "players": null, "max_players": null, "map": null}
        ]"#;
        let list = parse_presets(json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].players, None);
        assert!(parse_presets("{}").is_err());
    }

    fn preset(game: &str, address: &str, state: &str) -> Preset {
        Preset {
            id: format!("{game}-x"),
            name: format!("Server {game}"),
            game: game.into(),
            address: address.into(),
            always_on: false,
            state: state.into(),
            players: Some(1),
            max_players: Some(16),
            map: Some("DM-Rankin".into()),
        }
    }

    fn found(game: &'static str, ip: &str, port: u16) -> Found {
        Found {
            game,
            ip: ip.parse().unwrap(),
            info: Info {
                name: "LAN name".into(),
                map: "CTF-Face".into(),
                players: 3,
                max_players: 12,
                port: Some(port),
            },
            ping: Duration::from_micros(300),
        }
    }

    fn hosts() -> Vec<Host> {
        vec![
            Host {
                name: "gaming-server".into(),
                address: "192.0.2.10".into(),
            },
            Host {
                name: "seat-02".into(),
                address: "192.0.2.22".into(),
            },
        ]
    }

    #[test]
    fn merge_lists_running_presets_and_lan_games_once() {
        let presets = [
            preset("ut2004", "192.0.2.10:7800", "running"),
            preset("ut2004", "192.0.2.10:7820", "running"),
            preset("ut99", "192.0.2.10:7790", "stopped"),
        ];
        let found = [
            // The same server as the first preset, over the LAN.
            found("ut2004", "192.0.2.10", 7800),
            // Answered on both its query port and the LAN port.
            found("ut2004", "192.0.2.22", 7777),
            found("ut2004", "192.0.2.22", 7777),
            // This seat's own game.
            found("ut99", "192.0.2.21", 7777),
        ];
        let own = Some("192.0.2.21".parse().unwrap());
        let m = merge(&presets, &found, own, &hosts());
        let ut2004 = &m["ut2004"];
        assert_eq!(ut2004.len(), 3);
        assert_eq!(ut2004[0].address, "192.0.2.10:7800");
        assert_eq!(ut2004[0].name, "LAN name"); // the LAN answer wins
        assert_eq!(ut2004[0].ping_ms, Some(1));
        assert_eq!(ut2004[0].host, "gaming-server");
        assert_eq!(ut2004[1].host, "seat-02");
        assert_eq!(ut2004[2].address, "192.0.2.10:7820");
        assert_eq!(ut2004[2].ping_ms, None);
        assert!(ut2004.iter().all(|g| !g.mine));
        // Stopped presets stay out; this seat's game is marked.
        let ut99 = &m["ut99"];
        assert_eq!(ut99.len(), 1);
        assert!(ut99[0].mine);
        assert_eq!(ut99[0].host, "192.0.2.21");
    }

    #[test]
    fn merge_handles_a_preset_without_query_answer() {
        let mut p = preset("ut99", "192.0.2.10:7790", "running");
        p.players = None;
        p.max_players = None;
        p.map = None;
        let m = merge(&[p], &[], None, &[]);
        let g = &m["ut99"][0];
        assert_eq!((g.players, g.max_players, g.map.as_str()), (0, 0, ""));
        assert_eq!(g.host, "192.0.2.10");
    }

    #[test]
    fn card_fields_match_the_screen() {
        let m = merge(&[], &[found("ut99", "192.0.2.22", 7777)], None, &hosts());
        let v = serde_json::to_value(&m).unwrap();
        let card = &v["ut99"][0];
        for key in ["host", "name", "address", "map", "n", "max", "ping", "mine"] {
            assert!(card.get(key).is_some(), "missing {key}");
        }
    }

    /// A fake UT99 server (beacon and GameSpy) and a fake UT2004 server on
    /// local ports answer one scan.
    #[test]
    fn collect_over_udp() {
        let beacon = UdpSocket::bind("127.0.0.1:0").unwrap();
        let query = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ut2 = UdpSocket::bind("127.0.0.1:0").unwrap();
        let probes: Vec<(SocketAddr, &[u8])> = vec![
            (beacon.local_addr().unwrap(), UT99_BEACON),
            (ut2.local_addr().unwrap(), UT2004_INFO),
        ];
        let qport = query.local_addr().unwrap().port();
        let servers = [
            std::thread::spawn(move || {
                let mut b = [0u8; 64];
                let (n, from) = beacon.recv_from(&mut b).unwrap();
                assert_eq!(&b[..n], UT99_BEACON);
                beacon
                    .send_to(format!("ut {qport}").as_bytes(), from)
                    .unwrap();
            }),
            std::thread::spawn(move || {
                let mut b = [0u8; 64];
                let (n, from) = query.recv_from(&mut b).unwrap();
                assert_eq!(&b[..n], UT99_STATUS);
                // Two packets, like a real GameSpy reply.
                query
                    .send_to(b"\\hostname\\Seat\\hostport\\7777\\queryid\\1.1", from)
                    .unwrap();
                query
                    .send_to(
                        b"\\mapname\\DM-Deck16][\\numplayers\\1\\maxplayers\\16\\queryid\\1.2\\final\\",
                        from,
                    )
                    .unwrap();
            }),
            std::thread::spawn(move || {
                let mut b = [0u8; 64];
                let (n, from) = ut2.recv_from(&mut b).unwrap();
                assert_eq!(&b[..n], UT2004_INFO);
                ut2.send_to(&ut2004_reply("Room", "DM-Rankin", 2, 16), from)
                    .unwrap();
            }),
        ];
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut found = collect(&socket, &probes, Duration::from_millis(500));
        for s in servers {
            s.join().unwrap();
        }
        found.sort_by_key(|f| f.game);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].game, "ut2004");
        assert_eq!(found[0].info.port, Some(7800));
        assert_eq!(found[1].game, "ut99");
        assert_eq!(found[1].info.name, "Seat");
        assert_eq!(found[1].info.map, "DM-Deck16][");
        assert_eq!(found[1].info.port, Some(7777));
        assert_eq!((found[1].info.players, found[1].info.max_players), (1, 16));
    }

    #[test]
    fn probes_cover_each_game() {
        let p = probes(Ipv4Addr::BROADCAST);
        let port = |what: &[u8]| -> Vec<u16> {
            p.iter()
                .filter(|(_, w)| *w == what)
                .map(|(a, _)| a.port())
                .collect()
        };
        assert_eq!(port(UT99_BEACON), (8777..=8786).collect::<Vec<_>>());
        assert!(port(UT2004_INFO).contains(&7778));
        assert!(port(UT2004_INFO).contains(&10777));
        assert!(port(Q3_STATUS).contains(&27960));
        assert!(port(Q3_STATUS).contains(&29070));
    }

    #[test]
    fn http_get_reads_the_body() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let t = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            // Read the whole request head, however the client split it.
            let mut req = Vec::new();
            let mut buf = [0u8; 256];
            while !req.ends_with(b"\r\n\r\n") {
                let n = s.read(&mut buf).unwrap();
                assert!(n > 0, "client closed before the end of the request");
                req.extend_from_slice(&buf[..n]);
            }
            assert!(req.starts_with(b"GET /api/servers HTTP/1.0\r\n"));
            s.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n[]")
                .unwrap();
        });
        let base = format!("http://{addr}/");
        assert!(presets(&base).unwrap().is_empty());
        t.join().unwrap();
        assert!(http_get("https://example.invalid/", Duration::from_millis(10)).is_err());
    }
}
