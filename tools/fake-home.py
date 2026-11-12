#!/usr/bin/env python3
"""Builds a fake SteamOS home folder for tests and the screenshot checks: an internal Steam library and an SD card
library, launch options on some games, and Heroic (Flatpak) with an Epic and a GOG game. No art: Turbine Hub shows a
game's name alone when it has no real art. Every game is an empty file with the right name.

Usage: fake-home.py <folder> [--allyhub]
  --allyhub  also leaves what AllyHub keeps in a home folder (its program, settings, agent unit, quick panel and
             the files it adds for games), for the Moving from AllyHub screen."""
import json
import os
import sys

STEAM_USER = "77000001"


def write(path, text=""):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


def manifest(app_id, name, folder):
    return f'"AppState"\n{{\n\t"appid"\t\t"{app_id}"\n\t"name"\t\t"{name}"\n\t"StateFlags"\t\t"4"\n\t"installdir"\t\t"{folder}"\n\t"buildid"\t\t"18045112"\n}}\n'


def localconfig(options):
    apps = "".join(f'\t\t\t\t\t"{app_id}"\n\t\t\t\t\t{{\n\t\t\t\t\t\t"LaunchOptions"\t\t"{text}"\n\t\t\t\t\t}}\n' for app_id, text in options.items())
    return '"UserLocalConfigStore"\n{\n\t"Software"\n\t{\n\t\t"Valve"\n\t\t{\n\t\t\t"Steam"\n\t\t\t{\n\t\t\t\t"apps"\n\t\t\t\t{\n' + apps + "\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n"


# (library, app id, name, folder, program, runs through Proton, launch options or None for a game Steam has no entry for)
GAMES = [
    ("internal", "1245620", "ELDEN RING", "ELDEN RING", "Game/eldenring.exe", True, "PROTON_FSR4_UPGRADE=1 SteamDeck=1 %command%"),
    ("internal", "1086940", "Baldur's Gate 3", "Baldurs Gate 3", "bin/bg3.exe", True, ""),
    ("internal", "1145360", "Hades II", "Hades II", "Ship/Hades2.exe", True, "mangohud %command%"),
    ("internal", "1172470", "Apex Legends", "Apex Legends", "r5apex.exe", True, ""),
    ("sd", "892970", "Valheim", "Valheim", "valheim.x86_64", False, ""),
    ("sd", "632470", "Disco Elysium", "Disco Elysium", "disco.exe", True, None),
]


def main(home, allyhub):
    steam = os.path.join(home, ".local/share/Steam")
    sd = os.path.join(home, "sdcard")
    libs = {"internal": steam, "sd": sd}
    write(os.path.join(steam, "steamapps/libraryfolders.vdf"), f'"libraryfolders"\n{{\n"0"\n{{\n"path" "{steam}"\n}}\n"1"\n{{\n"path" "{sd}"\n}}\n}}\n')
    options = {}
    for lib, app_id, name, folder, exe, proton, launch in GAMES:
        apps = os.path.join(libs[lib], "steamapps")
        write(os.path.join(apps, f"appmanifest_{app_id}.acf"), manifest(app_id, name, folder))
        write(os.path.join(apps, "common", folder, exe))
        if proton:
            os.makedirs(os.path.join(apps, "compatdata", app_id, "pfx"), exist_ok=True)
        if launch is not None:
            options[app_id] = launch
    write(os.path.join(steam, "userdata", STEAM_USER, "config/localconfig.vdf"), localconfig(options))

    heroic = os.path.join(home, ".var/app/com.heroicgameslauncher.hgl/config/heroic")
    aw = os.path.join(home, "Games/Heroic/Alan Wake 2")
    sv = os.path.join(home, "Games/Heroic/Stardew Valley")
    write(os.path.join(aw, "AlanWake2.exe"))
    write(os.path.join(sv, "StardewValley"))
    write(os.path.join(heroic, "legendaryConfig/legendary/installed.json"), json.dumps({"Heron": {"title": "Alan Wake 2", "install_path": aw, "platform": "Windows", "version": "1.2.8"}}))
    write(os.path.join(heroic, "gog_store/installed.json"), json.dumps({"installed": [{"appName": "1453375253", "install_path": sv, "platform": "linux", "version": "1.6.15"}]}))
    write(os.path.join(heroic, "store_cache/gog_library.json"), json.dumps({"games": [{"app_name": "1453375253", "title": "Stardew Valley"}]}))

    if allyhub:
        write(os.path.join(home, ".local/share/allyhub/core.py"))
        write(os.path.join(home, ".local/share/allyhub/VERSION"), "1.5.0.18\n")
        write(os.path.join(home, ".config/allyhub/config.json"), json.dumps({
            "theme": {"preset": "ROG Crimson", "scale": 110, "advanced": True},
            "lighting": {"controller": "allyhub", "effect": "spiral", "custom": {"Evening": {}}},
            "game_colors": {"1245620": {"color": "#c8102e"}},
            "agent": {"enabled": True, "battery_rings": True},
            "saves": {"time_machine": True, "keep": 5},
            "performance": {"boost": True},
            "battery": {"limit": "80"},
            "updates": {"reporting": False, "channel": "stable"}}, indent=2))
        write(os.path.join(home, ".config/systemd/user/allyhub-agent.service"), "[Unit]\nDescription=Ally Hub agent\n")
        write(os.path.join(home, ".config/environment.d/90-allyhub-games.conf"), "PROTON_FSR4_UPGRADE=1\n")
        write(os.path.join(home, ".local/bin/allyhub"), "#!/bin/sh\n")
        write(os.path.join(home, ".local/share/applications/allyhub.desktop"), "[Desktop Entry]\nName=Ally Hub\n")
        write(os.path.join(home, "homebrew/plugins/AllyHub/plugin.json"), "{}\n")
        write(os.path.join(home, "AllyHub-Backups/saves/1245620/keep.txt"), "a save backup, which stays\n")
    print(f"fake home at {home}: {len(GAMES)} Steam games, 2 Heroic games" + (", AllyHub's files" if allyhub else ""))


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        sys.exit(__doc__)
    main(os.path.abspath(args[0]), "--allyhub" in sys.argv)
