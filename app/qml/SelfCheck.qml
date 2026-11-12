import QtQuick
import TurbineHub

// The build's checks on one page, loaded only in screenshot runs (Main.qml, --screenshot). After the page settles
// it walks everything on screen and fails the run when:
//   words are cut off (elided, or wider or taller than the box that clips them);
//   words or a control stick out of the window, or sideways out of the list they scroll in;
//   words are smaller than 12 pixels (the text floor, app.fpx), or taller or wider than the box drawn around them;
//   words don't read on what they are drawn on (under 4.5 to 1, measured from the colors really drawn);
//   something to press is smaller than a finger (44 pixels or 6 mm, app.touchFloor);
//   a state chip (ok, warn, stop) lacks its mark or its word, so only its color would tell;
//   something the highlight lands on has no name for a screen reader, or the screen reader's own view of the
//   window (Qt's accessibility tree, Hub.testSpoken) finds a control with no name, or more than one thing that
//   says it has the highlight;
//   any theme, accent and color vision mode (all 80, whatever this picture is drawn in) has words under 4.5 to 1
//   on a surface, or states that the eyes the mode is for can't tell apart (Palette.qml, checked by `palettes`);
//   words are squeezed into a column so narrow that each line holds about one word;
//   words are drawn over other words (except what is drawn over the page on purpose: `cover: true`);
//   the footer lacks B, lacks A where the highlighted thing takes A, offers A where it doesn't, or shows a hint
//   with no words;
//   nothing holds the highlight, or the highlighted thing isn't fully in view;
//   a dialog is open and the page under it still takes taps, or a finger's tap on the backdrop presses or
//   highlights what lies under it (tried on every picture of a dialog, after the picture is taken).
// Then it saves the picture, prints one line per finding ("SELFCHECK FAIL: ...") or "SELFCHECK ok", and ends the
// run with 0 or 1.
//
// A picture taken with animations (--motion=full or reduced) first waits until nothing moves any more: two looks
// in a row find every item where it was, as big and as clear. It prints "MOTIONCHECK ok: ..." then, and fails when
// things still move after four seconds. tools/ci-shots.sh holds the picture against the one the same presses give
// with animations off: they must be the same picture. At Reduced it also fails when anything changed its place or
// its size over time (only fades are left there).
//
// With --check=<name> it first works the page with real keys, a real mouse and a finger, sent to the window the
// way the system sends them (Hub.testKey, Hub.testMouse, Hub.testTouch), so the handlers a hand reaches are the
// ones tried:
//   taps    a tap and a click open a list entry; a click on a dialog's row in the moment it appears does nothing;
//           a click or a tap on the backdrop never reaches the entry under it and closes the dialog; nothing
//           takes a click, a tap or a key while work runs
//   hold    Enter held down (the keyboard repeating it) acts once on the thing named by --focus: a switch flips
//           once, a dialog it opens stays unanswered; a direction held down does keep moving
//   hover   the mouse moved onto a row takes the highlight; after a key moved it and the page rebuilt under the
//           resting mouse it stays where the key put it; moving the mouse again takes it back
//   typing  with a search box typed in (Game Mode), the controller's presses do nothing but B, which only ends
//           the typing; Enter, Esc and a tap elsewhere end it too, and none of them goes back a page
//   mouse   the wheel scrolls, the Back button is B, the right button is X, Ctrl with the wheel and Ctrl with -
//           ask for another interface size, W A S D, Page Up, Home and End do what they say
// Each prints "INPUTCHECK ok: <name>" when nothing failed.
//
// app (required): Main.qml's window. target (required): where the picture goes.
Item {
    id: check
    required property var app
    required property string target
    property var problems: []

    readonly property string mode: Hub.option("check")
    function start() {
        const steps = mode === "" ? [] : script(mode)
        if (steps.length === 0) { settle.start(); return }
        failedBefore = problems.length
        todo = steps
        next()
    }
    Timer { id: settle; interval: check.app.motion.settle; onTriggered: { if (check.app.motion.on) check.watch(); else check.run() } }

    // ---------------------------------------------------------------- animations come to rest
    // Where everything is, how big and how clear: two looks that agree mean nothing moves.
    function pose() {
        const out = []
        function walk(it) {
            if (!it || it.visible === false) return
            out.push([it.x, it.y, it.width, it.height, it.opacity, it.scale, it.rotation, it.contentY === undefined ? 0 : it.contentY].map(v => Math.round(v * 1000) / 1000).join(","))
            const kids = it.children
            for (let i = 0; i < kids.length; i++) walk(kids[i])
        }
        walk(app.contentItem)
        return out.join(";")
    }
    property string lastPose: ""
    property int watched: 0
    property int stillFor: 0
    function watch() { lastPose = pose(); watched = 0; stillFor = 0; looker.start() }
    Timer {
        id: looker
        interval: check.app.motion.settleLook; repeat: true
        onTriggered: {
            check.watched += interval
            const now = check.pose()
            check.stillFor = now === check.lastPose && check.app.ready ? check.stillFor + 1 : 0
            check.lastPose = now
            if (check.stillFor >= 3) {
                stop()
                if (check.traveled !== "" && !check.app.motion.moves) check.fail("animations are at Reduced and something still traveled or grew: " + check.traveled)
                else Hub.say("MOTIONCHECK ok: at rest after " + check.watched + " ms (animations " + check.app.motion.level + (check.traveled !== "" ? ", seen moving: " + check.traveled : "") + ")")
                check.run()
            } else if (check.watched >= check.app.motion.settleLimit) {
                stop()
                check.fail("still moving after " + check.watched + " ms with animations " + check.app.motion.level)
                check.run()
            }
        }
    }
    // While a picture with animations is worked: did anything change its place or its size over time (a list
    // gliding, a knob sliding, a dialog growing)? At Full some of it does; at Reduced nothing may. Looked at every
    // frame from the first press on. What moves in one step (a press scrolling a list at once) isn't travel: travel
    // is the same thing found at three or more places in a row.
    property string traveled: ""
    property var trail: ({})
    FrameAnimation {
        running: check.app.motion.on && check.traveled === ""
        onTriggered: {
            function walk(it, path) {
                if (!it || it.visible === false || check.traveled !== "") return
                const key = path
                const now = [it.scale, it.contentY === undefined ? 0 : it.contentY, it.switchKnob === true ? it.x : 0]
                const was = check.trail[key]
                if (was) {
                    const moved = Math.abs(now[0] - was.v[0]) > 0.0005 || Math.abs(now[1] - was.v[1]) > 0.5 || Math.abs(now[2] - was.v[2]) > 0.5
                    was.run = moved ? was.run + 1 : 0
                    was.v = now
                    if (was.run >= 3) check.traveled = (it.fid || it.objectName || path) + (Math.abs(now[0] - 1) > 0.0005 ? " (scale)" : it.switchKnob === true ? " (a switch's knob)" : " (scrolling)")
                } else check.trail[key] = { v: now, run: 0 }
                const kids = it.children
                for (let i = 0; i < kids.length; i++) walk(kids[i], path + "/" + i)
            }
            walk(check.app.contentItem, "")
        }
    }

    // ---------------------------------------------------------------- real keys and a real mouse
    // The steps still to take: [milliseconds to wait after it, what to do].
    property var todo: []
    property int failedBefore: 0
    // What a script remembers from one step to the next.
    property var v: ({})
    Timer { id: stepper; onTriggered: check.next() }
    function next() {
        if (todo.length === 0) {
            if (problems.length === failedBefore) Hub.say("INPUTCHECK ok: " + mode)
            // The picture shows the hints the run started with.
            app.lastInput = ""
            settle.start()
            return
        }
        const step = todo.shift()
        try { step[1]() } catch (e) { fail("input check " + mode + " broke: " + e) }
        stepper.interval = step[0]
        stepper.start()
    }
    function middle(it) { const p = it.mapToItem(null, it.width / 2, it.height / 2); return Qt.point(p.x, p.y) }
    function moveTo(p) { Hub.testMouse(0, p.x, p.y, 0, 0, 0) }
    function click(p, button) {
        const b = button || Qt.LeftButton
        moveTo(p)
        Hub.testMouse(1, p.x, p.y, b, 0, 0)
        Hub.testMouse(2, p.x, p.y, b, 0, 0)
    }
    // A finger put down and lifted.
    function tap(p) { Hub.testTouch(0, p.x, p.y); Hub.testTouch(2, p.x, p.y) }
    function key(k, mods) { Hub.testKey(k, mods || 0, true, false); Hub.testKey(k, mods || 0, false, false) }
    // Everything that can take the highlight under `root`, also where it is turned off (under a dialog).
    function every(root) {
        const out = []
        function walk(it) {
            if (!it || !it.visible) return
            if (it.focusable === true) out.push(it)
            const kids = it.children
            for (let i = 0; i < kids.length; i++) walk(kids[i])
        }
        walk(root)
        return out
    }
    // Whether the row named `id` is on, read from the page as the engine gave it (null when it isn't there).
    function switchOn(j, id) {
        if (!j || typeof j !== "object") return null
        if (j.id === id && j.on !== undefined) return j.on === true
        for (const k in j) { const r = switchOn(j[k], id); if (r !== null) return r }
        return null
    }
    function byFid(fid) { return every(app.parts.view.item).find(x => x.fid === fid) || null }
    // The entries of a two-sided page's list that aren't the open one.
    function closedEntries() { return every(app.parts.view.item).filter(x => x.sideEntry === true && x.modelData !== undefined && !x.on) }
    function inCard(p) {
        const card = app.parts.dialog.cardItem, at = card.mapToItem(null, 0, 0)
        return p.x >= at.x - 6 && p.x <= at.x + card.width + 6 && p.y >= at.y - 6 && p.y <= at.y + card.height + 6
    }
    // Whether a place in the window is on the dialog's backdrop: beside the card, above the footer.
    function onBackdrop(p) {
        const d = app.parts.dialog, at = d.mapToItem(null, 0, 0)
        return !inCard(p) && p.x > at.x + 2 && p.x < at.x + d.width - 2 && p.y > at.y + 2 && p.y < at.y + d.height - 2
    }
    // The thing that takes the highlight at a place in the window.
    function itemAt(p) {
        return every(app.parts.view.item).find(x => { const at = x.mapToItem(null, 0, 0); return p.x >= at.x && p.x <= at.x + x.width && p.y >= at.y && p.y <= at.y + x.height }) || null
    }

    function script(name) {
        const dialog = app.parts.dialog
        const ready = 2 * app.motion.pressGuard
        const quiet = app.motion.hoverQuiet + 150
        if (name === "taps") {
            const poke = (how, p) => { if (how === "tap") tap(p); else click(p) }
            // With the menu up: a click, or a finger's tap, on an entry of the page that lies under the backdrop.
            const through = (how) => [
                [ready, () => { app.doAct("menu", "", "") }],
                [ready, () => {
                    const t = closedEntries().find(x => onBackdrop(middle(x)))
                    if (!t) { fail("taps: no list entry lies under the backdrop"); return }
                    v.index = app.model.index; v.hits = 0
                    t.activated.connect(() => { v.hits++ })
                    poke(how, middle(t))
                    // Looked at right away: what a tap does it does at once.
                    if (v.hits > 0 || app.model.index !== v.index) fail("taps: a " + how + " on the backdrop pressed the entry under it (" + t.fid + ")")
                    if (app.focusItem === t) fail("taps: a " + how + " on the backdrop moved the highlight to the entry under it (" + t.fid + ")")
                }],
                [0, () => {
                    if (app.model.index !== v.index) fail("taps: a " + how + " on the backdrop opened the entry under it")
                    if (dialog.open) fail("taps: a " + how + " on the backdrop didn't close the dialog")
                }],
            ]
            const opened = (how) => { if (app.taken === v.taken || app.model.index === v.index || app.lastFid !== v.fid) fail("taps: a " + how + " on a list entry didn't open it, so this check can tell nothing (" + (app.taken - v.taken) + " taps taken, highlight on " + app.lastFid + ")") }
            return [
                [ready, () => {}],
                // A finger's tap and a click on a list entry open it: the handlers the next steps rely on do fire.
                [ready, () => { const t = closedEntries()[0]; v.fid = t.fid; v.index = app.model.index; v.taken = app.taken; tap(middle(t)) }],
                // (The mouse arriving on an entry shows it already; the click then lands on it.)
                [300, () => {
                    opened("tap")
                    // A finger has no buttons: its tap is never the mouse's Back or right button.
                    if (app.lastPress === "b" || app.lastPress === "x") fail("taps: a finger's tap counted as the mouse's " + (app.lastPress === "b" ? "Back" : "right") + " button")
                    const t = closedEntries()[0]; v.fid = t.fid; v.index = app.model.index; v.at = middle(t); moveTo(v.at) }],
                [300, () => { v.taken = app.taken; click(v.at) }],
                [ready, () => { opened("click"); app.doAct("menu", "", "") }],
                // Where the menu's first row is, for the click made the moment the menu appears again.
                [ready, () => { const row = every(dialog).find(x => x.fid === "menu:0"); if (!row) fail("taps: the menu has no rows"); else v.row = middle(row); key(Qt.Key_Escape) }],
                [100, () => { if (dialog.open) fail("taps: Esc didn't close the menu"); v.page = app.current.page; app.doAct("menu", "", "") }],
                [0, () => { v.refused = app.refused; click(v.row) }],
                [ready, () => {
                    if (app.refused === v.refused) fail("taps: the click made as the menu appeared never reached its row, so this check can tell nothing")
                    if (!dialog.open || app.current.page !== v.page) fail("taps: a click on a row of the menu in the moment it appeared was taken")
                    key(Qt.Key_Escape)
                }],
            ].concat(through("click")).concat(through("tap")).concat([
                // While work runs nothing takes a click, a tap or a key.
                [ready, () => { v.index = app.model.index; v.fid = app.lastFid; Hub.busy = true }],
                [200, () => { const t = closedEntries()[0]; if (t) { click(middle(t)); tap(middle(t)) } key(Qt.Key_Down); key(Qt.Key_Return) }],
                [ready, () => {
                    if (app.model.index !== v.index || app.lastFid !== v.fid) fail("taps: a click, a tap or a key was taken while work runs")
                    Hub.busy = false
                }],
            ])
        }
        if (name === "hold") {
            const again = () => Hub.testKey(Qt.Key_Return, 0, true, true)
            return [
                [ready, () => {
                    const t = byFid(Hub.option("focus"))
                    if (!t) { fail("hold: nothing is named " + Hub.option("focus")); return }
                    app.setFocus(t, false)
                    v.toggle = t.kind === "toggle"; v.on = v.toggle ? t.m.on === true : false; v.fid = t.fid
                    // How often the switch flipped and how often the dialog came or went while Enter was down.
                    v.changes = 0; v.dialogs = 0; v.last = v.on
                    app.modelChanged.connect(() => { const now = check.switchOn(app.model, v.fid); if (now !== null && now !== v.last) { v.last = now; v.changes++ } })
                    dialog.openChanged.connect(() => { v.dialogs++ })
                }],
                // Enter goes down and the keyboard repeats it seven times, for longer than a new dialog leaves A out.
                [90, () => Hub.testKey(Qt.Key_Return, 0, true, false)],
                [90, again], [90, again], [90, again], [90, again], [90, again], [90, again], [90, again],
                [ready, () => Hub.testKey(Qt.Key_Return, 0, false, false)],
                [0, () => {
                    const t = byFid(v.fid)
                    if (v.toggle && (!t || (t.m.on === true) === v.on || v.changes !== 1)) fail("hold: Enter held down didn't flip the switch exactly once (" + v.fid + ", " + v.changes + " changes)")
                    if (!v.toggle && (!dialog.open || v.dialogs !== 1)) fail("hold: Enter held down answered the dialog its first press opened (" + v.fid + ", it came or went " + v.dialogs + " times)")
                    v.moves = 0
                    app.lastFidChanged.connect(() => { v.moves++ })
                }],
                // A direction held down does repeat.
                [90, () => { if (!dialog.open) Hub.testKey(Qt.Key_Up, 0, true, false) }],
                [90, () => { if (!dialog.open) Hub.testKey(Qt.Key_Up, 0, true, true) }],
                [90, () => { if (!dialog.open) Hub.testKey(Qt.Key_Up, 0, true, true) }],
                [ready, () => { if (!dialog.open) Hub.testKey(Qt.Key_Up, 0, false, false) }],
                [0, () => { if (!dialog.open && v.moves < 2) fail("hold: Up held down moved the highlight " + v.moves + " times, not once per repeat") }],
            ]
        }
        if (name === "hover") return [
            [quiet, () => moveTo(Qt.point(3, 3))],
            // The mouse moved onto a row takes the highlight.
            [200, () => {
                const rows = every(app.parts.view.item).filter(x => x.kind !== undefined && x !== app.focusItem)
                if (rows.length < 2) { fail("hover: the page has no two rows"); return }
                v.at = middle(rows[0]); v.mouse = rows[0].fid
                moveTo(v.at)
            }],
            [200, () => { if (app.lastFid !== v.mouse) fail("hover: the mouse moved onto a row didn't take the highlight (" + v.mouse + ", it is on " + app.lastFid + ")"); key(Qt.Key_Down); v.key = app.lastFid }],
            // A key moved it on; then the page rebuilds under the resting mouse, as it does after every change.
            [200, () => {
                if (v.key === v.mouse) { fail("hover: Down didn't move the highlight off the row under the mouse"); return }
                if (app.lastFid !== v.key) fail("hover: the resting mouse took the highlight back right after Down (from " + v.key + " to " + app.lastFid + ")")
                app.reloadPage()
                moveTo(v.at)
            }],
            [quiet, () => moveTo(v.at)],
            [200, () => { if (app.lastFid !== v.key) fail("hover: the resting mouse took the highlight back after a rebuild (from " + v.key + " to " + app.lastFid + ")") }],
            // The mouse really moving takes it again.
            [200, () => { v.at = Qt.point(v.at.x + 7, v.at.y + 3); const t = itemAt(v.at); v.mouse = t ? t.fid : ""; moveTo(v.at) }],
            [200, () => { if (v.mouse === "" || app.lastFid !== v.mouse) fail("hover: the mouse moving again didn't take the highlight (" + v.mouse + ", it is on " + app.lastFid + ")") }],
        ]
        if (name === "mouse") {
            const mid = Qt.point(app.width / 2, app.height / 2)
            const pressed = (k, what) => { if (app.lastPress !== k) fail("mouse: " + what + " wasn't " + k.toUpperCase() + " (it was " + (app.lastPress || "nothing") + ")"); app.lastPress = "" }
            return [
                [ready, () => { v.list = null; for (let p = app.focusItem; p; p = p.parent) if (p.contentY !== undefined && p.contentItem !== undefined) { v.list = p; break } }],
                // The wheel scrolls the list under the pointer (where there is more of it than fits).
                [200, () => { if (!v.list) return; v.y = v.list.contentY; const p = v.list.mapToItem(null, v.list.width / 2, v.list.height / 2); moveTo(p); Hub.testMouse(3, p.x, p.y, 0, 0, 2) }],
                [0, () => { if (v.list && v.list.contentHeight > v.list.height + 240 && v.list.contentY <= v.y) fail("mouse: the wheel didn't scroll the list") }],
                [200, () => { key(Qt.Key_End) }],
                [200, () => { v.end = app.lastFid; key(Qt.Key_Home) }],
                [200, () => { if (app.lastFid === v.end) fail("mouse: Home and End land on the same thing"); v.home = app.lastFid; key(Qt.Key_S) }],
                [200, () => { pressed("down", "S"); if (app.lastFid === v.home) fail("mouse: S didn't move the highlight down"); key(Qt.Key_W) }],
                [200, () => { pressed("up", "W"); if (app.lastFid !== v.home) fail("mouse: W didn't move the highlight back up"); key(Qt.Key_A) }],
                [200, () => { pressed("left", "A"); key(Qt.Key_D) }],
                [200, () => { pressed("right", "D"); click(mid, Qt.RightButton) }],
                [200, () => { pressed("x", "the right button"); v.scale = app.st.scale; Hub.testMouse(3, mid.x, mid.y, 0, Qt.ControlModifier, -1) }],
                [ready, () => { if (app.st.scale !== v.scale + 10 || !dialog.open) fail("mouse: Ctrl with the wheel didn't ask for a bigger interface (" + v.scale + " to " + app.st.scale + ")"); key(Qt.Key_Escape) }],
                [ready, () => { if (app.st.scale !== v.scale || dialog.open) fail("mouse: Esc didn't put the size back"); key(Qt.Key_Minus, Qt.ControlModifier) }],
                [ready, () => { if (app.st.scale !== v.scale - 10 || !dialog.open) fail("mouse: Ctrl with - didn't ask for a smaller interface (" + v.scale + " to " + app.st.scale + ")"); key(Qt.Key_Escape) }],
                [ready, () => { if (app.st.scale !== v.scale) fail("mouse: the size didn't go back"); v.tab = app.tab; key(Qt.Key_PageUp) }],
                [ready, () => { pressed("lb", "Page Up"); if (app.tab !== v.tab - 1) fail("mouse: Page Up didn't switch to the tab before"); click(mid, Qt.BackButton) }],
                [ready, () => {
                    pressed("b", "the mouse's Back button"); if (app.tab !== 0) fail("mouse: the mouse's Back button didn't go back")
                    // The hints follow the last input: keys after the mouse or a key, the controller's after a press on it.
                    if (!app.keyboard) fail("mouse: the hints didn't turn to keys after the mouse")
                    app.padPress("down")
                    if (app.keyboard) fail("mouse: the hints didn't turn to the controller's after a press on it")
                    key(Qt.Key_Up)
                    if (!app.keyboard) fail("mouse: the hints didn't turn to keys after a key")
                }],
            ]
        }
        if (name === "typing") {
            const hints = () => app.parts.bottomBar.hints
            const start = () => { key(Qt.Key_Slash) }
            const typingNow = (what) => { if (!app.typing) fail("typing: " + what + " ended the typing") }
            const typingOver = (what) => { if (app.typing) fail("typing: " + what + " didn't end the typing") }
            const samePlace = (what) => { if (app.tab !== v.tab || app.stack.length !== v.depth || dialog.open) fail("typing: " + what + " also went back a page") }
            return [
                [ready, () => { v.tab = app.tab; v.depth = app.stack.length; start() }],
                // The search box has the keyboard: the footer says so, and the hints stay the controller's in Game Mode.
                [200, () => {
                    typingNow("nothing")
                    if (hints().length !== 1 || hints()[0].k !== "b" || hints()[0].label !== "Done typing") fail("typing: the footer doesn't say Done typing")
                    if (Hub.gamemode && app.keyboard) fail("typing: a key turned the hints to a keyboard's in Game Mode")
                    v.fid = app.lastFid
                    // The controller belongs to Steam's keyboard now: its presses do nothing here.
                    app.padPress("down"); app.padPress("a"); app.padPress("lb"); app.padPress("start")
                }],
                [200, () => {
                    typingNow("a press meant for Steam's keyboard")
                    samePlace("a press meant for Steam's keyboard")
                    if (app.lastFid !== v.fid) fail("typing: a press meant for Steam's keyboard moved the highlight")
                    app.padPress("b")
                }],
                // B ended the typing and did nothing else: no page back, no question to close.
                [ready, () => { typingOver("B"); samePlace("B"); start() }],
                // Enter (the number pad's too) ends it.
                [200, () => { typingNow("nothing"); key(Qt.Key_Enter) }],
                [ready, () => { typingOver("Enter"); samePlace("Enter"); start() }],
                // A finger's tap on something else ends it, and the highlight goes there.
                [200, () => { typingNow("nothing"); const t = closedEntries()[0]; v.entry = t.fid; tap(middle(t)) }],
                [ready, () => { typingOver("a tap elsewhere"); if (app.lastFid !== v.entry) fail("typing: a tap elsewhere didn't take the highlight"); start() }],
                // Esc ends it too, and only that.
                [200, () => { typingNow("nothing"); key(Qt.Key_Escape) }],
                [ready, () => { typingOver("Esc"); samePlace("Esc") }],
            ]
        }
        fail("no input check is called " + name)
        return []
    }

    function fail(text) { if (problems.indexOf(text) < 0 && problems.length < 25) problems.push(text) }
    function words(it) { const t = String(it.text); return "\"" + (t.length > 40 ? t.substring(0, 40) + "..." : t) + "\"" }

    // The nearest thing above `it` that clips what it draws: { item, scrolls }.
    function clipper(it) {
        let p = it.parent
        while (p) {
            if (p.clip === true) return { item: p, scrolls: p.contentY !== undefined }
            p = p.parent
        }
        return null
    }

    function look(it, counts) {
        if (!it || it.visible === false || it.opacity === 0) return
        const isText = it.truncated !== undefined && it.text !== undefined && it.font !== undefined
        if (isText && String(it.text) !== "" && it.width > 0) {
            counts.texts++
            const at = it.mapToItem(null, 0, 0)
            const w = Math.min(it.width, it.contentWidth > 0 ? it.contentWidth : it.width)
            if (it.truncated) fail("cut off: " + words(it))
            if (it.wrapMode === Text.NoWrap && it.contentWidth > it.width + 1 && it.width > 0 && it.elide === Text.ElideNone && it.fontSizeMode === Text.FixedSize) fail("wider than its box: " + words(it))
            if (it.font.pixelSize < app.textFloor) fail("too small (" + it.font.pixelSize + " px, the floor is " + app.textFloor + "): " + words(it))
            // Words taller or wider than the box drawn around them (a box that didn't grow with the text floor).
            const frame = boxAround(it)
            if (frame) {
                const fw = Math.min(it.width, it.contentWidth), fh = it.contentHeight
                if (fh > frame.height + 1.5) fail("taller than its box (" + Math.round(fh) + " in " + Math.round(frame.height) + " px): " + words(it))
                if (fw > frame.width + 1.5) fail("wider than its box (" + Math.round(fw) + " in " + Math.round(frame.width) + " px): " + words(it))
            }
            // Words read on what they are drawn on.
            const ink = inkOf(it)
            if (ink !== null) {
                const ground = groundOf(it)
                const ratio = app.colors.contrast(ink, ground)
                counts.contrast = Math.min(counts.contrast, ratio)
                if (ratio < app.colors.enough - 0.005) fail("hard to read (" + ratio.toFixed(2) + " to 1, " + ink + " on " + ground + "): " + words(it))
            }
            // Words squeezed into a column so narrow that each line holds about one word.
            const count = String(it.text).trim().split(/\s+/).length
            if (it.wrapMode !== Text.NoWrap && it.lineCount >= 3 && count >= 3 && it.lineCount >= count * 0.8) fail("squeezed to a word a line: " + words(it))
            const c = clipper(it)
            // Where its words are painted, cut to what clips it, for the overlap check.
            if (!counts.covered) {
                const cw = Math.min(it.width, it.contentWidth), ch = Math.min(it.height, it.contentHeight)
                let x0 = at.x + (it.horizontalAlignment === Text.AlignHCenter ? (it.width - cw) / 2 : it.horizontalAlignment === Text.AlignRight ? it.width - cw : 0)
                let y0 = at.y + (it.verticalAlignment === Text.AlignVCenter ? (it.height - ch) / 2 : it.verticalAlignment === Text.AlignBottom ? it.height - ch : 0)
                let x1 = x0 + cw, y1 = y0 + ch
                for (let p = it.parent; p; p = p.parent) {
                    if (p.clip !== true) continue
                    const b = p.mapToItem(null, 0, 0)
                    x0 = Math.max(x0, b.x); y0 = Math.max(y0, b.y); x1 = Math.min(x1, b.x + p.width); y1 = Math.min(y1, b.y + p.height)
                }
                if (x1 - x0 > 2 && y1 - y0 > 2) counts.boxes.push({ it: it, x0: x0, y0: y0, x1: x1, y1: y1 })
            }
            if (c) {
                const box = c.item.mapToItem(null, 0, 0)
                if (at.x < box.x - 1 || at.x + w > box.x + c.item.width + 1) fail("sticks out sideways of its list: " + words(it) + " at " + Math.round(at.x) + " w " + Math.round(w) + " box " + Math.round(box.x) + "+" + Math.round(c.item.width))
                if (!c.scrolls && (at.y < box.y - 1 || at.y + it.height > box.y + c.item.height + 1)) fail("clipped: " + words(it))
            } else if (at.x < -1 || at.x + w > app.width + 1 || at.y < -1 || at.y + it.height > app.height + 1) fail("outside the window: " + words(it) + " at " + Math.round(at.x) + "," + Math.round(at.y) + " " + Math.round(w) + "x" + Math.round(it.height))
        }
        if (it.focusable === true) {
            counts.controls++
            const at = it.mapToItem(null, 0, 0)
            if (at.x < -1 || at.x + it.width > app.width + 1) fail("a control sticks out of the window (" + (it.fid || "?") + ")")
            if (it.width < 8 || it.height < 8) fail("a control has no size (" + (it.fid || "?") + ")")
            // A finger fits on it (things that take no tap themselves are words to read: their row takes it).
            else if ((it.aLabel !== "" || it.tapActivates === false) && it.fid !== "list" && (it.height < app.touchFloor - 0.5 || it.width < app.touchFloor - 0.5)) fail("too small for a finger (" + Math.round(it.width) + " x " + Math.round(it.height) + ", the floor is " + app.touchFloor + "): " + (it.fid || "?"))
            if (String(it.sayName || "").trim() === "") fail("nothing for a screen reader to say (" + (it.fid || "?") + ")")
            else counts.named++
        }
        if (it.tapTarget === true && (it.height < app.touchFloor - 0.5 || it.width < app.touchFloor - 0.5)) fail("too small for a finger (" + Math.round(it.width) + " x " + Math.round(it.height) + ", the floor is " + app.touchFloor + "): " + (it.Accessible.name || "a tab or a hint"))
        // A state never rests on color alone: its chip has its mark and its word.
        if (it.stateChip === true && ["ok", "warn", "stop"].indexOf(it.tone) >= 0) {
            counts.chips++
            if (!it.marked) fail("a " + it.tone + " chip has no mark, only its color tells (" + it.text + ")")
            if (String(it.text).trim() === "") fail("a " + it.tone + " chip has no word")
        }
        // Things drawn over the page on purpose (a toast, a pinned heading) aren't overlaps.
        const was = counts.covered
        if (it.cover === true) counts.covered = true
        const kids = it.children
        for (let i = 0; i < kids.length; i++) look(kids[i], counts)
        counts.covered = was
    }
    // ---------------------------------------------------------------- what words are drawn on
    function isBox(it) { return it.color !== undefined && it.radius !== undefined && it.border !== undefined && it.text === undefined }
    // The filled or outlined box right around some words (their parent, or its parent when the words sit in a row).
    function boxAround(it) {
        let p = it.parent
        for (let hops = 0; p && hops < 2; hops++, p = p.parent) {
            if (isBox(p) && (p.color.a > 0 || p.border.width > 0) && p.height > 0) return p
            // Only a plain row or the box itself may sit between: a layout that wraps decides its own height.
            if (p.spacing === undefined || p.columns !== undefined) return null
        }
        return null
    }
    // The color of some words as they reach the eye: null for words that are faded on purpose (a button that can't
    // be pressed yet) or drawn in no color at all.
    function inkOf(it) {
        let strength = 1
        for (let p = it; p; p = p.parent) strength *= p.opacity === undefined ? 1 : p.opacity
        if (strength < 0.6 || it.color.a < 0.6) return null
        return it.color
    }
    // What lies under the middle of some words: every filled box below them, nearest first, down to the first one
    // that lets nothing through, laid over each other the way they are drawn.
    function groundOf(it) {
        const at = it.mapToItem(null, it.width / 2, it.height / 2)
        const layers = []
        const covers = (b) => {
            if (!isBox(b) || b.visible === false || b.color.a <= 0 || b.opacity <= 0) return false
            const o = b.mapToItem(null, 0, 0)
            return at.x >= o.x && at.x <= o.x + b.width && at.y >= o.y && at.y <= o.y + b.height
        }
        let solid = false
        let child = it
        for (let p = it.parent; p && !solid; child = p, p = p.parent) {
            const kids = p.children
            let mine = -1
            for (let i = 0; i < kids.length; i++) if (kids[i] === child) { mine = i; break }
            // What is drawn before it among its siblings lies under it.
            for (let i = kids.length - 1; i >= 0 && !solid; i--) {
                const k = kids[i]
                if (k === child) continue
                const below = k.z < child.z || (k.z === child.z && i < mine)
                if (below && covers(k)) { layers.push(Qt.rgba(k.color.r, k.color.g, k.color.b, k.color.a * k.opacity)); solid = k.color.a * k.opacity >= 0.999 }
            }
            if (!solid && covers(p)) { layers.push(Qt.rgba(p.color.r, p.color.g, p.color.b, p.color.a * p.opacity)); solid = p.color.a * p.opacity >= 0.999 }
        }
        let ground = app.colors.ground
        for (let i = layers.length - 1; i >= 0; i--) ground = app.colors.over(layers[i], ground)
        return ground
    }

    // ---------------------------------------------------------------- every theme, accent and color vision
    // How a color reaches red-weak, green-weak or blue-weak eyes (Machado, Oliveira and Fernandes 2009, full
    // strength), as [r, g, b] in the screen's own values.
    readonly property var eyes: ({
        red: [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
        green: [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
        blue: [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]] })
    function seen(col, vision) {
        const lin = v => v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        const l = [lin(col.r), lin(col.g), lin(col.b)]
        const m = eyes[vision]
        return m ? m.map(r => Math.max(0, Math.min(1, r[0] * l[0] + r[1] * l[1] + r[2] * l[2]))) : l
    }
    // How far apart two colors look (CIE76 in Lab): about 2 is the least an eye notices side by side; states have
    // to be told apart at a glance and far from each other, so they keep 15 or more.
    function apart(a, b, vision) {
        const lab = l => {
            const f = t => t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116
            const x = f((0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2]) / 0.95047), y = f(0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]), z = f((0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]) / 1.08883)
            return [116 * y - 16, 500 * (x - y), 200 * (y - z)]
        }
        const p = lab(seen(a, vision)), q = lab(seen(b, vision))
        return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2])
    }
    readonly property real statesApart: 15
    Component { id: paletteMaker; Palette { } }
    // Returns how many combinations were measured.
    function palettes() {
        let n = 0
        const p = paletteMaker.createObject(check)
        if (!p) { fail("the colors couldn't be made for checking"); return 0 }
        for (const theme of ["dark", "black", "light", "contrast"]) for (const accent of ["brass", "iron", "copper", "jade"]) for (const vision of ["typical", "red", "green", "blue", "none"]) {
            p.themeName = theme; p.accentName = accent; p.visionName = vision
            n++
            const where = theme + ", " + accent + ", color vision " + vision + ": "
            const need = (name, ink, on, ground) => { const r = p.contrast(ink, ground); if (r < p.enough - 0.005) fail(where + name + " on " + on + " is " + r.toFixed(2) + " to 1 (" + ink + " on " + ground + ")") }
            const surfaces = { ground: p.ground, bar: p.bar, panel: p.panel, card: p.card, raised: p.raised }
            const inks = { text: p.text, soft: p.soft, muted: p.muted, accentText: p.accentText, ok: p.ok, warn: p.warn, stop: p.stop }
            for (const i in inks) for (const s in surfaces) need(i, inks[i], s, surfaces[s])
            need("accentInk", p.accentInk, "accent", p.accent)
            for (const t of ["ok", "warn", "stop", ""]) need("a " + (t || "plain") + " chip's words", p.readable(p.tone(t), p.toneFill(t)), "its fill", p.toneFill(t))
            for (const t of ["ok", "warn", "stop"]) need("text", p.text, t + "Fill", p.toneFill(t))
            for (const k in p.pad) need("a face button's letter", p.padInk, "the " + k.toUpperCase() + " button", p.padOf(k))
            // The ring of the highlight and a picked option's outline are seen, not read: 3 to 1 is the rule for those.
            for (const s in surfaces) if (p.contrast(p.ring, surfaces[s]) < 3) fail(where + "the highlight ring on " + s + " is " + p.contrast(p.ring, surfaces[s]).toFixed(2) + " to 1")
            if (vision === "none") {
                // No color: the states are drawn in the words' own colors, and the accent is a grey.
                if (!Qt.colorEqual(p.ok, p.soft) || !Qt.colorEqual(p.warn, p.text) || !Qt.colorEqual(p.stop, p.text)) fail(where + "a state has a color of its own")
                const grey = (col) => Math.abs(col.r - col.g) < 0.02 && Math.abs(col.g - col.b) < 0.02
                if (!grey(p.accent) || !grey(p.ring) || !grey(p.padOf("a"))) fail(where + "the accent, the ring or a face button still has a color")
            } else {
                const pairs = [["ok", "warn"], ["warn", "stop"], ["ok", "stop"]]
                for (const pair of pairs) {
                    const d = apart(p[pair[0]], p[pair[1]], vision === "typical" ? "" : vision)
                    if (d < statesApart) fail(where + pair[0] + " and " + pair[1] + " look alike to these eyes (" + d.toFixed(1) + " apart, " + statesApart + " is the least)")
                }
            }
        }
        p.destroy()
        return n
    }

    // Words drawn over other words.
    function overlaps(boxes) {
        for (let i = 0; i < boxes.length; i++) {
            for (let j = i + 1; j < boxes.length; j++) {
                const a = boxes[i], b = boxes[j]
                const w = Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0), h = Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0)
                if (w > 4 && h > 4) fail("words over words: " + words(a.it) + " and " + words(b.it))
            }
        }
    }

    // The picture is taken first: taking it lays everything out and draws it, so the checks measure exactly what
    // the picture shows.
    function run() {
        const ok = app.contentItem.grabToImage(function (result) {
            // A check that breaks is a failed check, never a run that waits for ever.
            let counts = { texts: 0, controls: 0 }
            try { counts = check.measure() } catch (e) { check.fail("the checks broke: " + e + (e.lineNumber ? " (line " + e.lineNumber + ")" : "")) }
            if (!result.saveToFile(check.target)) check.fail("the picture couldn't be saved to " + check.target)
            try { check.probe(counts) } catch (e) { check.fail("the checks broke: " + e); check.finish(counts) }
        })
        if (!ok) { fail("the picture couldn't be taken"); finish({ texts: 0, controls: 0 }) }
    }

    function measure() {
        const counts = { texts: 0, controls: 0, boxes: [], covered: false, contrast: 21, chips: 0, named: 0, palettes: 0 }
        look(app.contentItem, counts)
        counts.palettes = palettes()
        // What a screen reader finds, asked the way it asks.
        counts.spoken = Hub.testSpoken(Hub.option("spoken") !== "" ? 4 : 0)
        const nameless = Hub.testSpoken(1), lit = Hub.testSpoken(2)
        if (counts.spoken < 0) fail("the screen reader's view of the window can't be read")
        else {
            if (nameless > 0) fail("a screen reader finds " + nameless + " controls with no name")
            if (counts.controls > 0 && counts.spoken === 0) fail("a screen reader finds no controls at all")
            if (lit > 1) fail("a screen reader finds " + lit + " things that say they have the highlight")
            if (counts.controls > 0 && app.focusItem && lit < 1) fail("a screen reader isn't told where the highlight is")
        }
        // With a dialog open, the page behind it is under the backdrop: only the dialog's own words count.
        overlaps(app.parts.dialog.open ? counts.boxes.filter(b => { for (let p = b.it; p; p = p.parent) if (p === app.parts.dialog) return true; return false }) : counts.boxes)
        const keys = app.parts.bottomBar.shownKeys
        const hints = app.parts.bottomBar.hints
        // A shows wherever the highlighted thing takes A, and only there.
        const takesA = app.focusItem && app.focusItem.aLabel && !app.typing
        if (takesA && !(app.parts.dialog.open && app.focusItem.fid === "cancel") && keys.indexOf("a") < 0) fail("the footer has no A hint")
        if (!takesA && keys.indexOf("a") >= 0) fail("the footer offers A where A does nothing")
        if (keys.indexOf("b") < 0) fail("the footer has no B hint")
        for (const h of hints) if (!h.label && h.k !== "a") fail("the footer's " + h.k + " hint has no words")
        if (counts.controls > 0) {
            const f = app.focusItem
            if (!f) fail("nothing holds the highlight")
            else {
                const at = f.mapToItem(null, 0, 0)
                if (at.y < -1 || at.y + f.height > app.height - app.parts.bottomBar.height + 1) fail("the highlighted control isn't fully in view (" + (f.fid || "?") + ")")
            }
        }
        if (counts.texts === 0) fail("the page is empty")
        return counts
    }

    // Nothing goes through a pop-up: with a dialog open, the page is off, and a finger's tap on something of the page
    // that lies under the backdrop neither presses it nor moves the highlight to it. (The tap closes the dialog,
    // as a tap beside it does, so this comes after the picture.)
    property int probeHits: 0
    function probe(counts) {
        const dialog = app.parts.dialog
        if (dialog.open) {
            if (app.parts.view.enabled) fail("the page under the dialog still takes taps")
            const t = every(app.parts.view.item).find(x => x.aLabel !== "" && x.tapActivates && onBackdrop(middle(x)))
            if (t) {
                probeHits = 0
                t.activated.connect(() => { check.probeHits++ })
                tap(middle(t))
                // Looked at right away: what a tap does it does at once, and the highlight is only put back on the
                // page (wherever it was) a moment after the dialog closes.
                if (probeHits > 0) fail("a tap on the dialog's backdrop pressed what is under it (" + (t.fid || "?") + ")")
                if (app.focusItem === t) fail("a tap on the dialog's backdrop moved the highlight under it (" + (t.fid || "?") + ")")
            }
        }
        finish(counts)
    }

    function finish(counts) {
        for (const p of problems) Hub.say("SELFCHECK FAIL: " + p)
        if (problems.length === 0) Hub.say("SELFCHECK ok: " + counts.texts + " texts, " + counts.controls + " controls, " + (counts.spoken || 0) + " spoken, least contrast " + (counts.contrast === undefined ? 0 : counts.contrast).toFixed(2) + ", " + (counts.chips || 0) + " state chips, " + (counts.palettes || 0) + " palettes")
        Qt.exit(problems.length === 0 ? 0 : 1)
    }
}
