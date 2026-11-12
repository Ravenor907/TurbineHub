import QtQuick
import QtQuick.Layouts

// One row of a rows page, drawn from the engine's row: its words on the left, its state and control on the right,
// and for a slider or a choice the control underneath. The row itself takes the highlight and the press; what the
// press does is the row's `act` and `arg`, sent to the engine.
//
// Kinds: toggle (a switch), check (a tick box), slider (Left and Right change it; A applies, or it applies as it
// moves when the row says `live`), choice (Left and Right move among the options, A picks), button (A presses it),
// info (words only; it still takes the highlight so a long page scrolls with the D-pad), preview (a live picture of
// the lights: `frames` of one color per part, `step` milliseconds apart, drawn as `rings` rings at `level`; it
// takes no highlight, and it plays only with Animations at Full: at Reduced and Off it shows its first frame),
// chart (a small line over time: `points` from oldest to newest on a scale from `min` to `max`, null where nothing
// was read, with the scale's ends in words as `low` and `high` and the time's as `from` and `to`; its title, its
// newest value and its line say in words what the picture shows), meter (how full something is: a bar filled to
// `part`, 0 to 1, in the warning color when the row's `tone` is "warn"; its words say the same, so the bar is
// only a picture of them). A choice with `cards` shows its options as cards with a strip of their `palettes`; Up
// and Down move between the lines of cards. A slider with `showFrom` shows where it starts and where it goes while
// it moves, and its `notes` ({below, text}) each show a warning line while the value is below the mark. A row with
// `alert` (a warning to read first) has a border in the stop color.
//
// app (required): Main.qml's window. m (required): the row.
Focusable {
    id: r
    required property var m
    readonly property string kind: m.kind
    fid: m.id
    offers: m.offers || []
    optionsTitle: m.title || ""
    optionLabel: kind === "button" && m.act === "open" && aLabel === "Open" ? "Open its page" : ""
    focusable: kind !== "preview"
    aLabel: kind === "toggle" ? (m.on ? "Turn off" : "Turn on") : kind === "check" ? (m.on ? "Untick" : "Tick") : kind === "slider" ? (m.live ? "" : "Apply") : kind === "choice" ? "Choose" : kind === "button" ? (m.button || "Open") : ""
    // As tall as its words need, and where it takes a press never shorter than a finger.
    implicitHeight: Math.max(body.implicitHeight + app.px(26), aLabel !== "" || kind === "slider" ? app.touchFloor : 0)
    tapActivates: kind !== "choice" && kind !== "slider"
    // For a screen reader: the kind of control, then its title with its state in words, then the line under it.
    sayRole: kind === "toggle" || kind === "check" ? Accessible.CheckBox : kind === "slider" ? Accessible.Slider : kind === "choice" ? Accessible.ComboBox : kind === "button" ? Accessible.Button : kind === "preview" || kind === "chart" ? Accessible.Graphic : Accessible.StaticText
    // (A meter's bar is said in words: "93% full".)
    readonly property string sayState: kind === "toggle" ? (m.on ? "on" : "off") : kind === "check" ? (m.on ? "ticked" : "not ticked")
        : kind === "slider" ? val + (m.unit || "")
        : kind === "choice" ? (options.length > 0 ? String(options[Math.max(0, Math.min(options.length - 1, m.index))]) + (cur !== m.index && cur < options.length ? ", highlight on " + options[cur] : "") : "")
        : (m.value !== undefined && kind !== "input" ? String(m.value) : "")
    sayName: [m.title || "", sayState, kind === "meter" ? (m.partWords || "") : "", m.badge || "", kind === "button" ? (m.button || "") : ""].filter(x => x !== "").join(", ")
    sayMore: m.line || ""
    Accessible.checkable: kind === "toggle" || kind === "check"
    Accessible.checked: (kind === "toggle" || kind === "check") && m.on === true

    // A slider's value while it moves, and the option the highlight is on inside a choice.
    property real val: m.value !== undefined && kind === "slider" ? m.value : 0
    property int cur: kind === "choice" ? Math.max(0, m.index) : 0
    readonly property var options: m.options || []
    // The state chip sits beside the words unless it would take more than a third of the row.
    readonly property bool chipBelow: (m.badge || "") !== "" && sideChip.implicitWidth > width * 0.3
    readonly property real cardWidth: app.px(158)
    readonly property int cardsAcross: Math.max(1, Math.floor((choices.width + app.px(10)) / (cardWidth + app.px(10))))

    // A slider's track, for the build's checks.
    readonly property Item trackItem: track
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
        // Cards: Up and Down stay inside while there is a line of cards that way.
        if (kind === "choice" && m.cards && (k === "up" || k === "down")) {
            const n = cur + (k === "down" ? cardsAcross : -cardsAcross)
            if (n < 0 || n >= options.length) return false
            cur = n
            app.reveal(r)
            return true
        }
        if (k === "a" && aLabel !== "") { activated(); return true }
        return false
    }

    // A warning that must be read first (`alert`) has a border in the stop color around it.
    Rectangle {
        anchors.fill: parent
        radius: r.app.px(10)
        color: r.app.colors.panel
        border.width: r.m.alert === true ? Math.max(2, r.app.px(2)) : Math.max(1, r.app.px(1))
        border.color: r.m.alert === true ? r.app.colors.stop : r.app.colors.line
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
                Icon { app: r.app; visible: r.m.on === true; anchors.centerIn: parent; d: r.app.icons.check; size: 18; tint: r.app.colors.accentInk }
            }
            Icon { app: r.app; visible: r.m.icon !== undefined && r.m.icon !== ""; d: r.m.icon ? (r.app.icons[r.m.icon] || "") : ""; size: 24; tint: r.app.colors.accentText }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: r.app.px(2)
                Label { app: r.app; Layout.fillWidth: true; text: r.m.title; font.pixelSize: r.app.fpx(17); font.weight: Font.DemiBold }
                Label { app: r.app; Layout.fillWidth: true; visible: text !== ""; text: r.m.line || ""; font.pixelSize: r.app.fpx(15); color: r.app.colors.muted }
                Chip { app: r.app; visible: text !== "" && r.chipBelow; text: r.m.badge || ""; tone: r.m.tone || ""; Layout.topMargin: r.app.px(4) }
            }
            Label {
                app: r.app
                visible: text !== ""
                // A slider that shows where it starts (`showFrom`) says both while it is moved: "0 mV → -30 mV".
                text: r.kind === "slider" ? (r.m.showFrom === true && r.val !== r.m.value ? r.m.value + (r.m.unit || "") + " → " + r.val + (r.m.unit || "") : r.val + (r.m.unit || "")) : (r.m.value !== undefined && r.kind !== "input" ? r.m.value : "")
                color: r.app.colors.accentText
                font.pixelSize: r.app.fpx(17)
                font.weight: Font.DemiBold
                wrapMode: Text.NoWrap
            }
            // A wide chip in a narrow row goes under the words instead, so the words never squeeze to a word a line.
            Chip { id: sideChip; app: r.app; visible: text !== "" && !r.chipBelow; text: r.m.badge || ""; tone: r.m.tone || "" }
            Toggle { app: r.app; visible: r.kind === "toggle"; on: r.m.on === true }
            ActionButton { app: r.app; visible: r.kind === "button" && (r.m.button || "") !== ""; plain: true; primary: r.m.primary === true; label: r.m.button || ""; cap: r.focused ? "a" : "" }
            Icon { app: r.app; visible: r.kind === "button" && (r.m.button || "") === ""; d: r.app.icons.chevron; size: 20 }
        }
        // A meter: how full it is, as a bar under the words that say it.
        Item {
            visible: r.kind === "meter"
            Layout.fillWidth: true
            Layout.preferredHeight: visible ? r.app.px(10) : 0
            Rectangle { anchors.fill: parent; radius: height / 2; color: r.app.colors.raised; border.width: 1; border.color: r.app.colors.line }
            Rectangle {
                height: parent.height
                width: Math.max(height, parent.width * Math.max(0, Math.min(1, r.m.part || 0)))
                radius: height / 2
                color: r.m.tone === "warn" ? r.app.colors.warn : r.app.colors.accent
            }
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
                // The value at a place along the track, on one of the slider's steps.
                function valueAt(x) {
                    const steps = Math.round((r.m.max - r.m.min) * Math.max(0, Math.min(1, x / track.width)) / (r.m.step || 1))
                    return Math.max(r.m.min, Math.min(r.m.max, r.m.min + steps * (r.m.step || 1)))
                }
                TapHandler {
                    // The track is thin; a finger a little above or below it still lands on it.
                    margin: Math.max(0, (r.app.touchFloor - track.height) / 2)
                    onTapped: (p) => {
                        if (!r.app.gate(r)) return
                        const v = track.valueAt(p.position.x)
                        if (v !== r.val) { r.val = v; r.send(v) }
                    }
                }
                // Dragged with the mouse, a finger or the controller's pointer (RT held): the knob follows, and the
                // value is sent once, when it is let go. It obeys what a press obeys, asked as the drag begins.
                DragHandler {
                    id: slide
                    target: null
                    yAxis.enabled: false
                    margin: Math.max(0, (r.app.touchFloor - track.height) / 2)
                    property bool taken: false
                    onActiveChanged: {
                        if (active) { taken = r.app.gate(r); if (taken) r.app.setFocus(r, true) }
                        else if (taken) { taken = false; if (r.val !== r.m.value) r.send(r.val) }
                    }
                    onCentroidChanged: if (active && taken) r.val = track.valueAt(centroid.position.x)
                }
            }
            Label { app: r.app; visible: text !== ""; text: r.m.high || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted }
        }
        // A slider's warnings (`notes`: {below, text}): each shows while the value is below its mark.
        Repeater {
            model: r.kind === "slider" ? (r.m.notes || []).filter(x => r.val < x.below) : []
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: r.app.px(8)
                Icon { app: r.app; d: r.app.icons.warn; size: 18; tint: r.app.colors.warn }
                Label { app: r.app; Layout.fillWidth: true; text: modelData.text; font.pixelSize: r.app.fpx(15); color: r.app.colors.warn }
            }
        }
        // A small chart: one line over time, the area under it tinted, a gap where nothing was read. The scale's two
        // ends are named at the left and the time's two ends underneath, so no value has to be guessed.
        RowLayout {
            visible: r.kind === "chart"
            Layout.fillWidth: true
            spacing: r.app.px(10)
            ColumnLayout {
                Layout.fillHeight: true
                Layout.bottomMargin: chartEnds.height + r.app.px(4)
                spacing: 0
                Label { app: r.app; text: r.m.high || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted; Layout.alignment: Qt.AlignRight }
                Item { Layout.fillHeight: true }
                Label { app: r.app; text: r.m.low || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted; Layout.alignment: Qt.AlignRight }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: r.app.px(4)
                Sparkline {
                    app: r.app
                    Layout.fillWidth: true
                    Layout.preferredHeight: r.app.px(88)
                    points: r.kind === "chart" ? (r.m.points || []) : []
                    min: r.m.min !== undefined ? r.m.min : 0
                    max: r.m.max !== undefined ? r.m.max : 100
                    framed: true
                }
                RowLayout {
                    id: chartEnds
                    Layout.fillWidth: true
                    Label { app: r.app; text: r.m.from || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted }
                    Item { Layout.fillWidth: true }
                    Label { app: r.app; text: r.m.to || ""; font.pixelSize: r.app.fpx(14); color: r.app.colors.muted }
                }
            }
        }
        // The live picture of the lights: each ring split into its parts, glowing in the frame's colors.
        Item {
            id: lights
            visible: r.kind === "preview"
            Layout.fillWidth: true
            Layout.preferredHeight: visible ? r.app.px(100) : 0
            readonly property var frames: r.kind === "preview" ? (r.m.frames || []) : []
            readonly property int rings: Math.max(1, r.m.rings || 1)
            property int frame: 0
            opacity: r.m.level !== undefined ? r.m.level : 1
            Timer {
                interval: Math.max(40, r.m.step || 100)
                repeat: true
                running: lights.visible && lights.frames.length > 1 && r.app.motion.moves
                onTriggered: lights.frame = (lights.frame + 1) % lights.frames.length
            }
            Row {
                anchors.centerIn: parent
                spacing: r.app.px(56)
                Repeater {
                    model: lights.rings
                    Canvas {
                        id: ring
                        required property int index
                        width: r.app.px(96); height: width
                        readonly property var colors: lights.frames.length > 0 ? lights.frames[lights.frame % lights.frames.length] : []
                        onColorsChanged: requestPaint()
                        onPaint: {
                            const ctx = getContext("2d")
                            ctx.reset()
                            const all = ring.colors
                            const per = Math.max(1, Math.floor(all.length / lights.rings))
                            const mine = all.slice(ring.index * per, ring.index * per + per)
                            const c = width / 2, rad = width / 2 - r.app.px(14)
                            // The stick in the middle.
                            ctx.fillStyle = r.app.colors.raised
                            ctx.beginPath(); ctx.arc(c, c, rad - r.app.px(12), 0, 2 * Math.PI); ctx.fill()
                            for (let i = 0; i < mine.length; i++) {
                                const a0 = Math.PI / 2 + i * 2 * Math.PI / mine.length, a1 = a0 + 2 * Math.PI / mine.length
                                // A soft glow, then the ring itself.
                                ctx.globalAlpha = 0.3
                                ctx.strokeStyle = mine[i]
                                ctx.lineWidth = r.app.px(22)
                                ctx.beginPath(); ctx.arc(c, c, rad, a0, a1); ctx.stroke()
                                ctx.globalAlpha = 1
                                ctx.lineWidth = r.app.px(9)
                                ctx.beginPath(); ctx.arc(c, c, rad, a0 + 0.04, a1 - 0.04); ctx.stroke()
                            }
                        }
                    }
                }
            }
        }
        // The options of a choice. The one in effect is filled ("selected"); the highlight inside the row is a ring.
        Flow {
            id: choices
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
                    readonly property string swatch: r.m.swatches && !r.m.cards ? r.m.swatches[index] : ""
                    readonly property var palette: r.m.cards && r.m.palettes ? r.m.palettes[index] : []
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: opt.modelData + (picked ? ", chosen" : "") + ", " + (index + 1) + " of " + r.options.length
                    Accessible.checkable: true
                    Accessible.checked: picked
                    Accessible.onPressAction: if (r.app.gate(r)) { r.cur = opt.index; r.send(opt.index) }
                    // Something to tap: never smaller than a finger.
                    readonly property bool tapTarget: true
                    width: r.m.cards ? Math.max(r.cardWidth, r.app.touchFloor) : Math.max(optRow.implicitWidth + r.app.px(28), r.app.touchFloor)
                    height: r.m.cards ? r.app.tpx(82) : r.app.tpx(40)
                    radius: r.app.px(8)
                    color: picked ? r.app.colors.accent : r.app.colors.raised
                    border.width: Math.max(1, r.app.px(2))
                    border.color: at ? r.app.colors.accentHi : picked ? r.app.colors.accent : r.app.colors.line
                    Rectangle { visible: opt.at; anchors.fill: parent; anchors.margins: -r.app.px(4); radius: parent.radius + r.app.px(4); color: "transparent"; border.width: Math.max(2, r.app.px(2)); border.color: r.app.colors.accentHi }
                    // A card's strip of its colors.
                    Row {
                        visible: opt.palette.length > 0
                        x: r.app.px(10); y: r.app.px(10)
                        width: parent.width - r.app.px(20)
                        height: r.app.px(22)
                        Repeater {
                            model: opt.palette
                            Rectangle {
                                required property var modelData
                                width: parent.width / Math.max(1, opt.palette.length)
                                height: parent.height
                                color: modelData
                            }
                        }
                    }
                    Row {
                        id: optRow
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.verticalCenter: r.m.cards ? undefined : parent.verticalCenter
                        anchors.bottom: r.m.cards ? parent.bottom : undefined
                        anchors.bottomMargin: r.app.px(10)
                        spacing: r.app.px(8)
                        Rectangle { visible: opt.swatch !== ""; width: r.app.px(18); height: width; radius: width / 2; color: opt.swatch !== "" ? opt.swatch : "transparent"; border.width: opt.picked ? Math.max(2, r.app.px(2)) : 1; border.color: opt.picked ? r.app.colors.accentInk : r.app.colors.ground; anchors.verticalCenter: parent.verticalCenter }
                        Icon { app: r.app; visible: opt.picked; d: r.app.icons.check; size: 16; tint: r.app.colors.accentInk; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: opt.modelData; color: opt.picked ? r.app.colors.accentInk : r.app.colors.text; font.family: r.app.bodyFont; font.pixelSize: r.app.fpx(16); font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
                    }
                    TapHandler { onTapped: if (r.app.gate(r)) { r.cur = opt.index; r.send(opt.index) } }
                }
            }
        }
    }
}
