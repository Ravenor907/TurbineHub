import QtQuick
import QtQuick.Shapes

// A line icon from SVG path data drawn on a 24 x 24 grid. The drawing sits inside a plain item of a fixed size, so
// rows and layouts always give it the same room whatever the path covers.
//
// Under Qt Quick's software drawing (pictures, machines without a GPU) a line drawing isn't clipped by a scrolling
// list, so there the icon is painted into a picture instead, which clips like any other.
//
// It is never drawn under three quarters of its design size (app.ipx), so a state's mark stays plain to see at the
// smallest interface size. A screen reader skips it: the words beside it say what it stands for.
//
// app (required): Main.qml's window. d: the path data (app.icons). size: design pixels. tint: its color.
Item {
    id: icon
    required property var app
    property string d: ""
    property real size: 22
    property color tint: app.colors.muted
    implicitWidth: app.ipx(size)
    implicitHeight: app.ipx(size)
    Accessible.ignored: true
    Loader {
        anchors.centerIn: parent
        width: icon.implicitWidth; height: icon.implicitHeight
        sourceComponent: icon.app.softwareDraw ? painted : drawn
    }
    Component {
        id: drawn
        Shape {
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
    Component {
        id: painted
        Canvas {
            id: canvas
            readonly property string look: icon.d + icon.tint + width + height
            onLookChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                ctx.clearRect(0, 0, width, height)
                if (icon.d === "") return
                ctx.scale(width / 24, height / 24)
                ctx.strokeStyle = icon.tint
                ctx.lineWidth = 2
                ctx.lineCap = "round"
                ctx.lineJoin = "round"
                ctx.path = icon.d
                ctx.stroke()
            }
        }
    }
}
