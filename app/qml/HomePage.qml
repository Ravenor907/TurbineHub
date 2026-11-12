import QtQuick
import QtQuick.Layouts

// Home. Guided: what needs a look in one sentence, setup until it is finished, the checkup's cards with their
// fixes, a few readings and a tip. Veteran: the readings in a row, the quick switches, the checkup in short and
// what Turbine Hub did lately. Both scroll as one page and fold into one column when the window is narrow or the
// interface size is large.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property bool guided: model.view !== "veteran"
    readonly property bool wide: width >= app.px(980)
    readonly property var hints: model.hints || []

    // One reading: a big value over a few words.
    component Tile: Rectangle {
        id: tile
        required property var modelData
        property bool compact: false
        // Grows with its words when they take more room than the design (a small window, a large text floor).
        implicitHeight: Math.max(page.app.px(compact ? 64 : 92), tileRow.implicitHeight + page.app.px(16))
        radius: page.app.px(10)
        color: page.app.colors.panel
        border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
        RowLayout {
            id: tileRow
            anchors { fill: parent; leftMargin: page.app.px(16); rightMargin: page.app.px(12) }
            spacing: page.app.px(12)
            Icon { app: page.app; d: page.app.icons[tile.modelData.icon] || page.app.icons.info; size: tile.compact ? 22 : 26; tint: page.app.colors.accent }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Label { app: page.app; Layout.fillWidth: true; text: tile.modelData.value; font.pixelSize: page.app.fpx(tile.compact ? 20 : 26); font.weight: Font.Bold; wrapMode: Text.NoWrap; fontSizeMode: Text.HorizontalFit; minimumPixelSize: page.app.fpx(14) }
                Label { app: page.app; Layout.fillWidth: true; text: tile.modelData.label; color: page.app.colors.muted; font.pixelSize: page.app.fpx(14) }
            }
        }
    }

    // One checkup finding with its fix.
    component Finding: Focusable {
        id: finding
        required property var modelData
        property bool compact: false
        app: page.app
        fid: "item:" + modelData.id
        aLabel: modelData.button
        sayName: [modelData.look ? "Needs a look" : "", modelData.title || "", modelData.button || ""].filter(x => x !== "").join(", ")
        sayMore: modelData.line || ""
        implicitHeight: findingRow.implicitHeight + page.app.px(compact ? 20 : 30)
        onActivated: page.app.doAct("checkup", modelData.opens, "")
        Rectangle { anchors.fill: parent; radius: page.app.px(10); color: page.app.colors.panel; border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line }
        // A thing to fix is marked apart from a tip by its icon's color and the filled button.
        RowLayout {
            id: findingRow
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(18); rightMargin: page.app.px(16) }
            spacing: page.app.px(14)
            Icon { app: page.app; d: page.app.icons[finding.modelData.icon] || page.app.icons.info; size: finding.compact ? 22 : 28; tint: finding.modelData.look ? page.app.colors.warn : page.app.colors.accent }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: page.app.px(2)
                Label { app: page.app; Layout.fillWidth: true; text: finding.modelData.title; font.pixelSize: page.app.fpx(finding.compact ? 16 : 18); font.weight: Font.DemiBold }
                Label { app: page.app; Layout.fillWidth: true; text: finding.modelData.line; color: page.app.colors.soft; font.pixelSize: page.app.fpx(15) }
            }
            ActionButton { app: page.app; plain: true; label: finding.modelData.button; primary: finding.modelData.look; cap: finding.focused ? "a" : "" }
        }
    }

    component Heading: Text {
        color: page.app.colors.accentText
        font.family: page.app.bodyFont
        font.pixelSize: page.app.fpx(14)
        font.weight: Font.Bold
        font.letterSpacing: page.app.px(1.2)
        font.capitalization: Font.AllUppercase
    }

    Flickable {
        id: flick
        anchors.fill: parent
        anchors.leftMargin: page.app.px(40); anchors.rightMargin: page.app.px(40)
        clip: true
        contentHeight: (page.guided ? guidedBody.implicitHeight : veteranBody.implicitHeight) + page.app.px(40)
        boundsBehavior: Flickable.StopAtBounds
        Behavior on contentY { enabled: page.app.motion.moves; NumberAnimation { duration: page.app.motion.moveStandard; easing.type: page.app.motion.ease } }

        // ------------------------------------------------------------ Guided
        GridLayout {
            id: guidedBody
            visible: page.guided
            y: page.app.px(24)
            x: page.app.px(4)
            width: flick.width - page.app.px(8)
            columns: page.wide ? 2 : 1
            columnSpacing: page.app.px(28)
            rowSpacing: page.app.px(18)
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                spacing: page.app.px(14)
                Label { app: page.app; Layout.fillWidth: true; text: page.model.headline || ""; color: page.app.colors.accentText; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(30); font.weight: Font.Bold }
                Label { app: page.app; Layout.fillWidth: true; visible: text !== ""; text: page.model.sub || ""; color: page.app.colors.muted; Layout.topMargin: -page.app.px(10) }

                // Setup, until every step is done or skipped.
                Focusable {
                    id: setupCard
                    readonly property var s: page.model.setup || ({})
                    app: page.app
                    visible: s.show === true
                    fid: "setup"
                    aLabel: "Continue"
                    sayName: "Finish setting up, " + (s.done || 0) + " of " + (s.total || 0) + " done, Continue"
                    sayMore: "Next: " + (s.next || []).join(", ")
                    Layout.fillWidth: true
                    implicitHeight: setupRow.implicitHeight + page.app.px(28)
                    onActivated: page.app.open("setup")
                    Rectangle { anchors.fill: parent; radius: page.app.px(10); color: page.app.colors.raised; border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.accentLo }
                    RowLayout {
                        id: setupRow
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(18); rightMargin: page.app.px(16) }
                        spacing: page.app.px(14)
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: page.app.px(8)
                            RowLayout {
                                spacing: page.app.px(12)
                                Label { app: page.app; text: "Finish setting up"; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(19); font.weight: Font.Bold }
                                Label { app: page.app; Layout.fillWidth: true; text: (setupCard.s.done || 0) + " of " + (setupCard.s.total || 0) + " done"; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
                            }
                            // The steps as a bar: the done part filled.
                            Rectangle {
                                Layout.fillWidth: true; Layout.preferredHeight: page.app.px(8); radius: height / 2
                                color: page.app.colors.ground
                                Rectangle { width: parent.width * (setupCard.s.total ? setupCard.s.done / setupCard.s.total : 0); height: parent.height; radius: height / 2; color: page.app.colors.accent }
                            }
                            Label { app: page.app; Layout.fillWidth: true; text: "Next: " + (setupCard.s.next || []).join(", "); color: page.app.colors.soft; font.pixelSize: page.app.fpx(15) }
                        }
                        ActionButton { app: page.app; plain: true; label: "Continue"; cap: setupCard.focused ? "a" : "" }
                    }
                }

                Heading { text: "Checkup"; visible: (page.model.items || []).length > 0 }
                Repeater {
                    model: page.guided ? (page.model.items || []) : []
                    Finding { Layout.fillWidth: true }
                }
                RowLayout {
                    visible: (page.model.more || "") !== "" || page.model.all === true
                    Layout.fillWidth: true
                    spacing: page.app.px(10)
                    Icon { app: page.app; d: page.app.icons.check; size: 18; tint: page.app.colors.ok }
                    Label { app: page.app; Layout.fillWidth: true; text: page.model.all ? "That's everything." : (page.model.more || ""); color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
                    ActionButton { app: page.app; fid: "showall"; label: page.model.all ? "Show fewer" : "Show all"; onActivated: page.app.doAct("home-all", "", "") }
                }
            }
            ColumnLayout {
                Layout.preferredWidth: page.wide ? page.app.px(380) : -1
                Layout.maximumWidth: page.wide ? page.app.px(380) : -1
                Layout.fillWidth: !page.wide
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: page.wide ? page.app.px(6) : 0
                spacing: page.app.px(12)
                GridLayout {
                    Layout.fillWidth: true
                    columns: 2
                    columnSpacing: page.app.px(12); rowSpacing: page.app.px(12)
                    Repeater {
                        model: page.guided ? (page.model.tiles || []) : []
                        Tile { Layout.fillWidth: true; Layout.preferredWidth: 1 }
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: tipCol.implicitHeight + page.app.px(28)
                    radius: page.app.px(10)
                    color: "transparent"
                    border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                    ColumnLayout {
                        id: tipCol
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: page.app.px(16) }
                        spacing: page.app.px(6)
                        RowLayout {
                            spacing: page.app.px(8)
                            Icon { app: page.app; d: page.app.icons.help; size: 20; tint: page.app.colors.accent }
                            Label { app: page.app; Layout.fillWidth: true; text: page.model.tip ? page.model.tip.title : ""; font.weight: Font.DemiBold }
                        }
                        Label { app: page.app; Layout.fillWidth: true; text: page.model.tip ? page.model.tip.line : ""; color: page.app.colors.soft; font.pixelSize: page.app.fpx(15) }
                    }
                }
            }
        }

        // ------------------------------------------------------------ Veteran
        ColumnLayout {
            id: veteranBody
            visible: !page.guided
            y: page.app.px(20)
            x: page.app.px(4)
            width: flick.width - page.app.px(8)
            spacing: page.app.px(14)
            GridLayout {
                Layout.fillWidth: true
                columns: Math.max(2, Math.min((page.model.tiles || []).length, Math.floor(veteranBody.width / page.app.px(190))))
                columnSpacing: page.app.px(12); rowSpacing: page.app.px(12)
                Repeater {
                    model: page.guided ? [] : (page.model.tiles || [])
                    Tile { compact: true; Layout.fillWidth: true; Layout.preferredWidth: 1 }
                }
            }
            Heading { text: "Quick switches" }
            GridLayout {
                Layout.fillWidth: true
                columns: Math.max(1, Math.min(4, Math.floor(veteranBody.width / page.app.px(250))))
                columnSpacing: page.app.px(12); rowSpacing: page.app.px(12)
                Repeater {
                    model: page.guided ? [] : (page.model.switches || [])
                    Focusable {
                        id: sw
                        required property var modelData
                        app: page.app
                        fid: "switch:" + modelData.id
                        aLabel: modelData.toggle ? (modelData.on ? "Turn off" : "Turn on") : "Open"
                        sayRole: modelData.toggle ? Accessible.CheckBox : Accessible.Button
                        sayName: [modelData.title || "", modelData.toggle ? (modelData.on ? "on" : "off") : "", modelData.value || ""].filter(x => x !== "").join(", ")
                        Accessible.checkable: modelData.toggle === true
                        Accessible.checked: modelData.toggle === true && modelData.on === true
                        Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.fillHeight: true
                        implicitHeight: Math.max(page.app.px(76), swRow.implicitHeight + page.app.px(24))
                        onActivated: modelData.toggle ? page.app.doAct(modelData.id, "", "") : page.app.doAct("open", modelData.id, "")
                        Rectangle { anchors.fill: parent; radius: page.app.px(10); color: page.app.colors.panel; border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line }
                        RowLayout {
                            id: swRow
                            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(14); rightMargin: page.app.px(12) }
                            spacing: page.app.px(10)
                            Icon { app: page.app; d: page.app.icons[sw.modelData.icon] || page.app.icons.info; size: 22; tint: page.app.colors.accent }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                Label { app: page.app; Layout.fillWidth: true; text: sw.modelData.title; font.weight: Font.DemiBold }
                                Label { app: page.app; Layout.fillWidth: true; text: sw.modelData.value; color: sw.modelData.toggle ? page.app.colors.muted : page.app.colors.accentText; font.pixelSize: page.app.fpx(15) }
                            }
                            Toggle { app: page.app; visible: sw.modelData.toggle === true; on: sw.modelData.on === true }
                            Icon { app: page.app; visible: sw.modelData.toggle !== true; d: page.app.icons.chevron; size: 18 }
                        }
                    }
                }
            }
            GridLayout {
                Layout.fillWidth: true
                columns: page.wide ? 2 : 1
                columnSpacing: page.app.px(24); rowSpacing: page.app.px(14)
                ColumnLayout {
                    Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.alignment: Qt.AlignTop
                    spacing: page.app.px(10)
                    Heading { text: "Checkup" }
                    Repeater {
                        model: page.guided ? [] : (page.model.items || [])
                        Finding { compact: true; Layout.fillWidth: true }
                    }
                    Label { app: page.app; visible: text !== ""; Layout.fillWidth: true; text: page.model.fine || ""; color: page.app.colors.muted }
                }
                ColumnLayout {
                    Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.alignment: Qt.AlignTop
                    spacing: page.app.px(8)
                    Heading { text: "Activity" }
                    Repeater {
                        model: page.guided ? [] : (page.model.activity || [])
                        RowLayout {
                            id: line
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: page.app.px(12)
                            Label { app: page.app; Layout.preferredWidth: page.app.px(60); text: line.modelData.when; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15); Layout.alignment: Qt.AlignTop }
                            Label { app: page.app; Layout.fillWidth: true; text: line.modelData.text; color: page.app.colors.soft; font.pixelSize: page.app.fpx(15) }
                        }
                    }
                    Label { app: page.app; visible: (page.model.activity || []).length === 0; Layout.fillWidth: true; text: "Nothing yet. What Turbine Hub does shows up here."; color: page.app.colors.muted }
                }
            }
        }
    }
    ScrollFade { app: page.app; view: flick }
}
