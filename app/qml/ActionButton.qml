import QtQuick

// One button look everywhere: the primary one filled with the accent, the others raised with a thin line, the
// highlight a ring outside (Focusable). A cap in front of the words says which button presses it.
//
// app (required): Main.qml's window. label: the words. cap: the key ("a", "b", "x"; "" for none). primary: the
// main action of the screen or dialog. icon: a name from app.icons, or "". plain: drawn, but not something the
// highlight lands on (a button inside a row that takes the press itself).
Focusable {
    id: ab
    property string label: ""
    property string cap: ""
    property bool primary: false
    property string icon: ""
    property bool plain: false
    focusable: !plain
    aLabel: label
    ringRadius: app.px(8)
    implicitWidth: Math.ceil(content.implicitWidth + app.px(32))
    implicitHeight: app.tpx(44)
    Rectangle {
        anchors.fill: parent
        radius: ab.app.px(8)
        color: ab.primary ? ab.app.colors.accent : ab.app.colors.raised
        border.width: Math.max(1, ab.app.px(2))
        border.color: ab.primary ? ab.app.colors.accent : ab.app.colors.line
    }
    Row {
        id: content
        anchors.centerIn: parent
        spacing: ab.app.px(9)
        KeyCap { app: ab.app; visible: ab.cap !== ""; k: ab.cap; small: true; anchors.verticalCenter: parent.verticalCenter }
        Icon { app: ab.app; visible: ab.icon !== ""; d: ab.icon !== "" ? ab.app.icons[ab.icon] : ""; size: 18; tint: words.color; anchors.verticalCenter: parent.verticalCenter }
        Text {
            id: words
            text: ab.label
            color: ab.primary ? ab.app.colors.onAccent : ab.app.colors.text
            font.family: ab.app.bodyFont
            font.pixelSize: ab.app.fpx(16)
            font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
