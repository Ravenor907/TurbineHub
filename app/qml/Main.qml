import QtQuick
import QtQuick.Window
import TurbineHub

// Turbine Hub's window: the look, sizes and icons every component draws with, the pages and their stack, one
// highlight that the controller, the keyboard, the mouse and a finger all move, the one dialog, the toast and the
// footer's hints. What each page shows and what each press does comes from the engine through Hub; nothing here
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
    // Text never goes below what can be read.
    function fpx(v) { return Math.max(9, Math.round(v * unit)) }
    // Touch targets keep a finger's size even at the smallest interface size.
    function tpx(v) { return Math.max(Math.round(v * unit), Math.round(v * 0.7 * Math.min(width / 1280, height / 800))) }

    FontLoader { id: cinzel; source: "file://" + Hub.share + "/fonts/Cinzel.ttf" }
    FontLoader { id: barlow; source: "file://" + Hub.share + "/fonts/Barlow-Regular.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-Medium.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-SemiBold.ttf" }
    FontLoader { source: "file://" + Hub.share + "/fonts/Barlow-Bold.ttf" }
    readonly property string titleFont: cinzel.status === FontLoader.Ready ? cinzel.font.family : "serif"
    readonly property string bodyFont: barlow.status === FontLoader.Ready ? barlow.font.family : "sans-serif"

    // ---------------------------------------------------------------- colors
    QtObject {
        id: c
        readonly property bool hc: app.st.theme === "contrast"
        readonly property bool light: app.st.theme === "light"
        readonly property bool black: app.st.theme === "black" || hc
        readonly property color ground: hc ? "#000000" : light ? "#F1EADB" : black ? "#000000" : "#0B0A08"
        readonly property color bar: hc ? "#000000" : light ? "#E6DCC6" : black ? "#000000" : "#080706"
        readonly property color panel: hc ? "#000000" : light ? "#E7DDC8" : black ? "#0C0B0A" : "#141109"
        readonly property color card: hc ? "#000000" : light ? "#F7F2E7" : black ? "#080707" : "#100E0A"
        readonly property color raised: hc ? "#1A1A1A" : light ? "#DDD1B8" : black ? "#161412" : "#1E1A10"
        readonly property color line: hc ? "#FFFFFF" : light ? "#CDBF9F" : black ? "#26221C" : "#2A2418"
        readonly property color text: hc ? "#FFFFFF" : light ? "#2A2216" : "#EADFC6"
        readonly property color soft: hc ? "#FFFFFF" : light ? "#4A3E2A" : "#D9C9A6"
        readonly property color muted: hc ? "#E0E0E0" : light ? "#5E5038" : "#AC9873"
        readonly property var swatches: ({ brass: "#C99E52", iron: "#A9B4BE", copper: "#B07A5A", jade: "#6FAE8C" })
        readonly property color accentBase: swatches[app.st.accent] || swatches.brass
        readonly property color accent: hc ? "#FFD54A" : light ? Qt.darker(accentBase, 1.8) : accentBase
        readonly property color accentHi: hc ? "#FFE680" : light ? Qt.darker(accentBase, 1.6) : Qt.lighter(accentBase, 1.22)
        readonly property color accentLo: light ? Qt.darker(accentBase, 2.1) : Qt.darker(accentBase, 1.35)
        readonly property color onAccent: light ? "#FFFFFF" : "#0B0A08"
        readonly property color accentText: hc ? "#FFD54A" : light ? Qt.darker(accentBase, 2.2) : Qt.lighter(accentBase, 1.12)
        readonly property color focusFill: hc ? "#333300" : light ? Qt.lighter(accentBase, 1.55) : Qt.darker(accentBase, 4.2)
        // The highlight ring: as plain on the light ground as on the dark ones.
        readonly property color ring: hc ? "#FFE680" : light ? Qt.darker(accentBase, 2.6) : Qt.lighter(accentBase, 1.22)
        // Status colors; the words beside them always carry the meaning too.
        readonly property color ok: hc ? "#9FE8B5" : light ? "#2F5A3C" : "#9FD3B5"
        readonly property color okFill: hc ? "#002A10" : light ? "#D5E6D2" : "#13201A"
        readonly property color warn: hc ? "#FFD080" : light ? "#7A4A10" : "#E6B36A"
        readonly property color warnFill: hc ? "#2A1A00" : light ? "#EEDDB8" : "#241B0C"
        readonly property color stop: hc ? "#FF8A80" : light ? "#8E2A1E" : "#F0907A"
        readonly property color stopFill: hc ? "#2A0500" : light ? "#F0D2C8" : "#26110D"
        readonly property color scrim: Qt.rgba(ground.r * 0.5, ground.g * 0.5, ground.b * 0.5, light ? 0.8 : 0.86)
        // The controller's face buttons, as they are colored on the pad.
        readonly property var pad: ({ a: "#6FAE8C", b: "#E6A98A", x: "#7FA7D9", y: "#E0C060" })
        function tone(name) { return name === "ok" ? ok : name === "warn" ? warn : name === "stop" ? stop : muted }
        function toneFill(name) { return name === "ok" ? okFill : name === "warn" ? warnFill : name === "stop" ? stopFill : raised }
    }
    readonly property var colors: c

    Motion { id: mo; level: app.st.animations && !shot.on ? "full" : "off" }
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
    property bool keyboard: st.window
    function glyph(k) {
        if (keyboard) return ({ a: "Enter", b: "Esc", x: "X", y: "Y", lb: "Q", rb: "E", lt: "Tab", rt: "/", view: "F1", start: "F10", move: "Arrows" })[k] || k.toUpperCase()
        return ({ view: "VIEW", start: "MENU", move: "D-PAD" })[k] || k.toUpperCase()
    }

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
        if (arrived) { fresh = true; freshGuard.restart() }
    }
    Timer { id: freshGuard; interval: mo.pressGuard; onTriggered: app.fresh = false }
    readonly property bool ready: !restoring && !fresh

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
        }
        // The presses that waited go on now (unless putting the highlight back rebuilt the page again).
        if (!restoring && held.length > 0) {
            const q = held
            held = []
            for (const k of q) press(k)
        }
    }
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
    function press(k) {
        if (Hub.busy) return
        if (restoring) { if (held.length < 4) held = held.concat([k]); return }
        if (fresh && ["a", "x", "y", "start"].indexOf(k) >= 0) return
        if (dialog.open && dialog.press(k)) return
        if (focusItem && focusItem.press && focusItem.press(k)) return
        if (k === "up" || k === "down" || k === "left" || k === "right") {
            // A page with two sides keeps the highlight on one side until asked across.
            if (!dialog.open && view.item && view.item.move && view.item.move(k)) return
            moveFocus(k)
            return
        }
        if (dialog.open) return
        if (view.item && view.item.press && view.item.press(k)) return
        if (k === "lb") switchTab(-1)
        else if (k === "rb") switchTab(1)
        else if (k === "b") { if (!back()) askClose() }
        // Help: what this page is and what its buttons do, with Where's my...? one press further.
        else if (k === "view") { if (current.page === "help") open("wheres"); else if (current.page !== "wheres") open("help|" + current.page + "@" + pageArg) }
        // The menu leads to the tabs' places, so it waits until first run is through.
        else if (k === "start" && !bare) doAct("menu", "", "")
    }
    // A page's own keys: X and Y from its model, and the primary button's key.
    function pageKey(m, k) {
        if (!m) return false
        if ((k === "x" || k === "y") && m[k]) { doAct(m[k].act, m[k].arg, ""); return true }
        if (m.primary && m.primary.k === k) { doAct(m.primary.act, m.primary.arg, ""); return true }
        return false
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: (e) => {
            const map = {}
            map[Qt.Key_Up] = "up"; map[Qt.Key_Down] = "down"; map[Qt.Key_Left] = "left"; map[Qt.Key_Right] = "right"
            map[Qt.Key_Return] = "a"; map[Qt.Key_Enter] = "a"; map[Qt.Key_Space] = "a"
            map[Qt.Key_Escape] = "b"; map[Qt.Key_Backspace] = "b"
            map[Qt.Key_Q] = "lb"; map[Qt.Key_E] = "rb"; map[Qt.Key_Tab] = "lt"
            map[Qt.Key_X] = "x"; map[Qt.Key_Y] = "y"; map[Qt.Key_F1] = "view"; map[Qt.Key_F10] = "start"; map[Qt.Key_F5] = "f5"
            map[Qt.Key_Slash] = "rt"
            const k = map[e.key]
            if (!k) return
            e.accepted = true
            app.keyboard = true
            if (k === "f5") app.reloadPage(); else app.press(k)
        }
    }
    // Qt Quick's software drawing (pictures, machines without a GPU): icons paint into pictures there (Icon.qml).
    readonly property bool softwareDraw: keys.GraphicsInfo.api === GraphicsInfo.Software
    // A text box took the keyboard; this gives it back to the window.
    function keysBack() { keys.forceActiveFocus() }
    // A text box wants typing: in Game Mode Steam's on-screen keyboard opens over the app.
    function askKeyboard() { if (Hub.gamemode && !shot.on) Qt.openUrlExternally("steam://open/keyboard") }

    // The controller and finished work, every frame.
    property string heldDir: ""
    Timer {
        interval: 16; repeat: true; running: !shot.on
        onTriggered: {
            const text = Hub.poll()
            if (text === "") return
            const r = JSON.parse(text)
            for (const kd of (r.keys || [])) {
                if (kd[1]) {
                    app.keyboard = false
                    app.press(kd[0])
                    if (["up", "down", "left", "right"].indexOf(kd[0]) >= 0) { app.heldDir = kd[0]; repeatDelay.restart() }
                } else if (kd[0] === app.heldDir) { app.heldDir = ""; repeatDelay.stop(); repeatTick.stop() }
            }
            if (r.done) {
                if (r.done.ok && r.done.open) app.open(r.done.open); else app.reloadPage()
                if (r.done.toast) toast.show(r.done.toast)
            } else if (r.tick && !dialog.open) app.reloadPage()
        }
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
        anchors { top: parent.top; left: parent.left; right: parent.right }
    }
    // A page arriving fades up from partly there, never from nothing.
    NumberAnimation { id: pageIn; target: view; properties: "opacity"; from: mo.swapFrom; to: 1; duration: mo.gentle; easing.type: mo.ease }
    Loader {
        id: view
        property string file: ""
        // On an ultrawide screen the page keeps a sensible width, centered, so a switch stays near its words.
        width: Math.min(parent.width, Math.max(app.px(1280), app.height * 1.9))
        anchors { top: app.bare ? parent.top : topBar.bottom; horizontalCenter: parent.horizontalCenter; bottom: bottomBar.top }
    }
    BottomBar {
        id: bottomBar
        app: app
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
        hints: app.footerHints()
    }
    // The footer's hints, most important first: A and B always, then the page's own, then Help and Menu.
    function footerHints() {
        const out = []
        const item = focusItem
        if (dialog.open) {
            // On the cancel button A and B do the same thing: B says it.
            if (item && item.aLabel && item.fid !== "cancel") out.push({ k: "a", label: item.aLabel })
            out.push({ k: "b", label: dialog.cancelLabel })
            return out
        }
        // A shows only where it does something: words to read take the highlight too, so a long page scrolls.
        if (item && item.aLabel) out.push({ k: "a", label: item.aLabel })
        // What B does here: the page's own say first (empty a search, the step before), then Back, or the
        // question to close at the very start.
        const pageB = view.item && view.item.bLabel ? view.item.bLabel : ""
        out.push({ k: "b", label: pageB !== "" ? pageB : atRoot ? "Close" : "Back" })
        const own = (view.item && view.item.hints ? view.item.hints : []).concat(item && item.hints ? item.hints : [])
        // A hint that says what A says already (the main button under the highlight) is said once.
        for (const h of own) if (!(item && item.aLabel && h.label === item.aLabel)) out.push(h)
        if (current.page !== "wheres") out.push({ k: "view", label: "Help" })
        // A page whose main button is on Start keeps Start for it.
        if (!bare && !own.some(h => h.k === "start")) out.push({ k: "start", label: "Menu" })
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
    DialogCard { id: dialog; app: app; anchors { top: parent.top; left: parent.left; right: parent.right; bottom: bottomBar.top } }

    // While a confirmed dialog's steps run: one line saying what is happening, nothing else to press.
    Rectangle {
        anchors.fill: parent
        visible: Hub.busy
        color: c.scrim
        MouseArea { anchors.fill: parent; hoverEnabled: true; onWheel: (w) => w.accepted = true }
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
            else { stop(); selfCheck.item.start() }
        }
    }
    onStChanged: Qt.callLater(function () { if (view.item) app.reloadPage() })
    // What the build's checks reach.
    readonly property var parts: ({ view: view, bottomBar: bottomBar, dialog: dialog, topBar: topBar })
}
