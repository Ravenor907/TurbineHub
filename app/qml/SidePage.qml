import QtQuick
import QtQuick.Layouts

// A list on the left, what the picked entry holds on the right: Games (every game, then its switches), Store and
// Settings (their sections). Moving the highlight down the list shows each entry at once; Right or A goes into it
// and Left comes back. The picked entry keeps an accent bar ("selected") while the highlight is on the right. The
// page's own actions (Library check, Update all) are buttons under the list.
//
// When the page's model has `search`, a search box sits above the list. Find (Y, or F on a keyboard) jumps to it
// and starts typing (in Game Mode Steam's on-screen keyboard opens, and closes again when the typing is done), and
// every letter goes to the engine, which answers with the narrowed page. B empties the box before it leaves the page.
//
// app (required): Main.qml's window (its model is the page).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property var side: model.side || []
    readonly property int picked: model.index || 0
    readonly property bool inSide: app.focusItem !== null && app.focusItem.sideEntry === true
    readonly property var hints: model.hints || []
    readonly property var search: model.search || null
    readonly property string searchValue: search ? (search.value || "") : ""
    // The engine's words win whenever they differ from the box (a page opened again, a letter it didn't keep).
    onSearchValueChanged: if (input.text !== searchValue) input.text = searchValue
    // B empties the search box before it leaves the page.
    readonly property string bLabel: search && searchValue !== "" ? "Clear search" : ""

    function press(k) {
        if (k === "b" && search && searchValue !== "") { input.text = ""; app.doAct("search", search.arg, ""); return true }
        return app.pageKey(model, k)
    }
    function move(k) {
        if (inSide) return k === "up" || k === "down" ? (app.moveFocus(k, sideArea), true) : false
        if (!app.moveFocus(k, rows) && k === "left") toOtherSide()
        return true
    }
    // Find: the search box takes the highlight and the keyboard (`text`: a letter typed already).
    function find(text) {
        if (!search) return false
        app.setFocus(searchBox, false)
        searchBox.activated()
        if (text !== "") { input.text = input.text + text; app.doAct("search", search.arg, input.text) }
        return true
    }
    function toOtherSide() {
        if (inSide) { const list = app.focusables(rows); if (list.length > 0) app.setFocus(list[0], false) }
        else { const list = app.focusables(sideList); const it = list.find(x => x.fid === "side:" + (side[picked] ? side[picked].id : "")); if (it) app.setFocus(it, false); else if (search) app.setFocus(searchBox, false) }
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
            Item {
                id: sideArea
                anchors { top: sideTitle.visible ? sideTitle.bottom : parent.top; bottom: parent.bottom; left: parent.left; right: parent.right; topMargin: page.app.px(12); bottomMargin: page.app.px(8) }
                // The search box. Typing asks the engine for the narrowed page.
                Focusable {
                    id: searchBox
                    readonly property bool sideEntry: true
                    app: page.app
                    visible: page.search !== null
                    focusable: visible
                    fid: "search"
                    aLabel: "Type"
                    sayRole: Accessible.EditableText
                    sayName: "Search" + (input.text !== "" ? ": " + input.text : (page.search && page.search.hint ? ", " + page.search.hint : ""))
                    x: page.app.px(12)
                    width: parent.width - page.app.px(24)
                    height: visible ? page.app.tpx(46) : 0
                    ringRadius: page.app.px(8)
                    ringGap: -page.app.px(1)
                    onActivated: page.app.typeIn(input)
                    Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.ground; border.width: Math.max(1, page.app.px(2)); border.color: input.activeFocus ? page.app.colors.accent : page.app.colors.line }
                    Icon { id: searchIcon; app: page.app; d: page.app.icons.search; size: 18; anchors.verticalCenter: parent.verticalCenter; x: page.app.px(12) }
                    TextInput {
                        id: input
                        Accessible.ignored: true
                        anchors { left: searchIcon.right; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(8); rightMargin: page.app.px(12) }
                        color: page.app.colors.text
                        font.family: page.app.bodyFont
                        font.pixelSize: page.app.fpx(16)
                        clip: true
                        maximumLength: 60
                        text: page.searchValue
                        onTextEdited: page.app.doAct("search", page.search.arg, text)
                        onActiveFocusChanged: if (activeFocus) page.app.typingBegan(input); else page.app.typingEnded(input)
                        Keys.onEscapePressed: page.app.keysBack()
                        Keys.onReturnPressed: page.app.keysBack()
                        Keys.onEnterPressed: page.app.keysBack()
                        Keys.onDownPressed: { page.app.keysBack(); page.app.moveFocus("down", sideArea) }
                        Text { visible: input.text === ""; text: page.search ? (page.search.hint || "") : ""; color: page.app.colors.muted; font: input.font; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
                Flickable {
                    id: sideFlick
                    anchors { top: searchBox.bottom; bottom: sideButtons.top; left: parent.left; right: parent.right; topMargin: searchBox.visible ? page.app.px(8) : 0; bottomMargin: sideButtons.visible ? page.app.px(8) : 0 }
                    clip: true
                    contentHeight: sideList.implicitHeight + page.app.px(10)
                    boundsBehavior: Flickable.StopAtBounds
                    Behavior on contentY { enabled: page.app.motion.moves; NumberAnimation { duration: page.app.motion.moveStandard; easing.type: page.app.motion.ease } }
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
                                // What the open entry holds is what it offers (a game: Play, its switches, its pages).
                                offers: on ? (page.model.entry || []) : []
                                optionsTitle: modelData.title || ""
                                optionLabel: "Go to its details"
                                sayRole: Accessible.ListItem
                                sayName: [modelData.title || "", on ? "open" : "", modelData.chip || "", modelData.lock === true ? "locked" : "", (index + 1) + " of " + page.side.length].filter(x => x !== "").join(", ")
                                Accessible.selected: on
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
                                    Chip { app: page.app; text: entry.modelData.chip || ""; tone: entry.modelData.tone || "" }
                                }
                            }
                        }
                    }
                }
                ScrollFade { app: page.app; view: sideFlick; color: page.app.colors.card }
                // The page's own actions, pinned under the list.
                PageButtons {
                    id: sideButtons
                    app: page.app
                    side: true
                    options: page.model.options || []
                    anchors { bottom: parent.bottom; left: parent.left; right: parent.right; leftMargin: page.app.px(12); rightMargin: page.app.px(12); bottomMargin: page.app.px(4) }
                    height: visible ? implicitHeight : 0
                }
            }
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
