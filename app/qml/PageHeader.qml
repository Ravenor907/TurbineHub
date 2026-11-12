import QtQuick

// The top of a page: where it sits ("My machine" before the title when it was opened from there), its title in the
// title font, and a line of plain words under it.
//
// app (required): Main.qml's window. crumb, title, line: the words. small: a tighter title (inside a side page).
Column {
    id: head
    required property var app
    property string crumb: ""
    property string title: ""
    property string line: ""
    property bool small: false
    spacing: app.px(4)
    Flow {
        width: parent.width
        spacing: head.app.px(10)
        Text {
            visible: head.crumb !== ""
            text: head.crumb + "  ›"
            color: head.app.colors.muted
            font.family: head.app.bodyFont
            font.pixelSize: head.app.fpx(16)
            height: titleText.height
            verticalAlignment: Text.AlignVCenter
        }
        Text {
            id: titleText
            width: Math.min(implicitWidth, head.width)
            wrapMode: Text.WordWrap
            text: head.title
            color: head.app.colors.accentText
            font.family: head.app.titleFont
            font.pixelSize: head.app.fpx(head.small ? 26 : 30)
            font.weight: Font.Bold
        }
    }
    Label { app: head.app; visible: text !== ""; width: parent.width; text: head.line; color: head.app.colors.muted }
}
