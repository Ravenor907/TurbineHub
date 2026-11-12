import QtQuick

// The one footer: what each button does right now. Hints arrive most important first; when the bar is too narrow
// the least important are left out, and A and B never are. Each hint is also a button for a finger or the mouse.
//
// app (required): Main.qml's window. hints: [{k, label, hold}], A and B first (`hold`: the button is held for it).
Rectangle {
    id: bar
    required property var app
    property var hints: []
    // The hints left out for room: the pointer's click and Find first, then Menu, Options and the page's main
    // button; never A or B.
    property var hidden: []
    readonly property var shownKeys: hints.filter((h, i) => hidden.indexOf(i) < 0).map(h => h.k)
    // Tall enough for its words and for a finger at every interface size.
    height: Math.max(app.px(58), app.touchFloor + app.px(6))
    color: app.colors.bar
    Rectangle { anchors.top: parent.top; width: parent.width; height: Math.max(1, bar.app.px(1)); color: bar.app.colors.line }

    function fit() {
        const room = width - moveHint.width - app.px(28) * 2 - app.px(32)
        const gap = app.px(22)
        const out = []
        function total() {
            let w = 0
            for (let i = 0; i < hints.length && i < measure.count; i++) if (out.indexOf(i) < 0 && measure.itemAt(i)) w += Math.max(measure.itemAt(i).implicitWidth, app.touchFloor) + gap
            return w
        }
        // Who gives way first: anything unlisted from the last one back, then these in turn.
        const last = ["r3", "y", "view", "x", "play", "start"]
        const order = []
        for (let i = hints.length - 1; i >= 0; i--) if (["a", "b"].concat(last).indexOf(hints[i].k) < 0) order.push(i)
        for (const k of last) { const i = hints.findIndex(h => h.k === k); if (i >= 0) order.push(i) }
        for (const i of order) { if (total() <= room) break; out.push(i) }
        hidden = out
    }
    onWidthChanged: fit()
    onHintsChanged: Qt.callLater(fit)

    Row {
        id: moveHint
        anchors { left: parent.left; leftMargin: bar.app.px(28); verticalCenter: parent.verticalCenter }
        spacing: bar.app.px(9)
        KeyCap { app: bar.app; k: "move"; anchors.verticalCenter: parent.verticalCenter }
        Text { text: "Move"; color: bar.app.colors.muted; font.family: bar.app.bodyFont; font.pixelSize: bar.app.fpx(16); anchors.verticalCenter: parent.verticalCenter }
    }
    Row {
        anchors { right: parent.right; rightMargin: bar.app.px(28); verticalCenter: parent.verticalCenter }
        spacing: bar.app.px(22)
        Repeater {
            id: measure
            model: bar.hints
            Row {
                id: hint
                required property var modelData
                required property int index
                visible: bar.hidden.indexOf(index) < 0
                spacing: bar.app.px(9)
                height: bar.height
                // Never narrower than a finger (a short hint on a dense screen).
                width: Math.max(implicitWidth, bar.app.touchFloor)
                // Something to tap, and for a screen reader a button: "A: Turn on".
                readonly property bool tapTarget: true
                Accessible.role: Accessible.Button
                readonly property string words: (modelData.hold === true ? "Hold: " : "") + modelData.label
                Accessible.name: bar.app.glyph(modelData.k) + ": " + words
                Accessible.onPressAction: if (bar.app.gate(hint, hint.modelData.k)) { if (bar.app.typing) bar.app.typingDone(); else bar.app.press(hint.modelData.k) }
                KeyCap { app: bar.app; k: hint.modelData.k; anchors.verticalCenter: parent.verticalCenter }
                Text { text: hint.words; color: bar.app.colors.text; font.family: bar.app.bodyFont; font.pixelSize: bar.app.fpx(16); anchors.verticalCenter: parent.verticalCenter }
                TapHandler { onTapped: if (bar.app.gate(hint, hint.modelData.k)) { if (bar.app.typing) bar.app.typingDone(); else bar.app.press(hint.modelData.k) } }
            }
        }
    }
}
