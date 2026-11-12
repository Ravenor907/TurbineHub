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
//   mouse   the wheel scrolls, the Back button is B, the right button opens Options, Ctrl with the wheel and Ctrl
//           with - ask for another interface size, W A S D, Page Up, Home and End do what they say
//   pointer the right stick (fed through Hub.testSticks, looked at as the real sticks are) shows the pointer and
//           moves it, a resting stick doesn't; the pointer on a row takes the highlight; R3 clicks through the
//           same gate as a tap, once; R3 held fills the ring and opens Options for what is under the pointer, and
//           letting go then does nothing; it clicks a row of a dialog, and never what lies under a dialog; LT is
//           the right button while it shows; a key, the D-pad and the left stick send it away; the left stick
//           swept round scrolls and presses no direction; with the pointer hidden R3 is A and R3 held is Options
//   drag    a slider's knob follows a mouse that drags it and the value is sent once, when it is let go; the
//           controller's pointer drags it the same way with RT held
//   options R opens Options for the highlighted game with its own press first, Play with its button and Help
//           last, and R closes it again; a right-click opens them for the row under the mouse and for the page
//           beside every row; F and a typed letter start Find; Tab opens the menu and moves through it; F1 is
//           Help; RT jumps further than Down; Space is A and Space held plays; Ctrl+Enter is Start
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
                [ready, () => { pressed("x", "the right button"); if (!dialog.open || dialog.d.options !== true) fail("mouse: the right button didn't open Options"); key(Qt.Key_Escape) }],
                [ready, () => { if (dialog.open) fail("mouse: Esc didn't close Options"); v.scale = app.st.scale; Hub.testMouse(3, mid.x, mid.y, 0, Qt.ControlModifier, -1) }],
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
        if (name === "drag") {
            const slider = () => every(app.parts.view.item).find(x => x.kind === "slider") || null
            // Where the knob is, and a place a third of the track away from it, toward the roomier side.
            const knob = (t) => { const tr = t.trackItem; return tr.mapToItem(null, (tr.width - app.px(22)) * tr.part + app.px(11), tr.height / 2) }
            const away = (t) => { const k = knob(t); return Qt.point(k.x + t.trackItem.width * (t.trackItem.part > 0.5 ? -0.33 : 0.33), k.y) }
            return [
                [ready, () => { const t = slider(); if (!t) { fail("drag: the page has no slider"); return } v.fid = t.fid; app.setFocus(t, false) }],
                [ready, () => {
                    const t = byFid(v.fid); if (!t) return
                    v.val = t.m.value; v.sent = 0
                    app.modelChanged.connect(() => { v.sent++ })
                    const from = knob(t), to = away(t)
                    // A real mouse: down on the knob, moved with the button held, let go.
                    Hub.pointerEvent(0, from.x, from.y, 0, 0, 0)
                    Hub.pointerEvent(1, from.x, from.y, Qt.LeftButton, Qt.LeftButton, 0)
                    for (let i = 1; i <= 10; i++) Hub.pointerEvent(0, from.x + (to.x - from.x) * i / 10, from.y, 0, Qt.LeftButton, 0)
                    if (t.val === v.val) fail("drag: the knob didn't follow the mouse (" + t.val + ")")
                    if (v.sent > 0) fail("drag: the value was sent while the knob was still held")
                    Hub.pointerEvent(2, to.x, to.y, Qt.LeftButton, 0, 0)
                }],
                [ready, () => {
                    const t = byFid(v.fid)
                    if (!t || t.m.value === v.val) fail("drag: letting go didn't send the new value")
                    if (!t) return
                    v.val = t.m.value
                    // The controller's pointer: RT held is the button held.
                    if (!steer(knob(t))) return
                    app.padEvent("rt", true)
                    steer(away(t))
                    if (t.val === v.val) fail("drag: the knob didn't follow the pointer with RT held (" + t.val + ")")
                    app.padEvent("rt", false)
                }],
                [ready, () => {
                    const t = byFid(v.fid)
                    if (!t || t.m.value === v.val) fail("drag: RT let go didn't send the new value")
                    if (!app.ptr.shown) fail("drag: the pointer hid while it dragged")
                    if (app.ptrButtons !== 0) fail("drag: a button is still held at the pointer")
                }],
            ]
        }
        if (name === "pointer") {
            const r3 = () => { app.padEvent("r3", true); app.padEvent("r3", false) }
            const hasHint = (k) => app.parts.bottomBar.hints.some(h => h.k === k)
            // (The switch named by --focus first: one that simply flips, without a question.)
            const toggles = () => every(app.parts.view.item).filter(x => x.kind === "toggle").sort((a, b) => (b.fid === Hub.option("focus") ? 1 : 0) - (a.fid === Hub.option("focus") ? 1 : 0))
            const isOn = () => switchOn(app.model, v.fid)
            return [
                [ready, () => {
                    if (app.ptr.shown) fail("pointer: it shows before the right stick moved")
                    sticks(0, 0, 0.1, -0.08, 400)
                    if (app.ptr.shown) fail("pointer: a stick resting a little off center showed it")
                    if (hasHint("r3")) fail("pointer: the footer offers its click while it is hidden")
                    sticks(0, 0, 1, 0, 160)
                }],
                [100, () => {
                    if (!app.ptr.shown || app.ptr.x <= app.width / 2 + 20) fail("pointer: the right stick didn't show it and move it right")
                    if (Math.abs(app.ptr.y - app.height / 2) > 1) fail("pointer: a push to the right also moved it up or down")
                    if (!hasHint("r3")) fail("pointer: the footer doesn't say what R3 does while it shows")
                    const t = toggles()[0]
                    if (!t) { fail("pointer: the page has no switch to try"); return }
                    v.fid = t.fid; v.title = t.optionsTitle; v.label = t.optionLabel || t.aLabel; v.on = isOn()
                    steer(middle(t))
                }],
                // On a row it takes the highlight, as the mouse does; R3 presses it, through the gate, once.
                [100, () => {
                    if (app.lastFid !== v.fid) fail("pointer: moved onto a row it didn't take the highlight (" + v.fid + ", it is on " + app.lastFid + ")")
                    if (app.keyboard) fail("pointer: its moves turned the hints to a keyboard's")
                    v.taken = app.taken
                    r3()
                }],
                [ready, () => {
                    if (app.taken !== v.taken + 1) fail("pointer: R3 didn't ask the gate exactly once (" + (app.taken - v.taken) + " times)")
                    if (isOn() === v.on) fail("pointer: R3 didn't press what is under it (" + v.fid + ")")
                    if (!app.ptr.shown) fail("pointer: it hid at its own click")
                    if (app.keyboard) fail("pointer: its click turned the hints to a keyboard's")
                    v.on = isOn()
                    // The row was built again by its press: the pointer is still on it.
                    const t = byFid(v.fid); if (t) { v.label = t.optionLabel || t.aLabel; steer(middle(t)) }
                }],
                // Held: the ring fills, then Options for what is under it; letting go afterwards does nothing more.
                [Math.round(app.motion.holdClick * 0.6), () => { app.padEvent("r3", true) }],
                [Math.round(app.motion.holdClick * 0.6) + 100, () => { if (app.holding !== "r3" || app.holdPart < 0.4) fail("pointer: no ring fills while R3 is held (" + app.holding + " " + app.holdPart + ")"); if (dialog.open) fail("pointer: Options came before the hold was through") }],
                [ready, () => {
                    if (!dialog.open || dialog.d.options !== true) { fail("pointer: R3 held didn't open Options"); return }
                    if (dialog.d.title !== v.title) fail("pointer: Options aren't for what is under it (" + dialog.d.title + ", not " + v.title + ")")
                    if (dialog.d.rows[0].title !== v.label || dialog.d.rows[0].local !== "a") fail("pointer: the thing's own press isn't first in its Options (" + dialog.d.rows[0].title + ")")
                    app.padEvent("r3", false)
                }],
                [ready, () => {
                    if (!dialog.open) fail("pointer: letting go of the held R3 closed Options")
                    if (isOn() !== v.on) fail("pointer: R3 held also pressed what is under it")
                    const row = every(dialog).find(x => x.fid === "menu:0")
                    if (!row || !steer(middle(row))) return
                    r3()
                }],
                // It works over a dialog: the first row of Options is the switch's own press.
                [ready, () => {
                    if (dialog.open) fail("pointer: R3 on a row of Options didn't choose it")
                    if (isOn() === v.on) fail("pointer: the first row of Options didn't press the switch")
                    app.doAct("menu", "", "")
                }],
                // And never through one: R3 on an entry that lies under the backdrop only closes the dialog.
                [ready, () => {
                    const t = closedEntries().find(x => onBackdrop(middle(x)))
                    if (!t) { fail("pointer: no list entry lies under the backdrop"); return }
                    v.index = app.model.index; v.focus = app.lastFid
                    steer(middle(t))
                    if (app.model.index !== v.index) fail("pointer: moved over the backdrop it opened the entry under it")
                    r3()
                    if (app.model.index !== v.index) fail("pointer: R3 on the backdrop pressed the entry under it")
                }],
                [ready, () => {
                    if (dialog.open) fail("pointer: R3 on the backdrop didn't close the dialog")
                    if (app.model.index !== v.index) fail("pointer: R3 on the backdrop opened the entry under it")
                    // Off the list (the entry it rests on shows as soon as it moves again, as under a mouse).
                    steer(Qt.point(app.width * 0.75, app.parts.topBar.height + 6))
                }],
                [ready, () => {
                    const t = every(app.parts.view.item).find(x => x.kind !== undefined && x.aLabel !== "")
                    if (!t) { fail("pointer: the page has nothing to press"); return }
                    v.title = t.optionsTitle; v.hits = 0
                    t.activated.connect(() => { v.hits++ })
                    steer(middle(t))
                }],
                // While it shows LT is the right button.
                [100, () => { app.padEvent("lt", true); app.padEvent("lt", false) }],
                [ready, () => {
                    if (!dialog.open || dialog.d.options !== true || dialog.d.title !== v.title) fail("pointer: LT didn't open Options for what is under it (" + (dialog.open ? dialog.d.title : "no dialog") + ", not " + v.title + ")")
                    if (v.hits > 0) fail("pointer: LT also pressed what is under it")
                    // A key: the keyboard is in use, the pointer goes.
                    key(Qt.Key_Escape)
                }],
                [ready, () => {
                    if (dialog.open) fail("pointer: Esc didn't close Options")
                    if (app.ptr.shown) fail("pointer: a key didn't send it away")
                    sticks(0, 0, -1, 0, 80)
                    if (!app.ptr.shown) fail("pointer: the right stick didn't bring it back")
                    // The left stick pushed one way is the D-pad: one direction, and the pointer goes.
                    v.moves = 0; app.lastPress = ""
                    sticks(0, 1, 0, 0, 200)
                    if (app.lastPress !== "down") fail("pointer: the left stick pushed down wasn't Down (" + (app.lastPress || "nothing") + ")")
                    if (app.ptr.shown) fail("pointer: the D-pad didn't send it away")
                    if (hasHint("r3")) fail("pointer: the footer still offers its click once it is hidden")
                    sticks(0, 0, 0, 0, 60)
                    if (app.heldDir !== "") fail("pointer: the left stick let go still holds " + app.heldDir)
                }],
                // Swept round the rim it is a scroll wheel, and presses no direction.
                [200, () => {
                    v.list = null
                    for (let p = app.focusItem; p; p = p.parent) if (p.contentY !== undefined && p.contentItem !== undefined) { v.list = p; break }
                    if (v.list) v.list.contentY = 0
                    v.y = 0; app.lastPress = ""
                    for (let a = -90; a <= 90; a += 12) sticks(Math.cos(a * Math.PI / 180) * 0.9, Math.sin(a * Math.PI / 180) * 0.9, 0, 0, 16)
                    sticks(0, 0, 0, 0, 40)
                    if (app.lastPress !== "") fail("pointer: the left stick swept round pressed " + app.lastPress)
                }],
                [300, () => {
                    if (v.list && v.list.contentHeight > v.list.height + 240 && v.list.contentY <= v.y) fail("pointer: the left stick swept round didn't scroll the list")
                    if (v.list) v.list.contentY = 0
                    // Hidden, R3 is A on the highlight, and R3 held is its Options.
                    const t = every(app.parts.view.item).find(x => x.kind !== undefined && x.aLabel !== "")
                    if (!t) { fail("pointer: the page has nothing to press"); return }
                    v.fid = t.fid; v.title = t.optionsTitle; v.own = 0
                    t.activated.connect(() => { v.own++ })
                    app.setFocus(t, false)
                    r3()
                    if (v.own !== 1) fail("pointer: with it hidden R3 wasn't A on the highlight (" + v.own + " presses)")
                }],
                [ready, () => {
                    const t = byFid(v.fid)
                    if (!t) { fail("pointer: the pressed row is gone"); return }
                    v.held = 0
                    t.activated.connect(() => { v.held++ })
                    app.setFocus(t, false)
                }],
                [app.motion.holdClick + 150, () => { app.padEvent("r3", true) }],
                [ready, () => {
                    if (!dialog.open || dialog.d.options !== true || dialog.d.title !== v.title) fail("pointer: with it hidden R3 held didn't open Options for the highlight (" + (dialog.open ? dialog.d.title : "no dialog") + ", not " + v.title + ")")
                    app.padEvent("r3", false)
                    if (v.held > 0) fail("pointer: with it hidden R3 held also pressed the highlight")
                    app.padPress("b")
                }],
                [ready, () => { if (dialog.open) fail("pointer: B didn't close Options") }],
            ]
        }
        if (name === "options") {
            const mid = (it) => middle(it)
            const rowsOf = () => dialog.open && dialog.d.rows ? dialog.d.rows : []
            const closed = (what) => { if (dialog.open) fail("options: " + what + " didn't close the menu") }
            return [
                [ready, () => { v.entry = app.focusItem; v.title = v.entry ? v.entry.optionsTitle : ""; key(Qt.Key_R) }],
                [ready, () => {
                    const rows = rowsOf()
                    if (!dialog.open || dialog.d.options !== true) { fail("options: R didn't open Options"); return }
                    if (v.title === "" || dialog.d.title !== v.title) fail("options: the menu isn't named after the highlighted game (" + dialog.d.title + ")")
                    if (rows[0].local !== "a" || rows[0].title !== (v.entry.optionLabel || v.entry.aLabel)) fail("options: the game's own press isn't first (" + rows[0].title + ")")
                    if (!rows.some(r => r.title === "Play" && r.key === "play")) fail("options: Play isn't there with its button")
                    if (!rows.some(r => r.title === "Library check")) fail("options: the page's own action isn't there")
                    if (!rows.some(r => r.local === "find")) fail("options: Find isn't there")
                    if (rows[rows.length - 1].key !== "f1") fail("options: Help isn't last")
                    const titles = rows.map(r => r.title)
                    if (titles.some((t, i) => titles.indexOf(t) !== i)) fail("options: an entry is there twice (" + titles.join(", ") + ")")
                    key(Qt.Key_R)
                }],
                // A right-click on a row is its Options; beside every row, the page's.
                [ready, () => {
                    closed("R again")
                    const t = every(app.parts.view.item).find(x => x.kind === "toggle")
                    if (!t) { fail("options: the game has no switch to try"); return }
                    v.row = t.optionsTitle; v.label = t.optionLabel || t.aLabel
                    click(mid(t), Qt.RightButton)
                }],
                [ready, () => {
                    if (!dialog.open || dialog.d.title !== v.row || rowsOf()[0].title !== v.label) fail("options: a right-click on a row didn't open its Options (" + (dialog.open ? dialog.d.title : "nothing") + ")")
                    key(Qt.Key_Escape)
                }],
                [ready, () => {
                    closed("Esc")
                    const p = Qt.point(app.width - 6, app.parts.topBar.height + 6)
                    if (app.focusableAt(p)) { fail("options: something to press lies at the page's very corner"); return }
                    click(p, Qt.RightButton)
                }],
                [ready, () => {
                    if (!dialog.open || dialog.d.options !== true) fail("options: a right-click beside every row didn't open the page's Options")
                    else if (rowsOf().some(r => r.local === "a")) fail("options: the page's own Options start with a press of something")
                    key(Qt.Key_Escape)
                }],
                // Find: F, and a letter that is no key of its own.
                [ready, () => { closed("Esc"); key(Qt.Key_F) }],
                [200, () => { if (!app.typing) fail("options: F didn't start Find"); key(Qt.Key_Escape) }],
                [ready, () => { if (app.typing) fail("options: Esc didn't end the typing"); v.count = (app.model.side || []).length; key(Qt.Key_B) }],
                [300, () => {
                    if (!app.typing || !app.model.search || app.model.search.value !== "b") fail("options: a typed letter didn't start Find with it (" + (app.model.search ? app.model.search.value : "") + ")")
                    if ((app.model.side || []).length >= v.count) fail("options: the typed letter didn't narrow the list")
                    key(Qt.Key_Escape)
                }],
                [ready, () => { key(Qt.Key_Escape) }],
                [ready, () => { if (app.model.search.value !== "") fail("options: Esc didn't empty the search"); key(Qt.Key_Tab) }],
                // Tab: the menu, with Help first; inside it Tab goes from row to row.
                [ready, () => {
                    if (!dialog.open || dialog.d.options === true || rowsOf().length < 5) { fail("options: Tab didn't open the menu"); return }
                    if (rowsOf()[0].key !== "f1") fail("options: the menu doesn't start with Help")
                    v.fid = app.lastFid
                    key(Qt.Key_Tab)
                    if (app.lastFid === v.fid) fail("options: Tab didn't move on inside the menu")
                    key(Qt.Key_Backtab, Qt.ShiftModifier)
                    if (app.lastFid !== v.fid) fail("options: Shift+Tab didn't move back inside the menu")
                    key(Qt.Key_Escape)
                }],
                [ready, () => { closed("Esc"); v.depth = app.stack.length; key(Qt.Key_F1) }],
                [ready, () => { if (app.current.page !== "help") fail("options: F1 didn't open Help (" + app.current.page + ")"); key(Qt.Key_Escape) }],
                // RT jumps further down the list than Down does.
                [ready, () => {
                    if (app.stack.length !== v.depth) fail("options: Esc didn't come back from Help")
                    const list = every(app.parts.view.item).filter(x => x.sideEntry === true && x.modelData !== undefined)
                    if (list.length < 4) { fail("options: the list is too short to jump in"); return }
                    app.setFocus(list[0], false)
                    app.padPress("rt")
                }],
                [ready, () => {
                    const list = every(app.parts.view.item).filter(x => x.sideEntry === true && x.modelData !== undefined)
                    const at = list.indexOf(app.focusItem)
                    if (at < 2) fail("options: RT didn't jump further than one step (it is on entry " + at + ")")
                    app.padPress("lt")
                }],
                // Space is A; held on a game it is Start held: Play.
                [ready, () => {
                    const list = every(app.parts.view.item).filter(x => x.sideEntry === true && x.modelData !== undefined)
                    if (list.indexOf(app.focusItem) !== 0) fail("options: LT didn't jump back up")
                    if (!app.canHold) { fail("options: this game can't be played, so holding can't be tried"); return }
                    v.fid = app.lastFid
                    key(Qt.Key_Space)
                }],
                [ready, () => {
                    if (app.lastFid === v.fid) fail("options: Space let go at once wasn't A")
                    if (dialog.open || app.parts.toast.up) fail("options: Space let go at once did more than A")
                    Hub.testKey(Qt.Key_Space, 0, true, false)
                }],
                [Math.round(app.motion.holdPlay * 0.5), () => { if (app.holding !== "space" || app.holdPart < 0.3) fail("options: no ring fills while Space is held") }],
                [Math.round(app.motion.holdPlay * 0.5) + 200, () => {}],
                [ready, () => {
                    if (!dialog.open && !app.parts.toast.up) fail("options: Space held didn't play the game")
                    Hub.testKey(Qt.Key_Space, 0, false, false)
                    if (dialog.open) key(Qt.Key_Escape)
                }],
                // Ctrl+Enter is Start: with no main button on the page, Settings.
                [ready, () => { key(Qt.Key_Return, Qt.ControlModifier) }],
                [ready, () => { if (app.tab !== 4) fail("options: Ctrl+Enter (Start) on a page with no main button didn't open Settings"); app.goTab(1) }],
                [ready, () => {}],
            ]
        }
        fail("no input check is called " + name)
        return []
    }

    // The sticks held somewhere for a while, looked at every 16 ms as the real ones are (Hub.testSticks), and what
    // came of it handed to the window as Hub.poll's answer is.
    function sticks(lx, ly, rx, ry, ms) { const t = Hub.testSticks(lx, ly, rx, ry, ms); if (t === "") fail("the sticks can't be fed"); else app.took(JSON.parse(t)) }
    // The right stick steering the pointer to a place, a look at a time, as a thumb does.
    function steer(to) {
        for (let i = 0; i < 600; i++) {
            const dx = to.x - app.ptr.x, dy = to.y - app.ptr.y, d = Math.hypot(dx, dy)
            if (app.ptr.shown && d < 5) return true
            const m = d > 150 ? 1 : d > 40 ? 0.55 : 0.32
            sticks(0, 0, dx / Math.max(d, 0.001) * m, dy / Math.max(d, 0.001) * m, 16)
        }
        fail("the right stick couldn't bring the pointer to " + Math.round(to.x) + "," + Math.round(to.y) + " (it is at " + Math.round(app.ptr.x) + "," + Math.round(app.ptr.y) + ")")
        return false
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
