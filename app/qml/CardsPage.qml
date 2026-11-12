import QtQuick
import QtQuick.Layouts

// A page of cards: My machine (one card per thing this machine has) and Bring it over (its six steps). The grid
// takes as many columns as the page asks for and fit at a readable width; cards in a line share its height.
//
// app (required): Main.qml's window. model: the page. embedded: drawn inside the first-run steps.
Item {
    id: page
    required property var app
    property var model: app.model
    property bool embedded: false
    readonly property var hints: model.hints || []
    readonly property var cards: model.cards || []
    readonly property real gap: app.px(16)
    readonly property int cols: Math.max(1, Math.min(model.columns || 3, Math.floor((grid.width + gap) / (app.px(250) + gap))))
    function press(k) { return app.pageKey(model, k) }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: page.embedded ? 0 : page.app.px(40)
        anchors.rightMargin: page.embedded ? 0 : page.app.px(40)
        anchors.topMargin: page.embedded ? 0 : page.app.px(24)
        anchors.bottomMargin: page.app.px(8)
        spacing: page.app.px(16)
        RowLayout {
            Layout.fillWidth: true
            spacing: page.app.px(16)
            PageHeader { app: page.app; Layout.fillWidth: true; title: page.model.title || ""; line: page.model.line || ""; small: page.embedded }
            Label { app: page.app; visible: text !== ""; text: page.model.note || ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15); Layout.maximumWidth: page.width * 0.3; horizontalAlignment: Text.AlignRight; Layout.alignment: Qt.AlignBottom }
        }
        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            layer.enabled: page.app.softwareDraw
            // A layer has no see-through parts under software drawing, so the list paints what it sits on first.
            Rectangle { parent: flick; anchors.fill: parent; z: -1; visible: page.app.softwareDraw; color: page.app.colors.ground }
            contentHeight: grid.implicitHeight + page.app.px(12)
            boundsBehavior: Flickable.StopAtBounds
            Behavior on contentY { enabled: page.app.motion.on; NumberAnimation { duration: page.app.motion.standard; easing.type: page.app.motion.ease } }
            GridLayout {
                id: grid
                // Room for the highlight's ring at the edges.
                x: page.app.px(4); y: page.app.px(4)
                width: flick.width - page.app.px(8)
                columns: page.cols
                columnSpacing: page.gap
                rowSpacing: page.gap
                Repeater {
                    model: page.cards
                    CardTile {
                        required property var modelData
                        app: page.app
                        m: modelData
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: 1
                    }
                }
            }
        }
    }
    ScrollFade { app: page.app; view: flick }
}
