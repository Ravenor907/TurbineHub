import QtQuick

// A switch as drawn (the row around it takes the press): the accent when on, with the knob to the right and a
// check mark on it, so on and off differ by more than color.
//
// app (required): Main.qml's window. on: its state.
Rectangle {
    id: t
    required property var app
    property bool on: false
    implicitWidth: app.px(52)
    implicitHeight: app.px(28)
    radius: height / 2
    color: on ? app.colors.accent : app.colors.raised
    border.width: Math.max(1, app.px(2))
    border.color: on ? app.colors.accent : app.colors.muted
    Rectangle {
        width: t.height - t.app.px(8); height: width; radius: width / 2
        y: t.app.px(4)
        x: t.on ? t.width - width - t.app.px(4) : t.app.px(4)
        color: t.on ? t.app.colors.onAccent : t.app.colors.muted
        Behavior on x { enabled: t.app.motion.on; NumberAnimation { duration: t.app.motion.quick; easing.type: t.app.motion.ease } }
        Icon { app: t.app; visible: t.on; anchors.centerIn: parent; d: t.app.icons.check; size: 13; tint: t.app.colors.accent }
    }
}
