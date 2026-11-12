import QtQuick
import QtQuick.Shapes

// A line icon from SVG path data drawn on a 24 x 24 grid. The drawing sits inside a plain item of a fixed size, so
// rows and layouts always give it the same room whatever the path covers.
//
// app (required): Main.qml's window. d: the path data (app.icons). size: design pixels. tint: its color.
Item {
    id: icon
    required property var app
    property string d: ""
    property real size: 22
    property color tint: app.colors.muted
    implicitWidth: app.px(size)
    implicitHeight: app.px(size)
    Shape {
        width: icon.implicitWidth; height: icon.implicitHeight
        anchors.centerIn: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: icon.tint
            strokeWidth: 2
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            scale: Qt.size(icon.implicitWidth / 24, icon.implicitHeight / 24)
            PathSvg { path: icon.d }
        }
    }
}
