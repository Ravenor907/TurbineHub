import QtQuick
import QtQuick.Layouts

// A page whose sections are chips across its top, with the picked section's rows under them: the Store, and My
// machine (whose first section, Overview, is a grid of cards instead of rows: `cards` in the model, each with its
// part's state and its main switch or button on it; and where B on another section goes back to that first one
// before it leaves the tab: `backTo`). LT and RT
// (or [ and ] on a keyboard) go to the section before and the next from anywhere on the page, round and round;
// a chip can also be highlighted and picked, clicked or tapped. Every row carries its own state and button, so
// nothing has to be opened first.
//
// When the page's model has `search`, a search box sits under the chips, beside the section's line. Find (Y, or F on a keyboard) jumps to
// it and starts typing (in Game Mode Steam's on-screen keyboard opens, and closes again when the typing is done),
// and every letter goes to the engine, which answers with the narrowed page (the chips step aside meanwhile: the
// matches come from every section). B empties the box before it leaves the page. The page's main button (Update
// all) and its own actions sit at the end of the same line.
//
// app (required): Main.qml's window (its model is the page: chips, index, search, header, sections or cards,
// primary, backTo, and `focus` for where the highlight starts).
Item {
    id: page
    required property var app
    readonly property var model: app.model
    readonly property var chips: model.chips || []
    readonly property int picked: model.index || 0
    readonly property var hints: model.hints || []
    readonly property var search: model.search || null
    readonly property string searchValue: search ? (search.value || "") : ""
    // The engine's words win whenever they differ from the box (a page opened again, a letter it didn't keep).
    onSearchValueChanged: if (input.text !== searchValue) input.text = searchValue
    // The section B goes back to first (My machine: Overview), while the page is the tab itself. Opened on top of
    // another page (a link from Home), B goes back to that page.
    readonly property var backTo: (model.backTo || "") !== "" && app.stack.length === 0 ? chips.find(c => c.id === model.backTo) || null : null
    // B empties the search box before it leaves the page.
    readonly property string bLabel: search && searchValue !== "" ? "Clear search" : backTo ? backTo.title : ""
    readonly property var cards: model.cards || []
    readonly property real gap: app.px(16)
    // The search box moves under the section's line where the two don't fit side by side.
    readonly property bool stacked: width < app.px(900)

    function press(k) {
        if (k === "b" && search && searchValue !== "") { input.text = ""; app.doAct("search", search.arg, ""); return true }
        if (k === "b" && backTo) {
            app.lastFid = app.focusItem && app.focusItem.sectionChip === true ? "chip:" + backTo.id : ""
            app.setArg(backTo.id)
            return true
        }
        return app.pageKey(model, k)
    }
    // LT and RT, [ and ]: the section before or the next. False where there are none (a search is on).
    function section(d) {
        if (chips.length < 2) return false
        const next = chips[(picked + d + chips.length) % chips.length]
        // The highlight starts at the top of the new section's rows, unless it is on the chips themselves.
        if (!(app.focusItem && app.focusItem.sectionChip === true)) app.lastFid = ""
        else app.lastFid = "chip:" + next.id
        app.setArg(next.id)
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

    // The rows come first in the page's order, so a fresh page starts with the highlight on its first row and not
    // on a chip: the sections have LT and RT.
    RowsView {
        id: rows
        app: page.app
        anchors { top: head.bottom; bottom: parent.bottom; left: parent.left; right: parent.right; leftMargin: page.app.px(32); rightMargin: page.app.px(36); topMargin: page.app.px(12); bottomMargin: page.app.px(10) }
        visible: page.cards.length === 0
        sections: page.cards.length === 0 ? (page.model.sections || []) : []
    }
    // A section of cards (My machine's Overview), in as many columns as fit at a readable width.
    Flickable {
        id: flick
        visible: page.cards.length > 0
        anchors.fill: rows
        clip: true
        contentHeight: grid.implicitHeight + page.app.px(12)
        boundsBehavior: Flickable.StopAtBounds
        Behavior on contentY { enabled: page.app.motion.moves; NumberAnimation { duration: page.app.motion.moveStandard; easing.type: page.app.motion.ease } }
        GridLayout {
            id: grid
            // Room for the highlight's ring at the edges.
            x: page.app.px(4); y: page.app.px(4)
            width: flick.width - page.app.px(8)
            columns: Math.max(1, Math.min(page.model.columns || 4, Math.floor((width + page.gap) / (page.app.px(250) + page.gap))))
            columnSpacing: page.gap
            rowSpacing: page.gap
            Repeater {
                model: page.cards
                CardTile {
                    required property var modelData
                    app: page.app
                    m: modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.preferredWidth: 1
                }
            }
        }
        ScrollFade { app: page.app; view: flick }
    }
    ColumnLayout {
        id: head
        anchors { top: parent.top; left: parent.left; right: parent.right; leftMargin: page.app.px(32); rightMargin: page.app.px(36); topMargin: page.app.px(16) }
        spacing: page.app.px(10)

        // The sections, between the caps of the two buttons that turn them.
        Flow {
            id: strip
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            visible: page.chips.length > 0
            spacing: page.app.px(8)
            Accessible.role: Accessible.PageTabList
            Accessible.name: "Sections"
            Item { width: prevCap.width; height: page.app.tpx(44); visible: prevCap.visible; KeyCap { id: prevCap; app: page.app; k: "sprev"; anchors.verticalCenter: parent.verticalCenter } }
            Repeater {
                model: page.chips
                Focusable {
                    id: chip
                    required property var modelData
                    required property int index
                    readonly property bool sectionChip: true
                    readonly property bool on: index === page.picked
                    app: page.app
                    fid: "chip:" + modelData.id
                    aLabel: on ? "" : "Show"
                    sayRole: Accessible.PageTab
                    sayName: [modelData.title || "", on ? "selected" : "", modelData.chip || "", "section " + (index + 1) + " of " + page.chips.length].filter(x => x !== "").join(", ")
                    Accessible.selected: on
                    width: Math.max(chipRow.implicitWidth + page.app.px(28), page.app.touchFloor)
                    height: Math.max(page.app.tpx(44), chipRow.implicitHeight + page.app.px(12))
                    ringRadius: height / 2
                    onActivated: if (!on) page.app.setArg(modelData.id)
                    Rectangle {
                        anchors.fill: parent
                        radius: height / 2
                        color: chip.on ? page.app.colors.raised : page.app.colors.panel
                        border.width: Math.max(1, page.app.px(chip.on ? 2 : 1))
                        border.color: chip.on ? page.app.colors.accent : page.app.colors.line
                    }
                    Row {
                        id: chipRow
                        anchors.centerIn: parent
                        spacing: page.app.px(8)
                        Text {
                            text: chip.modelData.title
                            color: chip.on ? page.app.colors.text : page.app.colors.soft
                            font.family: page.app.bodyFont
                            font.pixelSize: page.app.fpx(16)
                            font.weight: chip.on ? Font.DemiBold : Font.Medium
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Chip { app: page.app; text: chip.modelData.chip || ""; tone: chip.modelData.tone || ""; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
            }
            Item { width: nextCap.width; height: page.app.tpx(44); visible: nextCap.visible; KeyCap { id: nextCap; app: page.app; k: "snext"; anchors.verticalCenter: parent.verticalCenter } }
        }

        GridLayout {
            id: top
            Layout.fillWidth: true
            columns: page.stacked ? 1 : 2
            columnSpacing: page.app.px(16)
            rowSpacing: page.app.px(10)

            // What the section is; while a search is on, what it found.
            Label {
                app: page.app
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                text: page.model.header ? (page.model.header.line || "") : ""
                color: page.chips.length === 0 ? page.app.colors.text : page.app.colors.muted
                font.weight: page.chips.length === 0 ? Font.DemiBold : Font.Normal
            }

            RowLayout {
                Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                Layout.fillWidth: page.stacked
                spacing: page.app.px(10)
                // The search box. Typing asks the engine for the narrowed page.
                Focusable {
                    id: searchBox
                    app: page.app
                    visible: page.search !== null
                    focusable: visible
                    fid: "search"
                    aLabel: "Type"
                    sayRole: Accessible.EditableText
                    sayName: "Search" + (input.text !== "" ? ": " + input.text : (page.search && page.search.hint ? ", " + page.search.hint : ""))
                    Layout.fillWidth: page.stacked
                    Layout.preferredWidth: page.app.px(280)
                    Layout.minimumWidth: page.app.px(180)
                    implicitHeight: visible ? page.app.tpx(46) : 0
                    ringRadius: page.app.px(8)
                    ringGap: -page.app.px(1)
                    onActivated: page.app.typeIn(input)
                    Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.card; border.width: Math.max(1, page.app.px(2)); border.color: input.activeFocus ? page.app.colors.accent : page.app.colors.line }
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
                        Keys.onDownPressed: { page.app.keysBack(); page.app.moveFocus("down") }
                        Text { visible: input.text === ""; text: page.search ? (page.search.hint || "") : ""; color: page.app.colors.muted; font: input.font; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
                PageButtons { app: page.app; options: page.model.options || [] }
                // The page's main button (Update all), with Start's cap.
                ActionButton {
                    app: page.app
                    visible: !!page.model.primary
                    focusable: visible
                    fid: "primary"
                    primary: true
                    label: page.model.primary ? page.model.primary.label : ""
                    cap: page.model.primary ? page.model.primary.k : ""
                    onActivated: page.app.doAct(page.model.primary.act, page.model.primary.arg, "")
                }
            }
        }

        // What stands in the way (offline).
        Flow {
            visible: !!page.model.header && (page.model.header.chips || []).length > 0
            Layout.fillWidth: true
            spacing: page.app.px(8)
            Repeater {
                model: page.model.header ? (page.model.header.chips || []) : []
                Chip { required property var modelData; app: page.app; text: modelData.text; tone: modelData.tone }
            }
        }
    }
}
