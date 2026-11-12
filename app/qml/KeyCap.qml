import QtQuick

// One button's cap ("A", "LB", "Enter"). The controller's face buttons are filled circles in the pad's own colors;
// everything else, and every key of a keyboard, is an outlined cap with its name.
//
// app (required): Main.qml's window. k: the button ("a", "lb", "view"). small: the size used inside a button.
Rectangle {
    id: cap
    required property var app
    property string k: ""
    property bool small: false
    // Drawn on an accent fill (inside the main button): its outline and name take the fill's own text color.
    property bool onFill: false
    readonly property string words: app.glyph(k)
    readonly property bool face: !app.keyboard && ["a", "b", "x", "y"].indexOf(k) >= 0
    // As big as the design says, and never smaller than its letter needs (the text floor keeps letters readable
    // at the smallest interface size).
    readonly property real side: Math.max(app.px(small ? 24 : 28), Math.ceil(capText.implicitHeight) + 2)
    implicitWidth: Math.ceil(face ? side : Math.max(side, capText.implicitWidth + app.px(12)))
    implicitHeight: side
    radius: face ? side / 2 : app.px(6)
    color: face ? app.colors.pad[k] : "transparent"
    border.width: face ? 0 : Math.max(1, app.px(2))
    border.color: onFill ? app.colors.accentInk : app.colors.muted
    Text {
        id: capText
        anchors.centerIn: parent
        text: cap.words
        color: cap.face ? cap.app.colors.padInk : cap.onFill ? cap.app.colors.accentInk : cap.app.colors.text
        // A lone letter on a key (Q, E, X) in a plainer face: Barlow's small bold Q reads like a 0.
        font.family: !cap.face && cap.words.length === 1 ? "Noto Sans" : cap.app.bodyFont
        font.pixelSize: cap.app.fpx(cap.face ? (cap.small ? 14 : 15) : 13)
        font.bold: true
    }
}
