// The panel's words: node --experimental-strip-types --test tests/words.test.ts (from decky/).
import { test } from "node:test";
import assert from "node:assert/strict";
import { ago, batteryLine, gameFromPath, gameLine, keepText, lightingLine, pick, powerLine, readKept, tempsLine, usable, type Panel, type Readings } from "../src/words.ts";

const r: Readings = { cpuC: 41, gpuC: 38, gpuW: 34, batteryPct: 78, batteryW: 14.2, charging: false, fanRpm: 2400 };
const good: Panel = { ok: true, readings: r, boost: { available: true, on: true }, lighting: { available: true, percent: 50 }, hubUrl: "steam://rungameid/1" };

test("battery, temperatures and power in one line each", () => {
  assert.equal(batteryLine(r), "78%, using 14.2 W");
  assert.equal(batteryLine({ ...r, charging: true }), "78%, charging");
  assert.equal(batteryLine({ ...r, batteryW: null }), "78%");
  assert.equal(batteryLine({ ...r, batteryPct: null }), "");
  assert.equal(batteryLine(undefined), "");
  assert.equal(tempsLine(r), "CPU 41°C, GPU 38°C");
  assert.equal(tempsLine({ ...r, gpuC: null }), "CPU 41°C");
  assert.equal(tempsLine({ ...r, cpuC: null, gpuC: null }), "");
  assert.equal(powerLine(r), "Graphics 34 W, Fan 2400 rpm");
  assert.equal(powerLine({ ...r, gpuW: null, fanRpm: 0 }), "Fan stopped");
  assert.equal(powerLine({ ...r, gpuW: null, fanRpm: null }), "");
});

test("an age in plain words", () => {
  assert.equal(ago(3), "just now");
  assert.equal(ago(45), "45 seconds ago");
  assert.equal(ago(125), "2 minutes ago");
  assert.equal(ago(3600 * 3), "3 hours ago");
  assert.equal(ago(3600 * 1.6), "2 hours ago");
  assert.equal(ago(-5), "just now");
});

test("a fresh answer is shown and kept", () => {
  const got = pick(good, null, 1000);
  assert.deepEqual(got, { kept: { panel: good, at: 1000 }, note: "" });
});

test("when Turbine Hub fails or is slow, the last good readings stay with their age", () => {
  const last = { panel: good, at: 1_000_000 };
  for (const bad of [{ ok: false, slow: true, words: "Turbine Hub took too long to answer." }, { ok: false }, null, undefined, "nonsense", { ok: true }, 7]) {
    const got = pick(bad, last, 1_000_000 + 125_000);
    assert.equal(got.kept, last);
    assert.equal(got.note, "From 2 minutes ago. Turbine Hub isn't answering right now.");
  }
});

test("with nothing kept, the reason is shown instead of readings", () => {
  assert.deepEqual(pick({ ok: false, missing: true, words: "Turbine Hub isn't installed." }, null, 5), { kept: null, note: "Turbine Hub isn't installed." });
  assert.deepEqual(pick(null, null, 5), { kept: null, note: "Turbine Hub didn't answer." });
  assert.deepEqual(pick({ ok: false, error: "Unknown job" }, null, 5), { kept: null, note: "Unknown job" });
  // A kept answer from the future (the clock moved) or a damaged one isn't trusted.
  assert.equal(pick(null, { panel: good, at: 99 }, 5).kept, null);
  assert.equal(pick(null, { panel: { ok: false }, at: 1 }, 5).kept, null);
});

test("the kept answer survives being stored, and damaged text reads as nothing", () => {
  const k = { panel: good, at: 42 };
  assert.deepEqual(readKept(keepText(k)), k);
  for (const bad of [null, undefined, "", "{", "[]", "7", '{"at":"x","panel":{}}', '{"at":1,"panel":{"ok":false}}']) {
    assert.equal(readKept(bad), null);
  }
  assert.equal(usable(good), true);
  assert.equal(usable({ ok: true }), false);
});

test("a library page gives its app id", () => {
  assert.equal(gameFromPath("/routes/library/app/489830"), "489830");
  assert.equal(gameFromPath("/library/app/1245620/achievements"), "1245620");
  assert.equal(gameFromPath("/library/home"), null);
  assert.equal(gameFromPath(""), null);
});

test("the line under the game", () => {
  const sw = (on: boolean) => ({ id: "deck", title: "Steam Deck mode", line: "x", on });
  assert.equal(gameLine(null), "Asking Turbine Hub...");
  assert.equal(gameLine({ ok: false, words: "Turbine Hub took too long to answer." }), "Turbine Hub took too long to answer.");
  assert.equal(gameLine({ ok: true, found: false }), "Not a game Turbine Hub sees.");
  assert.equal(gameLine({ ok: true, found: true, locked: true, switches: [sw(false)] }), "Its anti-cheat runs online, so Turbine Hub leaves this game alone.");
  assert.equal(gameLine({ ok: true, found: true, switches: [sw(false)] }), "No switches on.");
  assert.equal(gameLine({ ok: true, found: true, switches: [sw(true), sw(false)] }), "1 switch on.");
  assert.equal(gameLine({ ok: true, found: true, switches: [sw(true), sw(true)] }), "2 switches on.");
  assert.equal(gameLine({ ok: true, found: true }), "No switches on.");
});

test("lighting brightness", () => {
  assert.equal(lightingLine(good), "50%");
  assert.equal(lightingLine({ ok: true, lighting: { available: true, percent: 0 } }), "Off");
  assert.equal(lightingLine({ ok: true, lighting: { available: false, percent: null } }), "");
  assert.equal(lightingLine({ ok: true }), "");
});

test("no line has a long dash", () => {
  const lines = [batteryLine(r), tempsLine(r), powerLine(r), ago(500), pick(null, { panel: good, at: 1 }, 99_000).note, gameLine({ ok: true, found: true, locked: true })];
  const dashes = new RegExp("[" + String.fromCharCode(0x2014, 0x2013) + "]");
  for (const l of lines) assert.doesNotMatch(l, dashes);
});
