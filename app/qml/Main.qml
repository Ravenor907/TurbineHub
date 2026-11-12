import QtQuick
import QtQuick.Window
import TurbineHub

// Turbine Hub's window: the look, sizes and icons every component draws with, the pages and their stack, one
// highlight that the controller, the keyboard, the mouse and a finger all move, the one dialog, the toast and the
// footer's hints.
//
// The controls are Turbine Mod Manager's (docs/DESIGN.md, Controls): A selects, B goes back, X opens Options for
// what the highlight is on, Y is Find, LB and RB turn the tabs, View opens the menu, Start presses the page's main
// button (held on a game: Play), LT and RT jump a screenful, the right stick moves a pointer and R3 clicks (held:
// Options). Every one has a key and a way with the mouse. Which actions a page has comes from the engine
// (`options`, `hints`); only A and B, which follow the highlight, are worked out here. What each page shows and what each press does comes from the engine through Hub; nothing here
// decides anything about the machine.
Window {
    id: app
    objectName: "turbinehubMain"
    visible: true
    title: "Turbine Hub"
    width: shot.on ? shot.w : 1280
    height: shot.on ? shot.h : 800
    minimumWidth: 640
    minimumHeight: 400
    visibility: (Hub.gamemode && !shot.on) ? Window.FullScreen : Window.Windowed
    color: c.ground

    // ---------------------------------------------------------------- screenshot mode (used by the build checks)
    QtObject {
        id: shot
        readonly property string path: Hub.option("screenshot")
        readonly property bool on: path !== ""
        readonly property var size: Hub.option("size").split("x")
        readonly property int w: parseInt(size[0]) || 1280
        readonly property int h: parseInt(size[1]) || 800
        readonly property string page: Hub.option("page")
        readonly property string arg: Hub.option("arg")
        readonly property string act: Hub.option("act")
        // Presses made before the picture, in order ("down,down,a"): the build's way to try the controls.
        readonly property var keys: Hub.option("keys") === "" ? [] : Hub.option("keys").split(",")
    }
    readonly property bool shotOn: shot.on
    Loader { id: selfCheck }

    // ---------------------------------------------------------------- state from the engine
    readonly property var st: JSON.parse(Hub.state)
    readonly property bool veteran: st.mode === "veteran"
    readonly property bool windowed: st.window || (!Hub.gamemode && !shot.on)

    // ---------------------------------------------------------------- sizes
    // Everything is drawn from a 1280 x 800 design, scaled to the window and by the player's interface size.
    // A window far from 16:10 (half a screen, a tall or a wide one) keeps a readable size on its roomy side and
    // re-flows on the other, instead of shrinking the whole 1280 x 800 layout into it. Nothing gets smaller than
    // the text floor allows, so boxes and their words always shrink together.
    readonly property real fit: Math.max(Math.min(width / 1280, height / 800), Math.min(0.85, Math.max(width / 1280, height / 800)))
    readonly property real unit: Math.max(0.5, fit * st.scale / 100)
    function px(v) { return Math.round(v * unit) }
    // The screen's pixels per point (display scaling).
    readonly property real ratio: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    // Text: never under 12 pixels, and never smaller than 90% of its design size, so the smallest interface size
    // and the smallest window stay readable (body text keeps 14 pixels). Boxes that hold words grow with them.
    readonly property int textFloor: 12
    function fpx(v) { return Math.max(textFloor, Math.round(v * Math.max(unit, 0.9))) }
    // Something to press never shrinks under a finger, whatever the interface size: 44 pixels, or 6 mm on a dense
    // screen (a 7 inch 1080p handheld has about 12 pixels to the millimetre).
    // (A picture can ask for a dense screen's floor with --finger=<pixels>, since a picture has no screen to measure.)
    readonly property int touchFloor: shot.on && parseInt(Hub.option("finger")) > 0 ? parseInt(Hub.option("finger")) : Math.max(44, Math.round(Screen.pixelDensity * 6))
    function tpx(v) { return Math.max(touchFloor, Math.round(v * unit)) }
    // An icon: never under three quarters of its design size, so it stays a mark and not a speck beside words that
    // keep nine tenths of theirs.
    function ipx(v) { return Math.round(v * Math.max(unit, 0.75)) }
    // A box that holds one line of words: its design size, or the room its words need at the size they are drawn
    // (the text floor can make words larger than their box's design).
    function bpx(v, textSize) { return Math.max(Math.round(v * unit), Math.round(fpx(textSize) * 1.45)) }

    FontLoader { id: cinzel; source: "file://" + Hub.share + "/fonts/Cinzel.ttf" }
    FontLoader { id: barlow; source: "file://" + Hub.share + "/fonts/Barlow-Regular.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-Medium.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-SemiBold.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-Bold.ttf" }
    readonly property string titleFont: cinzel.status === FontLoader.Ready ? cinzel.font.family : "serif"
    readonly property string bodyFont: barlow.status === FontLoader.Ready ? barlow.font.family : "sans-serif"

    // ---------------------------------------------------------------- colors
    // Every color, for the player's theme, accent and color vision (Palette.qml holds them and the rules they keep).
    Palette { id: c; themeName: app.st.theme; accentName: app.st.accent; visionName: app.st.vision || "typical" }
    readonly property var colors: c

    // The player's Animations setting: "full", "reduced" or "off" (a picture is taken with them off unless it asks,
    // see --motion in backend.rs).
    Motion { id: mo; level: ["full", "reduced", "off"].indexOf(app.st.animations) >= 0 ? app.st.animations : "full" }
    readonly property var motion: mo

    // Line icons on a 24 x 24 grid, one per kind of thing or action.
    readonly property var icons: ({
        home: "M4 11l8-7 8 7 M6 10v10h12V10",
        games: "M7 8h10a5 5 0 0 1 5 5v1a3 3 0 0 1-5.5 1.7L15.5 14h-7l-1 1.7A3 3 0 0 1 2 14v-1a5 5 0 0 1 5-5z M7 10.5v3 M5.5 12h3 M16 11v.2 M18 13v.2",
        store: "M5 8H19L18 21H6Z M9 8V6A3 3 0 0 1 15 6V8",
        bag: "M5 8H19L18 21H6Z M9 8V6A3 3 0 0 1 15 6V8",
        machine: "M8 6h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z M9 2v4 M15 2v4 M9 18v4 M15 18v4 M2 9h4 M2 15h4 M18 9h4 M18 15h4",
        settings: "M4 21v-7 M4 10V3 M12 21v-9 M12 8V3 M20 21v-5 M20 12V3 M2 14h4 M10 8h4 M18 16h4",
        gauge: "M4 17a8 8 0 1 1 16 0 M12 17l4-6",
        display: "M4 5h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z M9 21h6 M12 17v4",
        tv: "M5 5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z M8 21h8 M12 17v4",
        light: "M12 8.8a3.2 3.2 0 1 1 0 6.4 3.2 3.2 0 0 1 0-6.4z M12 2.5V5.5 M12 18.5V21.5 M2.5 12H5.5 M18.5 12H21.5 M5.3 5.3L7.4 7.4 M16.6 16.6L18.7 18.7 M5.3 18.7L7.4 16.6 M16.6 7.4L18.7 5.3",
        battery: "M4 7h13a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2z M22 10v4 M5.5 10v4 M8.5 10v4 M11.5 10v4",
        moon: "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z",
        shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z M9 12l2 2 4-4",
        dock: "M8 3h8v11H8z M4 19h16 M12 14v5",
        reboot: "M12 3v8 M6.5 6.5a8 8 0 1 0 11 0",
        drive: "M5 6h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z M6 12h6 M17 12v.2",
        speaker: "M4 9h4l5-4v14l-5-4H4z M16.5 9a4 4 0 0 1 0 6 M19 6.5a8 8 0 0 1 0 11",
        bolt: "M13 2L4 14H11L10 22L20 9H13Z",
        lock: "M6 11h12v9H6z M8.5 11V8a3.5 3.5 0 0 1 7 0v3",
        download: "M12 4v11 M7 10l5 5 5-5 M5 20h14",
        bring: "M7 4L3 8L7 12 M3 8H17 M17 12L21 16L17 20 M21 16H7",
        swap: "M7 4L3 8L7 12 M3 8H17 M17 12L21 16L17 20 M21 16H7",
        temp: "M10 14V5A2 2 0 0 1 14 5V14A4 4 0 1 1 10 14Z",
        fan: "M12 10.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3z M12 10.5C11 6 12 3.5 14.5 4S16 8.5 12 10.5 M13.5 12c4.5-1 7 0 6.500 2.500S15.500 16 13.500 12 M12 13.5c1 4.500 0 7-2.500 6.500S8 15.500 12 13.500 M10.5 12C6 13 3.500 12 4 9.500S8.500 8 10.500 12",
        spark: "M12 3l2.2 6.800L21 12l-6.800 2.200L12 21l-2.200-6.800L3 12l6.800-2.200z",
        cloud: "M7 18H17A4 4 0 0 0 17 10A6 6 0 0 0 6 11A3.5 3.5 0 0 0 7 18Z",
        help: "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18z M9.5 9.2a2.5 2.5 0 1 1 3.6 2.2c-.7.4-1.1.9-1.1 1.7v.5 M12 16.8v.4",
        info: "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18z M12 11v6 M12 7.5v.5",
        search: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14z M20 20l-4-4",
        eye: "M2 12C4.500 7 8 5 12 5S19.500 7 22 12C19.500 17 16 19 12 19S4.500 17 2 12z M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
        check: "M5 12.5L10 17L19 7",
        close: "M6 6l12 12 M18 6L6 18",
        restore: "M4 4v6h6 M5 15a8 8 0 1 0 2-8.500L4 10",
        save: "M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M7 3v5h8 M7 21v-7h10v7",
        external: "M14 4h6v6 M20 4l-9 9 M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
        log: "M6 3h9l4 4v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z M8 11h8 M8 15h8 M8 7h4",
        refresh: "M20 11a8 8 0 0 0-14.900-3.900L4 9 M4 4v5h5 M4 13a8 8 0 0 0 14.900 3.900L20 15 M20 20v-5h-5",
        remove: "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13",
        chevron: "M9 5l7 7-7 7",
        play: "M8 5v14l11-7z",
        warn: "M12 4l9 16H3z M12 10v4 M12 17v.3",
        turbine: "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18z M12 12C12 8 14 6.5 16 7 M12 12C15.5 12 17 14 16.5 16 M12 12C12 16 10 17.5 8 17 M12 12C8.5 12 7 10 7.5 8"
    })

    // ---------------------------------------------------------------- the last input: which glyphs the hints show
    // "pad", "keys" or "mouse"; "" until something was used. The hints follow it: the controller's buttons after a
    // press on it, a keyboard's keys after a key or the mouse. In Game Mode they are always the controller's (Steam
    // may be sending the controller as keys there), and a window starts on the controller's when one is seen.
    property string lastInput: ""
    // (A picture starts on keys when it is of a Desktop Mode window.)
    readonly property bool keyboard: !Hub.gamemode && (lastInput === "" ? (shot.on ? st.window : !Hub.pads) : lastInput !== "pad")
    // An Xbox 360 pad says Back and Start where newer pads and the handhelds say View and Menu. (Not in Game Mode,
    // where Steam's own pad stands in for whichever is held.)
    readonly property bool oldPad: !Hub.gamemode && /360/.test(Hub.padName) && !/steam|virtual/i.test(Hub.padName)
    function glyph(k) {
        // (Start is called Start on every pad: Turbine Hub's own menu is on View, so "MENU" would name two things.
        // F1 and F5 have no button on a controller: no cap there.)
        const names = keyboard ? { a: "Enter", b: "Esc", x: "R", y: "F", lb: "Q", rb: "E", lt: "Home", rt: "End", view: "Tab", start: "Ctrl+Enter", play: "Space", r3: "Click", f1: "F1", f5: "F5", move: "Arrows" }
            : { view: oldPad ? "BACK" : "VIEW", start: "START", play: "START", move: "D-PAD", f1: "", f5: "" }
        return names[k] !== undefined ? names[k] : k.toUpperCase()
    }
    // When a key or the controller was last pressed, or the page last rebuilt: for a moment after, the mouse
    // resting on something isn't the mouse being used (see hoverAt).
    property double quietAt: 0
    function keyUsed() { if (lastInput !== "keys") lastInput = "keys"; quietAt = Date.now(); pointerAway() }
    function padUsed() { if (lastInput !== "pad") lastInput = "pad"; quietAt = Date.now() }
    // (The controller's pointer makes mouse events too: those are the controller being used.)
    function mouseUsed() { if (ptrActing) { if (lastInput !== "pad") lastInput = "pad" } else if (lastInput !== "mouse") lastInput = "mouse" }

    // ---------------------------------------------------------------- pages
    readonly property var tabs: [
        { id: "home", title: "Home", icon: "home" }, { id: "games", title: "Games", icon: "games" }, { id: "store", title: "Store", icon: "store" },
        { id: "machine", title: "My machine", icon: "machine" }, { id: "settings", title: "Settings", icon: "settings" }]
    property int tab: 0
    // What each tab showed last (the picked game, the section), so coming back to it finds the same thing.
    property var tabArgs: ["", "", "", "", ""]
    property var tabFids: ["", "", "", "", ""]
    // Pages opened on top of the tab, newest last: { page, arg }. The first screen of a fresh start is marked
    // `first`: Back there asks to close Turbine Hub.
    property var stack: []
    readonly property var current: stack.length > 0 ? stack[stack.length - 1] : { page: tabs[tab].id, arg: tabArgs[tab] || "", fid: tabFids[tab] || "" }
    // The first-run pages fill the window: no tabs until a mode is picked and setup is through.
    readonly property bool bare: st.mode === "" || ["pick", "setup", "quick"].indexOf(current.page) >= 0
    // The tab a page belongs to (-1: it belongs to the one it was opened from).
    function tabOf(page) {
        if (page === "home") return 0
        if (["games", "library", "tool", "proton", "saves"].indexOf(page) >= 0) return 1
        if (page === "store" || page === "store-item") return 2
        if (page === "machine" || page.startsWith("module:")) return 3
        if (page === "settings" || page === "changes") return 4
        return -1
    }
    // The tab the top bar lights: always the one the page on screen belongs to, and none for a page that belongs to
    // no tab (Help, Bring it over).
    readonly property int shownTab: stack.length === 0 ? tab : tabOf(current.page)
    // Back from here asks to close Turbine Hub (the first screen, and Home with nothing opened on top).
    readonly property bool atRoot: (stack.length === 1 && stack[0].first === true) || (stack.length === 0 && tab === 0)
    property var model: ({})

    function fileFor(page) {
        if (page === "home") return "HomePage.qml"
        if (page === "games" || page === "store" || page === "settings") return "SidePage.qml"
        if (page === "machine" || page === "bring") return "CardsPage.qml"
        if (page === "pick") return "PickPage.qml"
        if (page === "setup") return "SetupPage.qml"
        return "RowsPage.qml"
    }
    // What the page remembers while it is open (the picked game, the section, the search words).
    property string pageArg: ""
    function loadPage() {
        pageArg = current.arg
        model = JSON.parse(Hub.page(current.page, pageArg))
        const file = fileFor(current.page)
        if (view.file !== file) { view.file = file; view.setSource(file, { app: app }) }
        focusItem = null
        // Back on a page that was left for another: the highlight where it was.
        lastFid = current.fid || ""
        // A text box of the page before doesn't keep the keyboard.
        keysBack()
        if (mo.on) pageIn.restart()
        // A note belongs to the page it was about.
        toast.up = false
        settle(true)
        Qt.callLater(restoreFocus)
    }
    // The same page again with fresh content; the highlight stays on the same thing (found again by its name,
    // however many times the page is rebuilt before the highlight is put back).
    function reloadPage() {
        model = JSON.parse(Hub.page(current.page, pageArg))
        settle(false)
        Qt.callLater(restoreFocus)
        clampLater.restart()
    }
    function setArg(arg) { pageArg = arg; reloadPage() }
    // The page on screen keeps what it shows (the game, the section) for when Back comes to it again.
    function remember() {
        if (stack.length > 0) {
            const s = stack.slice()
            s[s.length - 1] = Object.assign({}, s[s.length - 1], { arg: pageArg, fid: lastFid })
            stack = s
        } else {
            const a = tabArgs.slice()
            a[tab] = pageArg
            const f = tabFids.slice()
            f[tab] = lastFid
            tabFids = f
            tabArgs = a
        }
    }
    function open(target) {
        const parts = target.split("|")
        const page = parts[0], arg = parts.length > 1 ? parts[1] : ""
        if (page === "home") { root("home"); return }
        const t = tabs.findIndex(x => x.id === page)
        // No tabs before a mode is picked.
        if (t >= 0 && st.mode === "") return
        remember()
        // From a tab itself another tab is a switch; from a page opened on top (or a setup step) it goes on top,
        // so Back comes back to it.
        if (t >= 0 && stack.length === 0) { tab = t; if (arg !== "") { const a = tabArgs.slice(); a[t] = arg; tabArgs = a } loadPage(); return }
        stack = stack.concat([{ page: page, arg: arg }])
        loadPage()
    }
    function back() {
        if (stack.length > 0 && !(stack.length === 1 && stack[0].first === true)) { stack = stack.slice(0, stack.length - 1); loadPage(); return true }
        if (tab !== 0 && stack.length === 0) { tab = 0; loadPage(); return true }
        return false
    }
    function switchTab(d) { goTab(((shownTab >= 0 ? shownTab : tab) + d + tabs.length) % tabs.length) }
    function goTab(t) {
        if (bare) return
        remember()
        stack = []; tab = t; loadPage()
    }
    function root(page) {
        if (page === "home") { stack = []; tab = 0 }
        // The mode just picked on the first screen: the picker stays one Back away.
        else if (current.page === "pick" && (page === "setup" || page === "quick")) { remember(); stack = stack.concat([{ page: page, arg: "" }]) }
        else stack = [{ page: page, arg: "" }]
        loadPage()
    }
    // Back with nothing behind: a question, never a sudden exit.
    function askClose() {
        dialog.show({ title: "Close Turbine Hub?", text: "Everything you set up stays as it is.", items: [], confirm: "Close", cancel: "Cancel", ask: "", danger: true, local: "quit" })
    }

    // ---------------------------------------------------------------- presses wait for the page
    // While the highlight is being put back (a page just loaded or rebuilt, a dialog just opened or closed) presses
    // are held and sent on once it is back, so none lands on a row that is going away. For a moment after a new page
    // or dialog appears, A, X, Y and Menu are left out, so a double press never reaches what the first one opened.
    property bool restoring: false
    property bool fresh: false
    property var held: []
    function settle(arrived) {
        restoring = true
        // What the mouse rests on after a rebuild is not the mouse being used.
        quietAt = Date.now()
        // A direction held on the page before doesn't go on moving on this one.
        if (arrived) { fresh = true; freshGuard.restart(); stopHolds() }
    }
    Timer { id: freshGuard; interval: mo.pressGuard; onTriggered: app.fresh = false }
    readonly property bool ready: !restoring && !fresh

    // ---------------------------------------------------------------- taps and the mouse obey what presses obey
    // In Qt 6 a backdrop stops the items under it but not their tap and hover handlers, so every tap in every file
    // asks here first (tools/qml-check.js fails the build on one that doesn't), and the page is turned off under a
    // dialog and under the busy cover (`enabled` on the view and the bars below). `item`: what was tapped (null for
    // something that belongs to the whole window). `k`: the button the tap stands for ("a" when left out).
    // Refused: while work runs, while the highlight is being put back, for a moment after a page or dialog appeared
    // (as A is), and anything of the page while a dialog is open.
    // How many taps were refused and taken (the build's checks read them).
    property int refused: 0
    property int taken: 0
    function under(item, root) { for (let p = item; p; p = p.parent) if (p === root) return true; return false }
    function gate(item, k) {
        const key = k || "a"
        let ok = !Hub.busy && !restoring
        if (ok && fresh && ["a", "x", "y", "start"].indexOf(key) >= 0) ok = false
        if (ok && dialog.open && item && !under(item, dialog) && !under(item, bottomBar)) ok = false
        if (!ok) { refused++; return false }
        taken++
        mouseUsed()
        return true
    }
    // The mouse over something moves the highlight there, when it is the mouse being used: the pointer really moved
    // (Qt hovers what lies under a resting mouse again whenever the screen changes), and no key or controller press
    // and no rebuild came just before. A finger never hovers.
    property point mouseAt: Qt.point(-1, -1)
    property point mouseNext: Qt.point(-1, -1)
    function keepMouse() { mouseAt = mouseNext }
    function hoverAt(item, pos, finger) {
        if (finger) return
        const moved = Math.abs(pos.x - mouseAt.x) > 1 || Math.abs(pos.y - mouseAt.y) > 1
        // Everything under the pointer hears of the same move: the place is kept once all of them have.
        mouseNext = Qt.point(pos.x, pos.y)
        Qt.callLater(keepMouse)
        // (The controller's pointer only ever moves on purpose: no press before it makes its move a resting one.)
        if (!moved || (!ptrActing && Date.now() - quietAt < mo.hoverQuiet)) return
        // The real mouse in use: the controller's pointer steps aside.
        if (!ptrActing) pointerAway()
        if (Hub.busy || (item && dialog.open && !under(item, dialog))) return
        // While a text box is being typed in, the highlight stays with it: a tap elsewhere ends the typing, the
        // mouse drifting over the page doesn't.
        if (typing) return
        mouseUsed()
        if (item) setFocus(item, true)
    }

    // ---------------------------------------------------------------- typing
    // The text box that has the keyboard. In Game Mode Steam's on-screen keyboard opens over the app when typing
    // starts and closes when it ends; until then the controller belongs to that keyboard, so a B meant for it never
    // goes back a page here: B says "Done typing" and does only that.
    property Item typing: null
    function typeIn(input) { input.forceActiveFocus() }
    // Every text box calls these two as it gets and loses the keyboard (a tap straight into it counts too).
    function typingBegan(input) {
        if (typing === input) return
        typing = input
        // The highlight goes with it.
        for (let p = input.parent; p; p = p.parent) if (p.focusable === true) { setFocus(p, true); break }
        if (!keyboardUp) { keyboardUp = true; steamKeyboard("open") }
    }
    function typingEnded(input) {
        // The whole window going behind another (Steam's keyboard may do that) is not the end of typing.
        if (typing === input && (active || shot.on)) endTyping()
    }
    function endTyping() {
        typing = null
        // Once this press is through: going from one box to the next keeps the keyboard up.
        Qt.callLater(keyboardDown)
    }
    // Steam's on-screen keyboard was asked for and not yet sent away.
    property bool keyboardUp: false
    function keyboardDown() { if (!typing && keyboardUp) { keyboardUp = false; steamKeyboard("close") } }
    function steamKeyboard(what) { if (Hub.gamemode && !shot.on) Qt.openUrlExternally("steam://" + what + "/keyboard") }
    function typingDone() { keys.forceActiveFocus(); if (typing) endTyping() }

    // ---------------------------------------------------------------- presses to the engine
    function doAct(act, arg, value) {
        if (act === "none" || act === "" || act === undefined) return
        if (act === "tab") { open(arg); return }
        if (act === "quit") { Qt.quit(); return }
        handle(JSON.parse(Hub.act(act, arg || "", value === undefined ? "" : String(value))))
    }
    function handle(r) {
        if (r.root) root(r.root)
        if (r.open) open(r.open)
        // Continue goes on to the step after this one (after a step taken again with Back too).
        if (r.setupStep !== undefined) pageArg = r.setupStep
        if (r.refresh && !r.root && !r.open) reloadPage()
        if (r.toast) toast.show(r.toast)
        if (r.dialog) dialog.show(r.dialog)
        // A change that could leave the screen unreadable asks to be kept, and goes back by itself.
        if (r.keep) dialog.show({ title: r.keep.title, text: r.keep.text, items: [], confirm: r.keep.confirm, cancel: r.keep.cancel, ask: "", danger: true, keep: r.keep })
    }
    function confirmDialog(password, again) {
        const r = JSON.parse(Hub.confirm(password, again))
        if (r.error) { dialog.error = r.error; return }
        dialog.close()
        handle(r)
    }

    // ---------------------------------------------------------------- the highlight
    property Item focusItem: null
    // The name of the thing that held the highlight last on this page.
    property string lastFid: ""
    // Everything that can take the highlight under `root`, in drawing order.
    function focusables(root) {
        const out = []
        function walk(it) {
            if (!it || !it.visible || it.enabled === false) return
            if (it.focusable === true) out.push(it)
            const kids = it.children
            for (let i = 0; i < kids.length; i++) walk(kids[i])
        }
        walk(root)
        return out
    }
    readonly property Item scopeRoot: dialog.open ? dialog : view.item
    function setFocus(item, quiet) {
        if (focusItem === item) return
        focusItem = item
        // The highlight leaving a text box ends the typing in it.
        if (typing && item && !under(typing, item)) typingDone()
        if (item && item.fid !== "") lastFid = item.fid
        if (item && !quiet) reveal(item)
        placeToast()
        if (item && item.focusedNow) item.focusedNow()
    }
    // Layouts size their items lazily, just before drawing; a page just built gets its sizes now, so the highlight
    // lands on what will be drawn (a width can decide how many columns, and so which rows, there are).
    function settleLayouts(it) {
        if (!it || !it.visible) return
        if (typeof it.forceLayout === "function") it.forceLayout()
        const kids = it.children
        for (let i = 0; i < kids.length; i++) settleLayouts(kids[i])
    }
    function focusLost() {
        focusItem = null
        restoring = true
        Qt.callLater(restoreFocus)
    }
    function restoreFocus() {
        restoring = false
        settleLayouts(scopeRoot)
        const list = focusables(scopeRoot)
        if (list.length === 0) focusItem = null
        else {
            let pick = null
            if (lastFid !== "") pick = list.find(x => x.fid === lastFid) || null
            if (!pick) pick = list.find(x => x.firstFocus === true) || list[0]
            focusItem = null
            setFocus(pick, false)
            // The rebuilt rows are placed by their layouts a moment later, so the highlight is brought into view
            // again then (a press that reloads a long page left it under the footer).
            revealAgain.restart()
        }
        // The presses that waited go on now (unless putting the highlight back rebuilt the page again).
        if (!restoring && held.length > 0) {
            const q = held
            held = []
            for (const k of q) press(k)
        }
    }
    Timer { id: revealAgain; interval: 1; onTriggered: { app.clampScrolls(app.scopeRoot); if (app.focusItem) app.reveal(app.focusItem); clampLater.restart() } }
    // A page that got shorter (a card gone after a fix, a dialog closed) can't stay scrolled past its end: that hid
    // its top under the tabs and left the bottom of the screen empty. Every list is brought back inside its length.
    function clampScrolls(it) {
        if (!it) return
        if (it.contentY !== undefined && it.contentItem !== undefined && it.contentHeight !== undefined) {
            const top = it.originY || 0
            const max = top + Math.max(0, it.contentHeight - it.height)
            if (it.contentY > max) it.contentY = max
            else if (it.contentY < top) it.contentY = top
        }
        const kids = it.children
        for (let i = 0; i < kids.length; i++) clampScrolls(kids[i])
    }
    // Layouts finish placing rows a little after a reload; check once more then.
    Timer { id: clampLater; interval: 120; onTriggered: app.clampScrolls(app.scopeRoot) }
    // Scrolls every list around the item so all of it shows, clear of the fades at the edges. An item can ask for
    // more above it to show with it (`revealFrom`: the heading of its section).
    function reveal(item) {
        // Layouts settle their sizes lazily; measure what will be drawn, not what was drawn last.
        const chain = []
        for (let p = item.parent; p; p = p.parent) chain.unshift(p)
        for (const p of chain) if (typeof p.forceLayout === "function") p.forceLayout()
        const from = item.revealFrom ? item.revealFrom : item
        let p = item.parent
        while (p) {
            if (p.contentY !== undefined && p.contentItem !== undefined && p.contentHeight > p.height) {
                const y = from.mapToItem(p.contentItem, 0, 0).y
                const bottom = item.mapToItem(p.contentItem, 0, item.height).y
                const pad = px(28)
                // What stays pinned over the top of the list (its headings) covers that much more.
                const top = pad + (p.topInset || 0)
                // Anything on the first screenful shows with the top of the page, its heading included.
                if (bottom + pad <= p.height) p.contentY = 0
                else if (y - top < p.contentY) p.contentY = Math.max(0, y - top)
                // Scrolling down never takes the top of the item (or its heading) out of view.
                else if (bottom + pad > p.contentY + p.height) p.contentY = Math.max(0, Math.min(p.contentHeight - p.height, bottom + pad - p.height, y - top))
                // The last thing to highlight in the list brings its end into view: words after it (readings,
                // a tip, the activity list) can't be reached any other way.
                const after = focusables(p.contentItem).some(x => x !== item && x.mapToItem(p.contentItem, 0, 0).y >= bottom - 1)
                if (!after) p.contentY = Math.max(p.contentY, Math.min(p.contentHeight - p.height, y - top))
            }
            p = p.parent
        }
    }
    // Moves the highlight to the nearest thing in that direction.
    function moveFocus(dir, within) {
        const list = focusables(within || scopeRoot)
        if (list.length === 0) return false
        if (!focusItem || list.indexOf(focusItem) < 0) { setFocus(list[0], false); return true }
        const a = focusItem.mapToItem(null, 0, 0)
        const ar = { x: a.x, y: a.y, w: focusItem.width, h: focusItem.height }
        let best = null, bestScore = 1e12
        for (const it of list) {
            if (it === focusItem) continue
            const b = it.mapToItem(null, 0, 0)
            const br = { x: b.x, y: b.y, w: it.width, h: it.height }
            const dx = (br.x + br.w / 2) - (ar.x + ar.w / 2), dy = (br.y + br.h / 2) - (ar.y + ar.h / 2)
            let main, cross, overlap
            if (dir === "left" || dir === "right") {
                main = dir === "right" ? br.x - (ar.x + ar.w) : ar.x - (br.x + br.w)
                if ((dir === "right" ? dx : -dx) <= 1) continue
                overlap = Math.min(ar.y + ar.h, br.y + br.h) - Math.max(ar.y, br.y)
                cross = overlap > 0 ? 0 : Math.abs(dy)
            } else {
                main = dir === "down" ? br.y - (ar.y + ar.h) : ar.y - (br.y + br.h)
                if ((dir === "down" ? dy : -dy) <= 1) continue
                overlap = Math.min(ar.x + ar.w, br.x + br.w) - Math.max(ar.x, br.x)
                cross = overlap > 0 ? 0 : Math.abs(dx)
            }
            const score = Math.max(0, main) + cross * 3 + (overlap > 0 ? 0 : 4000)
            if (score < bestScore) { bestScore = score; best = it }
        }
        if (!best) return false
        setFocus(best, false)
        return true
    }

    // ---------------------------------------------------------------- one press, whatever it came from
    // The last press that arrived, whatever came of it (the build's checks read it).
    property string lastPress: ""
    function press(k) {
        lastPress = k
        if (Hub.busy) return
        if (restoring) { if (held.length < 4) held = held.concat([k]); return }
        if (fresh && ["a", "x", "y", "start"].indexOf(k) >= 0) return
        if (dialog.open && dialog.press(k)) return
        if (focusItem && focusItem.press && focusItem.press(k)) return
        if (k === "up" || k === "down" || k === "left" || k === "right") { step(k); return }
        // LT and RT: a screenful up or down the list the highlight is in.
        if (k === "lt" || k === "rt") { jumpBy(k === "rt"); return }
        if (dialog.open) return
        if (view.item && view.item.press && view.item.press(k)) return
        if (k === "lb") switchTab(-1)
        else if (k === "rb") switchTab(1)
        else if (k === "b") { if (!back()) askClose() }
        // Options: everything the highlighted thing and this page can do.
        else if (k === "x") openOptions(focusItem)
        // Find: the search box of a page that has a list.
        else if (k === "y") find("")
        // The menu leads to the tabs' places, so it waits until first run is through.
        else if (k === "view" && !bare) doAct("menu", current.page + "@" + pageArg, "")
        // The page's main button; where there is none, Settings (and from Settings, back).
        else if (k === "start") { if (!pageKey(model, k) && !bare) { if (stack.length === 0 && tab === 4) back(); else goTab(4) } }
        // What holding Start does, at once (a click on its hint in the footer).
        else if (k === "play") play()
        else if (k === "f1") help()
    }
    // One step of the highlight. A page with two sides keeps it on one side until asked across.
    function step(k) {
        if (!dialog.open && view.item && view.item.move && view.item.move(k)) return
        moveFocus(k)
    }
    // The page's main button is Start's.
    function pageKey(m, k) {
        if (!m) return false
        if (k === "start" && m.primary) { doAct(m.primary.act, m.primary.arg, ""); return true }
        return false
    }
    // Help: what this page is and what its controls do, with Where's my...? one press further.
    function help() {
        if (dialog.open) return
        if (current.page === "help") open("wheres"); else if (current.page !== "wheres") open("help|" + current.page + "@" + pageArg)
    }
    // Find: the highlight goes to the page's search box and typing starts (in Game Mode Steam's keyboard opens).
    // `text`: what was typed already (a letter typed on the list starts the search with it).
    function find(text) {
        if (dialog.open || !view.item || !view.item.find) return false
        return view.item.find(text || "")
    }
    // What holding Start (or Space) does on this page: Play, on a game.
    readonly property bool canHold: !!model.hold && !dialog.open && !typing && !Hub.busy
    function play() { if (model.hold && !dialog.open && !Hub.busy) doAct(model.hold.act, model.hold.arg, "") }
    // LT and RT: the highlight goes on that way until it is most of a screen further, or at the end of its list.
    function jumpBy(down) {
        const k = down ? "down" : "up"
        const from = focusItem
        if (!from) { step(k); return }
        let list = null
        for (let p = from.parent; p; p = p.parent) if (p.contentY !== undefined && p.contentItem !== undefined) { list = p; break }
        const place = (it) => list ? it.mapToItem(list.contentItem, 0, 0).y : it.mapToItem(null, 0, 0).y
        const far = (list ? list.height : height) * 0.8
        const y0 = place(from)
        for (let i = 0; i < 60; i++) {
            const before = focusItem
            step(k)
            if (focusItem === before || !focusItem) break
            // Out of the list (onto the search box over it, the buttons under it): its end was the end of the jump.
            if (list && !under(focusItem, list)) { setFocus(before, false); break }
            if (Math.abs(place(focusItem) - y0) >= far) break
        }
    }

    // ---------------------------------------------------------------- Options
    // X, R, a right-click or R3 held: a menu for one thing, with its own press first (what A does), then what else
    // it offers (`offers` on the thing, from the engine), then what the page offers (the page's `options`: its own
    // actions, its main button, Find, Help). Nothing in it is new: it is everything that is one press or one page
    // away, listed, each with its own button where it has one. `item`: the thing (null: the page alone).
    property Item optionsFor: null
    function openOptions(item) {
        if (dialog.open || Hub.busy) return
        const it = item && item.focusable === true ? item : null
        const rows = []
        if (it && it.aLabel) rows.push({ title: it.optionLabel || it.aLabel, key: "a", local: "a" })
        // Said once: what the page offers may be among the thing's own already (then it only brings its button).
        const add = (o) => {
            const at = rows.findIndex(r => r.title === o.title || (!!o.act && r.act === o.act && r.arg === o.arg))
            if (at < 0) rows.push(o)
            else if (o.key && !rows[at].key) rows[at] = Object.assign({}, rows[at], { key: o.key })
        }
        for (const o of (it && it.offers) || []) add(o)
        for (const o of model.options || []) add(o)
        if (rows.length === 0) return
        optionsFor = it
        dialog.show({ title: (it && it.optionsTitle) || model.title || (shownTab >= 0 ? tabs[shownTab].title : "") || "Options", text: "", items: [], confirm: "", cancel: "Close", ask: "", danger: false, rows: rows, options: true })
    }
    // What was picked in a menu (the dialog has closed by now).
    function chosen(row) {
        if (row.local === "a") {
            const it = optionsFor
            // The thing's own press, as A makes it.
            if (it && it.visible && it.press) { setFocus(it, true); if (it.fid !== "") lastFid = it.fid; it.press("a") }
        } else if (row.local === "find") Qt.callLater(find, "")
        else if (row.local === "reload") reloadPage()
        else doAct(row.act, row.arg, "")
        optionsFor = null
    }
    // The thing that takes the highlight at a place in the window: the one drawn last, and only where its list
    // shows it (a row scrolled out of view isn't there).
    function focusableAt(pos) {
        const list = focusables(scopeRoot)
        for (let i = list.length - 1; i >= 0; i--) {
            const it = list[i]
            const p = it.mapFromItem(null, pos.x, pos.y)
            if (p.x < 0 || p.y < 0 || p.x > it.width || p.y > it.height) continue
            let seen = true
            for (let c = it.parent; c && seen; c = c.parent) {
                if (c.clip !== true) continue
                const q = c.mapFromItem(null, pos.x, pos.y)
                seen = q.x >= 0 && q.y >= 0 && q.x <= c.width && q.y <= c.height
            }
            if (seen) return it
        }
        return null
    }
    // A right-click: Options for what is under the mouse, or for the page when nothing is.
    function optionsAt(pos) {
        lastPress = "x"
        if (dialog.open) { dialog.press("x"); return }
        const it = focusableAt(pos)
        if (it) setFocus(it, true)
        openOptions(it)
    }

    // ---------------------------------------------------------------- holds: R3, Start and Space
    // R3 pressed is a click and R3 held is Options; Start (Space on a keyboard) held on a game plays it. While one
    // is held a small ring fills (PointerLayer.qml), so letting go early is plainly a tap.
    property string holding: ""
    property real holdPart: 0
    property double holdAt: 0
    Timer {
        id: holdTick
        interval: 16; repeat: true
        onTriggered: {
            app.holdPart = Math.min(1, (Date.now() - app.holdAt) / (app.holding === "r3" ? mo.holdClick : mo.holdPlay))
            if (app.holdPart >= 1) app.holdDone()
        }
    }
    function holdBegin(what) { holding = what; holdAt = Date.now(); holdPart = 0; holdTick.start() }
    // Let go: true when it was let go before the hold was through (a tap).
    function holdEnd(what) {
        if (holding !== what) return false
        holdStop()
        return true
    }
    function holdStop() { holding = ""; holdPart = 0; holdTick.stop() }
    function holdDone() {
        const what = holding
        holdStop()
        if (what === "r3") {
            Hub.pointer(4)
            // Options for what is under the pointer; with the pointer hidden, for the highlight.
            if (ptr.shown) pointerClick(Qt.RightButton); else press("x")
        } else play()
    }

    // ---------------------------------------------------------------- the controller's pointer
    // The right stick moves a pointer that Turbine Hub draws itself (PointerLayer.qml; Game Mode has no cursor).
    // Where it is comes from pointer.rs through Hub.poll; what it does is made here, as the mouse events a real
    // mouse would make (Hub.pointerEvent), so its moves take the highlight as the mouse's do and its clicks pass
    // the same gate as a tap: R3 clicks, R3 held is the right button (Options), and while it shows RT holds the
    // left button down (to drag a slider) and LT is the right button. It hides after a few seconds, and at once
    // when a button, the D-pad, the left stick, a key or the real mouse is used. The left stick swept round is a
    // scroll wheel: at the pointer while it shows, else at the highlight.
    property var ptr: ({ x: -1, y: -1, shown: false })
    // A mouse event of the pointer's is being delivered right now.
    property bool ptrActing: false
    // Mouse buttons the triggers hold down at the pointer (Qt's values: 1 left, 2 right).
    property int ptrButtons: 0
    function ptrSend(kind, x, y, button, buttons, wheel) {
        ptrActing = true
        Hub.pointerEvent(kind, x, y, button, buttons, wheel)
        ptrActing = false
    }
    function pointerNews(p) {
        const was = ptr.shown
        ptr = { x: p.x, y: p.y, shown: p.shown === true }
        if (was && !ptr.shown) pointerGone()
        if (p.moved) ptrSend(0, ptr.x, ptr.y, 0, ptrButtons, 0)
        if (p.wheel) {
            // No pointer on screen: the wheel turns the list the highlight is in.
            const at = ptr.shown || !focusItem ? Qt.point(ptr.x, ptr.y) : focusItem.mapToItem(null, focusItem.width / 2, focusItem.height / 2)
            ptrSend(3, at.x, at.y, 0, ptrButtons, p.wheel)
        }
    }
    function pointerClick(button) {
        ptrSend(0, ptr.x, ptr.y, 0, 0, 0)
        ptrSend(1, ptr.x, ptr.y, button, button, 0)
        ptrSend(2, ptr.x, ptr.y, button, 0, 0)
    }
    // Hidden: a button the triggers still hold at it is let go.
    function pointerGone() {
        if (ptrButtons !== 0) { const b = ptrButtons; ptrButtons = 0; ptrSend(2, ptr.x, ptr.y, b, 0, 0) }
        if (holding === "r3") holdStop()
    }
    // A key or the real mouse is in use: the pointer goes.
    function pointerAway() {
        if (!ptr.shown) return
        Hub.pointer(0)
        ptr = { x: ptr.x, y: ptr.y, shown: false }
        pointerGone()
    }
    // While a text box has the keyboard in Game Mode (the controller belongs to Steam's keyboard) and while work
    // runs, the sticks rest.
    readonly property bool ptrPaused: (typing !== null && Hub.gamemode) || Hub.busy
    onPtrPausedChanged: { Hub.pointer(ptrPaused ? 1 : 2); if (ptrPaused && ptr.shown) { ptr = { x: ptr.x, y: ptr.y, shown: false }; pointerGone() } }
    function r3(down) {
        if (down) {
            if (Hub.busy || holding !== "") return
            // (The pointer stays while R3 is down.)
            Hub.pointer(3)
            holdBegin("r3")
        } else {
            // (Let go, the pointer may hide again, unless a trigger still holds a button at it.)
            if (ptrButtons === 0) Hub.pointer(4)
            // A click on what is under the pointer; with the pointer hidden, A on the highlight.
            if (holdEnd("r3")) { if (ptr.shown) pointerClick(Qt.LeftButton); else press("a") }
        }
    }

    // The keyboard is as good as the controller, with Turbine Mod Manager's keys: arrows or W A S D move, Enter or
    // Space is A, Esc or Backspace is B, R is Options (X, the Menu key and Shift+F10 too), F is Find (Ctrl+F, / and
    // Y too), Q and E switch tabs (Page Up and Page Down, [ and ] too), Tab opens the menu (V and F10 too; in a
    // dialog Tab and Shift+Tab go from one thing to the next), Ctrl+Enter is Start (P too), Space held is Start
    // held, F1 is Help, F5 reads the page again, Home and End jump to the first and the last thing of the list,
    // Ctrl with = or - changes the interface size. A letter or digit with no meaning of its own starts Find with
    // it on a page that has a list. A text box that is being typed in has the keys itself: none of these reach
    // here then.
    readonly property var keyMap: {
        const map = {}
        map[Qt.Key_Up] = "up"; map[Qt.Key_Down] = "down"; map[Qt.Key_Left] = "left"; map[Qt.Key_Right] = "right"
        map[Qt.Key_W] = "up"; map[Qt.Key_S] = "down"; map[Qt.Key_A] = "left"; map[Qt.Key_D] = "right"
        map[Qt.Key_Return] = "a"; map[Qt.Key_Enter] = "a"; map[Qt.Key_Space] = "a"
        map[Qt.Key_Escape] = "b"; map[Qt.Key_Backspace] = "b"
        map[Qt.Key_Q] = "lb"; map[Qt.Key_E] = "rb"; map[Qt.Key_PageUp] = "lb"; map[Qt.Key_PageDown] = "rb"
        map[Qt.Key_BracketLeft] = "lb"; map[Qt.Key_BracketRight] = "rb"
        map[Qt.Key_R] = "x"; map[Qt.Key_X] = "x"; map[Qt.Key_Menu] = "x"
        map[Qt.Key_F] = "y"; map[Qt.Key_Y] = "y"; map[Qt.Key_Slash] = "y"
        map[Qt.Key_Tab] = "tab"; map[Qt.Key_Backtab] = "backtab"; map[Qt.Key_V] = "view"; map[Qt.Key_F10] = "view"
        map[Qt.Key_P] = "start"; map[Qt.Key_F1] = "f1"; map[Qt.Key_F5] = "f5"
        map[Qt.Key_Home] = "home"; map[Qt.Key_End] = "end"
        return map
    }
    readonly property var dirs: ["up", "down", "left", "right"]
    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: (e) => {
            const ctrl = (e.modifiers & Qt.ControlModifier) !== 0
            if (ctrl && (e.key === Qt.Key_Equal || e.key === Qt.Key_Plus || e.key === Qt.Key_Minus)) {
                e.accepted = true
                app.keyUsed()
                app.sizeBy(e.key === Qt.Key_Minus ? -1 : 1)
                return
            }
            const shift = (e.modifiers & Qt.ShiftModifier) !== 0
            // Ctrl+F finds, Ctrl+Enter is Start, Shift+F10 is Options.
            let k = ctrl ? (e.key === Qt.Key_F ? "y" : e.key === Qt.Key_Return || e.key === Qt.Key_Enter ? "start" : "") : shift && e.key === Qt.Key_F10 ? "x" : shift && e.key === Qt.Key_Tab ? "backtab" : ""
            // Any other key with Ctrl, Alt or the system key belongs to the desktop.
            if (k === "" && (ctrl || (e.modifiers & (Qt.AltModifier | Qt.MetaModifier)) !== 0)) return
            if (k === "") k = app.keyMap[e.key] || ""
            if (k === "") {
                // A letter or a digit that is no key of its own: on a page with a list it starts Find.
                if (!e.isAutoRepeat && e.text.length === 1 && /[0-9a-z]/i.test(e.text) && app.ready && !Hub.busy && app.find(e.text)) { e.accepted = true; app.keyUsed() }
                return
            }
            e.accepted = true
            // A key held down repeats only where a held D-pad does: the directions. Enter held is one A, so it
            // never flips a switch back and forth or answers the dialog its first press opened.
            if (e.isAutoRepeat && app.dirs.indexOf(k) < 0) return
            app.keyUsed()
            // Space held on a game is Start held (Play); let go early, it is A.
            if (e.key === Qt.Key_Space && !ctrl && app.canHold && app.ready && app.holding === "") { app.holdBegin("space"); return }
            if (k === "f5") app.reloadPage()
            else if (k === "home" || k === "end") app.jump(k === "end")
            // Tab: from one thing of a dialog to the next; on a page, the menu.
            else if (k === "tab" || k === "backtab") { if (dialog.open) app.tabMove(k === "tab" ? 1 : -1); else if (k === "tab") app.press("view") }
            else app.press(k)
        }
        Keys.onReleased: (e) => {
            if (e.key === Qt.Key_Space && !e.isAutoRepeat && app.holdEnd("space")) { e.accepted = true; app.press("a") }
        }
        // Where the mouse is, also over nothing that takes the highlight.
        HoverHandler { id: mouseTrack; onPointChanged: app.hoverAt(null, mouseTrack.point.scenePosition, mouseTrack.point.device.pointerType === PointerDevice.Finger) }
    }
    // Qt Quick's software drawing (pictures, machines without a GPU): icons paint into pictures there (Icon.qml).
    readonly property bool softwareDraw: keys.GraphicsInfo.api === GraphicsInfo.Software
    // A text box had the keyboard; this gives it back to the window (and closes Steam's keyboard).
    function keysBack() { typingDone() }
    // Tab and Shift+Tab in a dialog: the next thing and the one before, round and round; a text box that gets the
    // highlight this way is typed in at once.
    function tabMove(d) {
        const list = focusables(scopeRoot)
        if (list.length === 0) return
        const at = list.indexOf(focusItem)
        const next = list[((at < 0 ? (d > 0 ? -1 : 0) : at) + d + list.length) % list.length]
        typingDone()
        setFocus(next, false)
        if (next.sayRole === Accessible.EditableText) next.activated()
    }
    // Home and End: the first and the last thing in the list the highlight is in.
    function jump(toEnd) {
        if (Hub.busy || restoring) return
        let within = scopeRoot
        for (let p = focusItem ? focusItem.parent : null; p; p = p.parent) if (p.contentY !== undefined && p.contentItem !== undefined) { within = p.contentItem; break }
        const list = focusables(within)
        if (list.length > 0) setFocus(toEnd ? list[list.length - 1] : list[0], false)
    }
    // Ctrl with = or -, and Ctrl with the mouse wheel: the interface size, a step at a time. The new size asks to
    // be kept, as it does in Settings.
    function sizeBy(d) {
        if (Hub.busy || restoring || dialog.open || st.mode === "") return
        const v = Math.max(50, Math.min(150, Math.round(st.scale / 10) * 10 + d * 10))
        if (v !== st.scale) doAct("scale", "", v)
    }
    // Over everything, taking nothing from what is under it: the mouse's Back button is B and its right button is
    // Options for what it is on, and Ctrl with the wheel changes the interface size. (A finger has no buttons, so
    // these two listen to mice only: a tap of a finger is never Back or Options.)
    Item {
        anchors.fill: parent
        z: 100
        TapHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; acceptedButtons: Qt.BackButton; onTapped: if (app.gate(null, "b")) app.press("b") }
        TapHandler { acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad; acceptedButtons: Qt.RightButton; onTapped: (p) => { if (app.gate(null, "x")) app.optionsAt(p.position) } }
        WheelHandler {
            property real turned: 0
            acceptedModifiers: Qt.ControlModifier
            target: null
            onWheel: (w) => {
                turned += w.angleDelta.y
                if (Math.abs(turned) < 120) return
                app.mouseUsed()
                app.sizeBy(turned > 0 ? 1 : -1)
                turned = 0
            }
        }
    }

    // The window came to the front or went behind another one (a game, in Game Mode). Behind, the controller's
    // presses are not for Turbine Hub; a direction held as it went is let go, since its release can't be seen from
    // there. Coming back, presses made in the game don't count, nor do new ones for a moment (pad.rs).
    onActiveChanged: windowActive(active)
    function windowActive(front) {
        stopHolds()
        holdStop()
        if (ptr.shown) { ptr = { x: ptr.x, y: ptr.y, shown: false }; pointerGone() }
        Hub.front(front)
    }
    // A held direction stops repeating: the window went to the back, the page changed, the controller went away.
    function stopHolds() { heldDir = ""; repeatDelay.stop(); repeatTick.stop() }
    // A button of the controller went down or up.
    function padEvent(k, down) {
        // While a text box has the keyboard in Game Mode the controller is Steam's keyboard's: only B counts.
        if (typing && Hub.gamemode && k !== "b") return
        if (k === "r3") { if (down) padUsed(); r3(down); return }
        // While the pointer shows, RT is the left mouse button (held: a drag) and LT the right one.
        const mouse = k === "rt" ? Qt.LeftButton : k === "lt" ? Qt.RightButton : 0
        if (mouse !== 0 && (down ? ptr.shown : (ptrButtons & mouse) !== 0)) {
            if (down) { padUsed(); Hub.pointer(3); ptrButtons |= mouse; ptrSend(0, ptr.x, ptr.y, 0, 0, 0); ptrSend(1, ptr.x, ptr.y, mouse, ptrButtons, 0) }
            else { ptrButtons &= ~mouse; ptrSend(2, ptr.x, ptr.y, mouse, ptrButtons, 0); if (ptrButtons === 0 && holding !== "r3") Hub.pointer(4) }
            return
        }
        // Start held on a game plays it; let go early, it is Start.
        if (k === "start" && (down ? canHold && ready && holding === "" : holding === "start")) {
            if (down) { padUsed(); holdBegin("start") } else if (holdEnd("start")) padPress("start")
            return
        }
        if (down) padPress(k)
        else if (k === heldDir) stopHolds()
    }
    // One press on the controller.
    function padPress(k) {
        padUsed()
        if (typing) {
            // B ends the typing and nothing else; in Game Mode everything else belongs to Steam's keyboard.
            if (k === "b") { typingDone(); return }
            if (Hub.gamemode) return
            typingDone()
        }
        press(k)
        if (dirs.indexOf(k) >= 0) { heldDir = k; repeatDelay.restart() }
    }

    // The controller and finished work, every frame. It runs behind other windows too: work that finishes there
    // still shows, and pad.rs passes on no presses from there.
    property string heldDir: ""
    Timer {
        interval: 16; repeat: true; running: !shot.on
        onTriggered: {
            const text = Hub.poll()
            if (text === "") return
            const r = JSON.parse(text)
            app.took(r)
            // Turbine Hub was started a second time: this window comes to the front instead.
            if (r.show) {
                if (app.visibility === Window.Minimized) { if (Hub.gamemode) app.showFullScreen(); else app.showNormal() }
                app.raise(); app.requestActivate()
            }
            if (r.done) {
                if (r.done.ok && r.done.open) app.open(r.done.open); else app.reloadPage()
                if (r.done.toast) toast.show(r.done.toast)
            } else if (r.tick && !dialog.open) app.reloadPage()
        }
    }
    // What the controller did since the last look: its buttons, and the pointer.
    function took(r) {
        if (r.lost) { stopHolds(); holdStop() }
        if (r.ptr) pointerNews(r.ptr)
        for (const kd of (r.keys || [])) padEvent(kd[0], kd[1])
    }
    // A direction held down keeps moving.
    Timer { id: repeatDelay; interval: mo.holdDelay; onTriggered: repeatTick.start() }
    Timer { id: repeatTick; interval: mo.holdRepeat; repeat: true; onTriggered: if (app.heldDir !== "") app.press(app.heldDir); else stop() }

    // ---------------------------------------------------------------- the window's parts
    // The ground, drawn as an item so a picture of the window's content has it too.
    Rectangle { anchors.fill: parent; z: -1; color: c.ground }
    TopBar {
        id: topBar
        app: app
        visible: !app.bare
        enabled: view.enabled
        anchors { top: parent.top; left: parent.left; right: parent.right }
    }
    // A page arriving fades up from partly there, never from nothing.
    NumberAnimation { id: pageIn; target: view; properties: "opacity"; from: mo.swapFrom; to: 1; duration: mo.gentle; easing.type: mo.ease }
    Loader {
        id: view
        property string file: ""
        // Off under a dialog and under the busy cover: nothing of the page takes a tap, a click or the mouse
        // resting on it then (a backdrop alone doesn't stop those, see Backdrop.qml).
        enabled: !dialog.open && !Hub.busy
        // On an ultrawide screen the page keeps a sensible width, centered, so a switch stays near its words.
        width: Math.min(parent.width, Math.max(app.px(1280), app.height * 1.9))
        anchors { top: app.bare ? parent.top : topBar.bottom; horizontalCenter: parent.horizontalCenter; bottom: bottomBar.top }
    }
    BottomBar {
        id: bottomBar
        app: app
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
        enabled: !Hub.busy
        hints: app.footerHints()
    }
    // The footer's hints, most important first: A and B (they follow the highlight), the pointer's click while it
    // shows, then the page's own as the engine worked them out (its main button, Find, Options, Menu).
    function footerHints() {
        const out = []
        const item = focusItem
        // While a text box has the keyboard, B (Esc on a keyboard) ends the typing and that is all there is.
        if (typing) return [{ k: "b", label: "Done typing" }]
        if (dialog.open) {
            // On the cancel button A and B do the same thing: B says it.
            if (item && item.aLabel && item.fid !== "cancel") out.push({ k: "a", label: item.aLabel })
            out.push({ k: "b", label: dialog.cancelLabel })
            if (ptr.shown) out.push({ k: "r3", label: "Click" })
            return out
        }
        // A shows only where it does something: words to read take the highlight too, so a long page scrolls.
        if (item && item.aLabel) out.push({ k: "a", label: item.aLabel })
        // What B does here: the page's own say first (empty a search, the step before), then Back, or the
        // question to close at the very start.
        const pageB = view.item && view.item.bLabel ? view.item.bLabel : ""
        out.push({ k: "b", label: pageB !== "" ? pageB : atRoot ? "Close" : "Back" })
        if (ptr.shown) out.push({ k: "r3", label: "Click" })
        // (No menu until a mode is picked: it leads to the tabs' places.)
        for (const h of (view.item && view.item.hints ? view.item.hints : [])) if (!(h.k === "view" && bare)) out.push(h)
        return out
    }
    // A note above the footer, or at the top when the highlight is down there: it never covers the highlight.
    Toast {
        id: toast
        app: app
        anchors.horizontalCenter: parent.horizontalCenter
        y: app.toastHigh ? (app.bare ? 0 : topBar.height) + app.px(16) : bottomBar.y - height - app.px(16)
    }
    property bool toastHigh: false
    function placeToast() {
        const it = focusItem
        if (!it || !toast.up) return
        const at = it.mapToItem(null, 0, 0)
        const low = bottomBar.y - toast.height - px(16)
        // Highlight and note overlapping at the bottom: the note goes up.
        toastHigh = at.y + it.height > low - px(8) && at.y < bottomBar.y
    }
    DialogCard { id: dialog; app: app; enabled: !Hub.busy; anchors { top: parent.top; left: parent.left; right: parent.right; bottom: bottomBar.top } }
    // The controller's pointer and the ring of a held button, over everything.
    PointerLayer { app: app }

    // While a confirmed dialog's steps run: one line saying what is happening, nothing else to press.
    Rectangle {
        anchors.fill: parent
        visible: Hub.busy
        z: 60
        color: c.scrim
        Backdrop { }
        // A screen reader says what is happening.
        Accessible.role: Accessible.AlertMessage
        Accessible.name: Hub.busyWords + ". Keep Turbine Hub open until this is done."
        Accessible.ignored: !Hub.busy
        Column {
            anchors.centerIn: parent
            spacing: app.px(16)
            Image {
                source: "file://" + Hub.share + "/art/emblem.png"
                width: app.px(64); height: width
                sourceSize: Qt.size(256, 256)
                mipmap: true
                anchors.horizontalCenter: parent.horizontalCenter
                RotationAnimator on rotation { from: 0; to: 360; duration: mo.spin; loops: Animation.Infinite; running: Hub.busy }
            }
            Label { app: app; text: Hub.busyWords; font.pixelSize: app.fpx(20); anchors.horizontalCenter: parent.horizontalCenter }
            Label { app: app; text: "Keep Turbine Hub open until this is done."; color: c.muted; anchors.horizontalCenter: parent.horizontalCenter }
        }
    }

    Component.onCompleted: {
        if (shot.on && shot.page !== "") {
            const t = tabs.findIndex(x => x.id === shot.page)
            // A picture of a page that opens from a tab shows that tab as the open one.
            if (shot.page.startsWith("module:")) tab = 3
            else if (shot.page === "changes") tab = 4
            else if (["library", "tool", "proton", "saves"].indexOf(shot.page) >= 0) tab = 1
            else if (shot.page === "store-item") tab = 2
            if (t >= 0) { tab = t; stack = [] } else stack = [{ page: shot.page, arg: shot.arg, first: shot.page === "pick" && st.mode === "" }]
            loadPage()
            if (t >= 0 && shot.arg !== "") setArg(shot.arg)
            if (shot.act !== "") { const i = shot.act.indexOf(":"); doAct(shot.act.substring(0, i), shot.act.substring(i + 1), Hub.option("value")) }
        } else if (st.mode === "") {
            stack = [{ page: "pick", arg: "", first: true }]
            loadPage()
        } else loadPage()
        keys.forceActiveFocus()
        if (shot.on) {
            selfCheck.setSource("SelfCheck.qml", { app: app, target: shot.path })
            scripted.start()
        }
    }
    // The picture run's presses, one at a time with a moment between them, as a hand would make them.
    property int scriptedAt: 0
    Timer {
        id: scripted
        interval: mo.scriptStep; repeat: true
        onTriggered: {
            // Each press waits until the page before it is ready, as a hand would.
            if (!app.ready) return
            if (app.scriptedAt < shot.keys.length) { app.press(shot.keys[app.scriptedAt]); app.scriptedAt++ }
            else {
                stop()
                // A picture with the controller's pointer on it ("--pointer=0.6,0.4": where, as parts of the window).
                const at = Hub.option("pointer").split(",")
                if (at.length === 2) { Hub.pointerPlace(app.width * parseFloat(at[0]), app.height * parseFloat(at[1])); app.pointerNews({ x: app.width * parseFloat(at[0]), y: app.height * parseFloat(at[1]), shown: true, moved: true, wheel: 0 }) }
                selfCheck.item.start()
            }
        }
    }
    onStChanged: Qt.callLater(function () { if (view.item) app.reloadPage() })
    // What the build's checks reach.
    readonly property var parts: ({ view: view, bottomBar: bottomBar, dialog: dialog, topBar: topBar, toast: toast })
}
