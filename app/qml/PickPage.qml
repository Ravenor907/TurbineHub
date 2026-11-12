import QtQuick
import QtQuick.Layouts
import QtQuick.Shapes
import TurbineHub

// The first screen: Guided or Veteran, two large cards. Left and Right switch, A chooses. The Guided card carries a
// emblem on a marked path; the Veteran card carries its portrait from the app's art folder.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property var hints: model.hints || []
    // Side by side while each card keeps a readable width, so Left and Right always switch.
    readonly property bool sideBySide: width >= app.px(640)

    Flickable {
        id: flick
        anchors.fill: parent
        clip: true
        contentHeight: body.height + page.app.px(64)
        boundsBehavior: Flickable.StopAtBounds
        ColumnLayout {
            id: body
            x: page.app.px(40); y: page.app.px(40)
            width: flick.width - page.app.px(80)
            height: Math.max(flick.height - page.app.px(64), implicitHeight)
            spacing: page.app.px(22)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: page.app.px(6)
                Label { app: page.app; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: page.model.title || ""; color: page.app.colors.accentText; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(34); font.weight: Font.Bold }
                Label { app: page.app; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: page.model.line || ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(18) }
            }
            GridLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.maximumWidth: page.app.px(1140)
                Layout.alignment: Qt.AlignHCenter
                columns: page.sideBySide ? 2 : 1
                columnSpacing: page.app.px(36); rowSpacing: page.app.px(18)
                Repeater {
                    model: page.model.cards || []
                    Focusable {
                        id: card
                        required property var modelData
                        required property int index
                        readonly property bool veteran: modelData.id === "veteran"
                        app: page.app
                        fid: "pick:" + modelData.id
                        firstFocus: index === 0
                        aLabel: "Choose"
                        sayName: [modelData.kicker || "", modelData.title || "", "Choose"].filter(x => x !== "").join(", ")
                        sayMore: modelData.line || ""
                        Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1
                        // The words always fit; the picture takes what is left, and the page scrolls when even that
                        // runs out (a large interface size in a small window).
                        Layout.minimumHeight: words.implicitHeight + page.app.px(110)
                        onActivated: page.app.doAct("pick", modelData.id, "")
                        Rectangle {
                            id: frame
                            anchors.fill: parent
                            radius: page.app.px(10)
                            color: card.focused ? page.app.colors.raised : page.app.colors.panel
                            border.width: Math.max(1, page.app.px(1)); border.color: page.app.colors.line
                            clip: true
                            // The picture takes what the words leave.
                            Rectangle {
                                id: art
                                anchors { top: parent.top; left: parent.left; right: parent.right; bottom: words.top; margins: Math.max(1, page.app.px(1)) }
                                color: card.veteran ? "#000000" : page.app.colors.card
                                // The portrait keeps its shape: as tall as the space, standing on its lower edge.
                                Image {
                                    visible: card.veteran
                                    source: card.veteran ? "file://" + Hub.share + "/art/veteran.jpg" : ""
                                    readonly property real shape: implicitHeight > 0 ? implicitWidth / implicitHeight : 1
                                    height: Math.min(parent.height, parent.width / shape)
                                    width: height * shape
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    anchors.bottom: parent.bottom
                                    asynchronous: false
                                    mipmap: true
                                }
                                // The emblem on a marked path, from the start dot to the flag.
                                Shape {
                                    id: drawing
                                    visible: !card.veteran
                                    readonly property real k: Math.min(parent.width / 300, parent.height / 220)
                                    width: 300 * k; height: 220 * k
                                    anchors.centerIn: parent
                                    preferredRendererType: Shape.CurveRenderer
                                    ShapePath {
                                        scale: Qt.size(drawing.k, drawing.k)
                                        strokeColor: page.app.colors.accent; strokeWidth: 4; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                                        strokeStyle: ShapePath.DashLine; dashPattern: [0.5, 3]
                                        PathSvg { path: "M30 196 C80 190 70 120 130 112 S210 70 240 40" }
                                    }
                                    ShapePath {
                                        scale: Qt.size(drawing.k, drawing.k)
                                        strokeColor: page.app.colors.text; strokeWidth: 2.5; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                                        PathSvg { path: "M240 40L240 8" }
                                    }
                                    ShapePath {
                                        scale: Qt.size(drawing.k, drawing.k)
                                        strokeColor: "transparent"; fillColor: page.app.colors.accent
                                        PathSvg { path: "M240 8L268 16L240 26Z" }
                                    }
                                    ShapePath {
                                        scale: Qt.size(drawing.k, drawing.k)
                                        strokeColor: "transparent"; fillColor: page.app.colors.accent
                                        PathSvg { path: "M30 189a7 7 0 1 1 0 14a7 7 0 1 1 0-14z" }
                                    }
                                    Image {
                                        source: "file://" + Hub.share + "/art/emblem.png"
                                        x: 70 * drawing.k; y: 52 * drawing.k
                                        width: 120 * drawing.k; height: 120 * drawing.k
                                        sourceSize: Qt.size(512, 512)
                                        asynchronous: false
                                        mipmap: true
                                    }
                                }
                            }
                            ColumnLayout {
                                id: words
                                anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: page.app.px(28); rightMargin: page.app.px(28); bottomMargin: page.app.px(20) }
                                spacing: page.app.px(6)
                                Item { Layout.preferredHeight: page.app.px(10) }
                                Label { app: page.app; Layout.fillWidth: true; text: card.modelData.kicker; color: page.app.colors.accentText; font.pixelSize: page.app.fpx(14); font.weight: Font.DemiBold; font.letterSpacing: page.app.px(2); font.capitalization: Font.AllUppercase }
                                Label { app: page.app; Layout.fillWidth: true; text: card.modelData.title; font.family: page.app.titleFont; font.pixelSize: page.app.fpx(30); font.weight: Font.Bold }
                                Label { app: page.app; Layout.fillWidth: true; text: card.modelData.line; color: page.app.colors.muted; font.pixelSize: page.app.fpx(18) }
                                Flow {
                                    Layout.fillWidth: true
                                    Layout.topMargin: page.app.px(6)
                                    spacing: page.app.px(8)
                                    Repeater {
                                        model: card.modelData.chips
                                        Rectangle {
                                            required property var modelData
                                            width: chipText.implicitWidth + page.app.px(24); height: page.app.px(32); radius: page.app.px(6)
                                            color: page.app.colors.ground
                                            Text { id: chipText; anchors.centerIn: parent; text: parent.modelData; color: page.app.colors.soft; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(15) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    ScrollFade { app: page.app; view: flick }
}
