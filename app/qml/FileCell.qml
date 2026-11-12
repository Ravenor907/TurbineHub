import QtQuick

// One thing in Files: a line of the list (name, date, type, size, as Explorer's Details view), a large icon, a
// short line of the List view, or a tile of the two start pages (a folder, a drive with how full it is).
//
// A press of A opens it. With a mouse a click picks it (Ctrl and Shift pick more, as on Windows), a double click
// opens it, the middle button opens a folder in a new tab, and it can be dragged onto a folder or a place in the
// navigation pane; a finger's tap and the controller pointer's click open it at once, as everywhere in Turbine
// Hub. Its Options menu (X, a right-click) is the engine's: FilesPage works it out when asked. A name that doesn't
// fit is cut at the end (`mayCut`); the whole name is what a screen reader hears and what the details pane shows.
//
// app (required): Main.qml's window. files (required): the FilesPage it is on. row: the engine's line. idx: its
// place among the lines (the tiles count from 0 too). mode: details, icons, list or tile.
Focusable {
    id: c
    required property var files
    property var row: ({})
    property int idx: -1
    property string mode: "details"
    readonly property bool tile: mode === "tile"
    readonly property bool sel: !tile && files.tickStamp >= 0 && files.ticked[row.path] === true
    readonly property bool renamingNow: !tile && files.renaming !== "" && files.renaming === row.path
    readonly property bool inBin: files.model.kind === "bin"
    // Where something dragged onto it goes: into it, when it is a folder that may be changed.
    readonly property string dropPath: row.dir === true && row.locked !== true && !inBin && !tile ? (row.path || "") : ""
    readonly property color ink: row.dim === true ? app.colors.muted : app.colors.text
    readonly property color iconInk: row.locked === true ? app.colors.muted : (row.icon === "folder" || row.icon === "drive" || row.icon === "games" || row.icon === "apps" ? app.colors.accent : app.colors.soft)
    fid: row.fid || ""
    firstFocus: tile ? idx === 0 : idx === files.cur
    aLabel: inBin ? "" : "Open"
    optionsTitle: row.name || ""
    tapActivates: false
    sayRole: Accessible.ListItem
    sayName: [row.name || "", row.type || "", row.date || "", row.size || row.room || "", sel ? "selected" : "", row.locked === true ? "locked" : "", !tile && idx >= 0 ? (idx + 1) + " of " + files.flat.length : ""].filter(x => x !== "").join(", ")
    Accessible.selected: sel
    ringRadius: app.px(8)
    ringGap: -app.px(1)
    function offersNow() { return files.offersFor(row) }
    function focusedNow() { files.cellFocused(c) }
    function press(k) {
        if (renamingNow) return false
        if (k === "a" && aLabel !== "") { files.openRow(row); return true }
        return false
    }
    onActivated: files.openRow(row)
    Component.onCompleted: if (renamingNow) Qt.callLater(beginRename)
    onRenamingNowChanged: if (renamingNow) beginRename()
    function beginRename() { if (!renamingNow || nameBox.activeFocus) return; nameBox.text = row.real || row.name || ""; app.typeIn(nameBox); const dot = nameBox.text.lastIndexOf("."); if (dot > 0 && row.dir !== true) nameBox.select(0, dot); else nameBox.selectAll() }

    Rectangle { anchors.fill: parent; radius: c.app.px(8); color: c.sel || c.tile ? (c.tile ? c.app.colors.card : c.app.colors.raised) : "transparent"; border.width: c.tile ? Math.max(1, c.app.px(1)) : 0; border.color: c.app.colors.line }

    // ---------------------------------------------------------------- a line of the Details view
    Item {
        visible: c.mode === "details"
        anchors { fill: parent; leftMargin: c.app.px(10); rightMargin: c.app.px(12) }
        Rectangle {
            id: tickBox
            visible: c.files.tickedCount > 0
            width: visible ? c.app.ipx(18) : 0; height: c.app.ipx(18)
            radius: c.app.px(4)
            anchors.verticalCenter: parent.verticalCenter
            color: c.sel ? c.app.colors.accent : "transparent"
            border.width: Math.max(1, c.app.px(1)); border.color: c.sel ? c.app.colors.accent : c.app.colors.muted
            Icon { app: c.app; visible: c.sel; anchors.centerIn: parent; d: c.app.icons.check; size: 13; tint: c.app.colors.accentInk }
        }
        Icon { id: lineIcon; app: c.app; d: c.app.icons[c.row.icon] || c.app.icons.file; size: 20; filled: c.row.icon === "folder"; tint: c.iconInk; anchors { left: tickBox.right; leftMargin: tickBox.visible ? c.app.px(8) : 0; verticalCenter: parent.verticalCenter } }
        CutText { app: c.app; visible: !c.renamingNow; anchors { left: lineIcon.right; leftMargin: c.app.px(10); right: dateCol.left; rightMargin: c.app.px(12); verticalCenter: parent.verticalCenter } text: c.row.name || ""; color: c.ink; font.pixelSize: c.app.fpx(15) }
        CutText { app: c.app; id: sizeCol; anchors { right: parent.right; verticalCenter: parent.verticalCenter } width: c.files.colSize; horizontalAlignment: Text.AlignRight; text: c.row.size || "" }
        CutText { app: c.app; id: rightsCol; visible: c.files.colRights > 0; anchors { right: sizeCol.left; rightMargin: visible ? c.app.px(12) : 0; verticalCenter: parent.verticalCenter } width: c.files.colRights; text: c.row.rights || "" }
        CutText { app: c.app; id: typeCol; visible: c.files.colType > 0; anchors { right: rightsCol.left; rightMargin: visible ? c.app.px(12) : 0; verticalCenter: parent.verticalCenter } width: c.files.colType; text: c.row.type || "" }
        CutText { app: c.app; id: dateCol; visible: c.files.colDate > 0; anchors { right: typeCol.left; rightMargin: visible ? c.app.px(12) : 0; verticalCenter: parent.verticalCenter } width: c.files.colDate; text: c.row.date || "" }
    }

    // ---------------------------------------------------------------- a short line of the List view
    Item {
        visible: c.mode === "list"
        anchors { fill: parent; leftMargin: c.app.px(10); rightMargin: c.app.px(10) }
        Icon { id: shortIcon; app: c.app; d: c.app.icons[c.row.icon] || c.app.icons.file; size: 20; filled: c.row.icon === "folder"; tint: c.sel ? c.app.colors.accentText : c.iconInk; anchors.verticalCenter: parent.verticalCenter }
        CutText { app: c.app; visible: !c.renamingNow; anchors { left: shortIcon.right; leftMargin: c.app.px(10); right: parent.right; verticalCenter: parent.verticalCenter } text: c.row.name || ""; color: c.ink; font.pixelSize: c.app.fpx(15) }
    }

    // ---------------------------------------------------------------- a large icon
    Item {
        visible: c.mode === "icons"
        anchors { fill: parent; margins: c.app.px(8) }
        Icon { id: bigIcon; app: c.app; d: c.app.icons[c.row.icon] || c.app.icons.file; size: 44; filled: c.row.icon === "folder"; tint: c.iconInk; anchors { top: parent.top; topMargin: c.app.px(6); horizontalCenter: parent.horizontalCenter } }
        CutText { app: c.app; visible: !c.renamingNow; anchors { top: bigIcon.bottom; topMargin: c.app.px(8); left: parent.left; right: parent.right } horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignTop; wrapMode: Text.WrapAnywhere; maximumLineCount: 2; text: c.row.name || ""; color: c.ink }
    }

    // ---------------------------------------------------------------- a tile of Home and This PC
    Item {
        visible: c.tile
        anchors { fill: parent; leftMargin: c.app.px(14); rightMargin: c.app.px(14) }
        Icon { id: tileIcon; app: c.app; d: c.app.icons[c.row.icon] || c.app.icons.file; size: 26; filled: c.row.icon === "folder"; tint: c.iconInk; anchors.verticalCenter: parent.verticalCenter }
        Column {
            anchors { left: tileIcon.right; leftMargin: c.app.px(12); right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: c.app.px(4)
            CutText { app: c.app; width: parent.width; text: c.row.name || ""; color: c.app.colors.text; font.pixelSize: c.app.fpx(16); font.weight: Font.Medium }
            // How full a drive is.
            Rectangle {
                visible: c.row.used !== null && c.row.used !== undefined
                width: parent.width; height: Math.max(4, c.app.px(6)); radius: height / 2
                color: c.app.colors.raised
                Rectangle { width: parent.width * Math.max(0, Math.min(100, c.row.used || 0)) / 100; height: parent.height; radius: height / 2; color: c.app.colors.accent }
            }
            CutText { app: c.app; visible: text !== ""; width: parent.width; text: c.row.room || (c.row.icon === "bin" ? c.row.size : "") || c.row.detail || ""; elide: Text.ElideMiddle }
        }
    }

    // The name being typed anew, in place (F2, or Rename in Options).
    Rectangle {
        visible: c.renamingNow
        x: c.mode === "icons" ? c.app.px(6) : c.app.px(34) + (c.files.tickedCount > 0 && c.mode === "details" ? c.app.ipx(18) + c.app.px(8) : 0)
        width: (c.mode === "details" ? Math.max(c.app.px(120), c.files.colW("name", parent.width - c.app.px(22)) - x + c.app.px(10)) : parent.width - x - c.app.px(6))
        height: Math.min(parent.height - c.app.px(4), c.app.bpx(34, 15))
        anchors.verticalCenter: c.mode === "icons" ? undefined : parent.verticalCenter
        y: c.mode === "icons" ? parent.height - height - c.app.px(6) : 0
        radius: c.app.px(6)
        color: c.app.colors.ground
        border.width: Math.max(1, c.app.px(2)); border.color: c.app.colors.accent
        TextInput {
            id: nameBox
            Accessible.ignored: true
            anchors { fill: parent; leftMargin: c.app.px(8); rightMargin: c.app.px(8) }
            verticalAlignment: TextInput.AlignVCenter
            color: c.app.colors.text
            selectionColor: c.app.colors.accent
            selectedTextColor: c.app.colors.accentInk
            font.family: c.app.bodyFont
            font.pixelSize: c.app.fpx(15)
            clip: true
            maximumLength: 255
            property bool dropped: false
            onActiveFocusChanged: {
                if (activeFocus) { dropped = false; c.app.typingBegan(nameBox) }
                else { c.app.typingEnded(nameBox); if (c.renamingNow) c.files.endRename(c.row, dropped ? "" : text) }
            }
            Keys.onEscapePressed: { dropped = true; c.app.keysBack() }
            Keys.onReturnPressed: (e) => { if (!e.isAutoRepeat) c.app.keysBack() }
            Keys.onEnterPressed: (e) => { if (!e.isAutoRepeat) c.app.keysBack() }
        }
    }

    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        onTapped: if (c.app.gate(c, "a")) c.files.tapped(c, tap.point.modifiers, tap.point.device.pointerType === PointerDevice.Finger, tap.tapCount)
    }
    // The middle button: a folder opens in a new tab.
    TapHandler { acceptedDevices: PointerDevice.Mouse; acceptedButtons: Qt.MiddleButton; onTapped: if (c.app.gate(c, "a")) c.files.middle(c.row) }
    // Dragged with the mouse (or the controller's pointer with RT held): onto a folder or a place it is moved there,
    // with Ctrl held it is copied.
    DragHandler {
        id: drag
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        enabled: !c.tile && !c.inBin && !c.renamingNow
        onActiveChanged: { if (active) c.files.dragBegan(c); else c.files.dragEnded() }
        onCentroidChanged: if (active) c.files.dragMoved(drag.centroid.scenePosition, drag.centroid.modifiers)
    }
}
