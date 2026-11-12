import QtQuick
import QtQuick.Layouts
import TurbineHub

// The one dialog. Every question Turbine Hub asks looks like this: a title, what will happen in plain words, the
// exact changes as a list, the password box when one is needed, and the buttons pinned at the bottom with the
// controller's button inside each. It sizes to what it holds and scrolls the list when that is long. The same
// card shows the menu (a list of places to go) and plain explanations (one Close button).
//
// A dark backdrop covers the page behind it, and nothing behind it takes a press while it is open.
//
// app (required): Main.qml's window. show(d): opens it with the engine's dialog. close(): closes it.
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
    z: 50
    visible: open

    function show(model) {
        d = model
        error = ""
        pw.text = ""; pw2.text = ""
        behind = app.lastFid
        app.lastFid = ""
        app.focusItem = null
        Qt.callLater(function () {
            app.restoreFocus()
            if (dlg.ask !== "" && app.keyboard) pw.forceActiveFocus()
        })
    }
    function close() {
        d = null
        app.keysBack()
        app.lastFid = behind
        app.focusItem = null
        Qt.callLater(app.restoreFocus)
    }
    function cancel() { Hub.cancel(); close() }
    function confirm() { app.confirmDialog(pw.text, pw2.text) }
    function press(k) {
        if (k === "b") { cancel(); return true }
        // The tabs, Help and the menu wait until the dialog is answered.
        return ["lb", "rb", "lt", "rt", "view", "start", "x", "y"].indexOf(k) >= 0
    }

    Rectangle {
        anchors.fill: parent
        color: dlg.app.colors.scrim
        MouseArea { anchors.fill: parent; hoverEnabled: true; onWheel: (w) => w.accepted = true; onClicked: dlg.cancel() }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(dlg.app.px(dlg.d && dlg.d.rows ? 460 : 680), parent.width - dlg.app.px(48))
        height: Math.min(body.implicitHeight + dlg.app.px(52), parent.height - dlg.app.px(40))
        radius: dlg.app.px(12)
        // It grows a little into place as it opens.
        scale: dlg.open ? 1 : dlg.app.motion.popScale
        Behavior on scale { enabled: dlg.app.motion.on; NumberAnimation { duration: dlg.app.motion.standard; easing.type: dlg.app.motion.ease } }
        color: dlg.app.colors.panel
        border.width: Math.max(1, dlg.app.px(2))
        border.color: dlg.app.colors.accentLo
        // Clicks on the card stay on the card.
        MouseArea { anchors.fill: parent }

        ColumnLayout {
            id: body
            anchors.fill: parent
            anchors.margins: dlg.app.px(26)
            spacing: dlg.app.px(14)

            Label { app: dlg.app; Layout.fillWidth: true; text: dlg.d ? dlg.d.title : ""; font.family: dlg.app.titleFont; font.pixelSize: dlg.app.fpx(24); font.weight: Font.Bold }
            Label { app: dlg.app; visible: text !== ""; Layout.fillWidth: true; text: dlg.d ? (dlg.d.text || "") : ""; color: dlg.app.colors.soft; font.pixelSize: dlg.app.fpx(17); lineHeight: 1.2 }

            // The exact changes, or the places the menu leads to.
            Flickable {
                id: list
                visible: dlg.d !== null && ((dlg.d.items || []).length > 0 || (dlg.d.rows || []).length > 0)
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: listCol.implicitHeight
                Layout.minimumHeight: Math.min(listCol.implicitHeight, dlg.app.px(90))
                clip: true
                layer.enabled: dlg.app.softwareDraw
                // A layer has no see-through parts under software drawing, so the list paints what it sits on first.
                Rectangle { parent: list; anchors.fill: parent; z: -1; visible: dlg.app.softwareDraw; color: dlg.app.colors.panel }
                contentHeight: listCol.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: listCol
                    width: list.width
                    spacing: dlg.app.px(8)
                    Repeater {
                        model: dlg.d ? (dlg.d.items || []) : []
                        RowLayout {
                            id: item
                            required property var modelData
                            width: listCol.width
                            spacing: dlg.app.px(10)
                            Icon { app: dlg.app; d: dlg.app.icons.check; size: 18; tint: dlg.app.colors.accent; Layout.alignment: Qt.AlignTop; Layout.topMargin: dlg.app.px(3) }
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
                            aLabel: "Open"
                            width: listCol.width - dlg.app.px(8)
                            x: dlg.app.px(4)
                            height: dlg.app.tpx(48)
                            ringRadius: dlg.app.px(8)
                            ringGap: -dlg.app.px(1)
                            onActivated: { const m = modelData; dlg.close(); dlg.app.doAct(m.act, m.arg, "") }
                            Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.raised }
                            Label { app: dlg.app; anchors.verticalCenter: parent.verticalCenter; x: dlg.app.px(16); text: menuRow.modelData.title; font.weight: Font.DemiBold }
                            Icon { app: dlg.app; anchors.verticalCenter: parent.verticalCenter; anchors.right: parent.right; anchors.rightMargin: dlg.app.px(12); d: dlg.app.icons.chevron; size: 18 }
                        }
                    }
                }
            }

            // The password, typed once (twice when it is a new one).
            ColumnLayout {
                visible: dlg.ask !== ""
                Layout.fillWidth: true
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
                        Layout.fillWidth: true
                        implicitHeight: dlg.app.tpx(48)
                        onActivated: pw.forceActiveFocus()
                        Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.ground; border.width: Math.max(1, dlg.app.px(2)); border.color: pw.activeFocus ? dlg.app.colors.accent : dlg.app.colors.line }
                        TextInput {
                            id: pw
                            anchors { fill: parent; leftMargin: dlg.app.px(14); rightMargin: dlg.app.px(14) }
                            verticalAlignment: TextInput.AlignVCenter
                            echoMode: showBox.shown ? TextInput.Normal : TextInput.Password
                            color: dlg.app.colors.text
                            font.family: dlg.app.bodyFont
                            font.pixelSize: dlg.app.fpx(18)
                            clip: true
                            Keys.onEscapePressed: dlg.cancel()
                            Keys.onReturnPressed: dlg.ask === "new" ? pw2.forceActiveFocus() : dlg.confirm()
                            Keys.onEnterPressed: dlg.ask === "new" ? pw2.forceActiveFocus() : dlg.confirm()
                        }
                    }
                    ActionButton {
                        id: showBox
                        property bool shown: false
                        app: dlg.app
                        focusable: dlg.ask !== ""
                        fid: "show"
                        label: shown ? "Hide" : "Show"
                        icon: "search"
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
                    Layout.fillWidth: true
                    implicitHeight: dlg.app.tpx(48)
                    onActivated: pw2.forceActiveFocus()
                    Rectangle { anchors.fill: parent; radius: dlg.app.px(8); color: dlg.app.colors.ground; border.width: Math.max(1, dlg.app.px(2)); border.color: pw2.activeFocus ? dlg.app.colors.accent : dlg.app.colors.line }
                    TextInput {
                        id: pw2
                        anchors { fill: parent; leftMargin: dlg.app.px(14); rightMargin: dlg.app.px(14) }
                        verticalAlignment: TextInput.AlignVCenter
                        echoMode: showBox.shown ? TextInput.Normal : TextInput.Password
                        color: dlg.app.colors.text
                        font.family: dlg.app.bodyFont
                        font.pixelSize: dlg.app.fpx(18)
                        clip: true
                        Keys.onEscapePressed: dlg.cancel()
                        Keys.onReturnPressed: dlg.confirm()
                        Keys.onEnterPressed: dlg.confirm()
                    }
                }
                Label { app: dlg.app; visible: Hub.gamemode; Layout.fillWidth: true; text: "Steam button and X opens the keyboard."; color: dlg.app.colors.muted; font.pixelSize: dlg.app.fpx(15) }
            }
            Label { app: dlg.app; visible: text !== ""; Layout.fillWidth: true; text: dlg.error; color: dlg.app.colors.stop; font.weight: Font.DemiBold }

            // The buttons, always in view.
            RowLayout {
                Layout.fillWidth: true
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
                    cap: focused ? "a" : ""
                    onActivated: dlg.confirm()
                }
            }
        }
    }
    ScrollFade { app: dlg.app; view: list; color: dlg.app.colors.panel }
}
