import QtQuick

// The controller's pointer, drawn over everything (dialogs included): an arrow where the right stick has moved it,
// its tip on the spot R3 clicks. Turbine Hub draws it itself because Game Mode has no cursor to show. While R3,
// Start or Space is held, a small ring fills beside it (at the highlight when the pointer is hidden), so it is
// plain when the hold is through. It takes no input: the pointer's clicks reach the window as mouse events
// (Main.qml's ptrSend), and nothing here is in the way of them or of the highlight.
//
// app (required): Main.qml's window (ptr: where the pointer is and whether it shows; holding and holdPart: the
// held button and how far the hold is).
Item {
    id: layer
    required property var app
    anchors.fill: parent
    z: 10000
    enabled: false
    Accessible.ignored: true
    readonly property bool shown: app.ptr.shown === true
    // The ring only shows once the press is plainly a hold, so a click never flashes one.
    readonly property bool ringOn: app.holding !== "" && app.holdPart > 0.2
    readonly property point ringAt: {
        if (shown && app.holding === "r3") return Qt.point(app.ptr.x + app.px(26), app.ptr.y + app.px(30))
        const it = app.focusItem
        if (!it) return Qt.point(width / 2, height / 2)
        const p = it.mapToItem(null, it.width / 2, it.height / 2)
        return Qt.point(p.x, p.y)
    }

    // The arrow: the accent with a dark edge, about 28 design pixels tall (never under three quarters of that).
    Canvas {
        id: arrow
        visible: layer.shown
        x: layer.app.ptr.x - 1
        y: layer.app.ptr.y - 1
        width: layer.app.ipx(28) + 2; height: layer.app.ipx(28) + 2
        readonly property string look: "" + width + layer.app.colors.accentHi + layer.app.colors.ground
        onLookChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            ctx.translate(1, 1)
            ctx.scale((width - 2) / 24, (height - 2) / 24)
            ctx.lineJoin = "round"
            ctx.lineWidth = 1.8
            ctx.strokeStyle = layer.app.colors.ground
            ctx.fillStyle = layer.app.colors.accentHi
            ctx.beginPath()
            ctx.moveTo(0, 0); ctx.lineTo(0, 19); ctx.lineTo(5, 14.5); ctx.lineTo(8.6, 22); ctx.lineTo(12, 20.4); ctx.lineTo(8.5, 13); ctx.lineTo(15, 13)
            ctx.closePath()
            ctx.fill()
            ctx.stroke()
        }
    }
    // The ring of a held button: a dark disc, and the accent going round as the hold goes on.
    Canvas {
        id: ring
        visible: layer.ringOn
        width: layer.app.ipx(30); height: width
        x: Math.max(0, Math.min(layer.width - width, layer.ringAt.x - width / 2))
        y: Math.max(0, Math.min(layer.height - height, layer.ringAt.y - height / 2))
        readonly property real part: layer.app.holdPart
        onPartChanged: requestPaint()
        onVisibleChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            const c = width / 2, r = width / 2 - Math.max(3, width * 0.14)
            ctx.fillStyle = layer.app.colors.ground
            ctx.globalAlpha = 0.85
            ctx.beginPath(); ctx.arc(c, c, width / 2, 0, 2 * Math.PI); ctx.fill()
            ctx.globalAlpha = 1
            ctx.lineWidth = Math.max(3, width * 0.14)
            ctx.strokeStyle = layer.app.colors.line
            ctx.beginPath(); ctx.arc(c, c, r, 0, 2 * Math.PI); ctx.stroke()
            ctx.strokeStyle = layer.app.colors.accentHi
            ctx.lineCap = "round"
            ctx.beginPath(); ctx.arc(c, c, r, -Math.PI / 2, -Math.PI / 2 + 2 * Math.PI * ring.part); ctx.stroke()
        }
    }
}
