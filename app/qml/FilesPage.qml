import QtQuick
import TurbineHub

// Files: a file explorer laid out the way Windows Explorer is, in Turbine Hub's own look. From the top: the tabs
// (in a Desktop Mode window; full screen they show once there is a second one), back, forward, up and refresh
// beside the address and the search box, the command bar (New, Cut, Copy, Paste, Rename, Delete, Sort, View, the
// Windows-like / True Linux switch, Details), then the navigation pane, the list and the details pane, and the
// status bar. What is where, what each line is called and what may be done all come from the engine
// (engine/src/ui/files.rs); this file draws it and keeps what the hand is doing: what is ticked, where the
// highlight was in the list, a name being typed, something being dragged.
//
// The controls are the shared ones (A opens, B goes back, X is Options, Y is Find, Start ticks), plus Explorer's
// own keys at a keyboard (F2, Del, Ctrl+C / X / V / A / Z, Ctrl+T / W, Ctrl+L, Alt+Up / Left / Right, Alt+Enter, a
// letter jumps to the name that starts with it) and its ways with a mouse (a click picks, Ctrl and Shift pick
// more, a double click opens, the middle button opens a tab, a drag moves).
//
// How big it is drawn: full screen it grows with the screen, with lines a thumb can hit; in a Desktop Mode window
// it has one size and lines for a mouse (app.dense). Under 900 units wide the navigation pane folds to icons, under
// 760 the list keeps Name and Size, under 1150 the command bar keeps its icons and drops their words.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property var hints: model.hints || []
    readonly property bool win: model.view !== "linux"
    readonly property bool dense: app.dense
    readonly property real wide: width / app.unit
    readonly property bool slim: wide < 900
    readonly property bool narrow: wide < 760
    readonly property bool few: wide < 1150
    readonly property bool tiles: model.kind === "tiles"
    readonly property bool inBin: model.kind === "bin"
    readonly property bool viewing: !!model.viewer
    readonly property string layout: model.layout || "details"
    readonly property int rowH: dense ? app.tpx(30) : app.tpx(44)
    // A group's heading is lower than a line.
    readonly property int groupH: app.bpx(dense ? 26 : 32, 13)
    readonly property int toolH: dense ? app.tpx(32) : app.tpx(44)
    readonly property bool paneOn: model.pane === true && !slim && !tiles && !viewing
    // The columns of the Details view (0: left out).
    readonly property int colSize: app.px(92)
    readonly property int colDate: narrow ? 0 : app.px(150)
    readonly property int colType: narrow ? 0 : app.px(paneOn ? 150 : 200)
    readonly property int colRights: !win && !narrow && listBox.width > app.px(940) ? app.px(170) : 0
    readonly property int colGap: app.px(12)
    // Where a column starts in a line that is `w` wide inside its margins, and how wide the name's column is.
    function colX(col, w) {
        let x = w - colSize
        if (col === "size") return x
        if (colRights > 0) x -= colGap + colRights
        if (col === "rights") return x
        if (colType > 0) x -= colGap + colType
        if (col === "type") return x
        if (colDate > 0) x -= colGap + colDate
        return col === "date" ? x : 0
    }
    function colW(col, w) { return col === "size" ? colSize : col === "rights" ? colRights : col === "type" ? colType : col === "date" ? colDate : Math.max(0, colX("date", w) - colGap) }
    // A large icon's cell and a tile grow with their words at a small interface size.
    readonly property int iconsH: Math.max(app.px(126), app.ipx(44) + Math.round(app.fpx(14) * 2.7) + app.px(34))

    // ---------------------------------------------------------------- the lines
    // `flat`: the engine's lines without the group headings. `lines`: what the list draws, a heading or a row of
    // one or more cells each. `where[i]`: the line and column of flat[i].
    readonly property int cols: layout === "details" || tiles ? 1 : Math.max(1, Math.floor((listBox.width - app.px(8)) / app.px(layout === "icons" ? 148 : 290)))
    property var lines: []
    property var flat: []
    property var where: []
    function rebuild() {
        const out = [], at = [], all = []
        let cells = null
        for (const r of (model.rows || [])) {
            if (r.group !== undefined) { cells = null; out.push({ group: r.group, count: r.count }); continue }
            if (!cells || cells.cells.length >= cols) { cells = { cells: [], first: all.length }; out.push(cells) }
            at.push({ line: out.length - 1, col: cells.cells.length })
            cells.cells.push(r)
            all.push(r)
        }
        flat = all; where = at; lines = out
    }
    onColsChanged: rebuild()

    // ---------------------------------------------------------------- the highlight in the list
    // The line the highlight was on last (an index into `flat`): the toolbar's buttons and the keys act on it when
    // nothing is ticked, and it gets the highlight back after the list was read again.
    property int cur: -1
    // The line that held the highlight scrolled out of view (the wheel): the list itself holds it until a direction
    // is pressed.
    property bool parked: false
    property double seenSeq: -1
    property bool firstLook: true
    // The line the engine wants highlighted (the folder just left, something new): kept until the highlight is put
    // back, however many times the page is read again before that.
    property string wantFid: ""
    // The list is being read again: its lines go and come back, and a name being typed in one stays in the typing.
    property bool reading: false
    onModelChanged: { reading = true; rebuild(); jobLive = null; if ((model.focus || "") !== "") wantFid = model.focus; Qt.callLater(fixFocus) }
    Component.onCompleted: { rebuild(); if ((model.focus || "") !== "") wantFid = model.focus; Qt.callLater(fixFocus) }
    // Runs once the new list is in, before the window puts the highlight back.
    function fixFocus() {
        // (The window has gone on to another page already: nothing of this one is left to fix.)
        if (model.seq === undefined) return
        const fresh = model.seq !== seenSeq
        seenSeq = model.seq
        reading = false
        const last = app.lastFid || ""
        let want = wantFid
        wantFid = ""
        if (fresh) { clearTicks(); pathEdit = false }
        else pruneTicks()
        if (renaming !== "" && !flat.some(r => r.path === renaming)) renaming = ""
        // Back on the tab: the line it was left on.
        if (want === "" && firstLook && last.startsWith("row:")) want = last
        firstLook = false
        const found = want === "" ? -1 : flat.findIndex(r => r.fid === want)
        if (found >= 0) { cur = found; app.lastFid = want }
        else if (fresh) {
            cur = flat.length > 0 ? 0 : -1
            // A new place opened from the list or the navigation pane: the highlight goes to its first line. A press
            // on a button (Back, Up, the view switch, a tab) keeps the highlight on that button.
            if (last === "" || last.startsWith("row:") || last.startsWith("nav:") || last === "list-park" || last === "viewer" || last === "search") app.lastFid = ""
        } else cur = Math.min(cur, flat.length - 1)
        if (cur >= 0 && where[cur] && !tiles && !viewing) { list.positionViewAtIndex(where[cur].line, ListView.Contain); list.forceLayout() }
        // (The old lines went as the new ones came: that is no line scrolling out of view.)
        parked = false
        if (app.lastFid === "list-park") app.lastFid = flat[cur] ? flat[cur].fid : ""
        peekSoon()
    }
    function cellAt(i) {
        const w = where[i]
        if (!w) return null
        const holder = list.itemAtIndex(w.line)
        return holder && holder.cellAt ? holder.cellAt(w.col) : null
    }
    function focusFlat(i) {
        if (i < 0 || i >= flat.length) return false
        list.positionViewAtIndex(where[i].line, ListView.Contain)
        list.forceLayout()
        const it = cellAt(i)
        if (!it) return false
        cur = i
        parked = false
        app.setFocus(it, true)
        return true
    }
    function cellFocused(c) { if (!c.tile) { cur = c.idx; parked = false; peekSoon() } }
    // The window asks where the highlight goes when the thing that held it is gone: a line that only scrolled out
    // of view leaves it with the list.
    Connections {
        target: page.app
        function onFocusItemChanged() {
            if (page.app.focusItem !== null || page.tiles || page.viewing) return
            const last = page.app.lastFid || ""
            if (last.startsWith("row:") && page.flat.some(r => r.fid === last) && page.cellAt(page.cur) === null) { page.parked = true; page.app.lastFid = "list-park" }
        }
    }
    function firstShown() {
        const i = list.indexAt(list.width / 2, list.contentY + rowH / 2)
        const line = i >= 0 ? i : 0
        for (let k = line; k < lines.length; k++) if (lines[k].group === undefined) return lines[k].first
        return flat.length > 0 ? 0 : -1
    }

    // ---------------------------------------------------------------- what is ticked
    property var ticked: ({})
    property int tickStamp: 0
    property int tickedCount: 0
    property int anchor: -1
    function setTicks(t) { ticked = t; tickedCount = Object.keys(t).length; tickStamp++ }
    function clearTicks() { if (tickedCount > 0) setTicks({}); anchor = -1 }
    function pruneTicks() {
        if (tickedCount === 0) return
        const t = {}
        for (const r of flat) if (ticked[r.path] === true) t[r.path] = true
        if (Object.keys(t).length !== tickedCount) setTicks(t)
    }
    function toggle(path) { const t = Object.assign({}, ticked); if (t[path]) delete t[path]; else t[path] = true; setTicks(t) }
    function only(path) { const t = {}; t[path] = true; setTicks(t) }
    function range(a, b) { const t = {}; for (let i = Math.min(a, b); i <= Math.max(a, b); i++) if (flat[i]) t[flat[i].path] = true; setTicks(t) }
    function selectAll() { const t = {}; for (const r of flat) t[r.path] = true; setTicks(t) }
    function tickedPaths() { return flat.filter(r => ticked[r.path] === true).map(r => r.path) }
    // What a press on one line is for: everything ticked when the line is among it, else the line alone.
    function pathsFor(r) { return ticked[r.path] === true ? tickedPaths() : [r.path] }
    // What the toolbar and the keys act on: what is ticked, else the line the highlight was on last.
    readonly property var curRow: cur >= 0 && cur < flat.length && !tiles ? flat[cur] : null
    function targets() { return tickedCount > 0 ? tickedPaths() : curRow ? [curRow.path] : [] }
    function can(letter) {
        if (tickedCount > 0) return flat.some(r => ticked[r.path] === true && String(r.o).indexOf(letter) >= 0)
        return curRow !== null && String(curRow.o).indexOf(letter) >= 0
    }
    function tickCur() {
        const f = app.focusItem
        if (f && f.idx !== undefined && f.tile !== true && flat[f.idx]) { toggle(flat[f.idx].path); anchor = f.idx }
    }

    // ---------------------------------------------------------------- presses
    function openRow(r) { if (renaming === "") app.doAct("files-open", r.path || "", r.opens || "") }
    // A click picks, Ctrl and Shift pick more, a double click opens; a finger and the controller's pointer open.
    function tapped(c, mods, finger, count) {
        if (renaming !== "" && renaming !== c.row.path) app.keysBack()
        if (c.tile || finger || app.ptrActing) { openRow(c.row); return }
        if (mods & Qt.ControlModifier) { toggle(c.row.path); anchor = c.idx; return }
        if (mods & Qt.ShiftModifier) { range(anchor < 0 ? c.idx : anchor, c.idx); return }
        if (count >= 2) { openRow(c.row); return }
        only(c.row.path)
        anchor = c.idx
    }
    function middle(r) { if (r.dir === true && (r.opens || r.key)) app.doAct("files-go", r.opens || r.key, "newtab") }
    // The Options of one line or one place: the engine's entries for it (its letters in `o`), made out for it.
    function offersFor(r) {
        const out = [], map = model.itemOptions || {}
        for (const id of String(r.o || "")) {
            const t = map[id]
            if (!t) continue
            const many = t.sel === true && r.path ? pathsFor(r) : null
            out.push({ title: t.title + (many && many.length > 1 ? " (" + many.length + ")" : ""), act: t.act, arg: id === "t" ? (r.opens || r.key || r.path) : (r.path || ""), value: many ? JSON.stringify(many) : (t.value || ""), key: t.key || "", icon: t.icon || "", strip: t.strip === true, local: t.local || "", path: r.path || "" })
        }
        return out
    }
    // An entry of Options that the page carries out itself.
    function local(what, row) {
        if (what === "rename") startRename(row.path)
        else if (what === "all") selectAll()
        else if (what === "path") editPath()
        else if (what === "tick") tickCur()
    }
    function press(k) {
        if (k === "b") {
            if (viewing) { app.doAct("files-close-view", "", ""); return true }
            if (tickedCount > 0) { clearTicks(); return true }
            if (searchValue !== "") { searchInput.text = ""; app.doAct("search", "files", ""); return true }
            if (model.canBack === true) { app.doAct("files-back", "", ""); return true }
            return false
        }
        // Start, the page's main button: tick the highlighted line.
        if (k === "start") { tickCur(); return true }
        return false
    }
    readonly property string bLabel: viewing ? "Close" : tickedCount > 0 ? "Clear selection" : searchValue !== "" ? "Clear search" : model.canBack === true ? "Back" : ""
    // Up and Down in the list go line by line (lines out of view aren't drawn until they are reached).
    function move(k) {
        if (viewing) return false
        const down = k === "down"
        if (parked && (k === "up" || down)) return focusFlat(Math.max(0, firstShown()))
        const f = app.focusItem
        if (!f || f.idx === undefined || f.tile === true) return false
        const w = where[f.idx]
        if (!w) return false
        if (k === "up" || down) {
            let line = w.line + (down ? 1 : -1)
            while (line >= 0 && line < lines.length && lines[line].group !== undefined) line += down ? 1 : -1
            if (line < 0 || line >= lines.length) return false
            return focusFlat(lines[line].first + Math.min(w.col, lines[line].cells.length - 1))
        }
        const col = w.col + (k === "right" ? 1 : -1)
        if (cols > 1 && col >= 0 && col < lines[w.line].cells.length) return focusFlat(f.idx + (k === "right" ? 1 : -1))
        return false
    }
    // Find: the search box takes the highlight and the keyboard.
    readonly property string searchValue: model.search ? (model.search.value || "") : ""
    onSearchValueChanged: if (searchInput.text !== searchValue) searchInput.text = searchValue
    function find(text) {
        if (viewing) return false
        app.setFocus(searchBox, false)
        searchBox.activated()
        if (text !== "") { searchInput.text = searchInput.text + text; app.doAct("search", "files", searchInput.text) }
        return true
    }
    // Explorer's own keys. True when the key was one of them.
    property string typedSoFar: ""
    property double typedAt: 0
    function key(e) {
        if (viewing) return false
        const ctrl = (e.modifiers & Qt.ControlModifier) !== 0, shift = (e.modifiers & Qt.ShiftModifier) !== 0, alt = (e.modifiers & Qt.AltModifier) !== 0
        const f = app.focusItem
        const inList = f !== null && f.idx !== undefined && f.tile !== true
        if (alt) {
            if (e.isAutoRepeat) return true
            if (e.key === Qt.Key_Up) app.doAct("files-up", "", "")
            else if (e.key === Qt.Key_Left) app.doAct("files-back", "", "")
            else if (e.key === Qt.Key_Right) app.doAct("files-fwd", "", "")
            else if (e.key === Qt.Key_D) editPath()
            else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { if (curRow) app.doAct("files-props", curRow.path, "") }
            else return false
            return true
        }
        if (ctrl) {
            if (e.isAutoRepeat) return [Qt.Key_C, Qt.Key_X, Qt.Key_V, Qt.Key_A, Qt.Key_Z, Qt.Key_T, Qt.Key_W, Qt.Key_L, Qt.Key_N].indexOf(e.key) >= 0
            if (e.key === Qt.Key_C && shift) { if (curRow) app.doAct("files-copy-path", curRow.path, win ? "windows" : "real") }
            else if (e.key === Qt.Key_C) clip("files-copy")
            else if (e.key === Qt.Key_X) clip("files-cut")
            else if (e.key === Qt.Key_V) app.doAct("files-paste", "", "")
            else if (e.key === Qt.Key_A) selectAll()
            else if (e.key === Qt.Key_Z) app.doAct("files-undo", "", "")
            else if (e.key === Qt.Key_T) app.doAct("files-tab", "new", "")
            else if (e.key === Qt.Key_W) app.doAct("files-tab", "close", "")
            else if (e.key === Qt.Key_L) editPath()
            else if (e.key === Qt.Key_N && shift) app.doAct("files-new", "folder", "")
            else if (e.key === Qt.Key_Tab) app.doAct("files-tab", "next", "")
            else if (e.key === Qt.Key_Backtab) app.doAct("files-tab", "prev", "")
            else if (e.key === Qt.Key_Space) tickCur()
            else return false
            return true
        }
        if (e.key === Qt.Key_F2) { if (!e.isAutoRepeat && curRow && can("r")) startRename(curRow.path); return true }
        if (e.key === Qt.Key_Delete) { if (!e.isAutoRepeat) remove(); return true }
        if (e.key === Qt.Key_F5) { if (!e.isAutoRepeat) app.doAct("files-refresh", "", ""); return true }
        if (inList && (e.key === Qt.Key_Home || e.key === Qt.Key_End)) { focusFlat(e.key === Qt.Key_Home ? 0 : flat.length - 1); return true }
        // Shift with Up or Down ticks from where it started to where it is now.
        if (inList && shift && (e.key === Qt.Key_Up || e.key === Qt.Key_Down)) {
            if (anchor < 0) anchor = f.idx
            if (move(e.key === Qt.Key_Down ? "down" : "up")) range(anchor, cur)
            return true
        }
        // A letter or a digit: the next line whose name starts with what was typed.
        if (!tiles && flat.length > 0 && e.text.length === 1 && /[^\s\/\x00-\x1f\x7f]/.test(e.text)) {
            const now = Date.now(), ch = e.text.toLowerCase()
            typedSoFar = now - typedAt < 900 ? typedSoFar + ch : ch
            typedAt = now
            // The same letter again goes on to the next name with it.
            const again = typedSoFar.length > 1 && typedSoFar.split("").every(x => x === ch)
            const want = again ? ch : typedSoFar
            const from = want.length === 1 ? cur + 1 : Math.max(cur, 0)
            for (let n = 0; n < flat.length; n++) {
                const i = (from + n + flat.length) % flat.length
                if (String(flat[i].name).toLowerCase().startsWith(want)) { focusFlat(i); break }
            }
            return true
        }
        return false
    }
    function clip(act) { const t = targets(); if (t.length > 0) app.doAct(act, "", JSON.stringify(t)) }
    function remove() { const t = targets(); if (t.length > 0) app.doAct(inBin ? "files-restore" : "files-delete", "", JSON.stringify(t)) }

    // ---------------------------------------------------------------- a name typed in place, a path typed
    property string renaming: ""
    // A name is being typed in the list: the page isn't read again under it for news that can wait.
    readonly property bool busyTyping: renaming !== ""
    function startRename(path) {
        const i = flat.findIndex(r => r.path === path)
        if (i < 0) return
        // (A line that isn't drawn yet starts its typing as soon as it is.)
        if (!focusFlat(i)) { cur = i; wantFid = flat[i].fid }
        renaming = path
    }
    // The typing is over: `name` is what was typed ("" when it was dropped with Esc).
    function endRename(row, name) {
        if (reading) return
        renaming = ""
        if (name !== "" && name !== (row.real || row.name)) app.doAct("files-rename", row.path, name)
    }
    property bool pathEdit: false
    function editPath() {
        if (viewing) return
        pathEdit = true
        pathInput.text = model.typed || ""
        app.setFocus(address, false)
        app.typeIn(pathInput)
        pathInput.selectAll()
    }

    // ---------------------------------------------------------------- dragging with the mouse
    property var dragging: null
    property point dragAt: Qt.point(0, 0)
    property int dragMods: 0
    function dragBegan(c) { dragging = { paths: pathsFor(c.row), name: c.row.name }; dragAt = c.mapToItem(null, c.width / 2, c.height / 2) }
    function dragMoved(pos, mods) {
        dragAt = pos; dragMods = mods
        const it = app.focusableAt(pos)
        if (it && it.dropPath && dragging && dragging.paths.indexOf(it.dropPath) < 0) app.setFocus(it, true)
    }
    function dragEnded() {
        const d = dragging
        dragging = null
        if (!d || Hub.busy || app.parts.dialog.open) return
        const it = app.focusableAt(dragAt)
        const dest = it && it.dropPath ? it.dropPath : ""
        if (dest === "" || d.paths.indexOf(dest) >= 0) return
        app.doAct("files-drop", ((dragMods & Qt.ControlModifier) ? "copy" : "move") + "|" + dest, JSON.stringify(d.paths))
    }

    // ---------------------------------------------------------------- the details pane and the status bar
    property var peek: null
    Timer { id: peekTimer; interval: 140; onTriggered: page.peekNow() }
    function peekSoon() { if (!paneOn) return; if (app.shotOn) peekNow(); else peekTimer.restart() }
    function peekNow() {
        const r = curRow
        if (!paneOn || !r) { peek = null; return }
        if (peek && peek.path === r.path && peek.stamp === model.seq) return
        const p = JSON.parse(Hub.act("files-peek", r.path, "")).peek || null
        if (p) p.stamp = model.seq
        peek = p
    }
    onPaneOnChanged: peekSoon()
    // A copy under way: what the engine said with the page, then what it says as it goes.
    property var jobLive: null
    readonly property var job: jobLive !== null ? jobLive : (model.job || null)
    function jobNews(j) { jobLive = j }

    // An icon button of the bars with only a word for a screen reader.
    readonly property var navIcons: ({ home: "home", folder: "folder", pc: "pc", drive: "drive", game: "games", app: "apps", bin: "bin" })

    // ================================================================ the page
    Rectangle { anchors.fill: parent; color: page.app.colors.ground }

    // ---------------------------------------------------------------- tabs
    Rectangle {
        id: strip
        visible: page.dense || (page.model.tabs || []).length > 1
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: visible ? page.toolH + page.app.px(10) : 0
        color: page.app.colors.bar
        Row {
            id: tabRow
            anchors { left: parent.left; leftMargin: page.app.px(10); bottom: parent.bottom }
            spacing: page.app.px(4)
            Repeater {
                model: page.model.tabs || []
                Focusable {
                    id: ftab
                    required property var modelData
                    required property int index
                    readonly property bool on: modelData.on === true
                    app: page.app
                    fid: "ftab:" + index
                    aLabel: on ? "" : "Switch"
                    optionsTitle: modelData.title || ""
                    offers: [{ title: "Close tab", act: "files-tab", arg: "close:" + index, key: "ctrl+w" }]
                    sayRole: Accessible.PageTab
                    sayName: (modelData.title || "") + (on ? ", open" : "") + ", tab " + (index + 1) + " of " + (page.model.tabs || []).length
                    width: Math.min(page.app.px(230), Math.max(page.app.px(120), (strip.width - page.app.px(260)) / Math.max(1, (page.model.tabs || []).length)))
                    height: page.toolH + page.app.px(4)
                    ringRadius: page.app.px(8)
                    ringGap: -page.app.px(1)
                    onActivated: page.app.doAct("files-tab", "go:" + index, "")
                    Rectangle { anchors.fill: parent; radius: page.app.px(8); color: ftab.on ? page.app.colors.ground : "transparent"; border.width: ftab.on ? Math.max(1, page.app.px(1)) : 0; border.color: page.app.colors.line }
                    Icon { id: ftabIcon; app: page.app; d: page.app.icons[ftab.modelData.icon] || page.app.icons.folder; size: 16; filled: ftab.modelData.icon === "folder"; tint: page.app.colors.accent; anchors { left: parent.left; leftMargin: page.app.px(10); verticalCenter: parent.verticalCenter } }
                    Text {
                        property bool mayCut: true
                        anchors { left: ftabIcon.right; leftMargin: page.app.px(8); right: closeSpot.left; verticalCenter: parent.verticalCenter }
                        text: ftab.modelData.title || ""
                        elide: Text.ElideRight
                        color: ftab.on ? page.app.colors.text : page.app.colors.soft
                        font.family: page.app.bodyFont
                        font.pixelSize: page.app.fpx(14)
                    }
                    // The cross that closes it (a mouse's; Options and Ctrl+W close it too).
                    Item {
                        id: closeSpot
                        visible: (page.model.tabs || []).length > 1
                        width: visible ? page.app.px(28) : page.app.px(8); height: parent.height
                        anchors.right: parent.right
                        Icon { app: page.app; anchors.centerIn: parent; d: page.app.icons.close; size: 12; tint: page.app.colors.muted }
                        TapHandler { onTapped: if (page.app.gate(ftab, "a")) page.app.doAct("files-tab", "close:" + ftab.index, "") }
                    }
                    TapHandler { acceptedDevices: PointerDevice.Mouse; acceptedButtons: Qt.MiddleButton; onTapped: if (page.app.gate(ftab, "a")) page.app.doAct("files-tab", "close:" + ftab.index, "") }
                }
            }
            FileTool { app: page.app; fid: "ftab-new"; icon: "plus"; label: "New tab"; showLabel: false; dense: page.dense; onActivated: page.app.doAct("files-tab", "new", "") }
        }
        // Turbine Hub's own name, in a window: its menu (Help, the other pages, Close).
        Focusable {
            id: hubMark
            app: page.app
            visible: page.dense
            focusable: visible
            fid: "hub-menu"
            aLabel: "Menu"
            sayName: "Turbine Hub, menu"
            anchors { right: parent.right; rightMargin: page.app.px(10); bottom: parent.bottom; bottomMargin: page.app.px(2) }
            width: hubWord.implicitWidth + page.app.px(24); height: page.toolH
            ringRadius: page.app.px(8)
            ringGap: -page.app.px(1)
            onActivated: page.app.press("view")
            Text { id: hubWord; anchors.centerIn: parent; text: "TURBINE HUB"; color: page.app.colors.accentText; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(13); font.letterSpacing: page.app.px(1.5) }
        }
    }

    // ---------------------------------------------------------------- back, forward, up, refresh; address; search
    Rectangle {
        id: bar
        anchors { top: strip.bottom; left: parent.left; right: parent.right }
        height: page.toolH + page.app.px(page.dense ? 12 : 16)
        color: page.app.colors.panel
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
        Row {
            id: goRow
            anchors { left: parent.left; leftMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
            spacing: page.app.px(6)
            FileTool { app: page.app; fid: "go-back"; icon: "back"; label: "Back"; showLabel: false; boxed: true; dense: page.dense; off: page.model.canBack !== true; onActivated: page.app.doAct("files-back", "", "") }
            FileTool { app: page.app; fid: "go-fwd"; icon: "forward"; label: "Forward"; showLabel: false; boxed: true; dense: page.dense; off: page.model.canFwd !== true; onActivated: page.app.doAct("files-fwd", "", "") }
            FileTool { app: page.app; fid: "go-up"; icon: "up"; label: "Up"; showLabel: false; boxed: true; dense: page.dense; off: page.model.canUp !== true; onActivated: page.app.doAct("files-up", "", "") }
            FileTool { app: page.app; fid: "go-refresh"; icon: "refresh"; label: "Refresh"; showLabel: false; boxed: true; dense: page.dense; onActivated: page.app.doAct("files-refresh", "", "") }
        }
        // The address: the way here as pieces to press; pressed, or with Ctrl+L, a box that takes a typed path,
        // Windows-style or real.
        Focusable {
            id: address
            app: page.app
            fid: "address"
            aLabel: "Type a path"
            tapActivates: false
            sayRole: Accessible.EditableText
            sayName: "Address: " + (page.model.typed || "")
            anchors { left: goRow.right; leftMargin: page.app.px(10); right: searchBox.left; rightMargin: page.app.px(10); verticalCenter: parent.verticalCenter }
            height: page.toolH
            ringRadius: page.app.px(8)
            ringGap: -page.app.px(1)
            onActivated: page.editPath()
            Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.ground; border.width: Math.max(1, page.app.px(page.pathEdit ? 2 : 1)); border.color: page.pathEdit ? page.app.colors.accent : page.app.colors.line }
            Item {
                visible: !page.pathEdit
                anchors { left: crumbRow.right; right: parent.right; top: parent.top; bottom: parent.bottom }
                TapHandler { onTapped: if (page.app.gate(address, "a")) page.editPath() }
            }
            Icon { id: addrIcon; app: page.app; visible: !page.pathEdit; d: page.app.icons[page.tiles ? (page.model.place === "home" ? "home" : "pc") : page.inBin ? "bin" : "folder"]; size: 16; filled: !page.tiles && !page.inBin; tint: page.app.colors.accent; anchors { left: parent.left; leftMargin: page.app.px(10); verticalCenter: parent.verticalCenter } }
            Row {
                id: crumbRow
                visible: !page.pathEdit
                anchors { left: addrIcon.right; leftMargin: page.app.px(8); verticalCenter: parent.verticalCenter }
                height: parent.height
                // The pieces that fit, from the end: the first ones give way in a narrow window.
                readonly property real room: address.width - addrIcon.width - page.app.px(30)
                property int stamp: 0
                readonly property int from: {
                    const n = crumbs.count + 0 * stamp
                    let total = 0, first = Math.max(0, n - 1)
                    for (let i = n - 1; i >= 0; i--) {
                        const it = crumbs.itemAt(i)
                        if (!it) break
                        total += it.implicitWidth
                        if (i < n - 1 && total > room) break
                        first = i
                    }
                    return first
                }
                Repeater {
                    id: crumbs
                    model: page.model.crumbs || []
                    onItemAdded: crumbRow.stamp++
                    Row {
                        id: crumb
                        required property var modelData
                        required property int index
                        readonly property bool last: index === crumbs.count - 1
                        visible: index >= crumbRow.from
                        height: crumbRow.height
                        spacing: page.app.px(4)
                        Text {
                            property bool mayCut: true
                            width: Math.min(implicitWidth, Math.max(page.app.px(40), crumbRow.room - page.app.px(20)))
                            elide: Text.ElideRight
                            text: crumb.modelData.label || ""
                            color: crumb.last ? page.app.colors.text : page.app.colors.soft
                            font.family: page.app.bodyFont
                            font.pixelSize: page.app.fpx(15)
                            anchors.verticalCenter: parent.verticalCenter
                            TapHandler { onTapped: if (page.app.gate(address, "a")) page.app.doAct("files-go", crumb.modelData.key, "") }
                        }
                        Icon { app: page.app; visible: !crumb.last && page.win; d: page.app.icons.chevron; size: 11; tint: page.app.colors.muted; anchors.verticalCenter: parent.verticalCenter }
                        Text { visible: !crumb.last && !page.win && crumb.modelData.label !== "/"; text: "/"; color: page.app.colors.muted; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(15); anchors.verticalCenter: parent.verticalCenter }
                        Item { width: page.app.px(2); height: 1 }
                    }
                }
            }
            TextInput {
                id: pathInput
                visible: page.pathEdit
                Accessible.ignored: true
                anchors { fill: parent; leftMargin: page.app.px(12); rightMargin: page.app.px(12) }
                verticalAlignment: TextInput.AlignVCenter
                color: page.app.colors.text
                selectionColor: page.app.colors.accent
                selectedTextColor: page.app.colors.accentInk
                font.family: page.app.bodyFont
                font.pixelSize: page.app.fpx(15)
                clip: true
                maximumLength: 1024
                property bool go: false
                onActiveFocusChanged: {
                    if (activeFocus) { go = false; page.app.typingBegan(pathInput) }
                    else { page.app.typingEnded(pathInput); page.pathEdit = false; if (go && text.trim() !== "") page.app.doAct("files-path", "", text) }
                }
                Keys.onEscapePressed: page.app.keysBack()
                Keys.onReturnPressed: (e) => { if (!e.isAutoRepeat) { go = true; page.app.keysBack() } }
                Keys.onEnterPressed: (e) => { if (!e.isAutoRepeat) { go = true; page.app.keysBack() } }
            }
        }
        Focusable {
            id: searchBox
            app: page.app
            fid: "search"
            aLabel: "Type"
            sayRole: Accessible.EditableText
            sayName: "Search" + (searchInput.text !== "" ? ": " + searchInput.text : ", " + (page.model.search ? page.model.search.hint : ""))
            anchors { right: parent.right; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
            width: Math.min(page.app.px(250), page.width * 0.24)
            height: page.toolH
            ringRadius: page.app.px(8)
            ringGap: -page.app.px(1)
            onActivated: page.app.typeIn(searchInput)
            Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.ground; border.width: Math.max(1, page.app.px(searchInput.activeFocus ? 2 : 1)); border.color: searchInput.activeFocus ? page.app.colors.accent : page.app.colors.line }
            Icon { id: searchIcon; app: page.app; d: page.app.icons.search; size: 16; anchors { left: parent.left; leftMargin: page.app.px(10); verticalCenter: parent.verticalCenter } }
            TextInput {
                id: searchInput
                Accessible.ignored: true
                anchors { left: searchIcon.right; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(8); rightMargin: page.app.px(10) }
                color: page.app.colors.text
                font.family: page.app.bodyFont
                font.pixelSize: page.app.fpx(15)
                clip: true
                maximumLength: 60
                text: page.searchValue
                onTextEdited: page.app.doAct("search", "files", text)
                onActiveFocusChanged: if (activeFocus) page.app.typingBegan(searchInput); else page.app.typingEnded(searchInput)
                Keys.onEscapePressed: page.app.keysBack()
                Keys.onReturnPressed: page.app.keysBack()
                Keys.onEnterPressed: page.app.keysBack()
                Keys.onDownPressed: { page.app.keysBack(); if (page.flat.length > 0 && !page.tiles) page.focusFlat(0) }
                Text { property bool mayCut: true; visible: searchInput.text === ""; width: parent.width; elide: Text.ElideRight; text: page.model.search ? (page.model.search.hint || "") : ""; color: page.app.colors.muted; font: searchInput.font; anchors.verticalCenter: parent.verticalCenter }
            }
        }
    }

    // ---------------------------------------------------------------- the command bar, in Explorer's order
    Rectangle {
        id: tools
        anchors { top: bar.bottom; left: parent.left; right: parent.right }
        height: page.toolH + page.app.px(page.dense ? 8 : 12)
        color: page.app.colors.ground
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
        Row {
            anchors { left: parent.left; leftMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
            spacing: page.app.px(page.few ? 2 : 4)
            FileTool { app: page.app; fid: "tool-new"; icon: "plus"; label: "New"; showLabel: !page.few; dense: page.dense; off: page.model.canAdd !== true; onActivated: page.app.doAct("files-menu", "new", "") }
            Rectangle { width: Math.max(1, page.app.px(1)); height: page.toolH * 0.6; color: page.app.colors.line; anchors.verticalCenter: parent.verticalCenter }
            FileTool { app: page.app; fid: "tool-cut"; icon: "cut"; label: "Cut"; showLabel: !page.few; dense: page.dense; off: !(page.tickStamp >= 0 && page.can("x")); onActivated: page.clip("files-cut") }
            FileTool { app: page.app; fid: "tool-copy"; icon: "copy"; label: "Copy"; showLabel: !page.few; dense: page.dense; off: !(page.tickStamp >= 0 && page.can("c")); onActivated: page.clip("files-copy") }
            FileTool { app: page.app; fid: "tool-paste"; icon: "paste"; label: "Paste"; showLabel: !page.few; dense: page.dense; off: !(page.model.canAdd === true && page.model.clip && page.model.clip.count > 0); onActivated: page.app.doAct("files-paste", "", "") }
            FileTool { app: page.app; fid: "tool-rename"; icon: "rename"; label: "Rename"; showLabel: !page.few; dense: page.dense; off: !(page.tickStamp >= 0 && page.curRow !== null && String(page.curRow.o).indexOf("r") >= 0); onActivated: page.startRename(page.curRow.path) }
            FileTool { app: page.app; fid: "tool-delete"; icon: "remove"; label: page.inBin ? "Restore" : "Delete"; showLabel: !page.few; dense: page.dense; off: !(page.tickStamp >= 0 && page.can(page.inBin ? "s" : "d")); onActivated: page.remove() }
            Rectangle { width: Math.max(1, page.app.px(1)); height: page.toolH * 0.6; color: page.app.colors.line; anchors.verticalCenter: parent.verticalCenter }
            FileTool { app: page.app; fid: "tool-sort"; icon: "sort"; label: "Sort"; showLabel: !page.few; dense: page.dense; onActivated: page.app.doAct("files-menu", "view", "") }
            FileTool { app: page.app; fid: "tool-view"; icon: page.layout === "icons" ? "grid" : "viewlist"; label: "View"; showLabel: !page.few; dense: page.dense; onActivated: page.app.doAct("files-layout", page.layout === "details" ? "icons" : page.layout === "icons" ? "list" : "details", "") }
        }
        Row {
            anchors { right: parent.right; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
            spacing: page.app.px(8)
            // The two views of the same disk, always here.
            Rectangle {
                width: segRow.width + Math.max(1, page.app.px(1)) * 2; height: page.toolH + Math.max(1, page.app.px(1)) * 2
                radius: page.app.px(8)
                color: page.app.colors.raised
                border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                anchors.verticalCenter: parent.verticalCenter
                Row {
                    id: segRow
                    anchors.centerIn: parent
                    Repeater {
                        model: [{ id: "windows", label: page.narrow ? "Windows" : "Windows-like" }, { id: "linux", label: page.narrow ? "Linux" : "True Linux" }]
                        Focusable {
                            id: seg
                            required property var modelData
                            readonly property bool on: (page.model.view || "windows") === modelData.id
                            app: page.app
                            fid: "view-" + modelData.id
                            aLabel: on ? "" : "Switch"
                            sayRole: Accessible.RadioButton
                            sayName: (modelData.id === "windows" ? "Windows-like view" : "True Linux view") + (on ? ", on" : "")
                            width: Math.max(segWord.implicitWidth + page.app.px(22), page.app.touchFloor); height: page.toolH
                            ringRadius: page.app.px(7)
                            ringGap: -page.app.px(1)
                            onActivated: page.app.doAct("files-view", modelData.id, "")
                            Rectangle { anchors.fill: parent; radius: page.app.px(7); color: seg.on ? page.app.colors.accent : "transparent" }
                            Text { id: segWord; anchors.centerIn: parent; text: seg.modelData.label; color: seg.on ? page.app.colors.accentInk : page.app.colors.soft; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(14); font.weight: Font.DemiBold }
                        }
                    }
                }
            }
            FileTool { app: page.app; fid: "tool-pane"; visible: !page.slim; icon: "pane"; label: "Details"; showLabel: !page.few; dense: page.dense; lit: page.model.pane === true; off: page.tiles; onActivated: page.app.doAct("files-pane", "", "") }
        }
    }

    // ---------------------------------------------------------------- the navigation pane
    Rectangle {
        id: nav
        anchors { top: tools.bottom; left: parent.left; bottom: status.top }
        width: page.slim ? Math.max(page.app.touchFloor, page.app.px(40)) + page.app.px(16) : page.app.px(236)
        color: page.app.colors.panel
        Rectangle { anchors.right: parent.right; width: Math.max(1, page.app.px(1)); height: parent.height; color: page.app.colors.line }
        Flickable {
            id: navFlick
            anchors { fill: parent; margins: page.app.px(8) }
            clip: true
            contentHeight: navCol.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: navCol
                width: navFlick.width
                spacing: page.app.px(2)
                Repeater {
                    model: page.model.nav || []
                    Column {
                        id: navEntry
                        required property var modelData
                        required property int index
                        width: navCol.width
                        spacing: page.app.px(2)
                        // The line that starts a group.
                        Item {
                            visible: navEntry.modelData.line === true && navEntry.index > 0
                            width: parent.width; height: visible ? page.app.px(9) : 0
                            Rectangle { anchors.centerIn: parent; width: parent.width - page.app.px(8); height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
                        }
                        Focusable {
                            id: navItem
                            readonly property var row: navEntry.modelData
                            readonly property bool on: row.on === true
                            readonly property string dropPath: row.path || ""
                            readonly property real inset: page.slim ? 0 : page.app.px(16) * (row.depth || 0)
                            app: page.app
                            fid: row.fid
                            aLabel: "Open"
                            optionsTitle: row.label || ""
                            sayRole: Accessible.ListItem
                            sayName: (row.label || "") + (row.pinned === true ? ", pinned" : "") + (on ? ", open" : "")
                            Accessible.selected: on
                            width: navCol.width
                            height: page.dense ? page.app.tpx(30) : page.app.tpx(44)
                            ringRadius: page.app.px(8)
                            ringGap: -page.app.px(1)
                            function offersNow() { return page.offersFor(row) }
                            onActivated: page.app.doAct("files-go", row.key, "")
                            Rectangle { anchors.fill: parent; radius: page.app.px(8); color: navItem.on ? page.app.colors.raised : "transparent" }
                            Rectangle { visible: navItem.on; x: 0; anchors.verticalCenter: parent.verticalCenter; width: Math.max(3, page.app.px(3)); height: parent.height - page.app.px(14); radius: width / 2; color: page.app.colors.accent }
                            Icon {
                                id: navIcon
                                app: page.app
                                d: page.app.icons[page.navIcons[navItem.row.icon] || "folder"]
                                size: 18
                                filled: navItem.row.icon === "folder"
                                tint: page.app.colors.accent
                                anchors.verticalCenter: parent.verticalCenter
                                x: page.slim ? (parent.width - width) / 2 : page.app.px(12) + navItem.inset
                            }
                            Text {
                                property bool mayCut: true
                                visible: !page.slim
                                anchors { left: navIcon.right; leftMargin: page.app.px(10); right: pinMark.left; rightMargin: page.app.px(4); verticalCenter: parent.verticalCenter }
                                text: navItem.row.label || ""
                                elide: page.win ? Text.ElideRight : Text.ElideMiddle
                                color: page.app.colors.text
                                font.family: page.app.bodyFont
                                font.pixelSize: page.app.fpx(15)
                                font.weight: navItem.on ? Font.DemiBold : Font.Normal
                            }
                            Icon { id: pinMark; app: page.app; visible: !page.slim && navItem.row.pinned === true; d: page.app.icons.pin; size: 12; tint: page.app.colors.muted; anchors { right: parent.right; rightMargin: visible ? page.app.px(8) : 0; verticalCenter: parent.verticalCenter } width: visible ? implicitWidth : 0 }
                            TapHandler { acceptedDevices: PointerDevice.Mouse; acceptedButtons: Qt.MiddleButton; onTapped: if (page.app.gate(navItem, "a")) page.app.doAct("files-go", navItem.row.key, "newtab") }
                        }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- the list
    Item {
        id: listBox
        anchors { top: tools.bottom; left: nav.right; right: pane.left; bottom: status.top }

        // A line of help for the place, why it is locked, or why it couldn't be read; the bin's own buttons; the
        // search below the folder.
        Column {
            id: above
            anchors { top: parent.top; left: parent.left; right: parent.right; topMargin: page.app.px(8); leftMargin: page.app.px(12); rightMargin: page.app.px(12) }
            spacing: page.app.px(8)
            visible: !page.viewing
            Rectangle {
                id: noteBox
                readonly property string words: page.model.problem || page.model.note || page.model.locked || ""
                visible: words !== ""
                width: parent.width
                height: noteText.implicitHeight + page.app.px(18)
                radius: page.app.px(8)
                color: page.app.colors.panel
                border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                Icon { id: noteIcon; app: page.app; d: page.app.icons[(page.model.locked || "") !== "" ? "lock" : "info"]; size: 18; tint: page.app.colors.accentText; anchors { left: parent.left; leftMargin: page.app.px(12); top: parent.top; topMargin: page.app.px(10) } }
                Label { id: noteText; app: page.app; anchors { left: noteIcon.right; leftMargin: page.app.px(10); right: parent.right; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter } text: noteBox.words; color: page.app.colors.soft; font.pixelSize: page.app.fpx(14) }
            }
            Flow {
                width: parent.width
                spacing: page.app.px(10)
                visible: binButtons.count > 0 || page.model.canDeep === true || !!page.model.deep
                PageButtons { id: binButtons; app: page.app; options: page.model.options || [] }
                ActionButton { id: deepGo; app: page.app; visible: page.model.canDeep === true; focusable: visible; fid: "deep"; icon: "search"; label: "Search everything below " + (page.model.title || ""); onActivated: page.app.doAct("files-deep", "", "") }
                Label { id: deepWords; app: page.app; visible: !!page.model.deep; height: page.app.tpx(44); text: page.model.deep ? (page.model.deep.running ? "Looking below " + page.model.title + ": " + page.model.deep.count + " found so far" : page.model.deep.count + " found below " + page.model.title + (page.model.deep.more ? ". There may be more: add a word to narrow it." : "")) : ""; color: page.app.colors.soft; font.pixelSize: page.app.fpx(14) }
                ActionButton { app: page.app; visible: !!page.model.deep && page.model.deep.running === true; focusable: visible; fid: "deep-stop"; label: "Stop"; onActivated: page.app.doAct("files-deep-stop", "", "") }
            }
        }

        // The column headings: a click sorts by that column, again turns the order round.
        Item {
            id: header
            visible: page.layout === "details" && !page.tiles && !page.viewing
            anchors { top: above.bottom; left: parent.left; right: parent.right; topMargin: page.app.px(4); leftMargin: page.app.px(8); rightMargin: page.app.px(8) }
            height: visible ? page.app.bpx(30, 12) : 0
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
            Repeater {
                model: ["name", "date", "type", "rights", "size"]
                Item {
                    id: head
                    required property string modelData
                    readonly property string col: modelData
                    readonly property bool rightSide: col === "size"
                    readonly property bool sorted: page.model.sort === col
                    readonly property real inner: header.width - page.app.px(22)
                    visible: page.colW(col, inner) > 0
                    x: page.app.px(10) + page.colX(col, inner)
                    width: page.colW(col, inner)
                    height: header.height
                    Text {
                        id: headWord
                        property bool mayCut: true
                        anchors.verticalCenter: parent.verticalCenter
                        x: head.rightSide ? parent.width - width : 0
                        width: Math.min(implicitWidth, Math.max(0, parent.width - page.app.px(16)))
                        elide: Text.ElideRight
                        text: ((page.model.columns || {})[head.col] || "").toUpperCase()
                        color: head.sorted ? page.app.colors.accentText : page.app.colors.muted
                        font.family: page.app.bodyFont
                        font.pixelSize: page.app.fpx(12)
                        font.letterSpacing: page.app.px(1)
                    }
                    Icon { app: page.app; visible: head.sorted; d: page.app.icons.chevron; size: 10; tint: page.app.colors.accentText; rotation: page.model.desc === true ? 90 : -90; anchors.verticalCenter: parent.verticalCenter; x: head.rightSide ? headWord.x - width - page.app.px(4) : headWord.x + headWord.width + page.app.px(4) }
                    TapHandler { onTapped: if (page.app.gate(null, "a") && head.col !== "rights") page.app.doAct("files-sort", head.col, "") }
                }
            }
        }

        // Holds the highlight for the list when the line that had it scrolled out of view.
        Focusable {
            app: page.app
            visible: page.parked && list.visible
            focusable: visible
            fid: "list-park"
            aLabel: ""
            tapActivates: false
            sayRole: Accessible.List
            sayName: (page.model.title || "") + ", " + (page.model.status ? page.model.status.count : "")
            anchors.fill: list
            ringGap: -page.app.px(2)
        }
        ListView {
            id: list
            visible: !page.tiles && !page.viewing
            anchors { top: header.bottom; left: parent.left; right: parent.right; bottom: parent.bottom; topMargin: page.app.px(2); leftMargin: page.app.px(8); rightMargin: page.app.px(8); bottomMargin: page.app.px(4) }
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            // A mouse drags files, not the list (the wheel scrolls it); a finger still flicks.
            acceptedButtons: Qt.NoButton
            cacheBuffer: page.rowH * 6
            model: page.lines
            delegate: Item {
                id: line
                required property var modelData
                required property int index
                readonly property bool heading: modelData.group !== undefined
                readonly property var cells: modelData.cells || []
                width: list.width
                height: heading ? page.groupH : page.layout !== "icons" ? page.rowH : page.iconsH
                function cellAt(col) { return cellsRep.itemAt(col) }
                // A group's heading: its name, how many, a rule.
                Item {
                    visible: line.heading
                    anchors.fill: parent
                    Text { id: groupWord; anchors { left: parent.left; leftMargin: page.app.px(8); bottom: parent.bottom; bottomMargin: page.app.px(4) } text: line.heading ? line.modelData.group + "  (" + line.modelData.count + ")" : ""; color: page.app.colors.accentText; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(13); font.weight: Font.DemiBold }
                    Rectangle { anchors { left: groupWord.right; leftMargin: page.app.px(10); right: parent.right; rightMargin: page.app.px(8); verticalCenter: groupWord.verticalCenter } height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
                }
                Row {
                    visible: !line.heading
                    anchors.fill: parent
                    Repeater {
                        id: cellsRep
                        model: line.cells
                        FileCell {
                            required property var modelData
                            required property int index
                            app: page.app
                            files: page
                            row: modelData
                            idx: line.modelData.first + index
                            mode: page.layout
                            width: page.cols === 1 ? line.width : Math.floor(line.width / page.cols)
                            height: line.height
                        }
                    }
                }
            }
        }
        // The two start pages: tiles under headings (folders; drives with how full they are).
        Flickable {
            id: tilesFlick
            visible: page.tiles && !page.viewing
            anchors { top: above.bottom; left: parent.left; right: parent.right; bottom: parent.bottom; topMargin: page.app.px(4) }
            clip: true
            contentHeight: tilesCol.implicitHeight + page.app.px(16)
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: tilesCol
                x: page.app.px(14)
                width: tilesFlick.width - page.app.px(28)
                spacing: page.app.px(8)
                Repeater {
                    model: page.tiles ? (page.model.sections || []) : []
                    Column {
                        id: sec
                        required property var modelData
                        required property int index
                        width: tilesCol.width
                        spacing: page.app.px(8)
                        Text { text: (sec.modelData.title || "").toUpperCase(); topPadding: page.app.px(8); color: page.app.colors.accentText; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(12); font.weight: Font.DemiBold; font.letterSpacing: page.app.px(1.5) }
                        Flow {
                            width: parent.width
                            spacing: page.app.px(10)
                            Repeater {
                                model: sec.modelData.tiles || []
                                FileCell {
                                    required property var modelData
                                    required property int index
                                    readonly property int perRow: Math.max(1, Math.floor((sec.width + page.app.px(10)) / page.app.px(270)))
                                    app: page.app
                                    files: page
                                    row: modelData
                                    idx: sec.index === 0 ? index : 1000 * sec.index + index
                                    mode: "tile"
                                    width: Math.floor((sec.width - page.app.px(10) * (perRow - 1)) / perRow)
                                    height: Math.max(page.app.tpx(68), Math.round(page.app.fpx(16) * 1.4 + page.app.fpx(14) * 1.4) + page.app.px(modelData.used !== null && modelData.used !== undefined ? 40 : 26))
                                }
                            }
                        }
                    }
                }
            }
        }
        Label {
            app: page.app
            visible: (page.model.empty || "") !== "" && !page.viewing
            anchors { top: header.bottom; topMargin: page.app.px(26); left: parent.left; right: parent.right; leftMargin: page.app.px(20); rightMargin: page.app.px(20) }
            horizontalAlignment: Text.AlignHCenter
            text: page.model.empty || ""
            color: page.app.colors.muted
        }

        // A text or a picture, shown here: B closes it.
        Item {
            id: viewer
            visible: page.viewing
            anchors.fill: parent
            readonly property var v: page.model.viewer || ({})
            Text {
                id: viewName
                property bool mayCut: true
                anchors { left: parent.left; leftMargin: page.app.px(16); right: viewClose.left; rightMargin: page.app.px(12); verticalCenter: viewClose.verticalCenter }
                elide: Text.ElideMiddle
                text: (viewer.v.name || "") + (viewer.v.note ? "   ·   " + viewer.v.note : "")
                color: page.app.colors.text
                font.family: page.app.bodyFont
                font.pixelSize: page.app.fpx(16)
                font.weight: Font.DemiBold
            }
            ActionButton { id: viewClose; app: page.app; visible: page.viewing; focusable: visible; fid: "view-close"; label: "Close"; anchors { top: parent.top; topMargin: page.app.px(8); right: parent.right; rightMargin: page.app.px(12) } onActivated: page.app.doAct("files-close-view", "", "") }
            Focusable {
                id: reader
                app: page.app
                visible: page.viewing
                focusable: visible
                fid: "viewer"
                firstFocus: true
                aLabel: ""
                tapActivates: false
                sayRole: Accessible.StaticText
                sayName: (viewer.v.name || "") + ". " + (viewer.v.kind === "text" ? viewer.v.text : viewer.v.note || "")
                anchors { top: viewClose.bottom; topMargin: page.app.px(8); left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: page.app.px(12); rightMargin: page.app.px(12); bottomMargin: page.app.px(8) }
                ringGap: -page.app.px(2)
                function press(k) {
                    if (k !== "up" && k !== "down") return false
                    const max = Math.max(0, viewFlick.contentHeight - viewFlick.height)
                    const to = Math.max(0, Math.min(max, viewFlick.contentY + (k === "down" ? 1 : -1) * viewFlick.height * 0.6))
                    if (Math.abs(to - viewFlick.contentY) < 1) return false
                    viewFlick.contentY = to
                    return true
                }
                Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.card; border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line }
                Flickable {
                    id: viewFlick
                    anchors { fill: parent; margins: page.app.px(12) }
                    clip: true
                    contentHeight: Math.max(1, viewText.implicitHeight)
                    boundsBehavior: Flickable.StopAtBounds
                    Text { id: viewText; width: viewFlick.width; wrapMode: Text.WrapAnywhere; textFormat: Text.PlainText; text: viewer.v.kind === "text" ? (viewer.v.text || "") : ""; color: page.app.colors.text; font.family: "monospace"; font.pixelSize: page.app.fpx(14) }
                }
                Item {
                    anchors { fill: parent; margins: page.app.px(12) }
                    Image { visible: viewer.v.kind === "picture"; anchors.fill: parent; fillMode: Image.PreserveAspectFit; asynchronous: false; cache: false; smooth: false; source: viewer.v.kind === "picture" && viewer.v.load ? "file://" + viewer.v.load : "" }
                }
            }
        }
    }

    // ---------------------------------------------------------------- the details pane
    Rectangle {
        id: pane
        visible: page.paneOn
        anchors { top: tools.bottom; right: parent.right; bottom: status.top }
        width: visible ? page.app.px(300) : 0
        color: page.app.colors.panel
        Rectangle { anchors.left: parent.left; width: Math.max(1, page.app.px(1)); height: parent.height; color: page.app.colors.line }
        Column {
            id: paneCol
            anchors { fill: parent; margins: page.app.px(16) }
            spacing: page.app.px(10)
            readonly property var p: page.peek || ({})
            Label { app: page.app; visible: page.peek === null; width: parent.width; text: "Pick something to see what it is."; color: page.app.colors.muted; font.pixelSize: page.app.fpx(14) }
            Item {
                visible: page.peek !== null && paneCol.p.kind !== "text"
                width: parent.width; height: page.app.px(150)
                Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.card; border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line }
                Icon { app: page.app; visible: paneCol.p.kind !== "picture"; anchors.centerIn: parent; d: page.app.icons[page.curRow ? page.curRow.icon : "file"] || page.app.icons.file; size: 56; filled: page.curRow !== null && page.curRow.icon === "folder"; tint: page.app.colors.accent }
                Image { visible: paneCol.p.kind === "picture"; anchors { fill: parent; margins: page.app.px(8) } fillMode: Image.PreserveAspectFit; cache: false; smooth: false; source: paneCol.p.kind === "picture" && paneCol.p.load ? "file://" + paneCol.p.load : "" }
            }
            // The start of a text: its first lines, each cut where the pane ends.
            Rectangle {
                visible: page.peek !== null && paneCol.p.kind === "text"
                width: parent.width; height: page.app.px(150)
                radius: page.app.px(8)
                color: page.app.colors.card
                border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                Column {
                    anchors { fill: parent; margins: page.app.px(10) }
                    Repeater {
                        model: paneCol.p.kind === "text" ? String(paneCol.p.text || "").split("\n").slice(0, Math.max(1, Math.floor((page.app.px(130)) / (page.app.fpx(12) * 1.35)))) : []
                        Text { required property string modelData; property bool mayCut: true; width: parent.width; height: Math.round(page.app.fpx(12) * 1.35); elide: Text.ElideRight; textFormat: Text.PlainText; text: modelData.replace(/\t/g, "    "); color: page.app.colors.soft; font.family: "monospace"; font.pixelSize: page.app.fpx(12) }
                    }
                }
            }
            Text { property bool mayCut: true; visible: page.peek !== null; width: parent.width; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight; text: paneCol.p.name || ""; color: page.app.colors.text; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(17); font.weight: Font.DemiBold }
            Repeater {
                model: page.peek !== null ? (paneCol.p.lines || []) : []
                Column {
                    id: fact
                    required property var modelData
                    width: paneCol.width
                    spacing: page.app.px(1)
                    Text { text: fact.modelData.title; color: page.app.colors.muted; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(12) }
                    Text { property bool mayCut: true; width: parent.width; elide: Text.ElideMiddle; text: fact.modelData.line; color: page.app.colors.soft; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(14) }
                }
            }
        }
    }

    // ---------------------------------------------------------------- the status bar
    Rectangle {
        id: status
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: page.job !== null ? page.app.tpx(44) + page.app.px(8) : page.app.bpx(30, 13)
        color: page.app.colors.bar
        Rectangle { anchors.top: parent.top; width: parent.width; height: Math.max(1, page.app.px(1)); color: page.app.colors.line }
        readonly property var st: page.model.status || ({})
        Text {
            id: countWord
            anchors { left: parent.left; leftMargin: page.app.px(14); verticalCenter: parent.verticalCenter }
            text: (status.st.count || "") + (page.tickedCount > 0 ? "   ·   " + page.tickedCount + " selected" : "") + ((status.st.locked || "") !== "" ? "   ·   " + status.st.locked : "")
            color: page.app.colors.muted
            font.family: page.app.bodyFont
            font.pixelSize: page.app.fpx(13)
        }
        // The real folder behind a Windows-like place; a copy's progress while one runs.
        Text {
            property bool mayCut: true
            visible: page.job === null
            anchors { left: countWord.right; leftMargin: page.app.px(24); right: viewSpots.left; rightMargin: page.app.px(16); verticalCenter: parent.verticalCenter }
            elide: Text.ElideMiddle
            text: (status.st.real || "") !== "" ? "Real folder: " + status.st.real : (status.st.words || "")
            color: page.app.colors.muted
            font.family: page.app.bodyFont
            font.pixelSize: page.app.fpx(13)
        }
        Item {
            visible: page.job !== null
            anchors { left: countWord.right; leftMargin: page.app.px(24); right: viewSpots.left; rightMargin: page.app.px(16); top: parent.top; bottom: parent.bottom }
            Text {
                id: jobWords
                property bool mayCut: true
                anchors { left: parent.left; right: jobBar.left; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
                elide: Text.ElideMiddle
                text: page.job ? page.job.words + (page.job.detail ? "   (" + page.job.detail + ")" : "") : ""
                color: page.app.colors.text
                font.family: page.app.bodyFont
                font.pixelSize: page.app.fpx(14)
            }
            Rectangle {
                id: jobBar
                anchors { right: jobStop.left; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
                width: Math.min(page.app.px(220), parent.width * 0.3); height: Math.max(5, page.app.px(8)); radius: height / 2
                color: page.app.colors.raised
                Rectangle { width: parent.width * (page.job ? Math.max(0, Math.min(1, page.job.part || 0)) : 0); height: parent.height; radius: height / 2; color: page.app.colors.accent }
            }
            ActionButton { id: jobStop; app: page.app; visible: page.job !== null; focusable: visible; fid: "job-stop"; label: "Stop"; anchors { right: parent.right; verticalCenter: parent.verticalCenter } onActivated: page.app.doAct("files-job-stop", "", "") }
        }
        // Details or large icons, for a mouse (View in the command bar is the same switch at a finger's size).
        Row {
            id: viewSpots
            anchors { right: parent.right; rightMargin: page.app.px(12); verticalCenter: parent.verticalCenter }
            spacing: page.app.px(4)
            visible: !page.tiles
            Repeater {
                model: [{ id: "details", icon: "viewlist" }, { id: "icons", icon: "grid" }]
                Rectangle {
                    id: spot
                    required property var modelData
                    readonly property bool on: page.layout === modelData.id
                    width: page.app.ipx(26); height: Math.min(status.height - page.app.px(6), page.app.ipx(22))
                    radius: page.app.px(5)
                    color: "transparent"
                    border.width: Math.max(1, page.app.px(1)); border.color: on ? page.app.colors.accentLo : page.app.colors.line
                    Icon { app: page.app; anchors.centerIn: parent; d: page.app.icons[spot.modelData.icon]; size: 13; tint: spot.on ? page.app.colors.accentText : page.app.colors.muted }
                    TapHandler { onTapped: if (page.app.gate(null, "a")) page.app.doAct("files-layout", spot.modelData.id, "") }
                }
            }
        }
    }

    // What is being dragged, beside the mouse.
    Rectangle {
        readonly property bool cover: true
        visible: page.dragging !== null
        z: 30
        x: Math.min(page.width - width - 4, page.mapFromItem(null, page.dragAt.x, page.dragAt.y).x + page.app.px(14))
        y: page.mapFromItem(null, page.dragAt.x, page.dragAt.y).y + page.app.px(10)
        width: dragWord.implicitWidth + page.app.px(22); height: page.app.bpx(32, 14)
        radius: page.app.px(8)
        color: page.app.colors.raised
        border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.accentLo
        Text { id: dragWord; anchors.centerIn: parent; text: page.dragging ? (page.dragging.paths.length > 1 ? page.dragging.paths.length + " items" : page.dragging.name) : ""; color: page.app.colors.text; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(14) }
    }
}
