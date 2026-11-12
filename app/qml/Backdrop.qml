import QtQuick

// The one thing that takes the mouse and the finger away from what lies under it: the dark backdrop of the dialog,
// the dialog's card (a click on the card stays on the card) and the cover shown while work runs. It takes every
// button, the wheel and the hover, and never lets a list beneath steal a drag.
//
// Why it exists: in Qt 6 a MouseArea that takes a press stops the items under it, but never their pointer handlers
// (TapHandler, HoverHandler): they still see the press and fire on release. So a backdrop alone can't stop a tap
// reaching a row beneath. Main.qml turns everything under a dialog or the busy cover off (`enabled`), and every tap
// asks app.gate() first; this takes what is left. No other file has a MouseArea (tools/qml-check.js).
//
// tapped(): a left click or a tap landed on it.
MouseArea {
    id: back
    signal tapped()
    anchors.fill: parent
    acceptedButtons: Qt.AllButtons
    hoverEnabled: true
    preventStealing: true
    onWheel: (w) => { w.accepted = true }
    onClicked: (m) => { if (m.button === Qt.LeftButton) back.tapped() }
}
