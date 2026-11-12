#!/usr/bin/env node
// A quick check of Turbine Hub's QML files with nothing to install (Node only), so a typo fails in seconds rather
// than deep into the Flatpak build. It reads each file's object tree (objects, ids, properties, signals,
// functions, inline components) and compiles every JavaScript binding, handler and function with Node's own parser.
// It doesn't know types or imports: the Flatpak build still finds those. It also holds the house style: no long
// dashes, text sizes through the window's px helpers, durations and curves from Motion.qml.
//
//   node tools/qml-check.js              every .qml under app/qml
//   node tools/qml-check.js a.qml b.qml  only these files
"use strict";

const fs = require("fs");
const vm = require("vm");

function checkFile(file) {
  const src = fs.readFileSync(file, "utf8");
  const errors = [];
  let i = 0;
  const n = src.length;
  const lineAt = (pos) => src.slice(0, pos).split("\n").length;
  const fail = (pos, msg) => errors.push(file + ":" + lineAt(pos) + ": " + msg);
  // Ids must be unique in a file (an inline component has its own).
  const idScopes = [new Set()];

  function skipSpace(stopAtNewline) {
    for (;;) {
      while (i < n && (src[i] === " " || src[i] === "\t" || src[i] === "\r" || (!stopAtNewline && src[i] === "\n") || src[i] === ";")) i++;
      if (src.startsWith("//", i)) { while (i < n && src[i] !== "\n") i++; continue; }
      if (src.startsWith("/*", i)) { const e = src.indexOf("*/", i + 2); i = e < 0 ? n : e + 2; continue; }
      return;
    }
  }
  function ident() {
    const m = /^[A-Za-z_$][\w$]*(\.[A-Za-z_$][\w$]*)*/.exec(src.slice(i, i + 200));
    if (!m) return null;
    i += m[0].length;
    return m[0];
  }
  // Reads one JavaScript value from i to its end: brackets, strings, templates, regexes and comments are followed; a
  // newline outside brackets ends it unless the line plainly goes on.
  function readExpr() {
    const start = i;
    const stack = [];
    let prev = "";
    while (i < n) {
      const c = src[i];
      if (c === '"' || c === "'") {
        i++;
        while (i < n && src[i] !== c) { if (src[i] === "\\") i++; if (src[i] === "\n") { fail(i, "string not closed"); break; } i++; }
        i++; prev = "a"; continue;
      }
      if (c === "`") {
        i++;
        while (i < n && src[i] !== "`") { if (src[i] === "\\") i++; i++; }
        i++; prev = "a"; continue;
      }
      if (src.startsWith("//", i)) { while (i < n && src[i] !== "\n") i++; continue; }
      if (src.startsWith("/*", i)) { const e = src.indexOf("*/", i + 2); i = e < 0 ? n : e + 2; continue; }
      if (c === "/" && (prev === "" || /[(,=:[!&|?{};+\-*%<>~^]/.test(prev) || /\b(return|typeof|in|of|case)$/.test(src.slice(Math.max(0, i - 7), i).trimEnd()))) {
        // A regular expression.
        i++;
        let cls = false;
        while (i < n && (src[i] !== "/" || cls)) {
          if (src[i] === "\\") i++;
          else if (src[i] === "[") cls = true;
          else if (src[i] === "]") cls = false;
          else if (src[i] === "\n") { fail(i, "regular expression not closed"); break; }
          i++;
        }
        i++;
        while (i < n && /[a-z]/.test(src[i])) i++;
        prev = "a"; continue;
      }
      if (c === "(" || c === "[" || c === "{") { stack.push(c); i++; prev = c; continue; }
      if (c === ")" || c === "]" || c === "}") {
        if (stack.length === 0) break;
        const o = stack.pop();
        if ((o === "(" && c !== ")") || (o === "[" && c !== "]") || (o === "{" && c !== "}")) { fail(i, "'" + o + "' closed by '" + c + "'"); }
        i++; prev = "a"; continue;
      }
      if (c === ";" && stack.length === 0) {
        // "if (a) b(); else c()" on one line goes on.
        if (/^;\s*else\b/.test(src.slice(i, i + 20))) { i++; continue; }
        break;
      }
      if (c === "\n" && stack.length === 0) {
        // Goes on when the line ends in an operator or the next line starts with one.
        let j = i + 1;
        // The next line that isn't blank or a comment.
        for (;;) {
          while (j < n && /\s/.test(src[j])) j++;
          if (src.startsWith("//", j)) { while (j < n && src[j] !== "\n") j++; continue; }
          break;
        }
        const next = src.slice(j, j + 5);
        if (/[+\-*/%=&|?:,.([{!<>]/.test(prev) && !/\+\+$|--$/.test(src.slice(start, i).trimEnd())) { i++; continue; }
        if (/^(\.[A-Za-z_]|\?|:|\+|-[^-]|\*|\/[^/*]|&&|\|\||===|!==|==|!=|<|>|else\b)/.test(next)) { i++; continue; }
        break;
      }
      if (!/\s/.test(c)) prev = c;
      i++;
    }
    if (stack.length) fail(start, "'" + stack[stack.length - 1] + "' never closed");
    return src.slice(start, i);
  }
  function compile(code, pos, how) {
    try {
      new vm.Script(how === "expr" ? "(" + code + "\n)" : how === "block" ? "(function(){" + code + "\n})" : code, { filename: file });
    } catch (e) {
      fail(pos, e.message + " in: " + code.trim().split("\n")[0].slice(0, 100));
    }
  }
  // A value after "name:": an object, a list of objects, or JavaScript.
  function value(handler) {
    skipSpace(true);
    const at = i;
    const save = i;
    const t = ident();
    if (t && /^[A-Z]/.test(t.split(".").pop())) {
      skipSpace(true);
      if (src[i] === "{") { object(t); return; }
    }
    i = save;
    if (src[i] === "[") {
      // A list of objects?
      const k = i;
      i++;
      skipSpace(false);
      const t2 = ident();
      skipSpace(false);
      if (t2 && /^[A-Z]/.test(t2.split(".").pop()) && src[i] === "{") {
        i = k + 1;
        for (;;) {
          skipSpace(false);
          if (src[i] === "]") { i++; return; }
          if (src[i] === ",") { i++; continue; }
          const ty = ident();
          skipSpace(false);
          if (!ty || src[i] !== "{") { fail(i, "expected an object in the list"); return; }
          object(ty);
        }
      }
      i = k;
    }
    const e = readExpr();
    if (e.trim() === "") { fail(at, "a property with no value"); return; }
    // A signal handler is statements; anything else is one expression (or a block that returns one).
    if (e.trim()[0] === "{") compile(e.trim().slice(1, -1), at, "block");
    else if (handler && !/^\s*(\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>/.test(e) && !/^\s*function\b/.test(e)) compile(e, at, "block");
    else compile(e, at, "expr");
  }
  function object(type) {
    const open = i;
    if (src[i] !== "{") { fail(i, "expected { after " + type); return; }
    i++;
    // A property or handler set twice in one object stops the file loading ("Property value set multiple times").
    const assigned = new Set();
    for (;;) {
      skipSpace(false);
      if (i >= n) { fail(open, type + " { never closed"); return; }
      if (src[i] === "}") { i++; return; }
      const at = i;
      let word = ident();
      if (!word) { fail(i, "unexpected '" + src[i] + "'"); i++; continue; }
      if (word === "readonly" || word === "required" || word === "default") { skipSpace(true); word = ident(); }
      if (word === "property") {
        skipSpace(true);
        if (src.startsWith("list<", i)) { const e = src.indexOf(">", i); i = e < 0 ? n : e + 1; } else ident();
        skipSpace(true);
        const name = ident();
        if (!name) { fail(i, "a property with no name"); continue; }
        skipSpace(true);
        if (src[i] === ":") { i++; value(); }
        continue;
      }
      if (word === "signal") {
        skipSpace(true); ident(); skipSpace(true);
        if (src[i] === "(") { const e = src.indexOf(")", i); i = e < 0 ? n : e + 1; }
        continue;
      }
      if (word === "function") {
        skipSpace(true);
        const name = ident();
        const s = i;
        if (src[i] !== "(") { fail(i, "function " + name + " without ("); continue; }
        readExpr();
        compile("function " + name + src.slice(s, i), at, "code");
        continue;
      }
      if (word === "component") {
        skipSpace(true); ident(); skipSpace(true);
        if (src[i] === ":") i++;
        skipSpace(true);
        const t = ident();
        skipSpace(true);
        // An inline component has ids of its own.
        idScopes.push(new Set());
        object(t);
        idScopes.pop();
        continue;
      }
      if (word === "enum") { const e = src.indexOf("}", i); i = e < 0 ? n : e + 1; continue; }
      if (word === "import" || word === "pragma") { while (i < n && src[i] !== "\n") i++; continue; }
      skipSpace(true);
      // "Behavior on x { }" and friends.
      if (src.startsWith("on ", i)) { i += 3; skipSpace(true); ident(); skipSpace(true); object(word); continue; }
      // An object, or a group such as font { }.
      if (src[i] === "{") { object(word); continue; }
      if (src[i] === ":") {
        if (assigned.has(word)) fail(at, word + " is set twice in one " + type);
        assigned.add(word);
        i++;
        if (word === "id") {
          skipSpace(true);
          const at = i, name = ident();
          if (!name) fail(i, "id with no name");
          else if (idScopes[idScopes.length - 1].has(name)) fail(at, "id " + name + " is not unique");
          else idScopes[idScopes.length - 1].add(name);
          continue;
        }
        value(/^on[A-Z]/.test(word.split(".").pop()));
        continue;
      }
      fail(i, "unexpected text after " + word);
      while (i < n && src[i] !== "\n") i++;
    }
  }

  // The file: imports, then one root object.
  for (;;) {
    skipSpace(false);
    if (src.startsWith("import ", i) || src.startsWith("pragma ", i)) { while (i < n && src[i] !== "\n") i++; continue; }
    break;
  }
  const root = ident();
  skipSpace(false);
  if (!root) fail(i, "no root object");
  else object(root);
  skipSpace(false);
  if (i < n) fail(i, "text after the root object");
  return errors;
}

// A property declared with the name of one of Item's own FINAL properties stops the whole file loading ("Cannot
// override FINAL property"), which this reader can't know from types: check the names.
// (Only the anchor lines: other Item names such as x or states are fine on a QtObject or a Settings.)
const FINAL = new Set(["left", "right", "top", "bottom", "horizontalCenter", "verticalCenter", "baseline"]);
function finalNames(file) {
  const src = require("fs").readFileSync(file, "utf8");
  const out = [];
  const re = /^\s*(?:readonly\s+|required\s+|default\s+)*property\s+[\w.<>]+\s+(\w+)/gm;
  let m;
  while ((m = re.exec(src))) {
    if (FINAL.has(m[1])) out.push(file + ":" + (src.slice(0, m.index).split("\n").length + 1) + ": property " + m[1] + " has the name of an Item property (Cannot override FINAL property)");
  }
  // A property with the name of a function in the same file hides the function ("is not a function" at run time).
  // (The root object's own, four spaces in.)
  const fns = new Set([...src.matchAll(/^    function\s+(\w+)\s*\(/gm)].map(x => x[1]));
  const re2 = /^    (?:readonly\s+|required\s+|default\s+)*property\s+[\w.<>]+\s+(\w+)/gm;
  while ((m = re2.exec(src))) {
    if (fns.has(m[1])) out.push(file + ":" + (src.slice(0, m.index).split("\n").length + 1) + ": property " + m[1] + " has the name of a function in this file");
  }
  return out;
}

// One motion system. Every duration and curve comes from Motion.qml: a
// duration written as a number, or an easing curve named, anywhere else is a problem. (Timers' intervals and the
// time a hold takes are not animations and are not looked at.)
function motionRule(files) {
  const out = [];
  let n = 0;
  for (const f of files) {
    if (/(^|\/)Motion\.qml$/.test(f)) continue;
    const lines = fs.readFileSync(f, "utf8").split("\n");
    for (let k = 0; k < lines.length; k++) {
      const l = lines[k].replace(/\/\/.*$/, "");
      if (/\b(duration|highlightMoveDuration|highlightResizeDuration|velocity)\s*:\s*-?\d/.test(l)) out.push(f + ":" + (k + 1) + ": a duration written as a number (take it from Motion.qml): " + l.trim().slice(0, 80));
      if (/\bEasing\.[A-Z]\w+/.test(l)) out.push(f + ":" + (k + 1) + ": an easing curve named here (take it from Motion.qml): " + l.trim().slice(0, 80));
      if (/\b(duration|easing\.type)\s*:/.test(l)) n++;
    }
  }
  console.log("Motion check: " + n + " durations and curves, " + out.length + " outside Motion.qml");
  return out;
}

// House style, line by line: no em or en dashes anywhere in a file, and no text size written as a bare number
// (sizes go through the window's helpers fpx(), px() and tpx(), so they follow the interface size).
function styleRule(files) {
  const out = [];
  for (const f of files) {
    const lines = fs.readFileSync(f, "utf8").split("\n");
    for (let k = 0; k < lines.length; k++) {
      if (/[\u2013\u2014]/.test(lines[k])) out.push(f + ":" + (k + 1) + ": a long dash (use a hyphen, a comma or a colon)");
      const l = lines[k].replace(/\/\/.*$/, "");
      if (/\bpixelSize\s*:\s*-?[\d.]+\s*(?:[;}]|$)/.test(l)) out.push(f + ":" + (k + 1) + ": a text size written as a number (use fpx(), px() or tpx()): " + l.trim().slice(0, 80));
    }
  }
  console.log("Style check: " + out.length + " problems");
  return out;
}

// Nothing goes through a pop-up. In Qt 6 a MouseArea that takes a press stops the items under it but never their
// pointer handlers (TapHandler, HoverHandler), so a backdrop of a file's own lets taps through to the page beneath.
// The rules that keep that from coming back:
//   the only MouseArea is Backdrop.qml (every backdrop and card uses it);
//   every tap asks app.gate() first, on the handler's line or the one after, so it obeys what a press obeys;
//   the mouse resting on things is heard in Focusable.qml and Main.qml only, which decide together (hoverAt);
//   Main.qml turns the page off under a dialog and under the busy cover.
function pointerRule(files) {
  const out = [];
  let taps = 0;
  const named = (f) => f.split("/").pop();
  for (const f of files) {
    const lines = fs.readFileSync(f, "utf8").split("\n");
    for (let k = 0; k < lines.length; k++) {
      const l = lines[k].replace(/\/\/.*$/, "");
      if (/\bMouseArea\s*\{/.test(l) && named(f) !== "Backdrop.qml") out.push(f + ":" + (k + 1) + ": a MouseArea of its own (use Backdrop.qml: it alone stops what is under it)");
      if (/\bHoverHandler\s*\{/.test(l) && ["Focusable.qml", "Main.qml"].indexOf(named(f)) < 0) out.push(f + ":" + (k + 1) + ": a HoverHandler of its own (the highlight follows the mouse through Focusable.qml)");
      if (/\bonTapped\s*:/.test(l)) {
        taps++;
        const next = (lines[k + 1] || "").replace(/\/\/.*$/, "");
        if (!/\bgate\(/.test(l) && !/\bgate\(/.test(next)) out.push(f + ":" + (k + 1) + ": a tap with no app.gate() first: " + l.trim().slice(0, 80));
      }
    }
    if (named(f) === "Main.qml") {
      const src = lines.join("\n");
      if (!/^\s*enabled: !dialog\.open && !Hub\.busy$/m.test(src)) out.push(f + ": the page isn't turned off under a dialog and the busy cover (enabled: !dialog.open && !Hub.busy on the view)");
      if (!/visible: Hub\.busy[\s\S]{0,200}?Backdrop \{/.test(src)) out.push(f + ": the busy cover has no Backdrop");
    }
    if (named(f) === "DialogCard.qml" && !/Backdrop \{ onTapped:/.test(lines.join("\n"))) out.push(f + ": the dialog's backdrop isn't a Backdrop");
  }
  console.log("Pointer check: " + taps + " taps ask the gate, " + out.length + " problems");
  return out;
}

// Every .qml file under a folder, in a steady order.
function qmlFiles(dir) {
  let out = [];
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true }).sort((x, y) => x.name < y.name ? -1 : 1)) {
    const p = dir + "/" + e.name;
    if (e.isDirectory()) out = out.concat(qmlFiles(p));
    else if (e.name.endsWith(".qml")) out.push(p);
  }
  return out;
}

const given = process.argv.slice(2);
const files = given.length ? given : qmlFiles("app/qml");
if (!files.length) {
  console.log("QML check: no QML files in app/qml yet");
  process.exit(0);
}
let all = [];
for (const f of files) {
  // A file that can't be read, or one cut off so badly the reader trips, is a problem of its own, not a crash.
  try {
    all = all.concat(checkFile(f)).concat(finalNames(f));
  } catch (e) {
    all.push(f + ": could not be checked: " + e.message);
  }
}
const readable = files.filter(f => fs.existsSync(f));
all = all.concat(motionRule(readable)).concat(styleRule(readable)).concat(pointerRule(readable));
for (const e of all) console.log(e);
console.log("QML check: " + files.length + " files, " + all.length + " problems");
process.exit(all.length ? 1 : 0);
