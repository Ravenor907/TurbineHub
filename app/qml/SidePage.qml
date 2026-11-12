import QtQuick
import QtQuick.Layouts

// A list on the left, what the picked entry holds on the right: Games (every game, then its switches), Store and
// Settings (their sections). Moving the highlight down the list shows each entry at once; Right or A goes into it,
// Left comes back, and LT jumps between the two sides. The picked entry keeps an accent bar ("selected") while the
// highlight is on the right.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property var side: model.side || []
    readonly property int picked: model.index || 0
    readonly property bool inSide: app.focusItem !== null && app.focusItem.sideEntry === true
    readonly property var hints: [{ k: "lt", label: inSide ? "Details" : "List" }].concat(model.hints || [])

    function press(k) {
        if (k === "lt") { toOtherSide(); return true }
        return app.pageKey(model, k)
    }
    function move(k) {
        if (inSide) return k === "up" || k === "down" ? (app.moveFocus(k, sideList), true) : false
        if (!app.moveFocus(k, rows) && k === "left") toOtherSide()
        return true
    }
    function toOtherSide() {
        if (inSide) { const list = app.focusables(rows); if (list.length > 0) app.setFocus(list[0], false) }
        else { const list = app.focusables(sideList); const it = list.find(x => x.fid === "side:" + (side[picked] ? side[picked].id : "")); if (it) app.setFocus(it, false) }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // The list.
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: Math.min(page.app.px(330), page.width * 0.4)
            color: page.app.colors.card
            Rectangle { anchors.right: parent.right; width: Math.max(1, page.app.px(1)); height: parent.height; color: page.app.colors.line }
            Label {
                id: sideTitle
                app: page.app
                visible: text !== ""
                x: page.app.px(24); y: page.app.px(18)
                width: parent.width - page.app.px(40)
                text: page.model.sideTitle || ""
                color: page.app.colors.muted
                font.pixelSize: page.app.fpx(15)
            }
            Flickable {
                id: sideFlick
                anchors { top: sideTitle.visible ? sideTitle.bottom : parent.top; bottom: parent.bottom; left: parent.left; right: parent.right; topMargin: page.app.px(12); bottomMargin: page.app.px(8) }
                clip: true
                layer.enabled: page.app.softwareDraw
                // A layer has no see-through parts under software drawing, so the list paints what it sits on first.
                Rectangle { parent: sideFlick; anchors.fill: parent; z: -1; visible: page.app.softwareDraw; color: page.app.colors.card }
                contentHeight: sideList.implicitHeight + page.app.px(10)
                boundsBehavior: Flickable.StopAtBounds
                Behavior on contentY { enabled: page.app.motion.on; NumberAnimation { duration: page.app.motion.standard; easing.type: page.app.motion.ease } }
                Column {
                    id: sideList
                    x: page.app.px(12)
                    width: sideFlick.width - page.app.px(24)
                    spacing: page.app.px(4)
                    topPadding: page.app.px(4)
                    Repeater {
                        model: page.side
                        Focusable {
                            id: entry
                            required property var modelData
                            required property int index
                            readonly property bool sideEntry: true
                            readonly property bool on: index === page.picked
                            app: page.app
                            fid: "side:" + modelData.id
                            firstFocus: on
                            aLabel: "Open"
                            width: sideList.width
                            implicitHeight: Math.max(page.app.tpx(48), entryRow.implicitHeight + page.app.px(16))
                            height: implicitHeight
                            ringRadius: page.app.px(8)
                            ringGap: -page.app.px(1)
                            function focusedNow() { if (!on) page.app.setArg(modelData.id) }
                            function press(k) {
                                if (k === "a" || k === "right") { page.toOtherSide(); return true }
                                return false
                            }
                            onActivated: { if (!on) page.app.setArg(modelData.id) }
                            Rectangle { anchors.fill: parent; radius: page.app.px(8); color: entry.on ? page.app.colors.raised : "transparent" }
                            Rectangle { visible: entry.on; x: 0; anchors.verticalCenter: parent.verticalCenter; width: Math.max(3, page.app.px(4)); height: parent.height - page.app.px(16); radius: width / 2; color: page.app.colors.accent }
                            RowLayout {
                                id: entryRow
                                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(16); rightMargin: page.app.px(10) }
                                spacing: page.app.px(8)
                                Icon { app: page.app; visible: entry.modelData.lock === true; d: page.app.icons.lock; size: 16 }
                                Label { app: page.app; Layout.fillWidth: true; text: entry.modelData.title; font.weight: entry.on ? Font.DemiBold : Font.Medium; color: entry.on ? page.app.colors.text : page.app.colors.soft }
                                Chip { app: page.app; text: entry.modelData.chip || ""; tone: entry.modelData.tone || ""; icon: "" }
                            }
                        }
                    }
                }
            }
            ScrollFade { app: page.app; view: sideFlick; color: page.app.colors.card }
        }

        // What the picked entry holds.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: page.app.px(32)
            Layout.rightMargin: page.app.px(36)
            Layout.topMargin: page.app.px(22)
            Layout.bottomMargin: page.app.px(10)
            spacing: page.app.px(14)
            Label { app: page.app; visible: (page.model.empty || "") !== ""; Layout.fillWidth: true; text: page.model.empty || ""; color: page.app.colors.muted }
            PageHeader { app: page.app; visible: !!page.model.header; Layout.fillWidth: true; small: true; title: page.model.header ? page.model.header.title : ""; line: page.model.header ? page.model.header.line : "" }
            Flow {
                visible: !!page.model.header && (page.model.header.chips || []).length > 0
                Layout.fillWidth: true
                spacing: page.app.px(8)
                Repeater {
                    model: page.model.header ? (page.model.header.chips || []) : []
                    Chip { required property var modelData; app: page.app; text: modelData.text; tone: modelData.tone }
                }
            }
            RowsView { id: rows; app: page.app; Layout.fillWidth: true; Layout.fillHeight: true; sections: page.model.sections || [] }
        }
    }
}
