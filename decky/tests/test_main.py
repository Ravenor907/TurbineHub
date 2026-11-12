"""The backend's parts: the arguments Turbine Hub gets, the environment, reading its answer, and never failing the
panel. Run with: python3 -m unittest discover -s tests (from decky/). Decky's own module is stood in for here."""

import asyncio
import os
import stat
import sys
import tempfile
import types
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

fake = types.ModuleType("decky")
fake.DECKY_USER_HOME = tempfile.mkdtemp()
fake.DECKY_PLUGIN_SETTINGS_DIR = tempfile.mkdtemp()


class _Log:
    def info(self, *a):
        pass

    def warning(self, *a):
        pass


fake.logger = _Log()
sys.modules["decky"] = fake

import main  # noqa: E402


class Args(unittest.TestCase):
    def test_each_job(self):
        self.assertEqual(main.job_args("panel"), ["--json", "panel"])
        self.assertEqual(main.job_args("game", "1245620"), ["--json", "game=1245620"])
        self.assertEqual(main.job_args("game", 1245620), ["--json", "game=1245620"])
        self.assertEqual(main.job_args("game", "epic:Heron"), ["--json", "game=epic:Heron"])
        self.assertEqual(main.job_args("open", "saves"), ["--json", "open=saves"])

    def test_refuses_what_isnt_a_game(self):
        for bad in ["", "   ", "--json", "-x", "a\nb", "x" * 201, "1=2", None, True, ["1"], {"a": 1}]:
            with self.assertRaises(ValueError, msg=repr(bad)):
                main.job_args("game", bad)

    def test_refuses_other_jobs_and_pages(self):
        for job, value in [("rm", None), ("apply", "x"), ("panel", "x"), ("open", "settings; rm"), ("open", None), ("checkup", None)]:
            with self.assertRaises(ValueError, msg=job):
                main.job_args(job, value)


class Env(unittest.TestCase):
    def test_decky_libraries_dont_reach_flatpak(self):
        env = main.host_env({"LD_LIBRARY_PATH": "/tmp/_MEI123", "PATH": "/usr/bin"}, "/home/deck", 1000)
        self.assertNotIn("LD_LIBRARY_PATH", env)
        self.assertEqual(env["HOME"], "/home/deck")
        self.assertEqual(env["XDG_RUNTIME_DIR"], "/run/user/1000")
        self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/run/user/1000/bus")
        self.assertEqual(env["PATH"], "/usr/bin")

    def test_the_original_library_path_comes_back(self):
        env = main.host_env({"LD_LIBRARY_PATH": "/tmp/_MEI123", "LD_LIBRARY_PATH_ORIG": "/usr/lib/x", "XDG_RUNTIME_DIR": "/run/user/7"}, "/home/deck", 1000)
        self.assertEqual(env["LD_LIBRARY_PATH"], "/usr/lib/x")
        self.assertEqual(env["XDG_RUNTIME_DIR"], "/run/user/7")
        self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/run/user/7/bus")


class Answers(unittest.TestCase):
    def test_the_last_json_line_is_the_answer(self):
        out = 'Gtk-Message: noise\n{"ok":false,"error":"old"}\n{"ok":true,"readings":{"batteryPct":78}}\n'
        self.assertEqual(main.parse_answer(0, out, ""), {"ok": True, "readings": {"batteryPct": 78}})
        self.assertEqual(main.parse_answer(2, '{"ok":false,"error":"Unknown job"}', "x"), {"ok": False, "error": "Unknown job"})

    def test_broken_or_missing_answers_become_words(self):
        a = main.parse_answer(1, "{broken\n[1,2]\n", "error: app/io.github.ravenor907.TurbineHub/x86_64/master not installed")
        self.assertEqual((a["ok"], a["missing"]), (False, True))
        self.assertIn("isn't installed", a["words"])
        self.assertTrue(main.parse_answer(127, "", "flatpak not found")["missing"])
        slow = main.parse_answer(124, "", "it took longer than 20 seconds")
        self.assertEqual((slow["ok"], slow["slow"]), (False, True))
        other = main.parse_answer(1, "", "first\nbwrap: something broke")
        self.assertEqual(other, {"ok": False, "words": "Turbine Hub didn't answer (bwrap: something broke)."})
        self.assertEqual(main.parse_answer(1, "", "")["words"], "Turbine Hub didn't answer (no answer).")


class Asking(unittest.TestCase):
    """The whole way through, with a stand-in for the flatpak program first on PATH."""

    def setUp(self):
        self.bin = tempfile.mkdtemp()
        self.old_path = os.environ.get("PATH", "")
        os.environ["PATH"] = self.bin + os.pathsep + self.old_path
        self.plugin = main.Plugin()

    def tearDown(self):
        os.environ["PATH"] = self.old_path

    def flatpak(self, script):
        path = os.path.join(self.bin, "flatpak")
        with open(path, "w") as f:
            f.write("#!/bin/sh\n" + script + "\n")
        os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR)

    def ask(self, coro):
        return asyncio.run(coro)

    def test_the_exact_command_reaches_the_app(self):
        self.flatpak('printf \'{"ok":true,"args":"%s"}\\n\' "$*"')
        self.assertEqual(self.ask(self.plugin.panel()), {"ok": True, "args": "run io.github.ravenor907.TurbineHub --json panel"})
        self.plugin = main.Plugin()
        self.assertEqual(self.ask(self.plugin.game(1245620))["args"], "run io.github.ravenor907.TurbineHub --json game=1245620")
        self.plugin = main.Plugin()
        self.assertEqual(self.ask(self.plugin.open_page("lighting"))["args"], "run io.github.ravenor907.TurbineHub --json open=lighting")

    def test_a_bad_request_never_runs_anything(self):
        self.flatpak("touch " + os.path.join(self.bin, "ran"))
        for coro in [self.plugin.game("--help"), main.Plugin().open_page("../../etc"), main.Plugin().game(None)]:
            a = self.ask(coro)
            self.assertFalse(a["ok"])
            self.assertIn("Can't ask Turbine Hub that", a["words"])
        self.assertFalse(os.path.exists(os.path.join(self.bin, "ran")))

    def test_a_failing_or_missing_app_still_gives_an_answer(self):
        self.flatpak("echo 'error: app/io.github.ravenor907.TurbineHub/x86_64/master not installed' >&2; exit 1")
        self.assertTrue(self.ask(self.plugin.panel())["missing"])
        self.flatpak("echo boom >&2; exit 3")
        self.assertEqual(self.ask(main.Plugin().panel()), {"ok": False, "words": "Turbine Hub didn't answer (boom)."})
        os.remove(os.path.join(self.bin, "flatpak"))
        os.environ["PATH"] = self.bin
        self.assertTrue(self.ask(main.Plugin().panel())["missing"])

    def test_a_slow_app_is_stopped(self):
        self.flatpak("sleep 30")
        old = main.TIMEOUT
        main.TIMEOUT = 0.3
        try:
            a = self.ask(self.plugin.panel())
        finally:
            main.TIMEOUT = old
        self.assertEqual((a["ok"], a["slow"]), (False, True))


if __name__ == "__main__":
    unittest.main()
