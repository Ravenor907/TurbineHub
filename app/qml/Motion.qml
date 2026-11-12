import QtQuick

// How Turbine Hub moves, in one place. Every animation in app/qml takes its time and its curve from here: no other
// file writes a duration in milliseconds or names an easing curve (tools/qml-check.js fails the build on one).
//
// The level follows the player's Animations setting: "full" fades and glides, "off" is instant (pictures are taken
// this way, so they never race an animation).
//
// The language: quick for the highlight, a press and words that change in place; standard for dialogs, toasts and
// lists that scroll to the highlight; gentle for a whole page. Things fade and settle on OutCubic; nothing bounces.
QtObject {
    id: motion
    property string level: "full"
    readonly property bool on: level !== "off"

    readonly property int quick: on ? 120 : 0
    readonly property int standard: on ? 200 : 0
    readonly property int gentle: on ? 280 : 0
    readonly property int ease: Easing.OutCubic

    // A page or words changing in place fade up from this, never from nothing.
    readonly property real swapFrom: 0.35
    // What a dialog grows from as it fades in, and what a button shrinks to while pressed.
    readonly property real popScale: on ? 0.96 : 1
    readonly property real pressScale: on ? 0.97 : 1

    // Not transitions, so they run at every level: how long a toast stays, the busy mark's turn, and how a held
    // direction repeats.
    readonly property int toastStay: 3200
    readonly property int spin: 1400
    readonly property int holdDelay: 380
    readonly property int holdRepeat: 110
    // How long a page or dialog that just appeared leaves out A, X, Y and Menu (a double press stays on the first).
    readonly property int pressGuard: 250
    // How long a picture waits for the page to settle before the checks run.
    readonly property int settle: 350
    // The time between two scripted presses in a picture run.
    readonly property int scriptStep: 60
}
