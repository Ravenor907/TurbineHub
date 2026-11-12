import QtQuick
import QtQuick.Layouts

// The engine's sections of rows as one scrolling list. A section may ask for columns (its rows flow into as many
// as fit, never narrower than a readable row), and the whole view may lay its sections side by side ("columns"
// flow, the quick setup). Edges fade where there is more, and the row under the highlight is always fully in view
// (Main.qml's reveal), with its section's heading when it is on the section's first line.
//
// Side by side, each column of sections scrolls by itself (a short one never scrolls away under a long one), and a
// section's heading stays pinned at the top of its column while the section scrolls under it.
//
// app (required): Main.qml's window. sections: [{title, note, rows, columns, empty}] (`note`: a line under the
// heading that says what the section holds). flow: "" or "columns".
// ground: the color the list sits on (for the fades).
Item {
    id: rv
    required property var app
    property var sections: []
    property string flow: ""
    property color ground: app.colors.ground
    // Sections side by side: as many as fit at a readable width.
    readonly property int across: flow === "columns" ? Math.max(1, Math.min(sections.length, Math.floor((width + gap) / (app.px(300) + gap)))) : 1
    readonly property real gap: app.px(18)
    readonly property bool sideBySide: across > 1
    // Rows never stretch wider than this, so a switch stays near its words in a very wide window.
    readonly property real widest: app.px(1400)

    component Heading: Text {
        wrapMode: Text.WordWrap
        color: rv.app.colors.accentText
        font.family: rv.app.bodyFont
        font.pixelSize: rv.app.fpx(14)
        font.weight: Font.Bold
        font.letterSpacing: rv.app.px(1.2)
        font.capitalization: Font.AllUppercase
        topPadding: rv.app.px(6)
    }

    // A list that scrolls.
    component List: Flickable {
        id: fl
        clip: true
        contentWidth: width
        boundsBehavior: Flickable.StopAtBounds
        // Pinned headings cover the top of the list: Main.qml's reveal keeps the highlight below them.
        readonly property real topInset: rv.sideBySide ? rv.app.fpx(14) * 2 : 0
        Behavior on contentY { enabled: rv.app.motion.moves; NumberAnimation { duration: rv.app.motion.moveStandard; easing.type: rv.app.motion.ease } }
    }

    // One section: its heading, a line when it is empty, and its rows.
    component Section: Item {
        id: sec
        required property var modelData
        required property Flickable list
        // Where the section's own column of content starts inside the list.
        property real offset: 0
        readonly property var rows: modelData.rows || []
        readonly property int cols: Math.max(1, Math.min(modelData.columns || 1, Math.floor((width + rv.gap) / (rv.app.px(330) + rv.gap))))
        readonly property string title: modelData.title || ""
        implicitHeight: secCol.implicitHeight
        height: implicitHeight
        visible: rows.length > 0 || (modelData.empty || "") !== "" || title !== ""
        Column {
            id: secCol
            width: parent.width
            spacing: rv.app.px(10)
            Heading { id: heading; visible: sec.title !== ""; width: parent.width; text: sec.title }
            Label { app: rv.app; visible: text !== ""; width: parent.width; text: sec.modelData.note || ""; color: rv.app.colors.muted; font.pixelSize: rv.app.fpx(15) }
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
                        required property int index
                        app: rv.app
                        m: modelData
                        // The first line of a section shows with its heading.
                        revealFrom: index < sec.cols ? sec : null
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: 1
                    }
                }
            }
        }
        // The heading again, pinned at the top of the column once the real one has scrolled away, until the
        // section itself goes.
        Rectangle {
            readonly property real under: sec.list.contentY - sec.y - sec.offset
            readonly property bool cover: true
            visible: rv.sideBySide && heading.visible && under > 0
            z: 30
            y: Math.max(0, Math.min(under, sec.height - height))
            x: -rv.app.px(4)
            width: parent.width + rv.app.px(8)
            height: pinnedWords.height + rv.app.px(8)
            color: rv.ground
            Heading { x: rv.app.px(4); id: pinnedWords; width: sec.width; text: sec.title }
        }
    }

    // One list for everything.
    List {
        id: flick
        visible: !rv.sideBySide
        width: Math.min(parent.width, rv.widest)
        height: parent.height
        contentHeight: content.implicitHeight + rv.app.px(16)
        Column {
            id: content
            // Room for the highlight's ring at the edges.
            x: rv.app.px(4); y: rv.app.px(4)
            width: flick.width - rv.app.px(8)
            spacing: rv.gap
            Repeater {
                model: rv.sideBySide ? [] : rv.sections
                Section { list: flick; offset: content.y; width: content.width }
            }
        }
        ScrollFade { app: rv.app; view: flick; color: rv.ground }
    }

    // Columns side by side, each its own list.
    Row {
        visible: rv.sideBySide
        anchors.fill: parent
        spacing: rv.gap - rv.app.px(8)
        Repeater {
            model: rv.sideBySide ? rv.across : 0
            List {
                id: colList
                required property int index
                width: Math.floor((rv.width - (rv.gap - rv.app.px(8)) * (rv.across - 1)) / rv.across)
                height: rv.height
                contentHeight: colContent.implicitHeight + rv.app.px(16)
                Column {
                    id: colContent
                    x: rv.app.px(4); y: rv.app.px(4)
                    width: colList.width - rv.app.px(8)
                    spacing: rv.gap
                    Repeater {
                        // Sections go into the columns in turn.
                        model: rv.sections.filter((s, i) => i % rv.across === colList.index)
                        Section { list: colList; offset: colContent.y; width: colContent.width }
                    }
                }
                // Pinned headings mark the top themselves; a fade there would wash them out.
                ScrollFade { app: rv.app; view: colList; color: rv.ground; topEdge: false }
            }
        }
    }
}
