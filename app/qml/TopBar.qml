import QtQuick

// The top bar: the five tabs between LB and RB (Q and E on a keyboard), then the mode and the machine's name.
// The tab that is open carries the accent underline ("selected"); the highlight never comes up here, the bumpers
// and a click do the switching.
//
// app (required): Main.qml's window.
Rectangle {
    id: bar
    required property var app
    height: app.px(64)
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
                width: tabContent.implicitWidth + bar.app.px(24)
                height: bar.height
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

    Row {
        anchors { right: parent.right; rightMargin: bar.app.px(24); verticalCenter: parent.verticalCenter }
        spacing: bar.app.px(12)
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: modeText.implicitWidth + bar.app.px(18); height: bar.app.px(26); radius: bar.app.px(6)
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
            anchors.verticalCenter: parent.verticalCenter
            // The name gives way before the tabs do: whole or not at all.
            width: implicitWidth
            visible: implicitWidth <= bar.width - tabsRow.width - bar.app.px(200)
            text: bar.app.st.machine
            color: bar.app.colors.muted
            font.family: bar.app.bodyFont
            font.pixelSize: bar.app.fpx(15)
        }
    }
}
