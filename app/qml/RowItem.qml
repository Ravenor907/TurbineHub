import QtQuick
import QtQuick.Layouts

// One row of a rows page, drawn from the engine's row: its words on the left, its state and control on the right,
// and for a slider or a choice the control underneath. The row itself takes the highlight and the press; what the
// press does is the row's `act` and `arg`, sent to the engine.
//
// Kinds: toggle (a switch), check (a tick box), slider (Left and Right change it; A applies, or it applies as it
// moves when the row says `live`), choice (Left and Right move among the options, A picks), button (A presses it),
// info (words only; it still takes the highlight so a long page scrolls with the D-pad).
//
// app (required): Main.qml's window. m (required): the row.
Focusable {
    id: r
    required property var m
    readonly property string kind: m.kind
    fid: m.id
    aLabel: kind === "toggle" ? (m.on ? "Turn off" : "Turn on") : kind === "check" ? (m.on ? "Untick" : "Tick") : kind === "slider" ? (m.live ? "" : "Apply") : kind === "choice" ? "Choose" : kind === "button" ? (m.button || "Open") : ""
    implicitHeight: body.implicitHeight + app.px(26)
    tapActivates: kind !== "choice" && kind !== "slider"

    // A slider's value while it moves, and the option the highlight is on inside a choice.
    property real val: m.value !== undefined && kind === "slider" ? m.value : 0
    property int cur: kind === "choice" ? Math.max(0, m.index) : 0
    readonly property var options: m.options || []

    function send(value) { app.doAct(m.act, m.arg, value) }
    function step(d) {
        const v = Math.max(m.min, Math.min(m.max, val + d * (m.step || 1)))
        if (v === val) return
        val = v
        if (m.live) send(v)
    }
    onActivated: {
        if (kind === "slider") { if (!m.live && val !== m.value) send(val) }
        else if (kind === "choice") send(cur)
        else if (kind !== "info") send("")
    }
    function press(k) {
        if (kind === "slider" && (k === "left" || k === "right")) { step(k === "right" ? 1 : -1); return true }
        if (kind === "choice" && (k === "left" || k === "right")) {
            const n = cur + (k === "right" ? 1 : -1)
            if (n < 0 || n >= options.length) return false
            cur = n
            return true
        }
        if (k === "a" && aLabel !== "") { activated(); return true }
        return false
    }

    Rectangle {
        anchors.fill: parent
        radius: r.app.px(10)
        color: r.app.colors.panel
        border.width: Math.max(1, r.app.px(1))
        border.color: r.app.colors.line
    }
    ColumnLayout {
        id: body
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: r.app.px(18); rightMargin: r.app.px(18) }
        spacing: r.app.px(10)
        RowLayout {
            Layout.fillWidth: true
            spacing: r.app.px(14)
            // The tick box of a check row.
            Rectangle {
                visible: r.kind === "check"
                Layout.preferredWidth: r.app.px(26); Layout.preferredHeight: r.app.px(26)
                radius: r.app.px(6)
                color: r.m.on ? r.app.colors.accent : "transparent"
                border.width: Math.max(1, r.app.px(2))
                border.color: r.m.on ? r.app.colors.accent : r.app.colors.muted
                Icon { app: r.app; visible: r.m.on === true; anchors.centerIn: parent; d: r.app.icons.check; size: 18; tint: r.app.colors.onAccent }
            }
            Icon { app: r.app; visible: r.m.icon !== undefined && r.m.icon !== ""; d: r.m.icon ? (r.app.icons[r.m.icon] || "") : ""; size: 24; tint: r.app.colors.accentText }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: r.app.px(2)
                Label { app: r.app; Layout.fillWidth: true; text: r.m.title; font.pixelSize: r.app.fpx(17); font.weight: Font.DemiBold }
                Label { app: r.app; Layout.fillWidth: true; visible: text !== ""; text: r.m.line || ""; font.pixelSize: r.app.fpx(15); color: r.app.colors.muted }
            }
            Label {
                app: r.app
                visible: text !== ""
                text: r.kind === "slider" ? r.val + (r.m.unit || "") : (r.m.value !== undefined && r.kind !== "input" ? r.m.value : "")
                color: r.app.colors.accentText
                font.pixelSize: r.app.fpx(17)
                font.weight: Font.DemiBold
                wrapMode: Text.NoWrap
            }
            Chip { app: r.app; text: r.m.badge || ""; tone: r.m.tone || "" }
            Toggle { app: r.app; visible: r.kind === "toggle"; on: r.m.on === true }
            ActionButton { app: r.app; visible: r.kind === "button" && (r.m.button || "") !== ""; plain: true; primary: r.m.primary === true; label: r.m.button || ""; cap: r.focused ? "a" : "" }
            Icon { app: r.app; visible: r.kind === "button" && (r.m.button || "") === ""; d: r.app.icons.chevron; size: 20 }
        }
        // The slider's track, with its ends named when the row names them.
        RowLayout {
            visible: r.kind === "slider"
            Layout.fillWidth: true
            spacing: r.app.px(12)
            Label { app: r.app; visible: text !== ""; text: r.m.low || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted }
            Item {
                id: track
                Layout.fillWidth: true
                Layout.preferredHeight: r.app.px(24)
                readonly property real part: r.kind === "slider" && r.m.max > r.m.min ? (r.val - r.m.min) / (r.m.max - r.m.min) : 0
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: r.app.px(8); radius: height / 2; color: r.app.colors.raised; border.width: 1; border.color: r.app.colors.line }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Math.max(height, parent.width * track.part); height: r.app.px(8); radius: height / 2; color: r.app.colors.accent }
                Rectangle { x: (parent.width - width) * track.part; anchors.verticalCenter: parent.verticalCenter; width: r.app.px(22); height: width; radius: width / 2; color: r.app.colors.accentHi; border.width: Math.max(1, r.app.px(2)); border.color: r.app.colors.ground }
                TapHandler {
                    onTapped: (p) => {
                        const steps = Math.round((r.m.min + (r.m.max - r.m.min) * Math.max(0, Math.min(1, p.position.x / track.width)) - r.m.min) / (r.m.step || 1))
                        const v = Math.max(r.m.min, Math.min(r.m.max, r.m.min + steps * (r.m.step || 1)))
                        if (v !== r.val) { r.val = v; r.send(v) }
                    }
                }
            }
            Label { app: r.app; visible: text !== ""; text: r.m.high || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted }
        }
        // The options of a choice. The one in effect is filled ("selected"); the highlight inside the row is a ring.
        Flow {
            visible: r.kind === "choice"
            Layout.fillWidth: true
            spacing: r.app.px(10)
            Repeater {
                model: r.kind === "choice" ? r.options : []
                Rectangle {
                    id: opt
                    required property var modelData
                    required property int index
                    readonly property bool picked: index === r.m.index
                    readonly property bool at: r.focused && index === r.cur
                    readonly property string swatch: r.m.swatches ? r.m.swatches[index] : ""
                    width: optRow.implicitWidth + r.app.px(28)
                    height: r.app.tpx(40)
                    radius: r.app.px(8)
                    color: picked ? r.app.colors.accent : r.app.colors.raised
                    border.width: Math.max(1, r.app.px(2))
                    border.color: at ? r.app.colors.accentHi : picked ? r.app.colors.accent : r.app.colors.line
                    Rectangle { visible: opt.at; anchors.fill: parent; anchors.margins: -r.app.px(4); radius: parent.radius + r.app.px(4); color: "transparent"; border.width: Math.max(2, r.app.px(2)); border.color: r.app.colors.accentHi }
                    Row {
                        id: optRow
                        anchors.centerIn: parent
                        spacing: r.app.px(8)
                        Rectangle { visible: opt.swatch !== ""; width: r.app.px(18); height: width; radius: width / 2; color: opt.swatch !== "" ? opt.swatch : "transparent"; border.width: 1; border.color: r.app.colors.ground; anchors.verticalCenter: parent.verticalCenter }
                        Icon { app: r.app; visible: opt.picked; d: r.app.icons.check; size: 16; tint: r.app.colors.onAccent; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: opt.modelData; color: opt.picked ? r.app.colors.onAccent : r.app.colors.text; font.family: r.app.bodyFont; font.pixelSize: r.app.fpx(16); font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
                    }
                    TapHandler { onTapped: { r.cur = opt.index; r.send(opt.index) } }
                }
            }
        }
    }
}
