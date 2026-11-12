import QtQuick

// Words that are cut at the end where they don't fit: a file's name, its date, type and size in Files' list. Every
// explorer cuts these; the whole name is what a screen reader hears and what the details pane and Properties show.
// The build's checks let such words be cut (`mayCut`), and only such.
//
// app (required): Main.qml's window.
Text {
    required property var app
    property bool mayCut: true
    elide: Text.ElideRight
    color: app.colors.soft
    font.family: app.bodyFont
    font.pixelSize: app.fpx(14)
    verticalAlignment: Text.AlignVCenter
}
