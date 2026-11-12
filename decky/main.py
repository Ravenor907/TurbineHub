"""Turbine Hub in Steam's Quick Access menu: the plugin's backend.

Decky runs this on the host, as the player (the plugin asks for no root). It runs Turbine Hub's Flatpak with one job
and hands its one JSON line back:

    flatpak run io.github.ravenor907.TurbineHub --json panel
    flatpak run io.github.ravenor907.TurbineHub --json game=<Steam app id>
    flatpak run io.github.ravenor907.TurbineHub --json open=<page>

Nothing is decided or changed here: every number, word and switch comes from Turbine Hub, and every change is made
in Turbine Hub itself.
"""

import asyncio
import json
import os
import time

import decky

APP_ID = "io.github.ravenor907.TurbineHub"
# How long one job may take. The panel asks every few seconds, so a slow answer is dropped rather than waited for.
TIMEOUT = 20
MAX_GAME = 200
# The jobs the panel may ask for; "game" and "open" take one value after an equals sign.
PLAIN_JOBS = ("panel",)
PAGES = ("home", "games", "saves", "boost", "lighting", "battery")


def clean_game(game):
    """The game as Turbine Hub's command line gets it, or None when it can't be one."""
    if not isinstance(game, (str, int)) or isinstance(game, bool):
        return None
    g = str(game).strip()
    if not g or len(g) > MAX_GAME or g.startswith("-") or "=" in g:
        return None
    if any(ord(c) < 32 or ord(c) == 127 for c in g):
        return None
    return g


def job_args(job, value=None):
    """Turbine Hub's arguments for one job (fixed words, the value inside one argument, never a shell)."""
    if job in PLAIN_JOBS and value is None:
        return ["--json", job]
    if job == "game":
        g = clean_game(value)
        if g is None:
            raise ValueError("that game can't be read")
        return ["--json", "game=" + g]
    if job == "open" and value in PAGES:
        return ["--json", "open=" + value]
    raise ValueError("unknown job")


def host_env(base, user_home, uid):
    """The environment for programs on the host. Decky's own (a bundled Python) points LD_LIBRARY_PATH at its
    libraries, which would break flatpak; the player's runtime folder and session bus are where Flatpak's portal is."""
    env = dict(base)
    orig = env.pop("LD_LIBRARY_PATH_ORIG", None)
    if orig:
        env["LD_LIBRARY_PATH"] = orig
    else:
        env.pop("LD_LIBRARY_PATH", None)
    env["HOME"] = user_home
    runtime = "/run/user/%d" % uid
    if not env.get("XDG_RUNTIME_DIR"):
        env["XDG_RUNTIME_DIR"] = runtime
    if not env.get("DBUS_SESSION_BUS_ADDRESS"):
        env["DBUS_SESSION_BUS_ADDRESS"] = "unix:path=%s/bus" % env["XDG_RUNTIME_DIR"]
    return env


def parse_answer(code, out, err):
    """Turbine Hub's answer: its last JSON line, or plain words when there isn't one."""
    for line in reversed((out or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                v = json.loads(line)
            except ValueError:
                continue
            if isinstance(v, dict):
                return v
    text = (err or "").strip().splitlines()
    if code == 127 or any("not installed" in t for t in text):
        return {"ok": False, "missing": True, "words": "Turbine Hub isn't installed. Install it from its release page, then open this panel again."}
    if code == 124:
        return {"ok": False, "slow": True, "words": "Turbine Hub took too long to answer."}
    detail = text[-1][:200] if text else "no answer"
    return {"ok": False, "words": "Turbine Hub didn't answer (%s)." % detail}


class Plugin:
    lock = None

    def _env(self):
        return host_env(os.environ, decky.DECKY_USER_HOME, os.getuid())

    async def _run(self, argv, timeout):
        """Runs one program on the host with fixed arguments and gives (exit code, stdout, stderr)."""
        try:
            proc = await asyncio.create_subprocess_exec(
                *argv,
                stdin=asyncio.subprocess.DEVNULL,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=self._env(),
                cwd=decky.DECKY_USER_HOME,
            )
        except FileNotFoundError:
            return 127, "", "%s not found" % argv[0]
        except OSError as e:
            return 126, "", str(e)
        try:
            out, err = await asyncio.wait_for(proc.communicate(), timeout)
        except asyncio.TimeoutError:
            proc.kill()
            await proc.wait()
            return 124, "", "it took longer than %d seconds" % timeout
        return proc.returncode, out.decode("utf-8", "replace"), err.decode("utf-8", "replace")

    async def _ask(self, job, value=None):
        try:
            args = job_args(job, value)
        except ValueError as e:
            return {"ok": False, "words": "Can't ask Turbine Hub that: %s." % e}
        # One job at a time, so a slow answer never piles up behind the panel's next question.
        if self.lock is None:
            self.lock = asyncio.Lock()
        try:
            async with self.lock:
                started = time.monotonic()
                code, out, err = await self._run(["flatpak", "run", APP_ID] + args, TIMEOUT)
                answer = parse_answer(code, out, err)
                decky.logger.info("Turbine Hub %s: exit %s in %.1f s: %s", args[-1], code, time.monotonic() - started, json.dumps(answer)[:300])
                return answer
        except Exception as e:  # the panel always gets an answer it can show
            decky.logger.warning("Turbine Hub %s: %s", job, e)
            return {"ok": False, "words": "Turbine Hub didn't answer (%s)." % str(e)[:200]}

    # ---- what the panel calls ----

    async def panel(self):
        """Readings, Game Boost, lighting and Turbine Hub's own entry in Steam."""
        return await self._ask("panel")

    async def game(self, game):
        """One game's switches, by its Steam app id."""
        return await self._ask("game", game)

    async def open_page(self, page):
        """Tells Turbine Hub which page to show when it comes to the front."""
        return await self._ask("open", page)

    async def _main(self):
        decky.logger.info("Turbine Hub's quick panel is ready")

    async def _unload(self):
        decky.logger.info("Turbine Hub's quick panel stopped")

    async def _uninstall(self):
        pass
