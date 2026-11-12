# The interface's parts

Everything in `app/qml` draws what the engine says. A page is one JSON object from `Hub.page(name, arg)`; a press is
`Hub.act(action, arg, value)`, which answers with a note, a page to open, or the one dialog. No file here decides
anything about the machine: filtering, sorting, counts, wording and what a button does all come from
`engine/src/ui`.

Every component takes `app`, Main.qml's window, for colors (`app.colors`), sizes (`app.px`, `app.fpx`, `app.tpx`,
`app.ipx`, `app.bpx`), fonts, icons (`app.icons`) and motion (`app.motion`).

## The window (Main.qml)

- **Sizes.** Drawn from a 1280 x 800 design, scaled to the window and by the interface size (50 to 150%).
  `px` for lengths; `fpx` for text (never under 12 pixels nor under nine tenths of its design size, so 50% and a
  small window stay readable and boxes re-flow around their words); `tpx` for anything to press (never under
  `touchFloor`: 44 pixels, or 6 mm on a dense screen); `ipx` for icons (never under three quarters); `bpx(v, text)`
  for a box that holds one line of words (its design size or the room the words need). The two bars keep a
  finger's height. Font sizes always go through `fpx` (tools/qml-check.js).
- **Colors.** Palette.qml holds every color for a theme (dark, black, light, high contrast), an accent (brass,
  iron, copper, jade) and a kind of color vision (typical, red-weak, green-weak, blue-weak, no color); Main.qml's
  is `app.colors`. Words read at 4.5 to 1 on every surface in all 80 combinations, and the three states (ok, warn,
  stop) stay apart for the eyes the mode is for (red-weak and green-weak: blue, yellow and a darker red; no color:
  none at all). A state is never color alone: a chip has its mark and its word. `app.colors.readable(color, on)`
  moves a color until it reads on a ground. No other file writes a color (tools/qml-check.js); a property is never
  named `on...` (QML takes that for a signal handler and the value is lost: the text on an accent fill was black on
  the light theme for that reason, and is `accentInk` now).
- **Animations.** `app.motion.level` is the player's: full, reduced (fades only: nothing travels, slides or grows)
  or off. A fade runs while `motion.on` and takes `quick`, `standard` or `gentle`; anything that changes a place or
  a size runs while `motion.moves` and takes `moveQuick` or `moveStandard` (tools/qml-check.js holds both).
- **For a screen reader.** Everything the highlight lands on says its kind, its name with its state in words, more
  about it, and that the highlight is on it (`sayRole`, `sayName`, `sayMore` on Focusable); tabs, footer hints,
  the dialog, the toast and the busy cover speak too; icons are skipped. Orca in Desktop Mode reads them.
- **Files in a window.** `app.dense`: Files in a Desktop Mode window is drawn at one size whatever the window's
  (`fit` is 1: a bigger monitor shows more files, not bigger words; the interface size still scales it) with a
  mouse's floor for things to press (28 pixels in place of a finger's 44). The top bar gives way to Files' own strip
  of tabs, and the footer shows only once a controller is used. Everywhere else, and Files full screen, sizes are
  as above.
- **Pages.** Six tabs, and a stack of pages opened on top. `fileFor` picks the component for a page name. A link to one part of the machine (`module:<id>`) is My machine on that section: a change of section from My machine itself, a page on top from anywhere else. A page can say where its highlight starts (`focus`).
  The first-run pages (pick, setup, quick) hide the tabs, and no tab opens before a mode is picked. A page opened
  on top keeps what it showed (the game, the section, the highlight) for when Back comes to it; the top bar lights
  the tab the page on screen belongs to (`tabOf`). Back on the first screen or on Home asks to close Turbine Hub.
- **The highlight.** One `focusItem` for controller, keyboard, mouse and touch. Anything with `focusable: true`
  can hold it. Directions move it to the nearest focusable on screen in that direction; `reveal` scrolls every
  list around it so the whole item shows (with its section heading on a section's first line, and the list's end
  after its last item). After a reload it returns to the item with the same `fid`.
- **Controls.** One scheme, Turbine Mod Manager's (docs/DESIGN.md, Controls; the table is `SCHEME` in
  engine/src/ui/controls.rs). `press(k)` takes every input: the controller (polled from `Hub.poll`), the keyboard
  (arrows or W A S D, Enter or Space, Esc, R for Options, F for Find, Q and E, Tab for the menu, Ctrl+Enter for
  Start, F1 for Help), the mouse's Back button (B) and right button (Options for what it is on), and taps on the
  footer. Order: the dialog, the highlighted item, the page, then the window (tabs, Back, Options, Find, the menu,
  Start). While the highlight is being put back after a page or dialog change, presses wait for it; for a moment
  after a new page or dialog appears, A, X, Y and Start are left out (no double presses). A key held down repeats
  only for the directions: Enter held is one A. LT and RT jump a screenful (`jumpBy`; on a page with sections see Sections below), Home and End go to the first
  and last thing of the list; Ctrl with = or -, or with the wheel, changes the interface size (and asks to keep
  it). In a dialog Tab and Shift+Tab go from one thing to the next (`tabMove`).
- **Work that runs while the player browses.** `app.status` is the engine's status for the top bar (the line of
  work, the battery, the clock, the updates). A confirmed dialog that joins the line answers with a note and the
  page again; nothing is covered. `Hub.poll` hands over each job's end (`jobNews`: the note, and the page read
  again, or once the dialog over it has closed: `pageStale`, `freshen`). Closing asks while the line isn't empty
  (`askClose` with `status.closing`, also from the window's own close button) and `quit()` ends the line first
  (`Hub.stopWork`). The busy cover is only for work the engine says must run alone.
- **Sections.** LT and RT (`sprev` and `snext` for [ and ]) first ask the page (`section(d)` on SectionsPage); where
  it has none, LT and RT jump a screenful and [ and ] turn the tabs.
- **What's new.** `whatsNew()` asks the engine once the window is free (at the start, when the notes arrive, when a
  dialog closes); the engine answers with the screen only when it is due.
- **Options.** `openOptions(item)` builds the menu for one thing and shows it in the dialog: the thing's own press
  first (its `aLabel`, carried out as A is), then its `offers` (from the engine: what the page it opens can do),
  then the page's `options` (its own actions, its main button, Find, Help). An entry said twice is said once. No
  page handles X or Y itself: pages only show `model.hints` and draw the options marked `button` (PageButtons).
- **Find.** `find(text)` asks the page (`find` on SidePage and RowsPage): the search box takes the highlight and
  the keyboard. A letter or digit with no meaning of its own, typed on such a page, starts it with that letter.
- **Holds.** R3 held (Options), Start held and Space held (Play, on a page with `hold`) go through `holdBegin`,
  `holdEnd` and `holdDone`; PointerLayer draws the ring that fills meanwhile. Let go early, each is its tap.
- **The pointer.** `Hub.poll` says where the right stick has moved it (`ptr`); `pointerNews` turns that into mouse
  events sent into the window (`Hub.pointerEvent`), so hovering, clicking, the gate, lists and dialogs treat it as
  a mouse. While one of its events is delivered `ptrActing` is true: the last input stays the controller's and a
  move counts at once. R3 is `r3()`: a click, or the right button when held; RT and LT are the two buttons while
  it shows. A key or the real mouse sends it away (`pointerAway`); it rests while a text box has the keyboard in
  Game Mode and while work runs. The left stick's wheel goes to the pointer, or to the highlight when it is hidden.
- **Only in front.** The controller is read behind other windows too, but pad.rs passes on releases only while
  another window is in front, and for a quarter of a second after coming back (`onActiveChanged`, `Hub.front`).
  A held direction stops repeating when the window goes back, the page changes or the controller goes away.
- **Taps and the mouse.** Every tap asks `gate()` first, so it obeys what a press obeys: nothing while work runs,
  while the highlight is being put back, or in the moment a page or dialog appears, and nothing of the page under
  a dialog. The page and the bars are also turned off (`enabled`) under a dialog and the busy cover, because a
  backdrop stops the items under it but not their tap and hover handlers; `Backdrop` takes what is left and is the
  only MouseArea. The mouse moves the highlight only when it really moved (`hoverAt`): what a resting mouse
  lies on after a key, a controller press or a rebuild is left alone. tools/qml-check.js holds these rules.
- **Typing.** A text box that gets the keyboard reports it (`typingBegan`, `typingEnded`). In Game Mode Steam's
  keyboard opens with it and closes when the typing is done; until then the controller belongs to that keyboard,
  and B only ends the typing ("Done typing" in the footer).
- **Glyphs.** The hints follow the last input (`lastInput`): the controller's after a press on it, keys after a key
  or the mouse. Always the controller's in Game Mode; a window starts on the controller's when one is seen.
- **Footer.** `footerHints` lists what each button does now, most important first: A only where A does something,
  B as Back, the page's own words for it, or Close at the very start.

## Looks that mean something

| Look | Means | Drawn by |
|---|---|---|
| A ring outside the item | The highlight is here | Focusable |
| An accent fill | The main action of a screen or dialog ("primary") | ActionButton |
| An accent bar or a filled option with a check | The entry or option in effect ("selected") | SidePage, RowItem |
| A tinted chip with a mark (tick, warning sign, cross) and a word | A state (works, needs a fix, won't run) | Chip, the library check's counts |

## Components

| File | What it is |
|---|---|
| Focusable | Anything the highlight can land on: the ring, `press`, `activated`, mouse and touch, and what a screen reader says (`sayRole`, `sayName`, `sayMore`). |
| Palette | Every color for a theme, an accent and a kind of color vision, with `contrast` and `readable`. |
| ActionButton | The one button look, with the pressing button's cap inside. |
| KeyCap | A button's cap: the pad's colored face buttons, outlined caps for the rest and for keys. |
| Label | Plain words that wrap instead of being cut. |
| Icon | A line icon from path data on a 24 x 24 grid (skipped by a screen reader). |
| Chip | A small state label: the tone's color on its fill, its mark and its word. |
| Toggle | A switch as drawn (the row around it takes the press). |
| RowItem | One engine row: toggle, check, slider, choice (as chips, or as cards with a color strip), button, info, a chart (a line over time with its scale's and its time's ends named), or a live preview of the lights. |
| RowsView | Sections of rows as a scrolling list, in columns where they fit. |
| CardTile | A card in a grid: icon, title, state, a line, status and button, or the name of its switch and the switch (A turns it). |
| Sparkline | One line over time on a scale, a gap where nothing was read: the battery tile on Home, the charts of My machine, Battery. A picture only; the words beside it say what it shows. |
| PageHeader | A page's title with where it was opened from. |
| ScrollFade | Fades at the edges of a list that has more. |
| TopBar | Tabs between LB and RB, then the status from `app.status` (the engine's): the work that runs while the player browses (a mark that turns, its words, how far it is), the updates mark, the battery with its percent and charging bolt, the clock, the mode, the machine's name. `fitted` works out what fits beside the tabs; the name goes first, then the clock, the long words, the percent. Nothing in it takes the highlight. |
| BottomBar | The footer's hints; when narrow drops the pointer's click and Find first, then Menu, Options and the main button, never A or B. |
| PageButtons | The page's own actions as buttons (the options the engine marks `button`): under a list, beside a header. |
| PointerLayer | The controller's pointer (an arrow drawn by Turbine Hub, its tip where R3 clicks) and the ring of a held button. Takes no input. |
| Toast | A short note above the footer, or at the top when the highlight is down there. |
| DialogCard | The one dialog: title, words, the exact changes, password box, pinned buttons. Also the menu and every Options menu (rows, each with its button where it has one). For Files: `strip` (a row of icons across the top of a menu), `input` (one line to type) and `then` (the press its confirm button makes). |
| Motion | Every duration and curve, at the player's level (full, reduced, off). No other file names one. |
| Backdrop | What takes the mouse and the finger away from everything under it: the dialog's backdrop and card, the busy cover. The only MouseArea. |
| SelfCheck | The picture run's checks: layout, text size, contrast of the words as drawn, finger sizes, state chips, names for a screen reader (also asked from Qt's accessibility tree), all 80 palettes, and with `--motion=full` or `reduced` that everything comes to rest. With `--check=taps`, `hold`, `hover`, `typing` or `mouse` the same page is worked with real keys, a real mouse and a finger first. Loaded only with `--screenshot`. |

## Pages

| File | Pages |
|---|---|
| HomePage | Home, Guided and Veteran: readings (Sparkline on the battery's), the quick switches as RowItem, the checkup, what passed, and the main button (Start) that fixes what it can. |
| SectionsPage | The Store and My machine: the sections as chips across the top (`model.chips`, `model.index`), the picked section's rows under them, the search box and the main button beside the section's line. `section(d)` goes to the section before or the next (LT and RT, [ and ]); the rows come first in the page's order, so the highlight starts on the first row (or where the engine's `focus` says). With `cards` in the model (My machine's Overview) the section is a grid of CardTile; with `backTo`, B on another section goes to that one before it leaves the tab. |
| SidePage | Games, Settings: a list on the left, its rows on the right. Games has a search box above the list (Find: Y, or F on a keyboard); every letter goes to the engine, which answers with the narrowed page. The page's own actions are buttons under the list. |
| CardsPage | Bring it over. |
| FilesPage | Files: Explorer's layout (tabs, back, forward, up, address, search, command bar, navigation pane, list or tiles, details pane, status bar). It keeps what the hand is doing (what is ticked, the line the highlight was on, a name being typed, a drag) and asks the engine for everything else. The list is a ListView: lines out of view aren't drawn, so Up and Down go by index (`move`), and a line that scrolls away leaves the highlight with the list (`list-park`). `key(e)` has Explorer's keys, `local()` the Options entries the page carries out itself. With FileCell (a line, a large icon, a short line or a tile), FileTool (a button of its bars) and CutText (words cut where they don't fit). |
| RowsPage | Every card's controls, the library check, a Store item's own page, Turbine Hub in Steam, a game's Proton, a game's saves, the Windows drive, Windows apps, Review changes, quick setup, Where's my...? |
| PickPage | Guided or Veteran. |
| SetupPage | Guided's four steps. |

## Pictures

`turbinehub --shots` lists the pictures; `turbinehub --screenshot=<png> --size=<W>x<H> <args>` takes one on a made-up
machine and runs SelfCheck. `tools/ci-shots.sh` takes them all at 1280x800, 1920x1080 and a 1600x900 Desktop Mode
window. `--theme=`, `--accent=`, `--vision=` and `--scale=` pick the look; `--finger=<pixels>` stands in for a dense
screen's finger size; `--motion=full` or `reduced` leaves
animations on (pictures are otherwise taken with them off); `--pointer=<x>,<y>` shows the controller's pointer there
(as parts of the window); `--half` takes it half as wide (a window snapped to half the screen);
`--check=taps|hold|hover|mouse|typing|pointer|options|sections|store-row|queue|files-mouse|files-keys|files-pad` works the page with real keys, a
mouse, a finger and fed sticks first; `--spoken` prints what a screen reader is told. A
picture named `motion-...` must end on the very picture the same presses give with animations off.
