"""Unit tests for ansible/files/steam-cafe (the Steam café library).
Run: python3 -m unittest discover -s tests/steamcafe
The copy tests use the local rsync and are skipped without it."""

import contextlib
import importlib.machinery
import importlib.util
import io
import os
import shutil
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "..", "ansible", "files", "steam-cafe")
loader = importlib.machinery.SourceFileLoader("steam_cafe", SRC)
spec = importlib.util.spec_from_loader("steam_cafe", loader)
sc = importlib.util.module_from_spec(spec)
loader.exec_module(sc)

HAVE_RSYNC = shutil.which("rsync") is not None
GNU_RSYNC = HAVE_RSYNC and "rsync  version 3" in subprocess.run(["rsync", "--version"], capture_output=True, text=True).stdout


def acf(appid, buildid, state=4, installdir=None, name="Game", size=1000):
    installdir = installdir if installdir is not None else f"Game {appid}"
    return f'''"AppState"
{{
\t"appid"\t\t"{appid}"
\t"Universe"\t\t"1"
\t"name"\t\t"{name}"
\t"StateFlags"\t\t"{state}"
\t"installdir"\t\t"{installdir}"
\t"LastOwner"\t\t"76561190000000000"
\t"SizeOnDisk"\t\t"{size}"
\t"buildid"\t\t"{buildid}"
\t"AutoUpdateBehavior"\t\t"0"
\t"InstalledDepots"
\t{{
\t\t"{appid + 1}"
\t\t{{
\t\t\t"manifest"\t\t"123456789"
\t\t\t"size"\t\t"{size}"
\t\t}}
\t}}
\t"UserConfig"
\t{{
\t\t"language"\t\t"english"
\t}}
}}
'''


def install(library, appid, buildid, state=4, files=None, installdir=None):
    """A Steam install of one app in library: manifest plus common/<dir>."""
    installdir = installdir if installdir is not None else f"Game {appid}"
    os.makedirs(os.path.join(library, "common", installdir), exist_ok=True)
    for rel, content in (files or {"game.bin": f"build {buildid}"}).items():
        path = os.path.join(library, "common", installdir, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
        # Each build has its own file times, as after a real Steam update
        # (rsync's quick check compares size and time).
        os.utime(path, (1_700_000_000 + buildid, 1_700_000_000 + buildid))
    with open(os.path.join(library, sc.manifest_name(appid)), "w") as f:
        f.write(acf(appid, buildid, state, installdir))


def read(path):
    with open(path) as f:
        return f.read()


def quiet(fn, *a, **kw):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        rc = fn(*a, **kw)
    return rc, out.getvalue()


class ManifestTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.dir)

    def test_parses_a_real_layout(self):
        data = sc.parse_vdf(acf(550, 21000000, installdir="Left 4 Dead 2"))
        state = data["appstate"]
        self.assertEqual(state["installdir"], "Left 4 Dead 2")
        self.assertEqual(state["stateflags"], "4")
        self.assertEqual(state["installeddepots"]["551"]["manifest"], "123456789")

    def test_escaped_quotes_and_comments(self):
        data = sc.parse_vdf('// note\n"A" { "k" "say \\"hi\\"" }')
        self.assertEqual(data["a"]["k"], 'say "hi"')

    def test_broken_files_are_none(self):
        for text in ('"AppState" {', '"AppState" { "appid" "x" }', "", '"AppState" { "appid" "5" } }'):
            path = os.path.join(self.dir, "m.acf")
            with open(path, "w") as f:
                f.write(text)
            self.assertIsNone(sc.read_manifest(path), text)
        self.assertIsNone(sc.read_manifest(os.path.join(self.dir, "missing.acf")))

    def test_read_manifest_fields(self):
        install(self.dir, 945360, 77, installdir="Among Us")
        m = sc.read_manifest(os.path.join(self.dir, "appmanifest_945360.acf"))
        self.assertEqual((m.appid, m.buildid, m.state, m.installdir, m.size), (945360, 77, 4, "Among Us", 1000))

    def test_only_fully_installed_copies_count(self):
        def m(state=4, installdir="Forts", appid=410900, buildid=5):
            return sc.Manifest(appid, buildid, state, installdir)

        self.assertTrue(sc.complete(m(), 410900))
        for state in (2, 6, 1026, 0):  # update required, updating, ...
            self.assertFalse(sc.complete(m(state=state), 410900), state)
        self.assertFalse(sc.complete(m(appid=1), 410900))
        self.assertFalse(sc.complete(None, 410900))
        self.assertFalse(sc.complete(m(buildid=-1), 410900))

    def test_installdir_never_leaves_common(self):
        for bad in ("", ".", "..", "a/b", "../etc", "a\\b", "x\ny"):
            self.assertFalse(sc.safe_installdir(bad), repr(bad))
        for good in ("Left 4 Dead 2", "SteamLinuxRuntime_sniper", "Proton - Experimental"):
            self.assertTrue(sc.safe_installdir(good), good)

    def test_status_line(self):
        self.assertEqual(sc.parse_status("steam=running game=stopped\n"), {"steam": "running", "game": "stopped"})
        self.assertIsNone(sc.parse_status(""))
        self.assertIsNone(sc.parse_status("refused"))

    def test_steam_running_from_proc(self):
        proc = os.path.join(self.dir, "proc")
        for pid, comm in (("1", "systemd"), ("42", "arcade-app"), ("self", "x")):
            os.makedirs(os.path.join(proc, pid))
            with open(os.path.join(proc, pid, "comm"), "w") as f:
                f.write(comm + "\n")
        self.assertFalse(sc.steam_running(proc))
        os.makedirs(os.path.join(proc, "99"))
        with open(os.path.join(proc, "99", "comm"), "w") as f:
            f.write("steamwebhelper\n")
        self.assertTrue(sc.steam_running(proc))
        self.assertFalse(sc.steam_running(os.path.join(self.dir, "nope")))


class RsyncErrorTest(unittest.TestCase):
    def test_only_unreadable_files(self):
        err = (
            'rsync: [sender] send_files failed to open "/x/ServerBrowser.vdf": Permission denied (13)\n'
            'rsync: [sender] opendir "/x/var" failed: Permission denied (13)\n'
            "IO error encountered -- skipping file deletion\n"
            "rsync error: some files/attrs were not transferred (see previous errors) (code 23) at main.c(1338)\n"
        )
        self.assertEqual(sc.unreadable_only(err), 2)

    def test_other_errors_count(self):
        err = 'rsync: [generator] failed to set permissions on "/d": Operation not permitted (1)\n'
        self.assertIsNone(sc.unreadable_only(err))
        self.assertIsNone(sc.unreadable_only(""))


class DecisionTest(unittest.TestCase):
    def M(self, appid, buildid, state=4):
        return sc.Manifest(appid, buildid, state, f"Game {appid}")

    def test_newest_complete_copy_wins(self):
        found = {
            "seat-01": {550: self.M(550, 10)},
            "seat-02": {550: self.M(550, 30, state=6)},  # newer but not finished
            "seat-03": {550: self.M(550, 20)},
            "seat-04": {},
        }
        seat, m = sc.pick_newest(550, found)
        self.assertEqual((seat, m.buildid), ("seat-03", 20))

    def test_tie_goes_to_the_first_seat(self):
        found = {"seat-01": {550: self.M(550, 20)}, "seat-02": {550: self.M(550, 20)}}
        self.assertEqual(sc.pick_newest(550, found)[0], "seat-01")

    def test_no_copy(self):
        self.assertEqual(sc.pick_newest(550, {"seat-01": {}}), (None, None))

    def test_pull_plan(self):
        server = {1: self.M(1, 10), 2: self.M(2, 10), 3: self.M(3, 10), 4: self.M(4, 10), 5: self.M(5, 10, state=6), 9: self.M(9, 99)}
        local = {1: self.M(1, 10), 2: self.M(2, 5), 3: self.M(3, 20), 5: self.M(5, 1), 6: self.M(6, 1)}
        # 4: missing here. 7: listed, nowhere. 9: on the server but not listed.
        local[8] = self.M(8, 3, state=1026)
        server[8] = self.M(8, 3)
        todo = [t[0] for t in sc.plan_pull([1, 2, 3, 4, 5, 6, 7, 8], server, local)]
        # 1 equal, 3 seat newer, 5 server unfinished, 6 server lacks it: no copy.
        # 8: same build but the seat's copy is unfinished: repaired.
        self.assertEqual(todo, [2, 4, 8])

    def test_filters_fetch_only_listed_manifests(self):
        self.assertEqual(
            sc.manifest_filters([550, 228980]),
            ["--include=appmanifest_550.acf", "--include=appmanifest_228980.acf", "--exclude=*"],
        )


@unittest.skipUnless(HAVE_RSYNC, "needs rsync")
class PullTest(unittest.TestCase):
    """A seat copying from a stand-in share (a local folder)."""

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.share = os.path.join(self.dir, "share", "steamapps")
        self.lib = os.path.join(self.dir, "seat", "steamapps")
        self.proc = os.path.join(self.dir, "proc")
        self.marker = os.path.join(self.dir, "session-used")
        os.makedirs(self.share)
        os.makedirs(self.lib)
        os.makedirs(self.proc)
        self.apps = [{"appid": 100, "name": "Listed"}, {"appid": 300, "name": "Not on the server"}]

    def tearDown(self):
        shutil.rmtree(self.dir)

    def pull(self, **kw):
        p = sc.Puller(self.share, self.lib, self.apps, marker=self.marker, proc=self.proc, reserve_gib=0, **kw)
        return quiet(p.run)

    def steam(self, on):
        path = os.path.join(self.proc, "77")
        if on:
            os.makedirs(path, exist_ok=True)
            with open(os.path.join(path, "comm"), "w") as f:
                f.write("steam\n")
        else:
            shutil.rmtree(path, ignore_errors=True)

    def game(self, rel):
        return os.path.join(self.lib, "common", "Game 100", rel)

    def test_copy_rerun_lower_and_unlisted(self):
        install(self.share, 100, 20, files={"game.bin": "v20", "data/a.pak": "a"})
        install(self.share, 200, 5)  # on the share, not listed
        install(self.lib, 400, 1, files={"mine.txt": "keep"})  # local, not listed
        rc, out = self.pull()
        self.assertEqual(rc, 0, out)
        self.assertIn("100 Listed: copied build 20 (new on this seat)", out)
        self.assertIn("300 Not on the server: not on the server", out)
        self.assertIn("changed=1", out)
        self.assertEqual(read(self.game("data/a.pak")), "a")
        self.assertEqual(sc.read_manifest(os.path.join(self.lib, "appmanifest_100.acf")).buildid, 20)
        self.assertFalse(os.path.exists(os.path.join(self.lib, "appmanifest_200.acf")))
        self.assertEqual(read(os.path.join(self.lib, "common", "Game 400", "mine.txt")), "keep")

        # Again: nothing to do.
        rc, out = self.pull()
        self.assertIn("100 Listed: up to date (build 20)", out)
        self.assertIn("changed=0", out)

        # A lower build on the server changes nothing.
        install(self.share, 100, 15, files={"game.bin": "v15"})
        rc, out = self.pull()
        self.assertIn("changed=0", out)
        self.assertEqual(read(self.game("game.bin")), "v20")

        # A newer build replaces the folder, removing files it no longer has.
        shutil.rmtree(os.path.join(self.share, "common", "Game 100"))
        install(self.share, 100, 30, files={"game.bin": "v30"})
        rc, out = self.pull()
        self.assertIn("copied build 30 (was 20)", out)
        self.assertEqual(read(self.game("game.bin")), "v30")
        self.assertFalse(os.path.exists(self.game("data/a.pak")))
        self.assertEqual(read(os.path.join(self.lib, "common", "Game 400", "mine.txt")), "keep")

    def test_skips_while_steam_runs_or_session_in_use(self):
        install(self.share, 100, 20)
        self.steam(True)
        rc, out = self.pull()
        self.assertIn("skipped: Steam is running", out)
        self.assertFalse(os.path.exists(os.path.join(self.lib, "appmanifest_100.acf")))
        self.steam(False)
        open(self.marker, "w").close()
        rc, out = self.pull()
        self.assertIn("skipped: the player session is in use", out)
        self.assertFalse(os.path.exists(os.path.join(self.lib, "common", "Game 100")))
        os.unlink(self.marker)
        rc, out = self.pull()
        self.assertIn("copied build 20", out)
        # A fresh common/ is shared like the library: group-writable, setgid.
        self.assertEqual(oct(os.stat(os.path.join(self.lib, "common")).st_mode & 0o7777), "0o2775")

    def test_dry_run_changes_nothing(self):
        install(self.share, 100, 20)
        rc, out = self.pull(dry_run=True)
        self.assertIn("would copy build 20", out)
        self.assertIn("changed=1", out)
        self.assertEqual(os.listdir(self.lib), [])

    def test_unreachable_share(self):
        shutil.rmtree(self.share)
        rc, out = self.pull()
        self.assertEqual(rc, 0)
        self.assertIn("share not reachable or empty", out)
        self.assertIn("changed=0", out)

    def test_unfinished_server_copy_is_not_taken(self):
        install(self.share, 100, 20, state=6)
        rc, out = self.pull()
        self.assertIn("100 Listed: not on the server", out)
        self.assertIn("changed=0", out)


class FakeCollector(sc.Collector):
    """Seats are local folders; status comes from a dict."""

    def __init__(self, config, roots, status, presence=None, **kw):
        super().__init__(config, **kw)
        self.roots, self.status, self.presence = roots, status, presence

    def seat_status(self, seat):
        st = self.status.get(seat["name"])
        if st is None:
            seat["error"] = "Connection timed out"
        return st

    def busy_addresses(self):
        return self.presence

    def seat_rsync(self, seat, args, src, dst):
        rc, _, err = sc.run(["rsync"] + args + [os.path.join(self.roots[seat["name"]], src), dst])
        return rc, err


@unittest.skipUnless(HAVE_RSYNC, "needs rsync")
class CollectTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.dest = os.path.join(self.dir, "server", "steamapps")
        self.roots = {}
        self.seats = []
        for n in (1, 2, 3):
            name = f"seat-0{n}"
            self.roots[name] = os.path.join(self.dir, name, "steamapps")
            os.makedirs(self.roots[name])
            self.seats.append({"name": name, "address": f"192.0.2.2{n}"})
        self.config = {
            "apps": [{"appid": 550, "name": "Left 4 Dead 2"}, {"appid": 945360, "name": "Among Us"}],
            "seats": self.seats, "dest": self.dest, "user": "cafe-sync",
            "ssh_key": "/nonexistent", "known_hosts": "/nonexistent", "reserve_gib": 0,
        }
        self.idle = {"steam": "stopped", "game": "stopped"}

    def tearDown(self):
        shutil.rmtree(self.dir)

    def collect(self, status, presence=None, **kw):
        return quiet(FakeCollector(self.config, self.roots, status, presence, **kw).run)

    def test_takes_newest_and_skips_busy_seats(self):
        install(self.roots["seat-01"], 550, 10, files={"left4dead2.bin": "v10"})
        install(self.roots["seat-02"], 550, 40, files={"left4dead2.bin": "v40"})  # Steam runs here
        install(self.roots["seat-03"], 550, 30, files={"left4dead2.bin": "v30"})
        install(self.roots["seat-03"], 945360, 7, state=1026)  # mid-update
        install(self.roots["seat-01"], 12345, 99)  # not on the list
        status = {"seat-01": self.idle, "seat-02": {"steam": "running", "game": "stopped"}, "seat-03": self.idle}
        rc, out = self.collect(status)
        self.assertEqual(rc, 0, out)
        self.assertIn("seat-02: skipped, Steam is running", out)
        self.assertIn("550 Left 4 Dead 2: copied build 30 from seat-03 (new on the server)", out)
        self.assertIn("945360 Among Us: no complete copy on the seats that answered", out)
        self.assertEqual(read(os.path.join(self.dest, "common", "Game 550", "left4dead2.bin")), "v30")
        self.assertEqual(sc.read_manifest(os.path.join(self.dest, "appmanifest_550.acf")).buildid, 30)
        self.assertFalse(os.path.exists(os.path.join(self.dest, "appmanifest_12345.acf")))
        self.assertFalse(os.path.exists(os.path.join(self.dest, "common", "Game 12345")))

        # Next night, nothing newer: nothing copied.
        rc, out = self.collect(status)
        self.assertIn("550 Left 4 Dead 2: up to date (build 30)", out)

        # A seat in the launcher's People list is not read.
        status["seat-02"] = self.idle
        rc, out = self.collect(status, presence={"192.0.2.22"})
        self.assertIn("seat-02: skipped, someone is using it (launcher)", out)
        self.assertIn("up to date (build 30)", out)

        # Free again: its newer build comes in.
        rc, out = self.collect(status, presence=set())
        self.assertIn("copied build 40 from seat-02 (server had 30)", out)
        self.assertEqual(read(os.path.join(self.dest, "common", "Game 550", "left4dead2.bin")), "v40")

    def test_a_game_running_skips_the_seat(self):
        install(self.roots["seat-01"], 550, 10)
        rc, out = self.collect({"seat-01": {"steam": "stopped", "game": "running"}})
        self.assertIn("seat-01: skipped, a game is running", out)
        self.assertIn("no seat could be read (of 3); the library stays as it is", out)

    def test_no_seats_answered(self):
        install(self.dest, 550, 5)
        rc, out = self.collect({})
        self.assertEqual(rc, 0)
        self.assertIn("seat-01: skipped, did not answer (Connection timed out)", out)
        self.assertIn("no seat could be read (of 3); the library stays as it is", out)
        self.assertEqual(sc.read_manifest(os.path.join(self.dest, "appmanifest_550.acf")).buildid, 5)

    def test_no_seats_listed(self):
        self.config["seats"] = []
        rc, out = self.collect({})
        self.assertEqual(rc, 0)
        self.assertIn("no seats in the list", out)

    def test_dry_run(self):
        install(self.roots["seat-01"], 550, 10)
        rc, out = self.collect({"seat-01": self.idle}, dry_run=True)
        self.assertIn("would copy build 10 from seat-01", out)
        self.assertFalse(os.path.exists(os.path.join(self.dest, "appmanifest_550.acf")))

    def test_seat_changed_during_copy_leaves_manifest_out(self):
        install(self.roots["seat-01"], 550, 10)
        install(self.dest, 550, 5)
        status = {"seat-01": self.idle}

        class Racing(FakeCollector):
            def seat_rsync(inner, seat, args, src, dst):
                rc = FakeCollector.seat_rsync(inner, seat, args, src, dst)
                if src.startswith("common/"):  # Steam updates mid-copy
                    install(self.roots["seat-01"], 550, 11)
                return rc

        rc, out = quiet(Racing(self.config, self.roots, status).run)
        self.assertEqual(rc, 1)
        self.assertIn("changed during the copy", out)
        # The old manifest went first, so seats see no half copy.
        self.assertFalse(os.path.exists(os.path.join(self.dest, "appmanifest_550.acf")))

    @unittest.skipIf(os.geteuid() == 0, "root reads every file")
    @unittest.skipUnless(GNU_RSYNC, "messages are GNU rsync's (macOS ships openrsync)")
    def test_unreadable_seat_files_are_left_out(self):
        install(self.roots["seat-01"], 550, 10, files={"left4dead2.bin": "v10"}, installdir="Left 4 Dead 2")
        private = os.path.join(self.roots["seat-01"], "common", "Left 4 Dead 2", "ServerBrowser.vdf")
        with open(private, "w") as f:
            f.write("player only")
        os.chmod(private, 0)
        try:
            rc, out = self.collect({"seat-01": self.idle})
        finally:
            os.chmod(private, 0o600)
        self.assertEqual(rc, 0, out)
        self.assertIn("1 unreadable seat files left out", out)
        self.assertTrue(os.path.exists(os.path.join(self.dest, "appmanifest_550.acf")))
        self.assertTrue(os.path.exists(os.path.join(self.dest, "common", "Left 4 Dead 2", "left4dead2.bin")))

    def test_failed_copy_is_reported(self):
        install(self.roots["seat-01"], 550, 10)
        shutil.rmtree(os.path.join(self.roots["seat-01"], "common"))
        rc, out = self.collect({"seat-01": self.idle})
        self.assertEqual(rc, 1)
        self.assertIn("not copied from seat-01: rsync failed", out)
        self.assertFalse(os.path.exists(os.path.join(self.dest, "appmanifest_550.acf")))


if __name__ == "__main__":
    unittest.main()
