import QtQuick

// How Turbine Hub moves, in one place. Every animation in app/qml takes its time and its curve from here: no other
// file writes a duration in milliseconds or names an easing curve (tools/qml-check.js fails the build on one).
//
// The level is the player's (Settings, Look, Animations):
//   full     fades, glides and small scales;
//   reduced  fades only: nothing travels, slides or grows (for people whom movement on screen makes unwell);
//   off      instant: nothing animates at all (pictures are taken this way, so they never race an animation).
//
// So every animation says which kind it is by the time it takes: a fade (opacity, a color) takes `quick`,
// `standard` or `gentle` and runs while `on`; anything that changes a place or a size (a list gliding to the
// highlight, a switch's knob, a dialog growing into place) takes `moveQuick` or `moveStandard` and runs while
// `moves`. At Reduced and Off those are 0 and the thing is simply there.
//
// The language: quick for the highlight, a press and words that change in place; standard for dialogs, toasts and
// lists that scroll to the highlight; gentle for a whole page. Things fade and settle on OutCubic; nothing bounces.
// An animation only ever follows a state that has already changed, so a press during one acts at once and the
// page ends where it would have without it (the build's motion-... pictures hold it to that).
QtObject {
    id: motion
    property string level: "full"
    readonly property bool on: level !== "off"
    // Place and size change over time only at Full; Reduced keeps the fades.
    readonly property bool moves: level === "full"

    // Fades (Full and Reduced).
    readonly property int quick: on ? 120 : 0
    readonly property int standard: on ? 200 : 0
    readonly property int gentle: on ? 280 : 0
    // Travel: glides, slides and scales (Full only).
    readonly property int moveQuick: moves ? 120 : 0
    readonly property int moveStandard: moves ? 200 : 0
    readonly property int ease: Easing.OutCubic

    // A page or words changing in place fade up from this, never from nothing.
    readonly property real swapFrom: 0.35
    // What a dialog grows from as it fades in, and what a button shrinks to while pressed.
    readonly property real popScale: moves ? 0.96 : 1
    readonly property real pressScale: moves ? 0.97 : 1

    // Not transitions, so they run at every level: how long a toast stays, the busy mark's turn, and how a held
    // direction repeats.
    readonly property int toastStay: 3200
    readonly property int spin: 1400
    readonly property int holdDelay: 380
    readonly property int holdRepeat: 110
    // How long a page or dialog that just appeared leaves out A, X, Y and Menu (a double press stays on the first).
    readonly property int pressGuard: 250
    // How long after a key, a controller press or a rebuild the mouse resting on something isn't the mouse being used.
    readonly property int hoverQuiet: 400
    // How long a picture waits for the page to settle before the checks run; with animations on, how often it
    // looks whether anything still moves, and how long it gives them to end.
    readonly property int settle: 350
    readonly property int settleLook: 80
    readonly property int settleLimit: 4000
    // The time between two scripted presses in a picture run.
    readonly property int scriptStep: 60
}
