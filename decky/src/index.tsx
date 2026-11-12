// Turbine Hub in Steam's Quick Access menu (optional; needs Decky Loader): battery, temperatures, this game's
// switches, Game Boost, lighting brightness, Back up saves and Open Turbine Hub. The panel only shows what Turbine
// Hub says: the backend (main.py) runs Turbine Hub's Flatpak with one job and hands its answer over. Changes are
// made in Turbine Hub, so every button here opens it on the right page.

import { ButtonItem, Field, PanelSection, PanelSectionRow, Router, staticClasses } from "@decky/ui";
import { callable, definePlugin } from "@decky/api";
import { Component, useEffect, useRef, useState, type ReactNode } from "react";
import { HubIcon } from "./icon";
import { BRASS, batteryLine, gameFromPath, gameLine, keepText, lightingLine, pick, powerLine, readKept, tempsLine, type GameAnswer, type Kept, type Panel } from "./words";

const askPanel = callable<[], Panel>("panel");
const askGame = callable<[game: string], GameAnswer>("game");
const askOpen = callable<[page: string], { ok: boolean; hubUrl?: string; words?: string }>("open_page");

/** How often the readings are asked for while the panel is open. */
const REFRESH_MS = 5000;
const STORE_KEY = "turbinehub-panel-last";

// ---------------------------------------------------------------------------------------------- the last answer
// The last readings that worked stay in memory while the plugin is loaded and in the browser's storage across
// restarts, so a slow or failing Turbine Hub shows them with their age instead of an error.

let lastGood: Kept | null = null;

function loadKept(): Kept | null {
  if (lastGood) return lastGood;
  try {
    lastGood = readKept(window.localStorage?.getItem(STORE_KEY));
  } catch (e) {
    console.warn("Turbine Hub: no stored readings", e);
  }
  return lastGood;
}

function storeKept(k: Kept) {
  lastGood = k;
  try {
    window.localStorage?.setItem(STORE_KEY, keepText(k));
  } catch (e) {
    console.warn("Turbine Hub: readings not stored", e);
  }
}

// ---------------------------------------------------------------------------------------------- Steam's side
// Steam's own objects, reached loosely: they are Steam's internals and can change with a Steam update, so every use
// is guarded and the panel says so plainly when one is missing.

type SteamApps = { RunGame?: (gameId: string, launchOptions: string, a: number, b: number) => void };
type SteamUrl = { ExecuteSteamURL?: (url: string) => void };
type SteamClientish = { Apps?: SteamApps; URL?: SteamUrl };

function steam(): SteamClientish | undefined {
  return (window as unknown as { SteamClient?: SteamClientish }).SteamClient;
}

/** The game in front: the running one, else the game page open in the library, else the last one seen. */
let lastSeen: { id: string; name: string } | null = null;
function currentGame(): { id: string; name: string } | null {
  try {
    const r = Router as unknown as { MainRunningApp?: { appid?: number | string; display_name?: string } };
    const app = r.MainRunningApp;
    if (app && app.appid) {
      lastSeen = { id: String(app.appid), name: app.display_name || "" };
      return lastSeen;
    }
  } catch (e) {
    console.warn("Turbine Hub: no running app", e);
  }
  try {
    const w = Router as unknown as { WindowStore?: { GamepadUIMainWindowInstance?: { m_history?: { location?: { pathname?: string } } } } };
    const id = gameFromPath(w.WindowStore?.GamepadUIMainWindowInstance?.m_history?.location?.pathname || "");
    if (id) {
      const store = (window as unknown as { appStore?: { GetAppOverviewByAppID?: (id: number) => { display_name?: string } | null } }).appStore;
      lastSeen = { id, name: store?.GetAppOverviewByAppID?.(Number(id))?.display_name || "" };
      return lastSeen;
    }
  } catch (e) {
    console.warn("Turbine Hub: no library page", e);
  }
  return lastSeen;
}

/** Starts Turbine Hub's own entry in Steam (steam://rungameid/<id>) the way Steam's own buttons do. */
function runSteamUrl(url: string): boolean {
  const sc = steam();
  const id = /^steam:\/\/rungameid\/(\d+)$/.exec(url || "");
  try {
    if (id && sc?.Apps?.RunGame) {
      sc.Apps.RunGame(id[1], "", -1, 100);
      return true;
    }
    if (id && sc?.URL?.ExecuteSteamURL) {
      sc.URL.ExecuteSteamURL(url);
      return true;
    }
  } catch (e) {
    console.warn("Turbine Hub: Steam didn't take " + url, e);
  }
  return false;
}

// ---------------------------------------------------------------------------------------------- the panel

const small = { fontSize: "12px", color: "#dcd3c0", padding: "2px 0 6px" };

function Content() {
  const [kept, setKept] = useState<Kept | null>(loadKept());
  const [note, setNote] = useState<string>("");
  const [game] = useState<{ id: string; name: string } | null>(currentGame());
  const [gameInfo, setGameInfo] = useState<GameAnswer | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  const [words, setWords] = useState<string>("");
  const alive = useRef<boolean>(true);
  const asking = useRef<boolean>(false);

  const refresh = async () => {
    // One question at a time: a slow answer is never queued behind.
    if (asking.current) return;
    asking.current = true;
    let fresh: unknown = null;
    try {
      fresh = await askPanel();
    } catch (e) {
      fresh = { ok: false, words: "The panel couldn't reach Turbine Hub: " + String(e) };
    }
    asking.current = false;
    if (!alive.current) return;
    const got = pick(fresh, loadKept(), Date.now());
    if (got.kept && !got.note) storeKept(got.kept);
    setKept(got.kept);
    setNote(got.note);
  };

  useEffect(() => {
    alive.current = true;
    void refresh();
    const timer = setInterval(() => void refresh(), REFRESH_MS);
    if (game) {
      askGame(game.id)
        .then((a) => alive.current && setGameInfo(a && typeof a === "object" ? a : { ok: false }))
        .catch((e) => alive.current && setGameInfo({ ok: false, words: "The panel couldn't reach Turbine Hub: " + String(e) }));
    }
    return () => {
      alive.current = false;
      clearInterval(timer);
    };
  }, []);

  // Every button: tell Turbine Hub which page to show, then bring it to the front through its entry in Steam.
  const open = async (page: string) => {
    if (busy) return;
    setBusy(true);
    setWords("");
    try {
      const a = await askOpen(page);
      const url = (a && a.hubUrl) || kept?.panel.hubUrl || "";
      if (a && a.ok === false && a.words) setWords(a.words);
      else if (!url) setWords("Turbine Hub isn't in your Steam library yet. Open it once in Desktop Mode, and it adds itself.");
      else if (!runSteamUrl(url)) setWords("Steam didn't open Turbine Hub. Start it from your library.");
    } catch (e) {
      setWords("The panel couldn't reach Turbine Hub: " + String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  const p = kept?.panel;
  const r = p?.readings;
  const battery = batteryLine(r);
  const temps = tempsLine(r);
  const power = powerLine(r);
  const light = p ? lightingLine(p) : "";
  const switches = gameInfo?.ok && gameInfo.found && !gameInfo.locked ? gameInfo.switches || [] : [];

  return (
    <>
      <PanelSection title={p?.machine?.name || "This machine"}>
        {note ? (
          <PanelSectionRow>
            <div style={{ ...small, color: p ? BRASS.mid : "#dcd3c0" }}>{note}</div>
          </PanelSectionRow>
        ) : null}
        {!p && !note ? (
          <PanelSectionRow>
            <div style={small}>Asking Turbine Hub...</div>
          </PanelSectionRow>
        ) : null}
        {battery ? (
          <PanelSectionRow>
            <Field label="Battery" description={battery} onClick={() => void open("battery")} />
          </PanelSectionRow>
        ) : null}
        {temps ? (
          <PanelSectionRow>
            <Field label="Temperatures" description={power ? temps + ". " + power : temps} />
          </PanelSectionRow>
        ) : power ? (
          <PanelSectionRow>
            <Field label="Power" description={power} />
          </PanelSectionRow>
        ) : null}
      </PanelSection>

      {game ? (
        <PanelSection title={gameInfo?.game?.name || game.name || "This game"}>
          <PanelSectionRow>
            <div style={small}>{gameLine(gameInfo)}</div>
          </PanelSectionRow>
          {switches.map((s) => (
            <PanelSectionRow key={s.id}>
              <Field label={s.title} description={s.line}>
                <span style={{ color: s.on ? BRASS.light : "#8b929a" }}>{s.on ? "On" : "Off"}</span>
              </Field>
            </PanelSectionRow>
          ))}
          {gameInfo?.ok && gameInfo.found && !gameInfo.locked ? (
            <PanelSectionRow>
              <ButtonItem layout="below" disabled={busy} onClick={() => void open("games")}>
                Change this game's switches
              </ButtonItem>
            </PanelSectionRow>
          ) : null}
        </PanelSection>
      ) : null}

      <PanelSection title="Turbine Hub">
        {p?.boost?.available ? (
          <PanelSectionRow>
            <ButtonItem layout="below" disabled={busy} description="Full CPU speed while a game runs." onClick={() => void open("boost")}>
              Game Boost: {p.boost.on ? "On" : "Off"}
            </ButtonItem>
          </PanelSectionRow>
        ) : null}
        {light ? (
          <PanelSectionRow>
            <ButtonItem layout="below" disabled={busy} onClick={() => void open("lighting")}>
              Lighting brightness: {light}
            </ButtonItem>
          </PanelSectionRow>
        ) : null}
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={busy} onClick={() => void open("saves")}>
            Back up saves
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={busy} onClick={() => void open("home")}>
            Open Turbine Hub
          </ButtonItem>
        </PanelSectionRow>
        {words ? (
          <PanelSectionRow>
            <div style={small}>{words}</div>
          </PanelSectionRow>
        ) : null}
      </PanelSection>
    </>
  );
}

/** Anything that goes wrong while drawing stays inside the panel: Steam's quick menu keeps working. */
class Guard extends Component<{ children: ReactNode }, { broken: boolean }> {
  state = { broken: false };

  static getDerivedStateFromError() {
    return { broken: true };
  }

  componentDidCatch(error: unknown) {
    console.warn("Turbine Hub: the panel stopped drawing", error);
  }

  render() {
    if (!this.state.broken) return this.props.children;
    return (
      <PanelSection title="Turbine Hub">
        <PanelSectionRow>
          <div style={small}>The panel hit a problem. Close the menu and open it again, or start Turbine Hub from your library.</div>
        </PanelSectionRow>
      </PanelSection>
    );
  }
}

export default definePlugin(() => {
  return {
    name: "Turbine Hub",
    titleView: (
      <div className={staticClasses.Title} style={{ color: BRASS.light }}>
        Turbine Hub
      </div>
    ),
    content: (
      <Guard>
        <Content />
      </Guard>
    ),
    icon: <HubIcon />,
  };
});
