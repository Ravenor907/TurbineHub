import QtQuick

// A short note that something happened, inside the content area just above the footer. It goes by itself; a new
// note takes the place of the last one.
//
// app (required): Main.qml's window. show(text): shows a note.
Rectangle {
    id: toast
    required property var app
    property string text: ""
    property bool up: false
    function show(words) { text = words; up = true; stay.restart() }
    z: 60
    width: Math.min(words.implicitWidth + app.px(44), parent.width - app.px(64))
    height: words.height + app.px(22)
    radius: app.px(10)
    color: app.colors.raised
    border.width: Math.max(1, app.px(2))
    border.color: app.colors.accentLo
    opacity: up ? 1 : 0
    visible: opacity > 0
    Behavior on opacity { enabled: toast.app.motion.on; NumberAnimation { duration: toast.app.motion.standard; easing.type: toast.app.motion.ease } }
    Label {
        id: words
        app: toast.app
        anchors.centerIn: parent
        width: Math.min(implicitWidth, toast.parent.width - toast.app.px(108))
        horizontalAlignment: Text.AlignHCenter
        text: toast.text
    }
    Timer { id: stay; interval: toast.app.motion.toastStay; onTriggered: toast.up = false }
}
