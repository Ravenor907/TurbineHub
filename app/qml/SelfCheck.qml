import QtQuick
import TurbineHub

// The build's checks on one page, loaded only in screenshot runs (Main.qml, --screenshot). After the page settles
// it walks everything on screen and fails the run when:
//   words are cut off (elided, or wider or taller than the box that clips them);
//   words or a control stick out of the window, or sideways out of the list they scroll in;
//   words are smaller than the smallest size the design allows;
//   words are squeezed into a column so narrow that each line holds about one word;
//   words are drawn over other words (except what is drawn over the page on purpose: `cover: true`);
//   the footer lacks B, lacks A where the highlighted thing takes A, offers A where it doesn't, or shows a hint
//   with no words;
//   nothing holds the highlight, or the highlighted thing isn't fully in view.
// Then it saves the picture, prints one line per finding ("SELFCHECK FAIL: ...") or "SELFCHECK ok", and ends the
// run with 0 or 1.
//
// app (required): Main.qml's window. target (required): where the picture goes.
Item {
    id: check
    required property var app
    required property string target
    property var problems: []

    function start() { settle.start() }
    Timer { id: settle; interval: check.app.motion.settle; onTriggered: check.run() }

    function fail(text) { if (problems.indexOf(text) < 0 && problems.length < 25) problems.push(text) }
    function words(it) { const t = String(it.text); return "\"" + (t.length > 40 ? t.substring(0, 40) + "..." : t) + "\"" }

    // The nearest thing above `it` that clips what it draws: { item, scrolls }.
    function clipper(it) {
        let p = it.parent
        while (p) {
            if (p.clip === true) return { item: p, scrolls: p.contentY !== undefined }
            p = p.parent
        }
        return null
    }

    function look(it, counts) {
        if (!it || it.visible === false || it.opacity === 0) return
        const isText = it.truncated !== undefined && it.text !== undefined && it.font !== undefined
        if (isText && String(it.text) !== "" && it.width > 0) {
            counts.texts++
            const at = it.mapToItem(null, 0, 0)
            const w = Math.min(it.width, it.contentWidth > 0 ? it.contentWidth : it.width)
            if (it.truncated) fail("cut off: " + words(it))
            if (it.wrapMode === Text.NoWrap && it.contentWidth > it.width + 1 && it.width > 0 && it.elide === Text.ElideNone && it.fontSizeMode === Text.FixedSize) fail("wider than its box: " + words(it))
            if (it.font.pixelSize < Math.floor(13 * app.unit) && it.font.pixelSize < 13) fail("too small (" + it.font.pixelSize + " px): " + words(it))
            // Words squeezed into a column so narrow that each line holds about one word.
            const count = String(it.text).trim().split(/\s+/).length
            if (it.wrapMode !== Text.NoWrap && it.lineCount >= 3 && count >= 3 && it.lineCount >= count * 0.8) fail("squeezed to a word a line: " + words(it))
            const c = clipper(it)
            // Where its words are painted, cut to what clips it, for the overlap check.
            if (!counts.covered) {
                const cw = Math.min(it.width, it.contentWidth), ch = Math.min(it.height, it.contentHeight)
                let x0 = at.x + (it.horizontalAlignment === Text.AlignHCenter ? (it.width - cw) / 2 : it.horizontalAlignment === Text.AlignRight ? it.width - cw : 0)
                let y0 = at.y + (it.verticalAlignment === Text.AlignVCenter ? (it.height - ch) / 2 : it.verticalAlignment === Text.AlignBottom ? it.height - ch : 0)
                let x1 = x0 + cw, y1 = y0 + ch
                for (let p = it.parent; p; p = p.parent) {
                    if (p.clip !== true) continue
                    const b = p.mapToItem(null, 0, 0)
                    x0 = Math.max(x0, b.x); y0 = Math.max(y0, b.y); x1 = Math.min(x1, b.x + p.width); y1 = Math.min(y1, b.y + p.height)
                }
                if (x1 - x0 > 2 && y1 - y0 > 2) counts.boxes.push({ it: it, x0: x0, y0: y0, x1: x1, y1: y1 })
            }
            if (c) {
                const box = c.item.mapToItem(null, 0, 0)
                if (at.x < box.x - 1 || at.x + w > box.x + c.item.width + 1) fail("sticks out sideways of its list: " + words(it) + " at " + Math.round(at.x) + " w " + Math.round(w) + " box " + Math.round(box.x) + "+" + Math.round(c.item.width))
                if (!c.scrolls && (at.y < box.y - 1 || at.y + it.height > box.y + c.item.height + 1)) fail("clipped: " + words(it))
            } else if (at.x < -1 || at.x + w > app.width + 1 || at.y < -1 || at.y + it.height > app.height + 1) fail("outside the window: " + words(it) + " at " + Math.round(at.x) + "," + Math.round(at.y) + " " + Math.round(w) + "x" + Math.round(it.height))
        }
        if (it.focusable === true) {
            counts.controls++
            const at = it.mapToItem(null, 0, 0)
            if (at.x < -1 || at.x + it.width > app.width + 1) fail("a control sticks out of the window (" + (it.fid || "?") + ")")
            if (it.width < 8 || it.height < 8) fail("a control has no size (" + (it.fid || "?") + ")")
        }
        // Things drawn over the page on purpose (a toast, a pinned heading) aren't overlaps.
        const was = counts.covered
        if (it.cover === true) counts.covered = true
        const kids = it.children
        for (let i = 0; i < kids.length; i++) look(kids[i], counts)
        counts.covered = was
    }
    // Words drawn over other words.
    function overlaps(boxes) {
        for (let i = 0; i < boxes.length; i++) {
            for (let j = i + 1; j < boxes.length; j++) {
                const a = boxes[i], b = boxes[j]
                const w = Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0), h = Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0)
                if (w > 4 && h > 4) fail("words over words: " + words(a.it) + " and " + words(b.it))
            }
        }
    }

    // The picture is taken first: taking it lays everything out and draws it, so the checks measure exactly what
    // the picture shows.
    function run() {
        const ok = app.contentItem.grabToImage(function (result) {
            const counts = check.measure()
            if (!result.saveToFile(check.target)) check.fail("the picture couldn't be saved to " + check.target)
            check.finish(counts)
        })
        if (!ok) { fail("the picture couldn't be taken"); finish({ texts: 0, controls: 0 }) }
    }

    function measure() {
        const counts = { texts: 0, controls: 0, boxes: [], covered: false }
        look(app.contentItem, counts)
        // With a dialog open, the page behind it is under the backdrop: only the dialog's own words count.
        overlaps(app.parts.dialog.open ? counts.boxes.filter(b => { for (let p = b.it; p; p = p.parent) if (p === app.parts.dialog) return true; return false }) : counts.boxes)
        const keys = app.parts.bottomBar.shownKeys
        const hints = app.parts.bottomBar.hints
        // A shows wherever the highlighted thing takes A, and only there.
        const takesA = app.focusItem && app.focusItem.aLabel
        if (takesA && !(app.parts.dialog.open && app.focusItem.fid === "cancel") && keys.indexOf("a") < 0) fail("the footer has no A hint")
        if (!takesA && keys.indexOf("a") >= 0) fail("the footer offers A where A does nothing")
        if (keys.indexOf("b") < 0) fail("the footer has no B hint")
        for (const h of hints) if (!h.label && h.k !== "a") fail("the footer's " + h.k + " hint has no words")
        if (counts.controls > 0) {
            const f = app.focusItem
            if (!f) fail("nothing holds the highlight")
            else {
                const at = f.mapToItem(null, 0, 0)
                if (at.y < -1 || at.y + f.height > app.height - app.parts.bottomBar.height + 1) fail("the highlighted control isn't fully in view (" + (f.fid || "?") + ")")
            }
        }
        if (counts.texts === 0) fail("the page is empty")
        return counts
    }

    function finish(counts) {
        for (const p of problems) Hub.say("SELFCHECK FAIL: " + p)
        if (problems.length === 0) Hub.say("SELFCHECK ok: " + counts.texts + " texts, " + counts.controls + " controls")
        Qt.exit(problems.length === 0 ? 0 : 1)
    }
}
