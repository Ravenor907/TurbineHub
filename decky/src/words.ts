// The panel's words and small display choices, kept apart from Steam and Decky so they can be tested on their own
// (tests/words.test.ts). Every number and switch comes from Turbine Hub; this only puts them into lines.

/** Turbine's brass, shared with Turbine Mod Manager. */
export const BRASS = { light: "#E6C67E", mid: "#C99E52", dark: "#8F6A2C", ground: "#141109" };

export type Readings = {
  cpuC: number | null;
  gpuC: number | null;
  gpuW: number | null;
  batteryPct: number | null;
  batteryW: number | null;
  charging: boolean;
  fanRpm: number | null;
};

export type Panel = {
  ok: boolean;
  words?: string;
  error?: string;
  missing?: boolean;
  slow?: boolean;
  version?: string;
  machine?: { name: string; kind: string };
  readings?: Readings;
  boost?: { available: boolean; on: boolean };
  lighting?: { available: boolean; percent: number | null };
  helper?: boolean;
  hubUrl?: string;
};

export type Switch = { id: string; title: string; line: string; on: boolean };

export type GameAnswer = {
  ok: boolean;
  words?: string;
  error?: string;
  found?: boolean;
  game?: { id: string; name: string; store: string };
  locked?: boolean;
  switches?: Switch[];
};

/** A panel answer that worked, and when it came (ms). */
export type Kept = { panel: Panel; at: number };

function isNumber(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v);
}

/** Is this an answer the panel can show readings from? */
export function usable(p: unknown): p is Panel {
  if (!p || typeof p !== "object") return false;
  const a = p as Panel;
  return a.ok === true && !!a.readings && typeof a.readings === "object";
}

/** "78%, using 14.2 W", "78%, charging", or "" on a machine without a battery. */
export function batteryLine(r: Readings | undefined): string {
  if (!r || !isNumber(r.batteryPct)) return "";
  const pct = Math.round(r.batteryPct) + "%";
  if (r.charging) return pct + ", charging";
  return isNumber(r.batteryW) && r.batteryW > 0 ? pct + ", using " + r.batteryW.toFixed(1) + " W" : pct;
}

/** "CPU 41°C, GPU 38°C", or "" when the machine reports neither. */
export function tempsLine(r: Readings | undefined): string {
  if (!r) return "";
  const parts: string[] = [];
  if (isNumber(r.cpuC)) parts.push("CPU " + Math.round(r.cpuC) + "°C");
  if (isNumber(r.gpuC)) parts.push("GPU " + Math.round(r.gpuC) + "°C");
  return parts.join(", ");
}

/** "Graphics 34 W, fan 2400 rpm", "fan stopped", or "". */
export function powerLine(r: Readings | undefined): string {
  if (!r) return "";
  const parts: string[] = [];
  if (isNumber(r.gpuW)) parts.push("Graphics " + Math.round(r.gpuW) + " W");
  if (isNumber(r.fanRpm)) parts.push(r.fanRpm > 0 ? "Fan " + Math.round(r.fanRpm) + " rpm" : "Fan stopped");
  return parts.join(", ");
}

/** How long ago, in plain words. */
export function ago(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 10) return "just now";
  if (s < 90) return s + " seconds ago";
  const mins = Math.round(s / 60);
  if (mins < 90) return mins + " minutes ago";
  const hours = Math.round(mins / 60);
  return hours + (hours === 1 ? " hour ago" : " hours ago");
}

/** What the panel shows after asking: the fresh answer when it worked, else the last one that did, with a line
 * saying how old it is. `note` is "" when the readings are fresh. Nothing to show gives the plain reason. */
export function pick(fresh: unknown, last: Kept | null, nowMs: number): { kept: Kept | null; note: string } {
  if (usable(fresh)) return { kept: { panel: fresh, at: nowMs }, note: "" };
  const why = fresh && typeof fresh === "object" ? (fresh as Panel).words || (fresh as Panel).error || "" : "";
  if (last && usable(last.panel) && isNumber(last.at) && last.at <= nowMs) {
    return { kept: last, note: "From " + ago((nowMs - last.at) / 1000) + ". Turbine Hub isn't answering right now." };
  }
  return { kept: null, note: why || "Turbine Hub didn't answer." };
}

/** The last good answer as kept text, and back. Anything that isn't one reads as nothing. */
export function keepText(k: Kept): string {
  return JSON.stringify(k);
}

export function readKept(text: string | null | undefined): Kept | null {
  if (!text) return null;
  try {
    const v = JSON.parse(text) as Kept;
    return v && isNumber(v.at) && usable(v.panel) ? { panel: v.panel, at: v.at } : null;
  } catch {
    return null;
  }
}

/** A Steam library page's app id, from the main window's address ("/library/app/489830"). */
export function gameFromPath(path: string): string | null {
  const m = /\/library\/app\/(\d{1,20})(?:\/|$)/.exec(path || "");
  return m ? m[1] : null;
}

/** The line under a game's name. */
export function gameLine(g: GameAnswer | null): string {
  if (!g) return "Asking Turbine Hub...";
  if (!g.ok) return g.words || g.error || "Turbine Hub didn't answer.";
  if (!g.found) return "Not a game Turbine Hub sees.";
  if (g.locked) return "Its anti-cheat runs online, so Turbine Hub leaves this game alone.";
  const on = (g.switches || []).filter((s) => s && s.on).length;
  if (on === 0) return "No switches on.";
  return on === 1 ? "1 switch on." : on + " switches on.";
}

/** "50%" for the ring lights, "Off" at zero, "" without ring lights. */
export function lightingLine(p: Panel): string {
  if (!p.lighting || !p.lighting.available || !isNumber(p.lighting.percent)) return "";
  return p.lighting.percent <= 0 ? "Off" : Math.round(p.lighting.percent) + "%";
}
