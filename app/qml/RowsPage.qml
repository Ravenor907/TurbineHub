import QtQuick
import QtQuick.Layouts

// A page of rows: a card's controls, the library check, the Windows drive, the apps to bring over, Review changes,
// the quick setup, Where's my...?. The header, the count tiles and the search box come when the page's model has
// them; a note and the page's main button stay pinned under the list.
//
// app (required): Main.qml's window. model: the page (Main.qml's, or its own when embedded in the first-run steps).
// embedded: drawn inside another page, which has the margins and the header.
Item {
    id: page
    required property var app
    property var model: app.model
    property bool embedded: false
    // The main button's key is one hint, with the button's own words.
    readonly property var hints: (model.hints || []).filter(h => !model.primary || h.k !== model.primary.k).concat(model.primary ? [{ k: model.primary.k, label: model.primary.label }] : [])
    function press(k) { return app.pageKey(model, k) }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: page.embedded ? 0 : page.app.px(40)
        anchors.rightMargin: page.embedded ? 0 : page.app.px(40)
        anchors.topMargin: page.embedded ? 0 : page.app.px(24)
        anchors.bottomMargin: page.app.px(14)
        spacing: page.app.px(14)

        PageHeader { app: page.app; Layout.fillWidth: true; crumb: page.embedded ? "" : (page.model.crumb || ""); title: page.model.title || ""; line: page.model.intro || ""; small: page.embedded }

        // The counts of the library check, and how fresh they are.
        Flow {
            visible: (page.model.tiles || []).length > 0
            Layout.fillWidth: true
            spacing: page.app.px(12)
            Repeater {
                model: page.model.tiles || []
                Rectangle {
                    id: tile
                    required property var modelData
                    width: tileRow.implicitWidth + page.app.px(28); height: page.app.px(52); radius: page.app.px(10)
                    color: page.app.colors.toneFill(modelData.tone)
                    border.width: page.app.colors.hc ? 1 : 0; border.color: page.app.colors.tone(modelData.tone)
                    Row {
                        id: tileRow
                        anchors.centerIn: parent
                        spacing: page.app.px(8)
                        Text { text: tile.modelData.value; color: page.app.colors.tone(tile.modelData.tone); font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(24); font.weight: Font.Bold; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: tile.modelData.label; color: page.app.colors.text; font.family: page.app.bodyFont; font.pixelSize: page.app.fpx(15); anchors.verticalCenter: parent.verticalCenter }
                    }
                }
            }
            Column {
                height: page.app.px(52)
                Label { app: page.app; text: page.model.note || ""; font.pixelSize: page.app.fpx(15); height: parent.height / 2 }
                Row {
                    spacing: page.app.px(6)
                    height: parent.height / 2
                    Icon { app: page.app; visible: page.model.stale === true; d: page.app.icons.warn; size: 16; tint: page.app.colors.warn; anchors.verticalCenter: parent.verticalCenter }
                    Label { app: page.app; text: page.model.fresh || ""; font.pixelSize: page.app.fpx(15); color: page.model.stale === true ? page.app.colors.warn : page.app.colors.muted; anchors.verticalCenter: parent.verticalCenter }
                }
            }
        }

        // The search box of the help page. Typing asks the engine for the matching rows.
        Focusable {
            id: searchBox
            app: page.app
            visible: page.model.search === true
            fid: "search"
            aLabel: "Type"
            Layout.fillWidth: true
            Layout.maximumWidth: page.app.px(560)
            implicitHeight: page.app.tpx(48)
            onActivated: page.app.typeIn(input)
            Rectangle { anchors.fill: parent; radius: page.app.px(8); color: page.app.colors.card; border.width: Math.max(1, page.app.px(2)); border.color: input.activeFocus ? page.app.colors.accent : page.app.colors.line }
            Icon { id: searchIcon; app: page.app; d: page.app.icons.search; size: 20; anchors.verticalCenter: parent.verticalCenter; x: page.app.px(14) }
            TextInput {
                id: input
                anchors { left: searchIcon.right; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: page.app.px(10); rightMargin: page.app.px(14) }
                color: page.app.colors.text
                font.family: page.app.bodyFont
                font.pixelSize: page.app.fpx(17)
                clip: true
                onTextEdited: page.app.setArg(text)
                onActiveFocusChanged: if (activeFocus) page.app.typingBegan(input); else page.app.typingEnded(input)
                Keys.onEscapePressed: page.app.keysBack()
                Keys.onReturnPressed: page.app.keysBack()
                Keys.onEnterPressed: page.app.keysBack()
                Keys.onDownPressed: { page.app.keysBack(); page.app.moveFocus("down") }
                Text { visible: input.text === "" && !input.activeFocus; text: "Search, like \"control panel\""; color: page.app.colors.muted; font: input.font; anchors.verticalCenter: parent.verticalCenter }
            }
        }

        RowsView {
            app: page.app
            Layout.fillWidth: true
            Layout.fillHeight: true
            // The search box is drawn above, so its row is left out of the list.
            sections: (page.model.sections || []).map(s => s.rows && s.rows.some(r => r.kind === "input") ? Object.assign({}, s, { rows: s.rows.filter(r => r.kind !== "input") }) : s)
            flow: page.model.flow || ""
        }

        RowLayout {
            visible: !page.embedded && ((page.model.note || "") !== "" && (page.model.tiles || []).length === 0 || !!page.model.primary)
            Layout.fillWidth: true
            spacing: page.app.px(16)
            Icon { app: page.app; visible: note.visible; d: page.app.icons.shield; size: 20 }
            Label { id: note; app: page.app; Layout.fillWidth: true; visible: (page.model.tiles || []).length === 0 && text !== ""; text: page.model.note || ""; color: page.app.colors.muted; font.pixelSize: page.app.fpx(15) }
            Item { Layout.fillWidth: true; visible: !note.visible }
            ActionButton {
                app: page.app
                visible: !!page.model.primary
                fid: "primary"
                primary: true
                label: page.model.primary ? page.model.primary.label : ""
                cap: page.model.primary ? page.model.primary.k : ""
                onActivated: page.app.doAct(page.model.primary.act, page.model.primary.arg, "")
            }
        }
    }
}
