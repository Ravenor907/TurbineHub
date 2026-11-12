import QtQuick

// The page's own actions as buttons: the entries of its Options menu that the engine marks `button` (Library
// check, Update all, Undo all, Tick all), so each is one click or tap away and not only in the menu. A page puts
// this where it has room: under its list, or beside its header.
//
// app (required): Main.qml's window. options: the page's options (model.options). side: the buttons are part of a
// two-sided page's list side.
Flow {
    id: pb
    required property var app
    property var options: []
    property bool side: false
    readonly property var shown: options.filter(o => o.button === true)
    readonly property int count: shown.length
    visible: count > 0
    spacing: app.px(10)
    Repeater {
        model: pb.shown
        ActionButton {
            required property var modelData
            readonly property bool sideEntry: pb.side
            app: pb.app
            fid: "option:" + modelData.title
            label: modelData.title
            onActivated: pb.app.chosen(modelData)
        }
    }
}
