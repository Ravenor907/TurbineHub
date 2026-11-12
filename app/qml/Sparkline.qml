import QtQuick

// One line over time, drawn small: the battery's charge on its tile on Home, the charge and the temperature of
// the last day on My machine, Battery. The points run from the oldest at the left to the newest at the right on a
// scale from `min` to `max`; a point that is null (nothing was read: the machine slept) breaks the line, so a gap
// is shown as a gap. The area under the line is tinted and the newest point has a dot. It is a picture only: the
// words beside it say what it shows, and a screen reader skips it.
//
// app (required): Main.qml's window. points: numbers and nulls. min, max: the scale. framed: a line along the top
// and the bottom of the scale and one in the middle (the chart on a page; the tile's line has none).
Canvas {
    id: sp
    required property var app
    property var points: []
    property real min: 0
    property real max: 100
    property bool framed: false
    readonly property color ink: app.colors.accent
    readonly property color rule: app.colors.line
    Accessible.ignored: true
    onPointsChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onInkChanged: requestPaint()
    onRuleChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        const n = points.length
        const pad = app.px(4)
        const w = width - pad * 2, h = height - pad * 2
        if (w <= 0 || h <= 0) return
        if (framed) {
            ctx.strokeStyle = rule
            ctx.lineWidth = 1
            for (const part of [0, 0.5, 1]) {
                const y = Math.round(pad + h * part) + 0.5
                ctx.beginPath(); ctx.moveTo(pad, y); ctx.lineTo(pad + w, y); ctx.stroke()
            }
        }
        if (n < 2 || max <= min) return
        const at = (i) => Qt.point(pad + w * i / (n - 1), pad + h * (1 - Math.max(0, Math.min(1, (points[i] - min) / (max - min)))))
        const has = (i) => points[i] !== null && points[i] !== undefined
        // Each unbroken stretch: the tint under it, then the line itself.
        let last = -1
        for (let i = 0; i <= n; i++) {
            if (i < n && has(i)) continue
            const from = last + 1, to = i - 1
            last = i
            if (to < from) continue
            if (to > from) {
                ctx.globalAlpha = 0.16
                ctx.fillStyle = ink
                ctx.beginPath()
                ctx.moveTo(at(from).x, pad + h)
                for (let k = from; k <= to; k++) ctx.lineTo(at(k).x, at(k).y)
                ctx.lineTo(at(to).x, pad + h)
                ctx.closePath(); ctx.fill()
                ctx.globalAlpha = 1
            }
            ctx.strokeStyle = ink
            ctx.lineWidth = Math.max(2, app.px(2))
            ctx.lineJoin = "round"; ctx.lineCap = "round"
            ctx.beginPath()
            ctx.moveTo(at(from).x, at(from).y)
            // (A lone reading between two gaps is a short dash, so it shows.)
            if (to === from) ctx.lineTo(at(from).x + 1, at(from).y)
            for (let k = from + 1; k <= to; k++) ctx.lineTo(at(k).x, at(k).y)
            ctx.stroke()
        }
        // The newest reading.
        for (let i = n - 1; i >= 0; i--) {
            if (!has(i)) continue
            ctx.fillStyle = ink
            ctx.beginPath(); ctx.arc(at(i).x, at(i).y, Math.max(3, app.px(3.5)), 0, 2 * Math.PI); ctx.fill()
            break
        }
    }
}
