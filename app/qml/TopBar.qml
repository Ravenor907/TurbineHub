import QtQuick

// The top bar: the tabs between LB and RB (Q and E on a keyboard), then the status: the work that runs while the
// player browses ("Working: Discord (2 more waiting)", with a mark that turns), a mark when updates are ready, the
// battery with its percent and a charging mark (only on a machine with one), the clock (Game Mode hides Steam's
// own), the mode and the machine's name. While a lower graphics card voltage is being checked its countdown comes
// first ("Checking -30 mV: 4:12 left") with Stop beside it, a tap that goes back at once; it never gives way. The tab that is open carries the accent underline ("selected"); the
// highlight never comes up here, the bumpers and a click do the switching.
//
// The status never crowds the tabs. What fits is worked out from the room that is left (`fitted`), and things give
// way in this order: the machine's name, the clock, the work's long words (a short form stays: "Discord +2"), the
// battery's percent, the short words, and last the updates mark and the battery itself. The work's mark and the
// mode always show. What the status says comes from the engine (app.status); nothing is decided here but room.
//
// app (required): Main.qml's window.
Rectangle {
    id: bar
    required property var app
    // Tall enough for its words and for a finger at every interface size.
    height: Math.max(app.px(64), app.touchFloor + app.px(8))
    color: app.colors.bar
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Math.max(1, bar.app.px(1)); color: bar.app.colors.line }

    Row {
        id: tabsRow
        anchors { left: parent.left; leftMargin: bar.app.px(24); verticalCenter: parent.verticalCenter }
        spacing: bar.app.px(6)
        KeyCap { app: bar.app; k: "lb"; anchors.verticalCenter: parent.verticalCenter }
        Repeater {
            model: bar.app.tabs
            Item {
                id: tabItem
                required property var modelData
                required property int index
                // The tab of the page on screen, wherever it was opened from.
                readonly property bool on: bar.app.shownTab === index
                // Narrow windows keep the icons and drop the other tabs' names.
                readonly property bool named: on || bar.width >= bar.app.px(1080)
                // Something to tap: never narrower than a finger.
                readonly property bool tapTarget: true
                width: Math.max(tabContent.implicitWidth + bar.app.px(24), bar.app.touchFloor)
                height: bar.height
                Accessible.role: Accessible.PageTab
                Accessible.name: modelData.title + (on ? ", selected" : "") + ", tab " + (index + 1) + " of " + bar.app.tabs.length
                Accessible.selected: on
                Accessible.onPressAction: if (bar.app.gate(tabItem, "lb")) bar.app.goTab(tabItem.index)
                Row {
                    id: tabContent
                    anchors.centerIn: parent
                    spacing: bar.app.px(8)
                    Icon { app: bar.app; d: bar.app.icons[tabItem.modelData.icon]; size: 20; tint: tabItem.on ? bar.app.colors.accentText : bar.app.colors.muted; anchors.verticalCenter: parent.verticalCenter }
                    Text {
                        visible: tabItem.named
                        text: tabItem.modelData.title
                        color: tabItem.on ? bar.app.colors.text : bar.app.colors.muted
                        font.family: bar.app.bodyFont
                        font.pixelSize: bar.app.fpx(17)
                        font.weight: tabItem.on ? Font.DemiBold : Font.Medium
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                Rectangle { visible: tabItem.on; anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - bar.app.px(12); height: Math.max(3, bar.app.px(3)); radius: height / 2; color: bar.app.colors.accent }
                TapHandler { onTapped: if (bar.app.gate(tabItem, "lb")) bar.app.goTab(tabItem.index) }
            }
        }
        KeyCap { app: bar.app; k: "rb"; anchors.verticalCenter: parent.verticalCenter }
    }

    // ---------------------------------------------------------------- the status
    readonly property var status: app.status
    readonly property bool working: (status.work || "") !== ""
    readonly property bool hasBattery: status.battery >= 0
    readonly property int updates: status.updates || 0
    readonly property string check: status.check || ""
    // The clock: the engine's own words in a picture, else the time here, looked at every few seconds.
    property date now: new Date()
    Timer { interval: 5000; repeat: true; running: bar.status.clock === true && (bar.status.clockText || "") === "" && bar.visible; onTriggered: bar.now = new Date() }
    readonly property string clockWords: (status.clockText || "") !== "" ? status.clockText : now.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
    readonly property real gap: app.px(12)
    // Where the tabs end and the status begins (the build's checks hold the two apart).
    readonly property real tabsEnd: tabsRow.x + tabsRow.width
    readonly property real statusStart: statusRow.x
    // The room beside the tabs, and what of the status fits in it.
    readonly property real room: width - tabsRow.x - tabsRow.width - app.px(24) - app.px(20)
    TextMetrics { id: fullWords; font: workText.font; text: bar.status.work || "" }
    TextMetrics { id: shortWords; font: workText.font; text: bar.status.workShort || "" }
    TextMetrics { id: checkFull; font: checkText.font; text: bar.check }
    TextMetrics { id: checkBrief; font: checkText.font; text: bar.status.checkShort || "" }
    readonly property var fitted: {
        const w = {
            mode: modeBox.width,
            name: nameText.implicitWidth,
            clock: status.clock === true ? clockText.implicitWidth : 0,
            pct: hasBattery ? pctText.implicitWidth + app.px(4) : 0,
            bat: hasBattery ? batIcon.width + (status.charging === true ? chargeIcon.width : 0) : 0,
            upd: updates > 0 ? updIcon.width + app.px(4) + updText.implicitWidth : 0,
            mark: working ? workIcon.width : 0,
            full: working ? fullWords.width + app.px(8) + (status.part >= 0 ? app.px(54) : 0) : 0,
            brief: working ? shortWords.width + app.px(8) : 0,
            // The check's mark and Stop always show; its words as long as there is room ("4:12" at least).
            check: bar.check !== "" ? app.px(18) + app.px(16) + stopItem.width : 0,
            cfull: bar.check !== "" ? checkFull.width : 0,
            cbrief: bar.check !== "" ? checkBrief.width : 0
        }
        const need = (t) => {
            const parts = [w.mode, w.check + (t.cw === 2 ? w.cfull : t.cw === 1 ? w.cbrief : 0), w.mark + (t.words === 2 ? w.full : t.words === 1 ? w.brief : 0), t.upd ? w.upd : 0, (t.bat ? w.bat : 0) + (t.pct ? w.pct : 0), t.clock ? w.clock : 0, t.name ? w.name : 0].filter(x => x > 0)
            return parts.reduce((a, b) => a + b, 0) + gap * Math.max(0, parts.length - 1)
        }
        // Most first: each step lets one more thing go.
        const tries = [
            { name: true, words: 2, clock: true, pct: true, bat: true, upd: true, cw: 2 },
            { name: false, words: 2, clock: true, pct: true, bat: true, upd: true, cw: 2 },
            { name: false, words: 2, clock: false, pct: true, bat: true, upd: true, cw: 2 },
            { name: false, words: 1, clock: false, pct: true, bat: true, upd: true, cw: 2 },
            { name: false, words: 1, clock: false, pct: false, bat: true, upd: true, cw: 2 },
            { name: false, words: 0, clock: false, pct: false, bat: true, upd: true, cw: 2 },
            { name: false, words: 0, clock: false, pct: false, bat: false, upd: false, cw: 2 },
            { name: false, words: 0, clock: false, pct: false, bat: false, upd: false, cw: 1 },
            { name: false, words: 0, clock: false, pct: false, bat: false, upd: false, cw: 0 }]
        for (const t of tries) if (need(t) <= room) return t
        return tries[tries.length - 1]
    }

    Row {
        id: statusRow
        anchors { right: parent.right; rightMargin: bar.app.px(24); verticalCenter: parent.verticalCenter }
        spacing: bar.gap
        // A lower graphics card voltage being checked: its countdown, and Stop.
        Row {
            id: checkRow
            visible: bar.check !== ""
            anchors.verticalCenter: parent.verticalCenter
            spacing: bar.app.px(8)
            Icon { app: bar.app; d: bar.app.icons.gauge; size: 18; tint: bar.app.colors.warn; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: checkText
                visible: bar.fitted.cw > 0
                text: bar.fitted.cw === 2 ? bar.check : (bar.status.checkShort || "")
                Accessible.role: Accessible.StaticText
                Accessible.name: bar.check
                Accessible.ignored: !checkRow.visible
                color: bar.app.colors.text
                font.family: bar.app.bodyFont
                font.pixelSize: bar.app.fpx(15)
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }
            Item {
                id: stopItem
                // Something to tap: never narrower than a finger.
                readonly property bool tapTarget: true
                width: Math.max(stopText.implicitWidth + bar.app.px(24), bar.app.touchFloor)
                height: bar.height
                Accessible.role: Accessible.Button
                Accessible.name: "Stop checking the lower voltage"
                Accessible.ignored: !checkRow.visible
                Accessible.onPressAction: bar.app.doAct("uv-stop", "", "")
                Rectangle {
                    anchors.centerIn: parent
                    width: parent.width; height: bar.app.bpx(30, 15); radius: height / 2
                    color: "transparent"
                    border.width: Math.max(1, bar.app.px(2)); border.color: bar.app.colors.warn
                }
                Text {
                    id: stopText
                    anchors.centerIn: parent
                    text: "Stop"
                    Accessible.ignored: true
                    color: bar.app.colors.text
                    font.family: bar.app.bodyFont
                    font.pixelSize: bar.app.fpx(15)
                    font.weight: Font.DemiBold
                }
                TapHandler { onTapped: if (bar.app.gate(stopItem, "a")) bar.app.doAct("uv-stop", "", "") }
            }
        }
        // The line of work: a mark that turns while something runs, and its words where there is room.
        Row {
            id: workRow
            visible: bar.working
            anchors.verticalCenter: parent.verticalCenter
            spacing: bar.app.px(8)
            Accessible.role: Accessible.StaticText
            Accessible.name: (bar.status.work || "") + ((bar.status.detail || "") !== "" ? ". " + bar.status.detail : "")
            Accessible.ignored: !bar.working
            Icon {
                id: workIcon
                app: bar.app; d: bar.app.icons.refresh; size: 18; tint: bar.app.colors.accentText
                anchors.verticalCenter: parent.verticalCenter
                RotationAnimator on rotation { from: 0; to: 360; duration: bar.app.motion.spin; loops: Animation.Infinite; running: bar.working && bar.app.motion.on }
            }
            Text {
                id: workText
                visible: bar.fitted.words > 0
                Accessible.ignored: true
                text: bar.fitted.words === 2 ? (bar.status.work || "") : (bar.status.workShort || "")
                color: bar.app.colors.text
                font.family: bar.app.bodyFont
                font.pixelSize: bar.app.fpx(15)
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }
            // How far it is, where the job knows.
            Rectangle {
                visible: bar.fitted.words === 2 && bar.status.part >= 0
                anchors.verticalCenter: parent.verticalCenter
                width: bar.app.px(46); height: Math.max(4, bar.app.px(5)); radius: height / 2
                color: bar.app.colors.line
                Rectangle { width: parent.width * Math.max(0, Math.min(1, bar.status.part)); height: parent.height; radius: height / 2; color: bar.app.colors.accent }
            }
        }
        // Updates are ready (Store, Installed has them).
        Row {
            visible: bar.updates > 0 && bar.fitted.upd
            anchors.verticalCenter: parent.verticalCenter
            spacing: bar.app.px(4)
            Accessible.role: Accessible.StaticText
            Accessible.name: bar.updates === 1 ? "1 update is ready" : bar.updates + " updates are ready"
            Accessible.ignored: !visible
            Icon { id: updIcon; app: bar.app; d: bar.app.icons.download; size: 18; tint: bar.app.colors.muted; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: updText
                Accessible.ignored: true
                text: bar.updates
                color: bar.app.colors.muted
                font.family: bar.app.bodyFont
                font.pixelSize: bar.app.fpx(15)
                font.weight: Font.DemiBold
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        // The battery, on a machine that has one: its percent, and a bolt while it charges.
        Row {
            visible: bar.hasBattery && bar.fitted.bat
            anchors.verticalCenter: parent.verticalCenter
            spacing: bar.app.px(4)
            Accessible.role: Accessible.StaticText
            Accessible.name: "Battery " + bar.status.battery + " percent" + (bar.status.charging === true ? ", charging" : "")
            Accessible.ignored: !visible
            Icon { id: chargeIcon; app: bar.app; visible: bar.status.charging === true; d: bar.app.icons.bolt; size: 14; tint: bar.app.colors.accentText; anchors.verticalCenter: parent.verticalCenter }
            Icon { id: batIcon; app: bar.app; d: bar.app.icons.battery; size: 20; tint: bar.app.colors.muted; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: pctText
                visible: bar.fitted.pct
                Accessible.ignored: true
                text: bar.status.battery + "%"
                color: bar.app.colors.muted
                font.family: bar.app.bodyFont
                font.pixelSize: bar.app.fpx(15)
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        // The clock: the first thing to go when the bar is narrow.
        Text {
            id: clockText
            visible: bar.status.clock === true && bar.fitted.clock
            Accessible.role: Accessible.StaticText
            Accessible.name: "The time is " + text
            Accessible.ignored: !visible
            text: bar.clockWords
            color: bar.app.colors.muted
            font.family: bar.app.bodyFont
            font.pixelSize: bar.app.fpx(15)
            anchors.verticalCenter: parent.verticalCenter
        }
        Rectangle {
            id: modeBox
            anchors.verticalCenter: parent.verticalCenter
            width: modeText.implicitWidth + bar.app.px(18); height: bar.app.bpx(26, 13); radius: bar.app.px(6)
            color: "transparent"
            border.width: Math.max(1, bar.app.px(2)); border.color: bar.app.colors.accentLo
            Text {
                id: modeText
                anchors.centerIn: parent
                text: bar.app.veteran ? "VETERAN" : "GUIDED"
                color: bar.app.colors.accentText
                font.family: bar.app.bodyFont
                font.pixelSize: bar.app.fpx(13)
                font.weight: Font.Bold
                font.letterSpacing: bar.app.px(1.5)
            }
        }
        Text {
            id: nameText
            anchors.verticalCenter: parent.verticalCenter
            // The name gives way before anything else does: whole or not at all.
            width: implicitWidth
            visible: bar.fitted.name
            text: bar.app.st.machine
            color: bar.app.colors.muted
            font.family: bar.app.bodyFont
            font.pixelSize: bar.app.fpx(15)
        }
    }
}
