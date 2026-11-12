# Hands-on checklist

Things only a real machine can show. Each step says what to do and what you should see. Note anything that looks
or feels off; that's the list to tweak from.

## Before you start
- Turbine Hub 0.1 is installed for your user. To update it later: `cd ~/Projects/TurbineHub && sh tools/dev-build.sh --pull`.
- Have a controller connected. Your SteamOS password is asked once, by the first system change (section 2), never before.

## 1. First start (Desktop Mode)
1. Open the app launcher, find **Turbine Hub**, open it.
   - You see the mode picker: Guided on the left, Veteran ("I know more than you.") on the right.
2. Pick **Guided**. Walk through Your machine, Your games, Bring it over and Picks (four steps, none asks for a password).
   - Your machine says Desktop PC, your CPU, the RX 9070 XT and SteamOS 3.9.5.
3. In Picks, the picks that change system settings carry a **Needs your password** mark. Untick those, leave
   **Turbine Hub in your Game Mode library** and the apps ticked, and apply. Quit Steam first if the step says so.
   - The dialog asks for no password.
   - Afterwards Turbine Hub is under Non-Steam in your library, with its own art.

## 2. The helper (your password, once)
1. Make any system change, for example My machine, Tune-up, **Huge pages**.
   - The dialog's first lines say why the password is asked now (a system setting, like the admin prompt on
     Windows), then it lists exactly what changes and asks for your password once.
   - On a machine that has no password yet, the same dialog has two boxes to make one, and says SteamOS can't
     recover it if it is forgotten.
2. Enter it.
   - The change is made. Settings, **Review changes** lists it in plain words.
3. Press **Undo** on it.
   - It's gone from the list and your system is back as it was.
4. Make a second change.
   - No password this time.

## 3. Game Mode
1. Switch to Game Mode and open Turbine Hub from your library.
2. Move around with the controller only: LB/RB for tabs, D-pad or left stick, A, B, X (Options) on a game, a Store row and a card, Y (Find) in Games and Store, View (the menu), Start on a page with a main button, Start held on a game (Play), LT and RT in a long list.
   - Nothing opens a new window. The highlight is always fully on screen. The hints at the bottom match the buttons.
   - Options always starts with what A does, and ends with Help.
3. Move the right stick: a pointer shows and the highlight follows it. R3 clicks, R3 held (a ring fills) opens Options for what is under it. Try it on a row, on a dialog's button and on a slider (RT held drags). Sweep the left stick round its rim: the list scrolls.
   - The pointer hides after a few seconds and as soon as the D-pad is used. Nothing under a dialog takes its click.
3. Settings, Look: try interface size 50% and 150%, and the themes.
   - Layouts re-flow; no text is cut off.

## 4. My machine
- **Monitors**: your AW2726DM, up to 240 Hz.
- **Drives**: the empty Samsung 990 PRO is "not set up"; the steps lead to Steam's Storage settings.
- **Sleep and wake**: your two wireless receivers can wake the PC. Block one, sleep the PC, check it stays asleep. Undo.
- **Graphics card**: power limit with a safe reset (try a change and Undo).

## 5. Background helper
1. Settings, General, **Background helper**: turn it on (no password).
2. Turn on **Game Boost**. Start a game, play a minute, quit.
   - Home's activity shows Game Boost on for that game, then off.
3. Turn on **Save time machine** (needs Ludusavi from the Store) and start a game.
   - Games, that game, Saves lists a snapshot taken at launch.

## 6. Games
1. Games, a Proton game: **Set up** the Turbine Hub tool. Restart Steam once.
2. In Steam: the game's gear, Properties, Compatibility, force a tool, pick **Turbine Hub**.
3. Back in Turbine Hub turn on **FSR 4 upgrade** for it, then start the game.
   - The game starts normally. (With a game that uses FSR 3.1, the image gets sharper.)
4. **Try another Proton**: pick one, start the game, Undo.
5. **Will my games run?** lists your games with Works, Needs a fix or Won't run here.

## 7. Store
- Install a small app (VLC), open its page, remove it.
- Decky plugins: install Decky Loader from here (it comes from Decky's official release, checked by checksum).
- The Turbine Hub quick panel needs a published release first (see the summary); then follow its steps.

## 8. Offline
- Turn off networking and open Games and Store.
  - They show the last data with its age ("Checked ... ago, offline now"), never an error page.

## 9. Undo everything
- Settings, Review changes, **Undo all**.
  - The list empties and the machine is as it was before Turbine Hub changed anything.
