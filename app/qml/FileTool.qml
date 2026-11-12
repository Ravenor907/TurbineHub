import QtQuick

// One button of Files' bars: back, forward, up and refresh beside the address, and the command bar's New, Cut,
// Copy, Paste, Rename, Delete, Sort, View and Details. An icon with its word beside it; in a narrow window the
// word goes and the icon stays. One that has nothing to act on right now is faded and the highlight skips it.
//
// app (required): Main.qml's window. icon: a name from app.icons. label: its word (always what a screen reader
// hears). showLabel: the word is drawn. lit: what it stands for is on. off: nothing to act on. dense: the smaller
// size of a Desktop Mode window.
Focusable {
    id: t
    property string icon: ""
    property string label: ""
    property bool showLabel: true
    property bool lit: false
    property bool off: false
    property bool dense: false
    // Drawn as a button with an edge (the four beside the address).
    property bool boxed: false
    focusable: !off && visible
    aLabel: label
    sayName: label + (lit ? ", on" : "")
    ringRadius: app.px(8)
    ringGap: -app.px(1)
    implicitHeight: dense ? app.tpx(32) : app.tpx(44)
    implicitWidth: Math.max(showLabel ? Math.ceil(content.implicitWidth + app.px(20)) : 0, implicitHeight, app.touchFloor)
    opacity: off ? 0.4 : 1
    Rectangle {
        anchors.fill: parent
        radius: t.app.px(8)
        color: t.lit || t.boxed ? t.app.colors.raised : "transparent"
        border.width: t.lit || t.boxed ? Math.max(1, t.app.px(1)) : 0
        border.color: t.lit ? t.app.colors.accentLo : t.app.colors.line
    }
    Row {
        id: content
        anchors.centerIn: parent
        spacing: t.app.px(7)
        Icon { app: t.app; d: t.app.icons[t.icon] || ""; size: 18; tint: t.app.colors.accentText; anchors.verticalCenter: parent.verticalCenter }
        Text {
            visible: t.showLabel
            text: t.label
            color: t.app.colors.soft
            font.family: t.app.bodyFont
            font.pixelSize: t.app.fpx(15)
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
