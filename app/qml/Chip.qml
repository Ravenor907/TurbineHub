import QtQuick

// A small label for a state ("Works", "Installed", "Experimental"): tinted fill, words in the tone's color, and an
// optional icon so the meaning never rests on color alone.
//
// app (required): Main.qml's window. text: the words. tone: "ok", "warn", "stop" or "" (muted). icon: a name from
// app.icons, or "".
Rectangle {
    id: chip
    required property var app
    property string text: ""
    property string tone: ""
    property string icon: tone === "ok" ? "check" : tone === "stop" ? "close" : tone === "warn" ? "warn" : ""
    visible: text !== ""
    implicitWidth: row.implicitWidth + app.px(18)
    implicitHeight: Math.max(app.px(26), row.implicitHeight + app.px(8))
    radius: app.px(6)
    color: app.colors.toneFill(tone)
    border.width: app.colors.hc ? 1 : 0
    border.color: app.colors.tone(tone)
    Row {
        id: row
        anchors.centerIn: parent
        spacing: chip.app.px(5)
        Icon { app: chip.app; visible: chip.icon !== ""; d: chip.icon !== "" ? chip.app.icons[chip.icon] : ""; size: 15; tint: chip.app.colors.tone(chip.tone); anchors.verticalCenter: parent.verticalCenter }
        Text {
            text: chip.text
            color: chip.app.colors.tone(chip.tone)
            font.family: chip.app.bodyFont
            font.pixelSize: chip.app.fpx(14)
            font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
