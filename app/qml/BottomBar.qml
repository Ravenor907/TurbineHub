import QtQuick

// The one footer: what each button does right now. Hints arrive most important first; when the bar is too narrow
// the least important are left out, and A and B never are. Each hint is also a button for a finger or the mouse.
//
// app (required): Main.qml's window. hints: [{k, label}], A and B first.
Rectangle {
    id: bar
    required property var app
    property var hints: []
    // How many hints fit, counted from the front (at least A and B).
    property int shown: hints.length
    readonly property var shownKeys: hints.slice(0, shown).map(h => h.k)
    height: app.px(58)
    color: app.colors.bar
    Rectangle { anchors.top: parent.top; width: parent.width; height: Math.max(1, bar.app.px(1)); color: bar.app.colors.line }

    function fit() {
        const room = width - moveHint.width - app.px(28) * 2 - app.px(32)
        let n = hints.length
        const gap = app.px(22)
        function total(count) {
            let w = 0
            for (let i = 0; i < count && i < measure.count; i++) w += measure.itemAt(i).implicitWidth + gap
            return w
        }
        while (n > 2 && total(n) > room) n--
        shown = n
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
                visible: index < bar.shown
                spacing: bar.app.px(9)
                height: bar.height
                KeyCap { app: bar.app; k: hint.modelData.k; anchors.verticalCenter: parent.verticalCenter }
                Text { text: hint.modelData.label; color: bar.app.colors.text; font.family: bar.app.bodyFont; font.pixelSize: bar.app.fpx(16); anchors.verticalCenter: parent.verticalCenter }
                TapHandler { onTapped: bar.app.press(hint.modelData.k) }
            }
        }
    }
}
