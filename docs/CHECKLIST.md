# Hands-on checklist

Things only a real machine can show. Each step says what to do and what you should see. Note anything that looks
or feels off; that's the list to tweak from.

## Before you start
- Turbine Hub 0.1 is installed for your user. To update it later: `cd ~/Projects/TurbineHub && sh tools/dev-build.sh --pull`.
- Have a controller connected, and your SteamOS password ready (asked once).

## 1. First start (Desktop Mode)
1. Open the app launcher, find **Turbine Hub**, open it.
   - You see the mode picker: Guided on the left, Veteran ("I know more than you.") on the right.
2. Pick **Guided**. Walk through Your machine, Password, Your games, Bring it over and Picks.
   - Your machine says Desktop PC, your CPU, the RX 9070 XT and SteamOS 3.9.5.
3. In Picks, leave **Turbine Hub in your Game Mode library** ticked. Quit Steam first if the step says so.
   - Afterwards Turbine Hub is under Non-Steam in your library, with its own art.

## 2. The helper (your password, once)
1. Make any system change, for example My machine, Tune-up, **Huge pages**.
   - The dialog lists exactly what changes and asks for your password once.
2. Enter it.
   - The change is made. Settings, **Review changes** lists it in plain words.
3. Press **Undo** on it.
   - It's gone from the list and your system is back as it was.
4. Make a second change.
   - No password this time.

## 3. Game Mode
1. Switch to Game Mode and open Turbine Hub from your library.
2. Move around with the controller only: LB/RB for tabs, D-pad, A, B, VIEW (help), MENU, RT (search in Games and Store).
   - Nothing opens a new window. The highlight is always fully on screen. The hints at the bottom match the buttons.
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
