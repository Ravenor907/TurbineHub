# The interface's parts

Everything in `app/qml` draws what the engine says. A page is one JSON object from `Hub.page(name, arg)`; a press is
`Hub.act(action, arg, value)`, which answers with a note, a page to open, or the one dialog. No file here decides
anything about the machine: filtering, sorting, counts, wording and what a button does all come from
`engine/src/ui`.

Every component takes `app`, Main.qml's window, for colors (`app.colors`), sizes (`app.px`, `app.fpx`, `app.tpx`),
fonts, icons (`app.icons`) and motion (`app.motion`).

## The window (Main.qml)

- **Sizes.** Drawn from a 1280 x 800 design, scaled to the window and by the interface size (50 to 150%).
  `px` for lengths, `fpx` for text (never below what can be read), `tpx` for touch targets (a finger's size even at
  50%). Font sizes are never bare numbers (tools/qml-check.js).
- **Colors.** Four themes (dark, black, light, high contrast) and four accents. Status colors come with a tinted
  fill and are never the only carrier of meaning: a chip has words and an icon too.
- **Pages.** Five tabs, and a stack of pages opened on top. `fileFor` picks the component for a page name.
  The first-run pages (pick, setup, quick) hide the tabs, and no tab opens before a mode is picked. A page opened
  on top keeps what it showed (the game, the section, the highlight) for when Back comes to it; the top bar lights
  the tab the page on screen belongs to (`tabOf`). Back on the first screen or on Home asks to close Turbine Hub.
- **The highlight.** One `focusItem` for controller, keyboard, mouse and touch. Anything with `focusable: true`
  can hold it. Directions move it to the nearest focusable on screen in that direction; `reveal` scrolls every
  list around it so the whole item shows (with its section heading on a section's first line, and the list's end
  after its last item). After a reload it returns to the item with the same `fid`.
- **Presses.** `press(k)` takes every input: the controller (polled from `Hub.poll`), the keyboard (arrows or
  W A S D, Enter, Esc, Q and E or Page Up and Page Down, X, Y, Tab, /, F1, F10), the mouse's Back button (B) and
  right button (X), and taps on the footer. Order: the dialog, the highlighted item, the page, then the window
  (tabs, Back, Help, Menu). While the highlight is being put back after a page or dialog change, presses wait for
  it; for a moment after a new page or dialog appears, A, X, Y and Menu are left out (no double presses). A key
  held down repeats only for the directions: Enter held is one A. Home and End jump to the first and last thing of
  the list; Ctrl with = or -, or with the wheel, changes the interface size (and asks to keep it).
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
| A tinted chip with an icon | A state (works, needs a fix, installed) | Chip |

## Components

| File | What it is |
|---|---|
| Focusable | Anything the highlight can land on: the ring, `press`, `activated`, mouse and touch. |
| ActionButton | The one button look, with the pressing button's cap inside. |
| KeyCap | A button's cap: the pad's colored face buttons, outlined caps for the rest and for keys. |
| Label | Plain words that wrap instead of being cut. |
| Icon | A line icon from path data on a 24 x 24 grid. |
| Chip | A small state label. |
| Toggle | A switch as drawn (the row around it takes the press). |
| RowItem | One engine row: toggle, check, slider, choice (as chips, or as cards with a color strip), button, info, or a live preview of the lights. |
| RowsView | Sections of rows as a scrolling list, in columns where they fit. |
| CardTile | A card in a grid: icon, title, state, a line, status and button. |
| PageHeader | A page's title with where it was opened from. |
| ScrollFade | Fades at the edges of a list that has more. |
| TopBar | Tabs between LB and RB, the mode, the machine's name. |
| BottomBar | The footer's hints; when narrow drops the page's extras first, then Help and Menu, never A or B. |
| Toast | A short note above the footer, or at the top when the highlight is down there. |
| DialogCard | The one dialog: title, words, the exact changes, password box, pinned buttons. Also the menu. |
| Motion | Every duration and curve. No other file names one. |
| Backdrop | What takes the mouse and the finger away from everything under it: the dialog's backdrop and card, the busy cover. The only MouseArea. |
| SelfCheck | The picture run's checks, and with `--check=taps`, `hold`, `hover`, `typing` or `mouse` the same page worked with real keys, a real mouse and a finger first. Loaded only with `--screenshot`. |

## Pages

| File | Pages |
|---|---|
| HomePage | Home, Guided and Veteran. |
| SidePage | Games, Store, Settings: a list on the left, its rows on the right. Games and Store have a search box above the list (RT, or / on a keyboard); every letter goes to the engine, which answers with the narrowed page. |
| CardsPage | My machine, Bring it over. |
| RowsPage | Every card's controls, the library check, a Store item's own page, Turbine Hub in Steam, a game's Proton, a game's saves, the Windows drive, Windows apps, Review changes, quick setup, Where's my...? |
| PickPage | Guided or Veteran. |
| SetupPage | Guided's five steps. |

## Pictures

`turbinehub --shots` lists the pictures; `turbinehub --screenshot=<png> --size=<W>x<H> <args>` takes one on a made-up
machine and runs SelfCheck. `tools/ci-shots.sh` takes them all at 1280x800, 1920x1080 and a 1600x900 Desktop Mode
window.
