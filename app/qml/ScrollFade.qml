import QtQuick

// Scroll cues for anything that scrolls: where there is more to see, that edge fades into the color the list sits
// on. Declare it anywhere; it puts itself on the Flickable (over the content, without scrolling with it), covers it
// and takes no input.
//
// app (required): Main.qml's window. view (required): the Flickable. color: what the view sits on.
Item {
    id: sf
    required property var app
    required property Flickable view
    property color color: app.colors.ground
    readonly property bool longer: view.visible && view.contentHeight > view.height + 1
    readonly property bool above: longer && !view.atYBeginning
    readonly property bool below: longer && !view.atYEnd
    Component.onCompleted: parent = view
    anchors.fill: parent
    z: 10
    enabled: false
    Rectangle {
        width: parent.width; height: sf.app.px(28)
        opacity: sf.above ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { enabled: sf.app.motion.on; NumberAnimation { duration: sf.app.motion.quick; easing.type: sf.app.motion.ease } }
        gradient: Gradient {
            GradientStop { position: 0; color: sf.color }
            GradientStop { position: 1; color: Qt.rgba(sf.color.r, sf.color.g, sf.color.b, 0) }
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width; height: sf.app.px(28)
        opacity: sf.below ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { enabled: sf.app.motion.on; NumberAnimation { duration: sf.app.motion.quick; easing.type: sf.app.motion.ease } }
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(sf.color.r, sf.color.g, sf.color.b, 0) }
            GradientStop { position: 1; color: sf.color }
        }
    }
    Icon { app: sf.app; visible: sf.below; d: "M6 9l6 6 6-6"; size: 18; tint: sf.app.colors.soft; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom }
}
