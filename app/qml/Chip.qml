import QtQuick

// A small label for a state ("Works", "Installed", "Experimental"): tinted fill, words in the tone's color, and for
// ok, warn and stop the tone's own mark (a tick, a warning sign, a cross), so the meaning never rests on color
// alone: with the Color vision setting at "No color" the three tones are drawn in the same color and the mark and
// the word tell them apart (the build's checks fail a chip with a tone and no mark or no word). Its words always
// read on its fill (4.5 to 1).
//
// app (required): Main.qml's window. text: the words. tone: "ok", "warn", "stop" or "" (muted). icon: a name from
// app.icons to use in place of the tone's own mark.
Rectangle {
    id: chip
    required property var app
    property string text: ""
    property string tone: ""
    property string icon: tone === "ok" ? "check" : tone === "stop" ? "close" : tone === "warn" ? "warn" : ""
    // Its words' color: the tone's, made to read on the fill.
    readonly property color ink: app.colors.readable(app.colors.tone(tone), color)
    // For the build's checks: this is a state chip.
    readonly property bool stateChip: true
    readonly property bool marked: mark.visible
    visible: text !== ""
    implicitWidth: row.implicitWidth + app.px(18)
    implicitHeight: Math.max(app.px(26), row.implicitHeight + app.px(8))
    Accessible.role: Accessible.StaticText
    Accessible.name: text
    radius: app.px(6)
    color: app.colors.toneFill(tone)
    // An outline where the fill alone doesn't set it apart: high contrast, and no color.
    border.width: app.colors.hc || app.colors.noColor ? 1 : 0
    border.color: ink
    Row {
        id: row
        anchors.centerIn: parent
        spacing: chip.app.px(5)
        Icon { id: mark; app: chip.app; visible: chip.icon !== ""; d: chip.icon !== "" ? chip.app.icons[chip.icon] : ""; size: 15; tint: chip.ink; anchors.verticalCenter: parent.verticalCenter }
        Text {
            text: chip.text
            color: chip.ink
            font.family: chip.app.bodyFont
            font.pixelSize: chip.app.fpx(14)
            font.weight: Font.DemiBold
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
