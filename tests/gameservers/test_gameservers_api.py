"""Unit tests for roles/server_gameservers/files/gameservers-api.
Run: python3 -m unittest discover -s tests/gameservers"""

import importlib.machinery
import importlib.util
import json
import os
import socket
import struct
import threading
import unittest
import urllib.request
from http.server import ThreadingHTTPServer
from types import SimpleNamespace

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "..", "ansible", "roles", "server_gameservers", "files", "gameservers-api")
loader = importlib.machinery.SourceFileLoader("gameservers_api", SRC)
spec = importlib.util.spec_from_loader("gameservers_api", loader)
api = importlib.util.module_from_spec(spec)
loader.exec_module(api)


def ut2_str(text):
    raw = text.encode("latin-1") + b"\x00"
    return bytes([len(raw)]) + raw


def ut2004_reply(name, map_name, players, max_players):
    return (
        b"\x80\x00\x00\x00\x00"
        + struct.pack("<i", 1)
        + ut2_str("")
        + struct.pack("<ii", 7800, 7801)
        + ut2_str(name)
        + ut2_str(map_name)
        + ut2_str("xDeathMatch")
        + struct.pack("<ii", players, max_players)
        + struct.pack("<ii", 0, 0)
    )


class Parsers(unittest.TestCase):
    def test_gamespy(self):
        blob = b"\\hostname\\Nimtech\\mapname\\MH-NaliVillage][\\numplayers\\3\\maxplayers\\12\\queryid\\1.1\\final\\"
        self.assertEqual(api.parse_gamespy(blob), {"players": 3, "max_players": 12, "map": "MH-NaliVillage]["})

    def test_ut2004(self):
        data = ut2004_reply("Nimtech InstaGib", "DM-Rankin", 2, 16)
        self.assertEqual(api.parse_ut2004(data), {"players": 2, "max_players": 16, "map": "DM-Rankin"})

    def test_quake3(self):
        data = b"\xff\xff\xff\xffstatusResponse\n\\sv_maxclients\\16\\mapname\\q3dm17\n0 50 \"Kaptein\"\n3 40 \"Lyn\"\n"
        self.assertEqual(api.parse_quake3(data), {"players": 2, "max_players": 16, "map": "q3dm17"})

    def test_query_over_udp(self):
        # A fake UT2004 server on a local port answers one query.
        srv = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        srv.bind(("127.0.0.1", 0))
        port = srv.getsockname()[1]

        def answer():
            data, addr = srv.recvfrom(100)
            if data == b"\x80\x00\x00\x00\x00":
                srv.sendto(ut2004_reply("x", "ONS-Torlan", 0, 32), addr)

        t = threading.Thread(target=answer)
        t.start()
        info = api.query("ut2004", "127.0.0.1", port, timeout=1.0)
        t.join()
        srv.close()
        self.assertEqual(info, {"players": 0, "max_players": 32, "map": "ONS-Torlan"})

    def test_no_answer_is_none(self):
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
        self.assertIsNone(api.query("quake3", "127.0.0.1", port, timeout=0.2))
        s.close()


CONFIG = {
    "address": "192.0.2.10",
    "listen": "127.0.0.1",
    "port": 0,
    "idle_minutes": 15,
    "max_running": 1,
    "presets": [
        {"id": "a", "name": "A", "game": "ut2004", "port": 7800, "query": {"type": "ut2004", "port": 7801}},
        {"id": "b", "name": "B", "game": "ut2004", "port": 7820, "query": {"type": "ut2004", "port": 7821}},
        {"id": "keep", "name": "K", "game": "ut99", "port": 7777, "query": {"type": "ut99", "port": 7778}, "always_on": True},
    ],
}


class FakeSystem:
    def __init__(self):
        self.active = {"keep"}
        self.calls = []
        self.players = {}
        self.now = 0.0

    def systemctl(self, *args):
        self.calls.append(args)
        name = args[-1][len("gs-"):-len(".service")]
        if args[0] == "is-active":
            return SimpleNamespace(returncode=0 if name in self.active else 3, stderr="")
        if args[0] == "start":
            self.active.add(name)
        if args[0] == "stop":
            self.active.discard(name)
        return SimpleNamespace(returncode=0, stderr="")

    def query(self, kind, host, port, timeout=1.0):
        for p in CONFIG["presets"]:
            if p["query"]["port"] == port:
                n = self.players.get(p["id"], 0)
                return None if n is None else {"players": n, "max_players": 16, "map": "DM-Rankin"}

    def clock(self):
        return self.now


def manager():
    fake = FakeSystem()
    return api.Manager(CONFIG, fake.systemctl, fake.query, fake.clock), fake


class Lifecycle(unittest.TestCase):
    def test_start_runs_the_unit_only(self):
        m, fake = manager()
        self.assertEqual(m.start("a")[0], 202)
        self.assertIn(("start", "--no-block", "gs-a.service"), fake.calls)

    def test_limit_counts_on_demand_servers_only(self):
        m, fake = manager()
        m.start("a")
        code, body = m.start("b")
        self.assertEqual(code, 409)
        self.assertNotIn("b", fake.active)

    def test_idle_server_stops_after_idle_minutes(self):
        m, fake = manager()
        m.start("a")
        fake.now = 14 * 60
        self.assertEqual(m.reap(), [])
        fake.now = 15 * 60
        self.assertEqual(m.reap(), ["a"])
        self.assertNotIn("a", fake.active)

    def test_players_keep_it_running(self):
        m, fake = manager()
        m.start("a")
        fake.players["a"] = 2
        fake.now = 60 * 60
        self.assertEqual(m.reap(), [])
        fake.players["a"] = 0
        fake.now += 14 * 60
        self.assertEqual(m.reap(), [])
        fake.now += 60
        self.assertEqual(m.reap(), ["a"])

    def test_no_answer_never_stops_a_game(self):
        m, fake = manager()
        m.start("a")
        fake.players["a"] = None
        fake.now = 10 * 60 * 60
        self.assertEqual(m.reap(), [])

    def test_always_on_is_never_reaped(self):
        m, fake = manager()
        fake.now = 10 * 60 * 60
        self.assertEqual(m.reap(), [])
        self.assertIn("keep", fake.active)

    def test_list_reports_address_and_players(self):
        m, fake = manager()
        m.start("a")
        fake.players["a"] = 3
        a = next(s for s in m.list() if s["id"] == "a")
        self.assertEqual(a["state"], "running")
        self.assertEqual(a["address"], "192.0.2.10:7800")
        self.assertEqual(a["players"], 3)
        b = next(s for s in m.list() if s["id"] == "b")
        self.assertEqual((b["state"], b["players"]), ("stopped", None))


class Http(unittest.TestCase):
    def setUp(self):
        self.m, self.fake = manager()
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), api.make_handler(self.m))
        self.base = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def tearDown(self):
        self.httpd.shutdown()
        self.httpd.server_close()

    def call(self, method, path):
        req = urllib.request.Request(self.base + path, method=method)
        try:
            with urllib.request.urlopen(req, timeout=5) as r:
                return r.status, json.loads(r.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read())

    def test_list_and_start(self):
        code, body = self.call("GET", "/api/servers")
        self.assertEqual(code, 200)
        self.assertEqual([s["id"] for s in body], ["a", "b", "keep"])
        self.assertEqual(self.call("POST", "/api/servers/a/start")[0], 202)

    def test_unknown_or_odd_ids_are_refused(self):
        for path in ["/api/servers/nope/start", "/api/servers/..%2Fx/start", "/api/servers/a;reboot/start"]:
            self.assertEqual(self.call("POST", path)[0], 404, path)
        self.assertEqual([c for c in self.fake.calls if c[0] == "start"], [])


if __name__ == "__main__":
    unittest.main()
