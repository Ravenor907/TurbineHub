import QtQuick
import QtQuick.Layouts

// A card in a grid: an icon in its own square, a title, the state in a few words, one line of what it is for, and
// at the bottom where it stands ("Done", "Not done yet") beside its button. The whole card takes the highlight
// and the press.
//
// app (required): Main.qml's window. m (required): the card {id, icon, title, value, line, badge, status, done,
// button, act, arg}.
Focusable {
    id: card
    required property var m
    fid: "card:" + m.id
    aLabel: m.act === "none" ? "" : (m.button || "Open")
    sayName: [m.title || "", m.value || "", m.badge || "", m.status || "", m.button || ""].filter(x => x !== "").join(", ")
    sayMore: m.line || ""
    implicitHeight: body.implicitHeight + app.px(36)
    onActivated: app.doAct(m.act, m.arg, "")
    Rectangle {
        anchors.fill: parent
        radius: card.app.px(10)
        color: card.focused ? card.app.colors.raised : card.app.colors.panel
        border.width: Math.max(1, card.app.px(1))
        border.color: card.app.colors.line
    }
    ColumnLayout {
        id: body
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: card.app.px(18) }
        spacing: card.app.px(10)
        RowLayout {
            Layout.fillWidth: true
            spacing: card.app.px(12)
            Rectangle {
                Layout.preferredWidth: card.app.px(46); Layout.preferredHeight: card.app.px(46)
                radius: card.app.px(10)
                color: card.app.colors.raised
                border.width: card.focused ? 1 : 0; border.color: card.app.colors.line
                Icon { app: card.app; anchors.centerIn: parent; d: card.app.icons[card.m.icon] || card.app.icons.info; size: 26; tint: card.app.colors.accent }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Label { app: card.app; Layout.fillWidth: true; text: card.m.title; font.family: card.app.titleFont; font.pixelSize: card.app.fpx(19); font.weight: Font.Bold }
                Label { app: card.app; Layout.fillWidth: true; visible: text !== ""; text: card.m.value || ""; color: card.app.colors.accentText; font.pixelSize: card.app.fpx(15); font.weight: Font.DemiBold }
            }
        }
        Label { app: card.app; Layout.fillWidth: true; text: card.m.line; color: card.app.colors.soft; font.pixelSize: card.app.fpx(15) }
        Chip { app: card.app; text: card.m.badge || ""; tone: "warn" }
        RowLayout {
            visible: (card.m.status || "") !== "" || (card.m.button || "") !== ""
            Layout.fillWidth: true
            spacing: card.app.px(10)
            Chip { app: card.app; visible: card.m.done === true; text: card.m.status || ""; tone: "ok" }
            Label { app: card.app; Layout.fillWidth: true; visible: card.m.done !== true; text: card.m.status || ""; color: card.app.colors.muted; font.pixelSize: card.app.fpx(15) }
            Item { Layout.fillWidth: true; visible: card.m.done === true }
            ActionButton { app: card.app; visible: (card.m.button || "") !== ""; plain: true; label: card.m.button || ""; primary: card.focused; cap: card.focused ? "a" : "" }
        }
    }
}
