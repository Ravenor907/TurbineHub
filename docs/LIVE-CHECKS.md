# Checks on a real machine

The tests in this repository run the helper against a made-up machine. Three bugs only ever showed on real ones: a
Windows boot entry for a Windows that was long gone, lights that closed again after a sleep, and the helper waiting
on itself while it was being set up. `tools/live-helper-check.sh` is the check for that kind of bug: it runs the
helper that is really installed, on the real kernel and the real udev, and times every call.

    sh tools/live-helper-check.sh --yes-this-machine
    sh tools/live-helper-check.sh --yes-this-machine --save ~/turbinehub-start

It needs SteamOS and a helper that is already set up in Turbine Hub, and it refuses to run without
`--yes-this-machine`. It never asks for a password. If a password box appears, cancel it: the helper isn't set up
for this user. It prints one line per check (`ok`, `FAILED` or `skipped`) and a count, and ends with 1 when
anything failed. A run takes about ten seconds.

## What it covers

- **The starting state.** A copy of the helper's record and of every file the helper owns under /etc is taken
  first (kept in the folder after `--save`). The changes already in the record are never touched: a setting the
  record holds is skipped, never changed.
- **What the app asks the machine (read only).** Each `--json` question of the installed Flatpak (version,
  machine, readings, checkup, cards, tuneup, changes, glossary, panel) must answer real JSON within 5 seconds, and
  the answers must fit the machine: the processor from /proc/cpuinfo, a graphics card that is in the machine, the
  memory, the SteamOS version, a battery only when there is one, Windows only when a Windows Boot Manager entry
  starts from a partition that is really there, lights only when the machine has them, and the same changes as
  the record.
- **Each kind of setting, there and back.** Apply, the value on the running machine, the file under /etc, the
  record with its "before", verify, reapply, undo, the value back, the record and every file as at the start:
  - a kernel setting (kernel.split_lock_mitigate and kernel.nmi_watchdog, each flipped and put back),
  - a recorded /sys value (the graphics card's power limit, 5 W down and back, and the card's own lowest and
    highest as the limits),
  - values set for now and not recorded, one and several in one call (what Game Boost does),
  - a wake rule for a USB device, with the device's wake switch,
  - the NTSync access rule and the NTSync module file,
  - the lighting rule, only on a machine without lights and without an ASUS controller (so only the file),
  - a change that fails (a service that isn't installed) leaves nothing behind.
- **Refusals.** A kernel setting that isn't on the list, values out of range, paths outside /sys, under /proc,
  climbing out with `..`, a /sys path that isn't on the list, a device that isn't there, a job name the helper
  doesn't have, details that are cut off, aren't JSON or are too long, settings and modules and services that
  aren't on the lists, undo of a change that isn't there or of half a number, the helper run without pkexec, and a
  job name that tries to write its own line into the helper's log. None of them may change the record, a file or a
  value.
- **Time.** Every call has 10 seconds. The helper runs as root, so a stuck one can't be stopped from here: the
  stopwatch stops waiting, the run says so, puts everything back and ends.
- **Two jobs at once.** Two changes and a check started at the same moment, then two undos: all answer, both
  changes land in the record and the file, both go again. And the lock itself: no program of the player's may be
  able to hold it.
- **The end.** The record's changes and every file under /etc are compared byte for byte with the copies from the
  start, every value that was set for a moment is read again, and no helper job may be left running. This also
  happens when a check fails half way or the run is stopped (Ctrl+C, a closed terminal).

Two things are different after a run, by design: the record's counter for the next change is higher (a number is
never used twice), and the helper's log has a line for every job.

A check that an older helper is known to fail says so on its line ("fixed in a newer helper than the one installed
here"). Update the helper in Turbine Hub (it asks for the password once) and run the check again.

## What it can't cover

- **Password prompts.** Setting up the helper, a wrong password, a machine without a password yet.
- **Installing, updating and removing the helper.** All need the password, and removing takes every change back.
- **Undo everything.** It would take the owner's own changes back.
- **Restart to Windows.** It changes what the next start does.
- **Decky Loader.** Installing it downloads and starts a service.
- **Services that exist** (SSH, Tailscale, Decky Loader) are never switched: a service turned on for a moment is
  an open door for a moment.
- **Compressed memory size.** It takes effect at the next start only.
- **Handheld hardware.** Ring lights and their rule on a machine that has them, ASUS controllers, the battery
  charge limit, the handheld power limits.
- **Sleep and wake, and a restart.** Whether a wake rule really keeps a device from waking the machine, whether
  the lights are still open after a sleep, whether everything is back in effect after a restart or a SteamOS
  update.
- **A link out of /sys.** Only the kernel makes links in the real /sys; the helper's own tests cover it.

## Checklist for the handheld

Do these by hand on the handheld, after running the script there. Each step says what to do and what you should
see.

### The script first
1. In Desktop Mode, open Konsole in the repository and run `sh tools/live-helper-check.sh --yes-this-machine`.
   - No password box. The last line counts no FAILED, or only ones marked "fixed in a newer helper".
   - The lighting rule, the battery and the power limit are skipped when they are already in your record.

### Password prompts and setting up
2. On a machine where the helper was never set up (don't remove it just for this: removing it takes every change
   back first), make a system change, for example My machine, Tune-up, Huge pages.
   - The dialog lists what changes and asks for the password once.
3. Type a wrong password.
   - "That password isn't right." Nothing changed. The dialog doesn't hang.
4. Type the right one.
   - Done within a few seconds (0.3 could wait for ever here with the lights set up). Review changes lists it.
5. Make a second change.
   - No password.
6. On a machine with an older helper, the app offers to update it. Accept, type the password.
   - Done within a few seconds, and every earlier change is still in Review changes.
7. Run the script again.
   - No line says "fixed in a newer helper" any more.

### Lights
8. Lighting, pick a color and an effect.
   - The rings change at once, no password after the first.
9. Close the lid or press the power button to sleep, wait ten seconds, wake it.
   - The rings come back with the same color and effect within a few seconds, and still change when you pick
     another.
10. Restart the handheld.
    - The same: the rings follow Turbine Hub without a password.
11. Settings, Review changes, Undo "Lighting without a password".
    - It leaves the list. After the next restart the rings no longer take colors from Turbine Hub.

### Battery and power
12. Battery, set the charge limit to 80%.
    - The limit shows in Review changes. Plugged in, the battery stops near 80%.
13. Restart, then look again.
    - Still 80% (the helper put it back at the start).
14. Undo it.
    - Back to what it was before (usually 100%).
15. Set a power limit, start a game, look at the reading in the quick panel.
    - The reading stays at or under the limit. Undo puts the old limit back.

### Sleep, wake and devices
16. My machine, Sleep and wake: switch off wake for a mouse or a receiver. Sleep the machine, move the mouse.
    - It stays asleep. The power button wakes it.
17. Undo that change, sleep again, move the mouse.
    - It wakes, without unplugging the receiver first.
18. With Game Boost on, start a game, sleep and wake during it, then quit.
    - The processor's setting is back to normal after quitting (My machine, Processor).

### Restart to Windows
19. On a machine with Windows on a drive: the Windows card is there, and **Restart to Windows** starts Windows
    once; the start after that is SteamOS again.
20. On a machine without Windows (or with only an old boot entry left from one): no Windows card anywhere, and
    `flatpak run io.github.ravenor907.TurbineHub --json machine` says `"windows":false`.

### After a SteamOS update
21. After the next SteamOS update, open Turbine Hub.
    - The checkup says everything is in effect, or offers to put the changes back, and that works without a
      password.
