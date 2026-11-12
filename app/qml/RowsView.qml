import QtQuick
import QtQuick.Layouts

// The engine's sections of rows as one scrolling list. A section may ask for columns (its rows flow into as many
// as fit, never narrower than a readable row), and the whole view may lay its sections side by side ("columns"
// flow, the quick setup). Edges fade where there is more, and the row under the highlight is always fully in view
// (Main.qml's reveal).
//
// app (required): Main.qml's window. sections: [{title, rows, columns, empty}]. flow: "" or "columns".
// ground: the color the list sits on (for the fades).
Item {
    id: rv
    required property var app
    property var sections: []
    property string flow: ""
    property color ground: app.colors.ground
    // Sections side by side: as many as fit at a readable width.
    readonly property int across: flow === "columns" ? Math.max(1, Math.min(sections.length, Math.floor((width + gap) / (app.px(270) + gap)))) : 1
    readonly property real gap: app.px(18)

    Flickable {
        id: flick
        anchors.fill: parent
        clip: true
        layer.enabled: rv.app.softwareDraw
        // A layer has no see-through parts under software drawing, so the list paints what it sits on first.
        Rectangle { parent: flick; anchors.fill: parent; z: -1; visible: rv.app.softwareDraw; color: rv.ground }
        contentWidth: width
        contentHeight: content.implicitHeight + rv.app.px(16)
        boundsBehavior: Flickable.StopAtBounds
        Behavior on contentY { enabled: rv.app.motion.on; NumberAnimation { duration: rv.app.motion.standard; easing.type: rv.app.motion.ease } }
        Flow {
            id: content
            // Room for the highlight's ring at the edges.
            x: rv.app.px(4); y: rv.app.px(4)
            width: flick.width - rv.app.px(8)
            spacing: rv.gap
            Repeater {
                model: rv.sections
                Column {
                    id: sec
                    required property var modelData
                    readonly property var rows: modelData.rows || []
                    readonly property int cols: Math.max(1, Math.min(modelData.columns || 1, Math.floor((width + rv.gap) / (rv.app.px(330) + rv.gap))))
                    width: Math.floor((content.width - rv.gap * (rv.across - 1)) / rv.across)
                    spacing: rv.app.px(10)
                    visible: rows.length > 0 || (modelData.empty || "") !== "" || (modelData.title || "") !== ""
                    Text {
                        visible: (sec.modelData.title || "") !== ""
                        width: parent.width
                        wrapMode: Text.WordWrap
                        text: sec.modelData.title || ""
                        color: rv.app.colors.accentText
                        font.family: rv.app.bodyFont
                        font.pixelSize: rv.app.fpx(14)
                        font.weight: Font.Bold
                        font.letterSpacing: rv.app.px(1.2)
                        font.capitalization: Font.AllUppercase
                        topPadding: rv.app.px(6)
                    }
                    Label { app: rv.app; visible: text !== ""; width: parent.width; text: sec.modelData.empty || ""; color: rv.app.colors.muted; topPadding: rv.app.px(8); bottomPadding: rv.app.px(8) }
                    // Rows side by side share their line's height.
                    GridLayout {
                        width: parent.width
                        columns: sec.cols
                        columnSpacing: rv.app.px(10)
                        rowSpacing: rv.app.px(10)
                        Repeater {
                            model: sec.rows
                            RowItem {
                                required property var modelData
                                app: rv.app
                                m: modelData
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.preferredWidth: 1
                            }
                        }
                    }
                }
            }
        }
    }
    ScrollFade { app: rv.app; view: flick; color: rv.ground }
}
