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
- The sections are chips across the top. LT and RT go from one to the next and the highlight lands on the section's first control; Up from there reaches the chips, and A on a chip opens it. B on a section goes back to Overview, B on Overview goes Home.
- **Overview**: every part has its state on its card. A on Processor turns Game Boost on (the first time it asks for the password in its dialog); X on a card lists what its section holds.
- **Processor**: pick Quiet, then Boost. Each asks first, shows the exact change, and Settings, Review changes can undo it. The one in effect is filled.
- **Monitors**: your AW2726DM, up to 240 Hz.
- **Drives**: the empty Samsung 990 PRO is "not set up"; the steps lead to Steam's Storage settings.
- **Sleep and wake**: your two wireless receivers can wake the PC. Block one, sleep the PC, check it stays asleep. Undo.
- **Graphics card**: power limit with a safe reset (try a change and Undo).

On a handheld also:
- **Power**: drag the power limit with a finger and let go: the dialog shows the old and the new watts. The three modes set 14, 21 and 35 W on a 7 to 35 W machine (yours may differ).
- **Battery**: the two charts fill in after the background helper has run for a while; a sleep shows as a gap. On Overview, "Stop at 80%" is one press, and one more for 100% before a trip.
- **Lighting**: on Overview the Lights switch turns the lights off and back on at the brightness they had.

## 4b. Home
- The quick switches are there in Guided and in Veteran. Guided: "More switches" shows the rest on the same page.
- Before the helper is set up, the switches that change the system say "Needs your password"; afterwards none does. Nothing else on Home mentions a password.
- The battery tile draws the last twelve hours once there are readings.
- **Show all checks** lists what was looked at, greyed with a tick. With more than one thing to fix, Start says "Fix all" and its dialog lists each.
- After a SteamOS update that removed Decky Loader: the checkup says so, and Repair brings it back with your plugins.

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
- Every row says how it stands and has its button. On **VLC** press **A** (Install), confirm.
  - No cover comes up. The top bar says "Working: VLC" with a turning mark, VLC's row says Working, and you can
    keep moving, change the section with LT and RT, and turn the tabs.
  - When it ends a note says "VLC installed" with the seconds it took, and the row says Installed with **Open**.
- While one install runs, ask for two more (Spotify, OBS Studio).
  - The bar counts them ("2 more waiting"), their rows say Waiting, and they run one after the other.
  - Asking for VLC again while it is in the line is refused in a sentence, with no dialog.
- With work running, press B on Home (or close the window in Desktop Mode).
  - It asks: "... still running. Close anyway?". Cancel keeps everything going. Close anyway stops the running
    install; afterwards `flatpak list` shows no half-installed app.
- In Files, copy a large folder while an install runs.
  - The copy waits ("Waiting for its turn" in the status bar), then runs with its progress in both bars.
- X on an installed row lists Remove, Details and Where it comes from. Remove VLC that way.
- With an update ready, **Start** is Update all in every section; Installed's chip counts the updates.
- The top bar shows the battery percent (a bolt while charging) on a handheld and the clock in Game Mode; make the
  Desktop Mode window narrow: the machine's name goes first, then the clock, then the percent, never the tabs.
- The menu (View): **Switch to Desktop** in Game Mode asks, then SteamOS switches; in Desktop Mode it is
  **Return to Gaming Mode**.
- Games, a Steam game, Won't start?, **Check the game's files**: Steam opens its own check for that game.
- After the next update of Turbine Hub, **What's new** shows once with the release's notes; Settings, Updates,
  What's new shows it again.
- Decky plugins: install Decky Loader from here (it comes from Decky's official release, checked by checksum).
- The Turbine Hub quick panel needs a published release first (see the summary); then follow its steps.

## 8. Offline
- Turn off networking and open Games and Store.
  - They show the last data with its age ("Checked ... ago, offline now"), never an error page.

## 9. Undo everything
- Settings, Review changes, **Undo all**.
  - The list empties and the machine is as it was before Turbine Hub changed anything.

## 10. GPU undervolt (this PC, with you at the machine)
Only with you in front of it: a value too low can freeze the screen until a hard restart. Nothing is damaged. Have
a game you know with a frame cap (Steam's quick menu, Performance, or the game's own limit) and a terminal in
Desktop Mode for the readings. `card1` and `hwmon` below are this PC's; `ls /sys/class/drm/card1/device/hwmon/`
gives the hwmon number.
1. Veteran mode, My machine, Graphics card, **Undervolting**, Show. Read the two warning rows.
   - Guided mode shows nothing of it; the power slider stops at 304 W.
2. **Turn on graphics card tuning**: the dialog lists `/etc/modprobe.d/turbinehub-amdgpu.conf` with
   `options amdgpu ppfeaturemask=0xfff7ffff`, the restart and the system log note, with Cancel highlighted. Turn on,
   then **Restart now**.
3. After the restart, in the terminal:
   - `cat /sys/module/amdgpu/parameters/ppfeaturemask` reads `0xfff7ffff`.
   - `cat /proc/sys/kernel/tainted` has bit 5 (32) set, and `journalctl -k | grep -i overdrive` shows "Overdrive is
     enabled".
   - `cat /sys/class/drm/card1/device/pp_od_clk_voltage` exists. Save its whole text as a fixture:
     `cat /sys/class/drm/card1/device/pp_od_clk_voltage > ~/rx9070xt-od-7.2.txt` and send it along (the tests use a
     text written from the kernel's source; the real one replaces it).
   - The page now has the slider (-150 to 0 mV) and the power slider goes to 340 W.
4. **Try 0 mV** is "already set"; move to -5 mV and back to 0 to see the before and after. Then Try **-10 mV**: the
   dialog, then "Keep -10 mV? Going back to 0 mV in 15 s". Let it run out: it goes back by itself
   (`pp_od_clk_voltage` reads `0mV` under OD_VDDGFX_OFFSET).
5. Start the frame-capped game. In the terminal:
   `watch -n1 'cat /sys/class/drm/card1/device/hwmon/hwmon*/in0_input /sys/class/drm/card1/device/hwmon/hwmon*/power1_average'`
   (in0_input is the graphics voltage in mV, power1_average the draw in microwatts). Note both at 0 mV.
6. Try -10, then -20, then -30 mV, each for a few minutes of play, keeping each one: in0_input should drop and
   power1_average fall a little at the same frame rate. Stop at the first crash, stutter or artifact and note the value.
7. **Keep** -30 mV (or the last good one). The fold's badge says on probation; Home's checkup has the probation item.
8. Sleep the PC and wake it. `pp_od_clk_voltage` still reads the kept value; Settings, Review changes lists
   "Graphics card voltage set to -30 mV".
9. Change the power limit (Graphics card, Power limit, 280 W). `tail /var/lib/turbinehub/helper.log` shows no
   re-commit failure, `pp_od_clk_voltage` still reads -30 mV, and in0_input under the same game load is still the
   lower one (kernel 7.2 drops the offset when the limit changes; Turbine Hub commits it again). Then the power
   limit's **Safe reset**: the same.
10. Restart: right after the start `pp_od_clk_voltage` reads 0 mV (the boot never puts it back); about a minute
    after the session is up it reads -30 mV again (the background helper must be on).
11. Undervolting's **Safe reset**: one press, no question, 0 mV at once, the probation item gone.
12. **Turn tuning off**: the dialog lists the reset and the driver option going, then Restart now. After the restart
    `ppfeaturemask` reads `0xfff7bfff` again, `pp_od_clk_voltage` is gone, `/etc/modprobe.d/turbinehub-amdgpu.conf`
    is gone, and the power slider stops at 304 W.
- Not to try on purpose, but if a value freezes the machine: hold the power button. At the next start the page and
  Home say the lower voltage was turned off because the machine stopped, and nothing puts it back by itself.
