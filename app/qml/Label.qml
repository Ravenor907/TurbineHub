import QtQuick

// Plain words: the body font at 16, the text color, wrapped onto more lines when the width runs out (never cut off,
// so a larger interface size re-flows instead of losing words).
//
// app (required): Main.qml's window (colors, fonts and sizes).
Text {
    required property var app
    color: app.colors.text
    font.family: app.bodyFont
    font.pixelSize: app.fpx(16)
    wrapMode: Text.WordWrap
    verticalAlignment: Text.AlignVCenter
}
