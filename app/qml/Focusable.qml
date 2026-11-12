import QtQuick

// Anything the highlight can land on. Main.qml finds these by `focusable`, moves between them by where they are on
// screen, and sends the pressed button to `press`. A click or a tap lands the highlight and activates in one go,
// when a press of A would count right now (app.gate); the mouse moved onto one moves the highlight there.
//
// The highlight is always a ring outside the item, never a fill, so it reads the same on a card, a row or a
// button and can't be mistaken for "selected" (an accent mark) or "primary" (an accent fill).
//
// A screen reader (Orca in Desktop Mode) hears each one: what kind of thing it is (sayRole, a button unless said
// otherwise), its name with its state in words (sayName: "Game Boost, on"), more about it (sayMore), and that the
// highlight is on it now. The build's checks fail a picture in which one has no name (SelfCheck).
//
// app (required): Main.qml's window. fid: a name that survives a reload, so the highlight comes back to the same
// thing. aLabel: what A does here, for the footer ("" when A does nothing). hints: more footer hints while the
// highlight is here. firstFocus: where the highlight starts on a page.
Item {
    id: f
    required property var app
    property bool focusable: true
    property string fid: ""
    property string aLabel: "Select"
    property var hints: []
    property bool firstFocus: false
    // Something above it that shows with it when a list scrolls to it (its section's heading).
    property Item revealFrom: null
    // A click or tap anywhere on it presses it (off for rows whose inner parts take the tap themselves).
    property bool tapActivates: true
    property real ringRadius: app.px(10)
    // The ring sits this far outside the item (negative draws it inside, for items at a clipped edge).
    property real ringGap: app.px(3)
    readonly property bool focused: app.focusItem === f
    signal activated()
    property int sayRole: Accessible.Button
    property string sayName: aLabel
    property string sayMore: ""
    Accessible.role: sayRole
    Accessible.name: sayName
    Accessible.description: sayMore
    Accessible.focusable: focusable
    Accessible.focused: focused
    // Drawn but not something the highlight lands on (a button inside a row that takes the press): the row speaks.
    Accessible.ignored: !focusable
    // A screen reader's own "press" is a press of A here, and obeys what a press obeys.
    Accessible.onPressAction: if (f.focusable && f.app.gate(f)) { f.app.setFocus(f, true); if (f.aLabel !== "") f.activated() }
    // Rebuilt away while it held the highlight (its list re-flowed): the highlight finds its new self.
    Component.onDestruction: if (app && app.focusItem === f) app.focusLost()

    function press(k) {
        if (k === "a" && aLabel !== "") { activated(); return true }
        return false
    }

    Rectangle {
        z: 20
        anchors.fill: parent
        anchors.margins: -f.ringGap
        radius: f.ringRadius + f.ringGap
        color: "transparent"
        border.width: f.app.colors.light ? Math.max(3, f.app.px(4)) : Math.max(2, f.app.px(3))
        border.color: f.app.colors.ring
        opacity: f.focused ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { enabled: f.app.motion.on; NumberAnimation { duration: f.app.motion.quick; easing.type: f.app.motion.ease } }
    }
    // The mouse over it: Main.qml decides whether that is the mouse being used (hoverAt).
    HoverHandler {
        id: hover
        enabled: f.focusable
        function tell() { if (hovered) f.app.hoverAt(f, hover.point.scenePosition, hover.point.device.pointerType === PointerDevice.Finger) }
        onHoveredChanged: tell()
        onPointChanged: tell()
    }
    // A tap is a press of A here, and obeys what a press obeys (gate).
    TapHandler { enabled: f.focusable; onTapped: if (f.app.gate(f)) { f.app.setFocus(f, true); if (f.tapActivates && f.aLabel !== "") f.activated() } }
}
