import QtQuick
import QtQuick.Layouts
import TurbineHub

// Guided's first run: the five steps down the left, the step itself on the right. A step is either a few words
// with its buttons (your machine, the password), another page shown inside (the library check, Bring it over), or
// a list of things to tick (the picks). B leaves; the Home page's setup card comes back to the same step.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property string embed: model.embed || ""
    // The page shown inside this step, asked again whenever this page is.
    readonly property var inner: embed !== "" ? JSON.parse(Hub.page((page.model, embed), "")) : ({})
    readonly property var buttons: model.buttons || []
    readonly property var hints: buttons.filter(b => b.k !== "a").map(b => ({ k: b.k, label: b.label })).concat(inner.hints || [])

    function press(k) {
        const b = buttons.find(x => x.k === k && k !== "a")
        if (b) { app.doAct(b.act, b.arg, ""); return true }
        return embed !== "" && app.pageKey(inner, k)
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: page.app.px(48); anchors.rightMargin: page.app.px(48)
        anchors.topMargin: page.app.px(36); anchors.bottomMargin: page.app.px(16)
        spacing: page.app.px(36)

        // The steps.
        ColumnLayout {
            Layout.preferredWidth: page.app.px(230)
            Layout.maximumWidth: page.width * 0.28
            Layout.alignment: Qt.AlignTop
            spacing: page.app.px(8)
            Label { app: page.app; text: "Welcome"; color: page.app.colors.accentText; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(30); font.weight: Font.Bold }
            Label { app: page.app; text: page.model.count || ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
            Item { Layout.preferredHeight: page.app.px(8) }
            Repeater {
                model: page.model.rail || []
                RowLayout {
                    id: stepRow
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    Layout.bottomMargin: page.app.px(10)
                    spacing: page.app.px(12)
                    Rectangle {
                        Layout.preferredWidth: page.app.px(28); Layout.preferredHeight: page.app.px(28); radius: width / 2
                        color: stepRow.modelData.done || stepRow.modelData.current ? page.app.colors.accent : page.app.colors.raised
                        border.width: stepRow.modelData.done || stepRow.modelData.current ? 0 : Math.max(1, page.app.px(1)); border.color: page.app.colors.muted
                        Icon { app: page.app; visible: stepRow.modelData.done && !stepRow.modelData.current; anchors.centerIn: parent; d: page.app.icons.check; size: 16; tint: page.app.colors.onAccent }
                        Text { visible: !stepRow.modelData.done || stepRow.modelData.current; anchors.centerIn: parent; text: stepRow.index + 1; color: stepRow.modelData.current ? page.app.colors.onAccent : page.app.colors.muted; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(14); font.bold: true }
                    }
                    Label { app: page.app; Layout.fillWidth: true; text: stepRow.modelData.title; font.pixelSize: page.app.fpx(17); font.weight: stepRow.modelData.current ? Font.Bold : Font.Medium; color: stepRow.modelData.current ? page.app.colors.accentText : stepRow.modelData.done ? page.app.colors.text : page.app.colors.muted }
                }
            }
        }

        // The step.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: page.app.px(18)
            Rectangle {
                visible: !!page.model.found
                Layout.fillWidth: true
                implicitHeight: foundRow.implicitHeight + page.app.px(28)
                radius: page.app.px(10)
                color: page.app.colors.panel
                border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                RowLayout {
                    id: foundRow
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(18); rightMargin: page.app.px(18) }
                    spacing: page.app.px(16)
                    Icon { app: page.app; d: page.app.icons.machine; size: 30; tint: page.app.colors.accent }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: page.app.px(2)
                        Label { app: page.app; Layout.fillWidth: true; text: page.model.found ? page.model.found.title : ""; font.pixelSize: page.app.fpx(18); font.weight: Font.DemiBold }
                        Label { app: page.app; Layout.fillWidth: true; text: page.model.found ? page.model.found.line : ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
                    }
                    Chip { app: page.app; text: page.model.found ? page.model.found.chip : ""; tone: "ok" }
                }
            }
            Label { app: page.app; visible: text !== ""; Layout.fillWidth: true; text: page.model.title || ""; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(28); font.weight: Font.Bold }
            Label { app: page.app; visible: text !== ""; Layout.fillWidth: true; Layout.maximumWidth: page.app.px(780); text: page.model.text || ""; color: page.app.colors.soft; font.pixelSize: page.app.fpx(18); lineHeight: 1.25 }
            RowLayout {
                visible: (page.model.note || "") !== ""
                Layout.fillWidth: true
                spacing: page.app.px(8)
                Icon { app: page.app; d: page.app.icons.shield; size: 18 }
                Label { app: page.app; Layout.fillWidth: true; text: page.model.note || ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
            }
            Loader {
                id: innerPage
                visible: page.embed !== ""
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: page.embed === "library" ? libraryPart : page.embed === "bring" ? bringPart : null
            }
            Component { id: libraryPart; RowsPage { app: page.app; embedded: true; model: page.inner } }
            Component { id: bringPart; CardsPage { app: page.app; embedded: true; model: page.inner } }
            RowsView {
                visible: page.embed === "" && (page.model.sections || []).length > 0
                app: page.app
                Layout.fillWidth: true
                Layout.fillHeight: true
                sections: page.embed === "" ? (page.model.sections || []) : []
                flow: page.model.flow || ""
            }
            Item { Layout.fillHeight: true; visible: page.embed === "" && (page.model.sections || []).length === 0 }
            Flow {
                Layout.fillWidth: true
                spacing: page.app.px(14)
                layoutDirection: page.embed !== "" || (page.model.sections || []).length > 0 ? Qt.RightToLeft : Qt.LeftToRight
                Repeater {
                    model: page.buttons
                    ActionButton {
                        required property var modelData
                        required property int index
                        app: page.app
                        fid: "step:" + modelData.label
                        firstFocus: index === 0 && page.embed === "" && (page.model.sections || []).length === 0
                        label: modelData.label
                        cap: modelData.k
                        primary: modelData.primary
                        onActivated: page.app.doAct(modelData.act, modelData.arg, "")
                    }
                }
            }
        }
    }
}
