import QtQuick

// Turbine Hub's colors, all of them, for one theme, one accent and one kind of color vision. Main.qml holds the one
// the window draws with (app.colors); the build's checks make one for every combination and measure it (SelfCheck).
//
// The rules it keeps, in every theme (dark, black, light, contrast), accent (brass, iron, copper, jade) and color
// vision mode:
//   words (text, soft, muted, accentText, ok, warn, stop) read at 4.5 to 1 or better on every surface (ground, bar,
//   panel, card, raised), a state's color on its own fill, and accentInk on the accent;
//   the three states (ok: fine, warn: needs a look, stop: won't work) stay apart for the eyes the mode is for: seen
//   the way red-weak, green-weak or blue-weak eyes see them they still differ plainly from each other. With "none"
//   they carry no color at all. In every mode a state also has its own mark and its own word (Chip.qml), so the
//   meaning never rests on color alone.
//
// themeName: "dark", "black", "light" or "contrast". accentName: "brass", "iron", "copper" or "jade". visionName:
// "typical", "red", "green", "blue" or "none". readable(color, ground): the color, moved away from the ground
// until it reads on it. contrast(a, b): how far apart two colors are, 1 to 21.
QtObject {
    id: c
    property string themeName: "black"
    property string accentName: "jade"
    property string visionName: "typical"

    readonly property bool hc: themeName === "contrast"
    readonly property bool light: themeName === "light"
    readonly property bool black: themeName === "black" || hc
    // Red-weak and green-weak eyes mix up the same colors, so they share a set; blue-weak eyes mix up others.
    readonly property bool redGreen: visionName === "red" || visionName === "green"
    readonly property bool blueWeak: visionName === "blue"
    readonly property bool noColor: visionName === "none"

    readonly property color ground: hc ? "#000000" : light ? "#F1EADB" : black ? "#000000" : "#0B0A08"
    readonly property color bar: hc ? "#000000" : light ? "#E6DCC6" : black ? "#000000" : "#080706"
    readonly property color panel: hc ? "#000000" : light ? "#E7DDC8" : black ? "#0C0B0A" : "#141109"
    readonly property color card: hc ? "#000000" : light ? "#F7F2E7" : black ? "#080707" : "#100E0A"
    readonly property color raised: hc ? "#1A1A1A" : light ? "#DDD1B8" : black ? "#161412" : "#1E1A10"
    readonly property color line: hc ? "#FFFFFF" : light ? "#CDBF9F" : black ? "#26221C" : "#2A2418"
    readonly property color text: hc ? "#FFFFFF" : light ? "#2A2216" : "#EADFC6"
    readonly property color soft: hc ? "#FFFFFF" : light ? "#4A3E2A" : "#D9C9A6"
    readonly property color muted: hc ? "#E0E0E0" : light ? "#5E5038" : "#AC9873"
    readonly property var swatches: ({ brass: "#C99E52", iron: "#A9B4BE", copper: "#B07A5A", jade: "#6FAE8C" })
    // With no color, the accent is a plain grey.
    readonly property color accentBase: noColor ? "#BDBDBD" : (swatches[accentName] || swatches.jade)
    readonly property color accent: hc ? (noColor ? "#E6E6E6" : "#FFD54A") : light ? Qt.darker(accentBase, 1.8) : accentBase
    readonly property color accentHi: hc ? (noColor ? "#FFFFFF" : "#FFE680") : light ? Qt.darker(accentBase, 1.6) : Qt.lighter(accentBase, 1.22)
    readonly property color accentLo: light ? Qt.darker(accentBase, 2.1) : Qt.darker(accentBase, 1.35)
    // What is written on an accent fill. (Not named "on...": QML takes such a name for a signal handler and never
    // gives the property its value.)
    readonly property color accentInk: light ? "#FFFFFF" : "#0B0A08"
    // Accent-colored words.
    readonly property color accentText: hc ? (noColor ? "#FFFFFF" : "#FFD54A") : light ? Qt.darker(accentBase, 2.2) : Qt.lighter(accentBase, 1.12)
    readonly property color focusFill: hc ? (noColor ? "#333333" : "#333300") : light ? Qt.lighter(accentBase, 1.55) : Qt.darker(accentBase, 4.2)
    // The highlight ring: as plain on the light ground as on the dark ones.
    readonly property color ring: hc ? (noColor ? "#FFFFFF" : "#FFE680") : light ? Qt.darker(accentBase, 2.6) : Qt.lighter(accentBase, 1.22)

    // The three states. Typical: green, amber, red. Red-weak and green-weak: blue, yellow and a red that is darker
    // than the yellow (on the light theme: blue, olive and a red-violet). Blue-weak: green, a light yellow and a
    // strong red. No color: the words' own colors. The mark and the word beside them always carry the meaning too.
    readonly property color ok: noColor ? soft
        : redGreen ? (hc ? "#8CC8FF" : light ? "#1F4E8C" : "#7FB8F0")
        : blueWeak ? (hc ? "#9FE8B5" : light ? "#2F5A3C" : "#8FD8A8")
        : (hc ? "#9FE8B5" : light ? "#2F5A3C" : "#9FD3B5")
    readonly property color warn: noColor ? text
        : redGreen ? (hc ? "#FFE680" : light ? "#5F4A00" : "#F0DC6A")
        : blueWeak ? (hc ? "#FFE080" : light ? "#6A5200" : "#F0D878")
        : (hc ? "#FFD080" : light ? "#7A4A10" : "#E6B36A")
    readonly property color stop: noColor ? text
        : redGreen ? (hc ? "#FF8A80" : light ? "#8E1A5A" : "#F0806A")
        : blueWeak ? (hc ? "#FF8A80" : light ? "#9E1A1A" : "#FF7A70")
        : (hc ? "#FF8A80" : light ? "#8E2A1E" : "#F0907A")
    readonly property color okFill: noColor ? raised
        : redGreen ? (hc ? "#00182E" : light ? "#D3DDEE" : "#101A26")
        : (hc ? "#002A10" : light ? "#D5E6D2" : "#13201A")
    readonly property color warnFill: noColor ? raised : (hc ? "#2A1A00" : light ? "#EEDDB8" : "#241B0C")
    readonly property color stopFill: noColor ? raised
        : redGreen && light ? "#EFD2E0"
        : (hc ? "#2A0500" : light ? "#F0D2C8" : "#26110D")
    readonly property color scrim: Qt.rgba(ground.r * 0.5, ground.g * 0.5, ground.b * 0.5, light ? 0.8 : 0.86)
    // The controller's face buttons, as they are colored on the pad (each carries its letter; with no color they
    // are greys).
    readonly property var padColors: ({ a: "#6FAE8C", b: "#E6A98A", x: "#7FA7D9", y: "#E0C060" })
    readonly property var padGreys: ({ a: "#C8C8C8", b: "#B4B4B4", x: "#A8A8A8", y: "#D2D2D2" })
    readonly property var pad: noColor ? padGreys : padColors
    // What is written on a face button.
    readonly property color padInk: "#0B0A08"
    // A face button's color as a color (for measuring).
    function padOf(k) { return Qt.darker(pad[k], 1.0) }
    function tone(name) { return name === "ok" ? ok : name === "warn" ? warn : name === "stop" ? stop : muted }
    function toneFill(name) { return name === "ok" ? okFill : name === "warn" ? warnFill : name === "stop" ? stopFill : raised }

    // ---------------------------------------------------------------- contrast
    // How bright a color is to the eye, 0 to 1, and how far apart two colors are (1: the same, 21: black on white).
    // Words need 4.5 or more.
    readonly property real enough: 4.5
    function lum(col) {
        const f = v => v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        return 0.2126 * f(col.r) + 0.7152 * f(col.g) + 0.0722 * f(col.b)
    }
    function contrast(a, b) {
        const la = lum(a), lb = lum(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }
    // The same hue, lightened on a dark ground or darkened on a light one until it reads on it.
    function readable(col, bg) {
        let out = col
        const dark = lum(bg) < 0.18
        for (let i = 0; i < 16 && contrast(out, bg) < enough; i++) out = dark ? Qt.lighter(out, 1.12) : Qt.darker(out, 1.12)
        return out
    }
    // One color drawn over another at its own strength.
    function over(top, under) {
        const a = top.a
        return Qt.rgba(top.r * a + under.r * (1 - a), top.g * a + under.g * (1 - a), top.b * a + under.b * (1 - a), 1)
    }
}
