import QtQuick
import QtQuick.Layouts

// A card in a grid: an icon in its own square, a title, the state in a few words, one line of what it is for, and
// at the bottom where it stands ("Done", "Not done yet") beside its button, or the name of its switch beside the
// switch (My machine's Overview: Game Boost, Lights, Stop at 80%). The whole card takes the highlight and the
// press: A presses the button or turns the switch.
//
// app (required): Main.qml's window. m (required): the card {id, icon, title, value, line, badge, tone, status,
// done, button, toggle, on, act, arg}.
Focusable {
    id: card
    required property var m
    fid: "card:" + m.id
    offers: m.offers || []
    optionsTitle: m.title || ""
    readonly property bool toggle: m.toggle === true
    optionLabel: toggle ? (m.status || m.title || "") + ": turn " + (m.on ? "off" : "on") : m.act === "open" && aLabel === "Open" ? (String(m.arg || "").startsWith("module:") ? "Go to " + (m.title || "it") : "Open its page") : ""
    aLabel: m.act === "none" ? "" : toggle ? (m.on ? "Turn off" : "Turn on") : (m.button || "Open")
    sayRole: toggle ? Accessible.CheckBox : Accessible.Button
    sayName: [m.title || "", m.value || "", m.badge || "", toggle ? (m.status || "") + (m.on ? " on" : " off") : (m.status || ""), m.button || ""].filter(x => x !== "").join(", ")
    sayMore: m.line || ""
    Accessible.checkable: toggle
    Accessible.checked: toggle && m.on === true
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
        Chip { app: card.app; text: card.m.badge || ""; tone: card.m.tone !== undefined ? card.m.tone : "warn" }
        RowLayout {
            visible: (card.m.status || "") !== "" || (card.m.button || "") !== ""
            Layout.fillWidth: true
            spacing: card.app.px(10)
            Chip { app: card.app; visible: card.m.done === true; text: card.m.status || ""; tone: "ok" }
            Label { app: card.app; Layout.fillWidth: true; visible: card.m.done !== true; text: card.m.status || ""; color: card.toggle ? card.app.colors.text : card.app.colors.muted; font.pixelSize: card.app.fpx(card.toggle ? 16 : 15); font.weight: card.toggle ? Font.DemiBold : Font.Normal }
            Item { Layout.fillWidth: true; visible: card.m.done === true }
            ActionButton { app: card.app; visible: (card.m.button || "") !== ""; plain: true; label: card.m.button || ""; primary: card.focused; cap: card.focused ? "a" : "" }
            Toggle { app: card.app; visible: card.toggle; on: card.m.on === true }
        }
    }
}
