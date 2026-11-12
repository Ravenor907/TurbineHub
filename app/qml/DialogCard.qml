import QtQuick
import QtQuick.Layouts
import TurbineHub

// The one dialog. Every question Turbine Hub asks looks like this: a title, what will happen in plain words, the
// exact changes as a list, the password box when one is needed, and the buttons pinned at the bottom with the
// controller's button inside each. It sizes to what it holds and scrolls the list when that is long. The same
// card shows the menu (a list of places to go), an Options menu (`options`: what one thing and its page can do,
// each row with the button that does it directly, where it has one) and plain explanations (one Close button).
// At a keyboard Tab and Shift+Tab go from one thing in it to the next.
//
// A dark backdrop covers the page behind it (the footer stays clear, it says what A and B do here), and nothing
// behind it takes a press while it is open.
//
// app (required): Main.qml's window. show(d): opens it with the engine's dialog. close(): closes it. Two dialogs
// are answered here instead of by the engine: `local: "quit"` closes Turbine Hub on confirm, and `keep` (a change
// that could leave the screen unreadable) counts down `keep.seconds` in its words ("{n}") and then goes back by
// sending `keep.act`, as Cancel does.
Item {
    id: dlg
    required property var app
    property var d: null
    readonly property bool open: d !== null
    property string error: ""
    // What held the highlight on the page behind, to give it back on closing.
    property string behind: ""
    readonly property string ask: d ? (d.ask || "") : ""
    readonly property string cancelLabel: d ? d.cancel : "Close"
    readonly property bool hasConfirm: d !== null && (d.confirm || "") !== ""
    // The confirm button only works once the password it needs is typed (twice for a new one).
    readonly property bool typed: ask === "" || (pw.text !== "" && (ask !== "new" || pw2.text !== ""))
    z: 50
    visible: open
    // A screen reader hears the question when it opens: its title, then what will happen.
    Accessible.role: d !== null && d.rows ? Accessible.PopupMenu : Accessible.Dialog
    Accessible.name: d ? (d.title || "") : ""
    Accessible.description: d ? (d.keep ? (d.text || "").replace("{n}", remaining) : (d.text || "")) : ""
    // The card, for the build's checks.
    readonly property Item cardItem: card

    function show(model) {
        // A dialog over a dialog (the menu's choice asking something) keeps the page's highlight to come back to.
        if (d === null) behind = app.lastFid
        d = model
        error = ""
        pw.text = ""; pw2.text = ""
        list.contentY = 0
        remaining = d.keep ? (d.keep.seconds || 10) : 0
        app.lastFid = ""
        app.focusItem = null
        app.settle(true)
        Qt.callLater(function () {
            app.restoreFocus()
            // At a keyboard the password can be typed at once.
            if (dlg.ask !== "" && app.keyboard) app.typeIn(pw)
        })
    }
    function close() {
        d = null
        app.keysBack()
        app.lastFid = behind
        app.focusItem = null
        app.settle(true)
        Qt.callLater(app.restoreFocus)
    }
    function cancel() {
        // A size that asked to be kept goes back.
        if (d && d.keep) { const k = d.keep; close(); app.doAct(k.act, k.arg, k.value); return }
        Hub.cancel(); close()
    }
    function confirm() {
        if (d && d.local === "quit") { Qt.quit(); return }
        if (d && d.keep) { close(); return }
        if (!typed) { error = ask === "new" ? "Type the new password in both boxes first." : "Type the password first."; return }
        app.confirmDialog(pw.text, pw2.text)
    }
    // Enter in a password box: on to the second box when there is one to fill, else the typing is done and the
    // dialog is answered. A key held down counts once.
    function entered(e, box) {
        if (e.isAutoRepeat) return
        if (box === pw && ask === "new") { app.typeIn(pw2); return }
        app.typingDone()
        confirm()
    }
    // A row of a menu: the menu closes, then the row is carried out (the row is gone by then, so the window does it).
    function choose(row) {
        close()
        app.chosen(row)
    }
    // Scrolls the words and the list by most of what shows; false at the end.
    function scroll(dir) {
        const max = Math.max(0, list.contentHeight - list.height)
        const to = Math.max(0, Math.min(max, list.contentY + dir * Math.max(app.px(60), list.height * 0.6)))
        if (Math.abs(to - list.contentY) < 1) return false
        list.contentY = to
        return true
    }
    readonly property bool scrolls: list.contentHeight > list.height + 1
    function press(k) {
        if (k === "b") { cancel(); return true }
        // The button that opened a menu closes it again.
        if (d && d.rows && ((k === "x" && d.options === true) || (k === "view" && d.options !== true))) { cancel(); return true }
        // Down from the buttons, where nothing is below, reads on in the list.
        if (k === "down" && scrolls && app.focusItem && (app.focusItem.fid === "cancel" || app.focusItem.fid === "confirm")) return scroll(1)
        // The tabs, Help and the menu wait until the dialog is answered.
        return ["lb", "rb", "lt", "rt", "view", "start", "play", "f1", "x", "y"].indexOf(k) >= 0
    }
    // A change that goes back by itself counts down out loud.
    property int remaining: 0
    Timer {
        interval: 1000; repeat: true
        running: dlg.open && dlg.d.keep !== undefined && dlg.remaining > 0
        onTriggered: { dlg.remaining--; if (dlg.remaining <= 0) dlg.cancel() }
    }

    Rectangle {
        anchors.fill: parent
        color: dlg.app.colors.scrim
        // A tap beside the card closes the dialog, when a press would count (not in the moment it appears).
        Backdrop { onTapped: if (dlg.app.gate(dlg, "a")) dlg.cancel() }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(dlg.app.px(dlg.d && dlg.d.rows ? 460 : 680), parent.width - dlg.app.px(48))
        height: Math.min(body.implicitHeight + dlg.app.px(52), parent.height - dlg.app.px(40))
        radius: dlg.app.px(12)
        // It fades in as it opens, and at Full it also grows a little into place.
        opacity: dlg.open ? 1 : dlg.app.motion.swapFrom
        Behavior on opacity { enabled: dlg.app.motion.on; NumberAnimation { duration: dlg.app.motion.standard; easing.type: dlg.app.motion.ease } }
        scale: dlg.open ? 1 : dlg.app.motion.popScale
        Behavior on scale { enabled: dlg.app.motion.moves; NumberAnimation { duration: dlg.app.motion.moveStandard; easing.type: dlg.app.motion.ease } }
        color: dlg.app.colors.panel
        border.width: Math.max(1, dlg.app.px(2))
        border.color: dlg.app.colors.accentLo
        // Clicks on the card stay on the card.
        Backdrop { }

        ColumnLayout {
            id: body
            anchors.fill: parent
            anchors.margins: dlg.app.px(26)
            spacing: dlg.app.px(14)

            Label { app: dlg.app; Layout.fillWidth: true; Layout.minimumHeight: implicitHeight; text: dlg.d ? dlg.d.title : ""; font.family: dlg.app.titleFont; font.pixelSize: dlg.app.fpx(24); font.weight: Font.Bold }

            // What will happen in words, then the exact changes or the places the menu leads to. When it is longer
            // than the room, it scrolls: Up from the buttons lands on it and Up and Down read through it; Down on
            // the buttons reads on too. The buttons stay where they are.
            Focusable {
                id: reader
                app: dlg.app
                visible: dlg.d !== null && ((dlg.d.text || "") !== "" || (dlg.d.items || []).length > 0 || (dlg.d.rows || []).length > 0)
                focusable: dlg.open && dlg.scrolls && !(dlg.d && dlg.d.rows)
                fid: "list"
                aLabel: ""
                sayRole: Accessible.StaticText
                sayName: dlg.d ? [dlg.d.text || ""].concat((dlg.d.items || []).map(x => x.title + (x.line ? ": " + x.line : ""))).filter(x => x !== "").join(". ") : ""
                tapActivates: false
                ringGap: dlg.app.px(2)
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: listCol.implicitHeight
                Layout.minimumHeight: Math.min(listCol.implicitHeight, dlg.app.px(90))
                function press(k) {
                    if (k === "up" || k === "down") return dlg.scroll(k === "down" ? 1 : -1)
                    return false
                }
                Flickable {
                    id: list
                    anchors.fill: parent
                    clip: true
                    contentHeight: listCol.implicitHeight
                    boundsBehavior: Flickable.StopAtBounds
                    Behavior on contentY { enabled: dlg.app.motion.moves; NumberAnimation { duration: dlg.app.motion.moveStandard; easing.type: dlg.app.motion.ease } }
                    Column {
                        id: listCol
                        width: list.width
                        spacing: dlg.app.px(8)
                        Label { app: dlg.app; visible: text !== ""; width: listCol.width; bottomPadding: dlg.app.px(6); text: dlg.d ? (dlg.d.keep ? (dlg.d.text || "").replace("{n}", dlg.remaining) : (dlg.d.text || "")) : ""; color: dlg.app.colors.soft; font.pixelSize: dlg.app.fpx(17); lineHeight: 1.2 }
                        Repeater {
                            model: dlg.d ? (dlg.d.items || []) : []
                            RowLayout {
                                id: item
                                required property var modelData
                                width: listCol.width
                                spacing: dlg.app.px(10)
                                // A tick for a change the dialog makes; a plain marker for a step of a how-to (nothing is done yet).
                                Icon { app: dlg.app; d: dlg.d && dlg.d.guide === true ? dlg.app.icons.chevron : dlg.app.icons.check; size: 18; tint: dlg.d && dlg.d.guide === true ? dlg.app.colors.muted : dlg.app.colors.accent; Layout.alignment: Qt.AlignTop; Layout.topMargin: dlg.app.px(3) }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Label { app: dlg.app; Layout.fillWidth: true; text: item.modelData.title; font.weight: Font.DemiBold; wrapMode: Text.Wrap }
                                    Label { app: dlg.app; Layout.fillWidth: true; visible: text !== ""; text: item.modelData.line; color: dlg.app.colors.muted; font.pixelSize: dlg.app.fpx(15); wrapMode: Text.Wrap }
                                }
                            }
                        }
                        Repeater {
                            model: dlg.d ? (dlg.d.rows || []) : []
                            Focusable {
                                id: menuRow
                                required property var modelData
                                required property int index
                                app: dlg.app
                                fid: "menu:" + index
                                firstFocus: index === 0
                                aLabel: dlg.d && dlg.d.options === true ? "Choose" : "Open"
                                sayRole: Accessible.MenuItem
                                readonly property string keyName: modelData.key ? dlg.app.glyph(modelData.key) : ""
                                sayName: (modelData.title || "") + (keyName !== "" && modelData.key !== "a" ? ", " + (modelData.key === "play" ? "hold " : "") + keyName : "") + ", " + (index + 1) + " of " + (dlg.d ? (dlg.d.rows || []).length : 0)
                                width: listCol.width - dlg.app.px(8)
                                x: dlg.app.px(4)
                                height: Math.max(dlg.app.tpx(48), rowTitle.implicitHeight + dlg.app.px(16))
                                ringRadius: dlg.app.px(8)
                                ringGap: -dlg.app.px(1)
                                onActivated: dlg.choose(modelData)
                                Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.raised }
                                Label { app: dlg.app; anchors.verticalCenter: parent.verticalCenter; x: dlg.app.px(16); id: rowTitle; width: parent.width - x - rowKey.width - dlg.app.px(20); text: menuRow.modelData.title; font.weight: Font.DemiBold }
                                // The button that does this directly, where it has one (its key at a keyboard); else the mark of a row that leads on.
                                Row {
                                    id: rowKey
                                    anchors { verticalCenter: parent.verticalCenter; right: parent.right; rightMargin: dlg.app.px(12) }
                                    spacing: dlg.app.px(6)
                                    Text { visible: menuRow.modelData.key === "play" && menuRow.keyName !== ""; text: "Hold"; color: dlg.app.colors.muted; font.family: dlg.app.bodyFont; font.pixelSize: dlg.app.fpx(13); anchors.verticalCenter: parent.verticalCenter }
                                    KeyCap { app: dlg.app; visible: menuRow.keyName !== ""; k: menuRow.modelData.key || ""; small: true; anchors.verticalCenter: parent.verticalCenter }
                                    Icon { app: dlg.app; visible: menuRow.keyName === ""; d: dlg.app.icons.chevron; size: 18; anchors.verticalCenter: parent.verticalCenter }
                                }
                            }
                        }
                    }
                }
            }

            // The password, typed once (twice when it is a new one).
            ColumnLayout {
                visible: dlg.ask !== ""
                Layout.fillWidth: true
                Layout.minimumHeight: implicitHeight
                spacing: dlg.app.px(8)
                Label { app: dlg.app; text: dlg.ask === "new" ? "New password" : "Your SteamOS password (asked once)"; color: dlg.app.colors.muted; font.pixelSize: dlg.app.fpx(15) }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: dlg.app.px(10)
                    Focusable {
                        id: pwBox
                        app: dlg.app
                        focusable: dlg.ask !== ""
                        fid: "password"
                        firstFocus: true
                        aLabel: "Type"
                        sayRole: Accessible.EditableText
                        // (What is typed is never said, only how much: the box says it is a password by its name.)
                        sayName: (dlg.ask === "new" ? "New password" : "Your SteamOS password") + (pw.text === "" ? ", empty" : ", " + pw.text.length + " typed")
                        Layout.fillWidth: true
                        implicitHeight: dlg.app.tpx(48)
                        onActivated: dlg.app.typeIn(pw)
                        Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.ground; border.width: Math.max(1, dlg.app.px(2)); border.color: pw.activeFocus ? dlg.app.colors.accent : dlg.app.colors.line }
                        TextInput {
                            id: pw
                            Accessible.ignored: true
                            anchors { fill: parent; leftMargin: dlg.app.px(14); rightMargin: dlg.app.px(14) }
                            verticalAlignment: TextInput.AlignVCenter
                            echoMode: showBox.shown ? TextInput.Normal : TextInput.Password
                            color: dlg.app.colors.text
                            font.family: dlg.app.bodyFont
                            font.pixelSize: dlg.app.fpx(18)
                            clip: true
                            onActiveFocusChanged: if (activeFocus) dlg.app.typingBegan(pw); else dlg.app.typingEnded(pw)
                            Keys.onEscapePressed: dlg.app.keysBack()
                            Keys.onReturnPressed: (e) => dlg.entered(e, pw)
                            Keys.onEnterPressed: (e) => dlg.entered(e, pw)
                            Keys.onTabPressed: dlg.app.tabMove(1)
                            Keys.onBacktabPressed: dlg.app.tabMove(-1)
                        }
                    }
                    ActionButton {
                        id: showBox
                        property bool shown: false
                        app: dlg.app
                        focusable: dlg.ask !== ""
                        fid: "show"
                        label: shown ? "Hide" : "Show"
                        sayName: shown ? "Hide the password" : "Show the password"
                        icon: "eye"
                        onActivated: shown = !shown
                    }
                }
                Label { app: dlg.app; visible: dlg.ask === "new"; text: "Type it again"; color: dlg.app.colors.muted; font.pixelSize: dlg.app.fpx(15) }
                Focusable {
                    app: dlg.app
                    visible: dlg.ask === "new"
                    focusable: dlg.ask === "new"
                    fid: "password2"
                    aLabel: "Type"
                    sayRole: Accessible.EditableText
                    sayName: "Type the new password again" + (pw2.text === "" ? ", empty" : ", " + pw2.text.length + " typed")
                    Layout.fillWidth: true
                    implicitHeight: dlg.app.tpx(48)
                    onActivated: dlg.app.typeIn(pw2)
                    Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.ground; border.width: Math.max(1, dlg.app.px(2)); border.color: pw2.activeFocus ? dlg.app.colors.accent : dlg.app.colors.line }
                    TextInput {
                        id: pw2
                        Accessible.ignored: true
                        anchors { fill: parent; leftMargin: dlg.app.px(14); rightMargin: dlg.app.px(14) }
                        verticalAlignment: TextInput.AlignVCenter
                        echoMode: showBox.shown ? TextInput.Normal : TextInput.Password
                        color: dlg.app.colors.text
                        font.family: dlg.app.bodyFont
                        font.pixelSize: dlg.app.fpx(18)
                        clip: true
                        onActiveFocusChanged: if (activeFocus) dlg.app.typingBegan(pw2); else dlg.app.typingEnded(pw2)
                        Keys.onEscapePressed: dlg.app.keysBack()
                        Keys.onReturnPressed: (e) => dlg.entered(e, pw2)
                        Keys.onEnterPressed: (e) => dlg.entered(e, pw2)
                        Keys.onTabPressed: dlg.app.tabMove(1)
                        Keys.onBacktabPressed: dlg.app.tabMove(-1)
                    }
                }
                Label { app: dlg.app; visible: Hub.gamemode; Layout.fillWidth: true; text: "No keyboard on screen? Steam button and X opens it."; color: dlg.app.colors.muted; font.pixelSize: dlg.app.fpx(15) }
            }
            Label { app: dlg.app; visible: text !== ""; Layout.fillWidth: true; Layout.minimumHeight: implicitHeight; text: dlg.error; color: dlg.app.colors.readable(dlg.app.colors.stop, dlg.app.colors.panel); font.weight: Font.DemiBold; Accessible.role: Accessible.AlertMessage; Accessible.name: text }

            // The buttons, always in view.
            RowLayout {
                Layout.fillWidth: true
                Layout.minimumHeight: implicitHeight
                spacing: dlg.app.px(14)
                Item { Layout.fillWidth: true }
                ActionButton {
                    app: dlg.app
                    focusable: dlg.open
                    fid: "cancel"
                    firstFocus: dlg.ask === "" && (!dlg.hasConfirm || (dlg.d !== null && dlg.d.danger === true)) && !(dlg.d && dlg.d.rows)
                    label: dlg.cancelLabel
                    cap: "b"
                    onActivated: dlg.cancel()
                }
                ActionButton {
                    app: dlg.app
                    visible: dlg.hasConfirm
                    focusable: dlg.hasConfirm
                    fid: "confirm"
                    firstFocus: dlg.ask === "" && dlg.d !== null && dlg.d.danger !== true
                    primary: true
                    label: dlg.d ? (dlg.d.confirm || "") : ""
                    opacity: dlg.typed ? 1 : 0.45
                    cap: focused ? "a" : ""
                    onActivated: dlg.confirm()
                }
            }
        }
    }
    ScrollFade { app: dlg.app; view: list; color: dlg.app.colors.panel }
}
