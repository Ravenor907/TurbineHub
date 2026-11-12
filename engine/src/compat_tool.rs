//! Turbine Hub's own Steam compatibility tool, "Turbine Hub": how a game's switches reach the game.
//!
//! A compatibility tool is a folder under Steam's compatibilitytools.d, which Steam only reads. The player picks it
//! once per game in the game's Properties, Compatibility; Steam sees new tools when it starts. Turbine Hub never
//! writes Steam's own settings files and never restarts Steam.
//!
//! The folder holds three files:
//! - compatibilitytool.vdf: the tool's name in Steam's list (for Windows games, run on Linux).
//! - toolmanifest.vdf: the command Steam runs (`turbinehub-tool <verb>`, then the game's program).
//! - turbinehub-tool: a script. A tool chosen in place of Proton gets no Proton from Steam, so the script builds the
//!   line Steam builds itself: the Steam Linux Runtime the chosen Proton asks for (from Proton's own toolmanifest),
//!   then that Proton, then the game. For a game launch it then reads the game's line in Turbine Hub's games file
//!   and sets what each switch stands for. With Turbine Hub's files gone it runs the plain line, so the game always
//!   starts.
//!
//! Which Proton: the player's pick for the game (`<app id>.proton` in the launch folder), else `default.proton`
//! (Steam's choice for all games), else the newest finished numbered Proton installed, then a Beta, Proton
//! Experimental, Hotfix, then other tools in compatibilitytools.d. The script says which one and why in launch.log.
//!
//! Games whose anti-cheat runs online are listed in `locked.list`: the script starts them as they are, with no
//! switch at all, even when the games file names them.
//!
//! Turbine Hub only reads Steam's config.vdf (CompatToolMapping) to see which tool a game uses. A name Steam doesn't
//! know starts the game with no tool at all, so the folder is never removed while a game still names it.

use crate::vdf;
use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The tool's internal name (what Steam keeps in CompatToolMapping).
pub const TOOL_NAME: &str = "turbine-hub";
/// The name in Steam's Compatibility list.
pub const DISPLAY_NAME: &str = "Turbine Hub";
/// The tool's folder under compatibilitytools.d.
pub const FOLDER: &str = "TurbineHub";
/// The script Steam runs.
pub const SCRIPT: &str = "turbinehub-tool";
/// Turbine Mod Manager's tool, as Steam keeps it. A game can use one tool only, so a game on that one gets its
/// switches from Turbine Mod Manager.
pub const MOD_TOOL_NAME: &str = "turbine-mods";

/// Steam's folder for tools that don't come from Steam itself.
pub fn tools_dir(steam_root: &Path) -> PathBuf {
    steam_root.join("compatibilitytools.d")
}

/// Turbine Hub's tool folder.
pub fn tool_dir(steam_root: &Path) -> PathBuf {
    tools_dir(steam_root).join(FOLDER)
}

/// Where the script reads the Proton picks and the locked list, and writes its log: a folder inside Turbine Hub's.
pub fn launch_dir(folder: &Path) -> PathBuf {
    folder.join("launch")
}

/// The script's log of every start.
pub fn launch_log(folder: &Path) -> PathBuf {
    launch_dir(folder).join("launch.log")
}

/// compatibilitytool.vdf: the tool as Steam lists it.
pub fn compatibilitytool_vdf() -> String {
    format!(
        "\"compatibilitytools\"\n{{\n  \"compat_tools\"\n  {{\n    \"{TOOL_NAME}\"\n    {{\n      \"install_path\" \".\"\n      \"display_name\" \"{DISPLAY_NAME}\"\n      \"from_oslist\"  \"windows\"\n      \"to_oslist\"    \"linux\"\n    }}\n  }}\n}}\n"
    )
}

/// toolmanifest.vdf: the command Steam runs. No required tool: the script starts the runtime the chosen Proton
/// needs itself (one tool can name only one runtime, and different Protons need different ones).
pub fn toolmanifest_vdf() -> String {
    format!("\"manifest\"\n{{\n  \"version\" \"2\"\n  \"commandline\" \"/{SCRIPT} %verb%\"\n  \"use_sessions\" \"1\"\n}}\n")
}

/// The script. `folder`: Turbine Hub's own folder (the games file and the launch folder live in it).
pub fn script_text(folder: &Path) -> String {
    let dir = folder.to_string_lossy().replace('\'', "");
    SCRIPT_TEXT.replace("@HUB_DIR@", &dir)
}

const SCRIPT_TEXT: &str = r##"#!/bin/bash
# Turbine Hub's Steam compatibility tool, "Turbine Hub". Steam runs it as
#   turbinehub-tool <verb> <the game's program> [arguments]
# behind its own launch wrapper and reaper. It builds the line Steam builds itself for a Proton game (the Steam Linux
# Runtime the chosen Proton asks for, then that Proton, then the game) and, for a game launch, sets what the game's
# switches in Turbine Hub stand for. Without Turbine Hub's files it runs the plain line, so the game always starts.
# It runs nothing else. Every step goes to launch.log.
tool=$(cd "$(dirname "$0")" && pwd)
hub='@HUB_DIR@'
launch="$hub/launch"
if [ -d "$launch" ]; then log="$launch/launch.log"; else log="$tool/launch.log"; fi
# The log keeps its newest lines only.
if [ -f "$log" ] && [ "$(wc -c < "$log" 2> /dev/null || echo 0)" -gt 262144 ]; then
  tail -n 400 "$log" > "$log.new" 2> /dev/null && mv "$log.new" "$log" 2> /dev/null
fi
say() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" >> "$log" 2>/dev/null; }
verb="${1:-}"
[ "$#" -gt 0 ] && shift
app="${STEAM_COMPAT_APP_ID:-${SteamAppId:-}}"
say "Turbine Hub: Steam started app ${app:-unknown} ($verb) in $(pwd): $*"
case "$verb" in
  run|waitforexitandrun) ;;
  *) say "Turbine Hub: not a launch Steam makes ($verb): nothing started"; exit 1 ;;
esac
if [ "$#" -eq 0 ]; then
  say "Turbine Hub: no program from Steam: nothing started"
  exit 1
fi
steam="${STEAM_COMPAT_CLIENT_INSTALL_PATH:-}"
if [ -z "$steam" ] || [ ! -d "$steam" ]; then steam=$(dirname "$(dirname "$tool")"); fi
# Steam's libraries: what Steam handed over, Steam's own folder, and its list of library folders.
libs=()
add_lib() {
  local l="${1%/}"
  l="${l%/steamapps}"
  [ -n "$l" ] && [ -d "$l/steamapps" ] || return 0
  local have
  for have in "${libs[@]}"; do [ "$have" = "$l" ] && return 0; done
  libs+=("$l")
}
IFS=: read -r -a handed <<< "${STEAM_COMPAT_LIBRARY_PATHS:-}"
for l in "${handed[@]}"; do add_lib "$l"; done
add_lib "$steam"
if [ -f "$steam/steamapps/libraryfolders.vdf" ]; then
  while IFS= read -r l; do add_lib "$l"; done < <(sed -n 's/^[[:space:]]*"path"[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$steam/steamapps/libraryfolders.vdf")
fi
value_of() { sed -n 's/^[[:space:]]*"'"$1"'"[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$2" 2> /dev/null | head -n 1; }
# The runtime a Proton needs, from Proton's own toolmanifest: its folder and its command. Answers 1 when the
# runtime it names isn't installed.
runtime=
rtcmd=()
runtime_for() {
  runtime=
  rtcmd=()
  local need
  need=$(value_of require_tool_appid "$1/toolmanifest.vdf")
  [ -n "$need" ] || return 0
  local l dir line
  for l in "${libs[@]}"; do
    dir=$(value_of installdir "$l/steamapps/appmanifest_$need.acf")
    if [ -n "$dir" ] && [ -f "$l/steamapps/common/$dir/toolmanifest.vdf" ]; then
      runtime="$l/steamapps/common/$dir"
      line=$(value_of commandline "$runtime/toolmanifest.vdf")
      [ -n "$line" ] || line='/_v2-entry-point --verb=%verb% --'
      line="${line//%verb%/$verb}"
      read -r -a rtcmd <<< "$line"
      rtcmd[0]="$runtime${rtcmd[0]}"
      [ -f "${rtcmd[0]}" ] && return 0
      runtime=
      rtcmd=()
    fi
  done
  missing="the Steam Linux Runtime it needs (Steam app $need) isn't installed"
  return 1
}
proton=
why=
missing=
passed=()
try() {
  [ -z "$proton" ] || return 0
  [ -n "$1" ] || return 0
  if [ ! -f "$1/proton" ] || [ ! -f "$1/toolmanifest.vdf" ]; then
    passed+=("$1 (no Proton there)")
    return 0
  fi
  if runtime_for "$1"; then
    proton="$1"
    why="$2"
  else
    passed+=("$1 ($missing)")
  fi
}
for f in "$launch/$app.proton" "$launch/default.proton"; do
  if [ -n "$app" ] && [ -f "$f" ]; then
    d=$(head -n 1 "$f")
    d="${d%$'\r'}"
    case "$f" in
      */default.proton) try "$d" "Steam's own choice for all games" ;;
      *) try "$d" "your pick in Turbine Hub for this game" ;;
    esac
  fi
done
if [ -z "$proton" ]; then
  # The newest finished Proton ("Proton 11.0"), then numbered ones that are not finished ("Proton 12.0 (Beta)"),
  # then Experimental, Hotfix and other tools.
  found=()
  beta=()
  tab=$'\t'
  for l in "${libs[@]}"; do
    for d in "$l"/steamapps/common/Proton\ [0-9]*; do
      [ -f "$d/proton" ] || continue
      name=$(basename "$d")
      num=$(printf '%s' "$name" | sed 's/^Proton \([0-9.]*\).*/\1/')
      if [ "$name" = "Proton $num" ]; then found+=("$num$tab$d"); else beta+=("$num$tab$d"); fi
    done
  done
  count=$(( ${#found[@]} + ${#beta[@]} ))
  if [ "${#found[@]}" -gt 0 ]; then
    while IFS= read -r line; do
      try "${line#*"$tab"}" "the newest Proton installed ($count numbered Proton found)"
    done < <(printf '%s\n' "${found[@]}" | sort -t "$tab" -k1,1 -V -r)
  fi
  if [ "${#beta[@]}" -gt 0 ]; then
    while IFS= read -r line; do
      try "${line#*"$tab"}" "the newest Proton installed is a Beta (no finished Proton could be used; $count numbered Proton found)"
    done < <(printf '%s\n' "${beta[@]}" | sort -t "$tab" -k1,1 -V -r)
  fi
  for l in "${libs[@]}"; do try "$l/steamapps/common/Proton - Experimental" "Proton Experimental (no numbered Proton could be used)"; done
  for l in "${libs[@]}"; do try "$l/steamapps/common/Proton Hotfix" "Proton Hotfix (no numbered Proton could be used)"; done
  while IFS= read -r d; do
    [ "$d" = "$tool" ] || try "$d" "another tool's Proton in compatibilitytools.d (none of Steam's own could be used)"
  done < <(for d in "$steam"/compatibilitytools.d/*/; do d="${d%/}"; [ -f "$d/proton" ] && printf '%s\n' "$d"; done | sort -V -r)
fi
for p in "${passed[@]}"; do say "Turbine Hub: not used: $p"; done
if [ -z "$proton" ]; then
  say "Turbine Hub: no Proton that can run was found in ${libs[*]:-no Steam library}: nothing started. In Steam, start any game with Proton once (Steam then installs Proton and its runtime), or pick another tool in this game's Properties, Compatibility"
  exit 1
fi
say "Turbine Hub: app ${app:-unknown} runs with $proton [why: $why]"
if [ -n "$runtime" ]; then
  say "Turbine Hub: its runtime: $runtime"
  export STEAM_COMPAT_TOOL_PATHS="$proton:$runtime"
  export STEAM_COMPAT_MOUNTS="${STEAM_COMPAT_MOUNTS:+$STEAM_COMPAT_MOUNTS:}$proton:$runtime"
else
  say "Turbine Hub: this Proton names no runtime: it runs as it is"
  export STEAM_COMPAT_TOOL_PATHS="$proton"
fi
chain=("${rtcmd[@]}" "$proton/proton" "$verb" "$@")
if [ "$verb" != waitforexitandrun ]; then
  say "Turbine Hub: a set-up step of Steam's ($verb), never with switches: ${chain[*]}"
  exec "${chain[@]}"
fi
# The game's switches: its own line in Turbine Hub's games file, one game per line.
games="$hub/games.json"
on=
if [ -z "$app" ]; then
  say "Turbine Hub: Steam named no app: the game starts as it is"
elif [ -f "$launch/locked.list" ] && grep -q -x -F "$app" "$launch/locked.list" 2> /dev/null; then
  say "Turbine Hub: this game's anti-cheat runs online: it starts as it is, with no switch"
elif [ -f "$games" ]; then
  line=$(grep -m 1 -F "\"steam:$app\": {" "$games" 2> /dev/null)
  has() { case "$line" in *"\"$1\""*) return 0 ;; esac; return 1; }
  if has fsr4 || grep -q '"fsr4All": true' "$games" 2> /dev/null; then
    export PROTON_FSR4_UPGRADE=1
    on="$on fsr4"
  fi
  if has handheld; then
    export SteamDeck=1
    on="$on handheld"
  fi
  if has log; then
    export PROTON_LOG=1
    on="$on log"
  fi
  if has framegen; then
    if [ -x "$HOME/lsfg" ]; then
      chain=("$HOME/lsfg" "${chain[@]}")
      on="$on framegen"
    else
      say "Turbine Hub: frame generation is on, but its wrapper ($HOME/lsfg) isn't installed: the game starts without it"
    fi
  fi
else
  say "Turbine Hub: no games file ($games): the game starts as it is"
fi
say "Turbine Hub: switches on:${on:- none}: ${chain[*]}"
exec "${chain[@]}"
"##;

/// What `install` did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Installed {
    /// The files written now (names), empty when everything was already in place.
    pub written: Vec<String>,
    /// The folder is new: Steam lists the tool after its next start.
    pub new_folder: bool,
}

fn write_if_different(path: &Path, text: &str, executable: bool) -> io::Result<bool> {
    if fs::read_to_string(path).is_ok_and(|t| t == text) {
        return Ok(false);
    }
    let tmp = path.with_extension("turbinehub-new");
    fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }))?;
    }
    fs::rename(&tmp, path)?;
    Ok(true)
}

fn files(folder: &Path) -> [(&'static str, String, bool); 3] {
    // The script and the manifest first: once compatibilitytool.vdf is there the tool is complete.
    [(SCRIPT, script_text(folder), true), ("toolmanifest.vdf", toolmanifest_vdf(), false), ("compatibilitytool.vdf", compatibilitytool_vdf(), false)]
}

/// Writes the tool into Steam's compatibilitytools.d (only what is missing or different). Steam never writes in this
/// folder, so there is nothing to race with and Steam can be running. `folder`: Turbine Hub's own folder.
pub fn install(steam_root: &Path, folder: &Path) -> io::Result<Installed> {
    if !steam_root.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Steam's folder wasn't found"));
    }
    let dir = tool_dir(steam_root);
    let new_folder = !dir.join("compatibilitytool.vdf").is_file();
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(launch_dir(folder))?;
    let mut written = Vec::new();
    for (name, text, exec) in files(folder) {
        if write_if_different(&dir.join(name), &text, exec)? {
            written.push(name.to_string());
        }
    }
    Ok(Installed { written, new_folder })
}

/// Whether the tool is in Steam's folder, and as this Turbine Hub writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Missing,
    /// There, but written by another version of Turbine Hub (or for another folder).
    Outdated,
    Current,
}

pub fn state(steam_root: &Path, folder: &Path) -> State {
    let dir = tool_dir(steam_root);
    if !dir.join("compatibilitytool.vdf").is_file() {
        return State::Missing;
    }
    let same = files(folder).iter().all(|(name, text, _)| fs::read_to_string(dir.join(name)).is_ok_and(|t| t == *text));
    if same { State::Current } else { State::Outdated }
}

/// Has Steam started since the tool's folder was made? Steam reads tools only when it starts, so until then the
/// tool isn't in its Compatibility list. `steam_started`: when the running Steam started, in seconds since 1970
/// (None: not running or unknown, which counts as not seen yet).
pub fn seen_by_steam(steam_root: &Path, steam_started: Option<u64>) -> bool {
    let Some(started) = steam_started else { return false };
    let Ok(made) = fs::metadata(tool_dir(steam_root).join("compatibilitytool.vdf")).and_then(|m| m.modified()) else { return false };
    made.duration_since(std::time::UNIX_EPOCH).is_ok_and(|d| d.as_secs() <= started)
}

/// Steam's settings file with each game's compatibility tool (read only).
pub fn config_file(steam_root: &Path) -> PathBuf {
    steam_root.join("config").join("config.vdf")
}

const MAPPING: &[&str] = &["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping"];

/// The compatibility tool Steam has for a game in config.vdf's text (its internal name), when one is forced.
pub fn chosen_in(config_text: &str, app_id: &str) -> Option<String> {
    let v = vdf::parse(config_text).ok()?;
    let name = v.path(MAPPING)?.get(app_id)?.str_at(&["name"])?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The games (app ids) that use the tool called `tool`, from config.vdf's text.
pub fn games_using_in(config_text: &str, tool: &str) -> Vec<String> {
    let Ok(v) = vdf::parse(config_text) else { return Vec::new() };
    let Some(map) = v.path(MAPPING) else { return Vec::new() };
    map.entries().iter().filter(|(id, e)| id != "0" && e.str_at(&["name"]) == Some(tool)).map(|(id, _)| id.clone()).collect()
}

/// Steam's config.vdf as text ("" when it can't be read).
pub fn config_text(steam_root: &Path) -> String {
    fs::read(config_file(steam_root)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// The games that use Turbine Hub's tool, per Steam's config.vdf.
pub fn games_using(steam_root: &Path) -> Vec<String> {
    games_using_in(&config_text(steam_root), TOOL_NAME)
}

/// Takes the tool's folder out. Refused while a game still names the tool (Steam would start that game with no
/// tool at all): the answer is then the app ids to switch back first. Only Turbine Hub's own three files and its
/// log go.
pub fn remove(steam_root: &Path) -> Result<bool, Vec<String>> {
    let using = games_using(steam_root);
    if !using.is_empty() {
        return Err(using);
    }
    let dir = tool_dir(steam_root);
    if !dir.is_dir() {
        return Ok(false);
    }
    for name in [SCRIPT, "toolmanifest.vdf", "compatibilitytool.vdf", "launch.log"] {
        let _ = fs::remove_file(dir.join(name));
    }
    let _ = fs::remove_dir(&dir);
    Ok(true)
}

/// Keeps the list of games the script must leave alone (Steam app ids, one per line). Written only when it changes.
pub fn write_locked(folder: &Path, app_ids: &[String]) -> io::Result<()> {
    let mut ids: Vec<&str> = app_ids.iter().map(String::as_str).filter(|id| app_id_ok(id)).collect();
    ids.sort_unstable();
    ids.dedup();
    let text: String = ids.iter().map(|id| format!("{id}\n")).collect();
    let file = launch_dir(folder).join("locked.list");
    if fs::read_to_string(&file).is_ok_and(|t| t == text) {
        return Ok(());
    }
    crate::write_atomic(&file, text.as_bytes())
}

// ---- Which Proton ----

/// The file that names the Proton folder for one game (read by the script).
pub fn proton_file(folder: &Path, app_id: &str) -> PathBuf {
    launch_dir(folder).join(format!("{app_id}.proton"))
}

/// What the game's pick was before the last change, for Undo: a Proton folder, or one empty line for "Automatic".
fn before_file(folder: &Path, app_id: &str) -> PathBuf {
    launch_dir(folder).join(format!("{app_id}.proton.before"))
}

fn app_id_ok(app_id: &str) -> bool {
    !app_id.is_empty() && app_id.len() <= 20 && app_id.bytes().all(|b| b.is_ascii_digit())
}

/// Is this folder a Proton the script can use (its program and its manifest are there)?
pub fn is_proton(dir: &Path) -> bool {
    dir.join("proton").is_file() && dir.join("toolmanifest.vdf").is_file()
}

fn first_line(file: &Path) -> Option<String> {
    let t = fs::read_to_string(file).ok()?;
    Some(t.lines().next().unwrap_or("").trim_end_matches('\r').to_string())
}

/// The player's pick for a game, when there is one.
pub fn proton_pick(folder: &Path, app_id: &str) -> Option<PathBuf> {
    first_line(&proton_file(folder, app_id)).filter(|l| !l.is_empty()).map(PathBuf::from)
}

/// What Undo would put back: None when there is nothing to undo, Some(None) for "Automatic", Some(folder) for a
/// Proton.
pub fn proton_before(folder: &Path, app_id: &str) -> Option<Option<PathBuf>> {
    first_line(&before_file(folder, app_id)).map(|l| (!l.is_empty()).then(|| PathBuf::from(l)))
}

fn write_pick(folder: &Path, app_id: &str, proton_dir: Option<&Path>) -> io::Result<()> {
    let file = proton_file(folder, app_id);
    match proton_dir {
        Some(d) => {
            let text = d.to_string_lossy();
            if text.contains('\n') || !d.is_absolute() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a folder path"));
            }
            crate::write_atomic(&file, format!("{text}\nyou\n").as_bytes())
        }
        None => match fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

/// Sets the player's pick for a game: a Proton folder, or None for "Automatic" (the tool then takes Steam's choice
/// for all games, or the newest Proton, at each launch). What was set before is kept for Undo.
pub fn set_proton_pick(folder: &Path, app_id: &str, proton_dir: Option<&Path>) -> io::Result<()> {
    if !app_id_ok(app_id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a Steam app id"));
    }
    if let Some(d) = proton_dir
        && !is_proton(d)
    {
        return Err(io::Error::new(io::ErrorKind::NotFound, "that Proton isn't installed any more"));
    }
    let before = proton_pick(folder, app_id);
    if before.as_deref() == proton_dir {
        return Ok(());
    }
    write_pick(folder, app_id, proton_dir)?;
    let line = before.map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    crate::write_atomic(&before_file(folder, app_id), format!("{line}\n").as_bytes())
}

/// Puts the pick from before the last change back. Answers what is set now (None: Automatic). The change that was
/// undone becomes the one Undo would put back, so a second Undo brings it back.
pub fn undo_proton_pick(folder: &Path, app_id: &str) -> io::Result<Option<PathBuf>> {
    if !app_id_ok(app_id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a Steam app id"));
    }
    let Some(back) = proton_before(folder, app_id) else { return Err(io::Error::new(io::ErrorKind::NotFound, "nothing to undo")) };
    let now = proton_pick(folder, app_id);
    write_pick(folder, app_id, back.as_deref())?;
    let line = now.map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    crate::write_atomic(&before_file(folder, app_id), format!("{line}\n").as_bytes())?;
    Ok(back)
}

// ---- The Protons installed ----

/// What kind of Proton a folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtonKind {
    /// A finished, numbered Proton from Steam ("Proton 11.0").
    Stable,
    /// A numbered one that isn't finished ("Proton 12.0 (Beta)").
    Beta,
    Experimental,
    Hotfix,
    /// Another tool's Proton in compatibilitytools.d (GE-Proton and the like).
    Other,
}

impl ProtonKind {
    /// In plain words, for the list.
    pub fn words(self) -> &'static str {
        match self {
            ProtonKind::Stable => "From Steam",
            ProtonKind::Beta => "From Steam, not finished yet",
            ProtonKind::Experimental => "From Steam, the newest fixes",
            ProtonKind::Hotfix => "From Steam, quick fixes for new games",
            ProtonKind::Other => "Added by you",
        }
    }
}

/// One Proton installed on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proton {
    pub dir: PathBuf,
    /// Its folder's name ("Proton 11.0", "GE-Proton10-4").
    pub name: String,
    pub kind: ProtonKind,
    /// Its number ("11.0"), for numbered ones.
    pub number: String,
    /// The Steam Linux Runtime it asks for is installed (or it asks for none): the tool can start it.
    pub usable: bool,
    /// Its name in Steam's settings when its folder says so (another tool's compatibilitytool.vdf).
    pub steam_name: Option<String>,
}

impl Proton {
    /// Is this the Proton Steam's settings call `name` ("proton_11", "proton_experimental", "GE-Proton10-4")?
    pub fn matches_steam_name(&self, name: &str) -> bool {
        if self.steam_name.as_deref() == Some(name) {
            return true;
        }
        match self.kind {
            ProtonKind::Experimental => name == "proton_experimental",
            ProtonKind::Hotfix => name == "proton_hotfix",
            ProtonKind::Stable | ProtonKind::Beta => name.strip_prefix("proton_").is_some_and(|n| !n.is_empty() && n == self.number.split('.').next().unwrap_or("")),
            ProtonKind::Other => self.name == name,
        }
    }
}

/// Compares names with numbers in them the way a person reads them: "10-4" before "10-12", "9.0" before "11.0".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn parts(v: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = v.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &v[start..i]));
                start = i;
            }
        }
        out
    }
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(&pb) {
        let o = match (x, y) {
            ((true, m), (true, n)) => {
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                m.len().cmp(&n.len()).then_with(|| m.cmp(n))
            }
            ((_, m), (_, n)) => m.to_lowercase().cmp(&n.to_lowercase()),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    pa.len().cmp(&pb.len())
}

/// Steam's library folders (its own folder first), from steamapps/libraryfolders.vdf.
pub fn library_folders(steam_root: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam_root.to_path_buf()];
    if let Some(v) = fs::read_to_string(steam_root.join("steamapps/libraryfolders.vdf")).ok().and_then(|t| vdf::parse(&t).ok()) {
        for (_, lib) in v.get("libraryfolders").map(|l| l.entries()).unwrap_or(&[]) {
            if let Some(p) = lib.str_at(&["path"]).map(PathBuf::from)
                && p.is_absolute()
                && p.join("steamapps").is_dir()
                && !out.contains(&p)
            {
                out.push(p);
            }
        }
    }
    out
}

/// Every Proton installed: Steam's own in each library, then other tools' in compatibilitytools.d. Best first:
/// finished ones by number (newest first), then Betas, Experimental, Hotfix, others; usable ones before the rest
/// within each kind.
pub fn installed_protons(steam_root: &Path) -> Vec<Proton> {
    let libs = library_folders(steam_root);
    let value = |file: &Path, key: &str| -> Option<String> {
        let v = vdf::parse(&fs::read_to_string(file).ok()?).ok()?;
        v.entries().first().and_then(|(_, body)| body.str_at(&[key])).map(str::to_string)
    };
    let usable = |dir: &Path| -> bool {
        let Some(need) = value(&dir.join("toolmanifest.vdf"), "require_tool_appid") else { return true };
        libs.iter().any(|l| value(&l.join(format!("steamapps/appmanifest_{need}.acf")), "installdir").is_some_and(|d| l.join("steamapps/common").join(d).join("toolmanifest.vdf").is_file()))
    };
    let mut out: Vec<Proton> = Vec::new();
    for l in &libs {
        let Ok(rd) = fs::read_dir(l.join("steamapps/common")) else { continue };
        for e in rd.flatten() {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("Proton") || !is_proton(&dir) || out.iter().any(|p| p.name == name) {
                continue;
            }
            let number: String = name.strip_prefix("Proton ").unwrap_or("").chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            let kind = if name == "Proton - Experimental" {
                ProtonKind::Experimental
            } else if name == "Proton Hotfix" {
                ProtonKind::Hotfix
            } else if !number.is_empty() && name == format!("Proton {number}") {
                ProtonKind::Stable
            } else if !number.is_empty() {
                ProtonKind::Beta
            } else {
                ProtonKind::Other
            };
            out.push(Proton { usable: usable(&dir), dir, name, kind, number, steam_name: None });
        }
    }
    if let Ok(rd) = fs::read_dir(tools_dir(steam_root)) {
        for e in rd.flatten() {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name == FOLDER || !is_proton(&dir) {
                continue;
            }
            let steam_name = fs::read_to_string(dir.join("compatibilitytool.vdf")).ok().and_then(|t| vdf::parse(&t).ok()).and_then(|v| v.path(&["compatibilitytools", "compat_tools"]).and_then(|c| c.entries().first().map(|(k, _)| k.clone())));
            out.push(Proton { usable: usable(&dir), dir, name, kind: ProtonKind::Other, number: String::new(), steam_name });
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.usable.cmp(&a.usable)).then_with(|| natural_cmp(&b.number, &a.number)).then_with(|| natural_cmp(&b.name, &a.name)));
    out
}

/// The file that names the Proton for games without a pick of their own (read by the script).
pub fn default_proton_file(folder: &Path) -> PathBuf {
    launch_dir(folder).join("default.proton")
}

/// Steam's own choice for all games (the "0" entry of its settings), when that Proton is installed and usable.
pub fn steam_default<'a>(list: &'a [Proton], config_text: &str) -> Option<&'a Proton> {
    let name = chosen_in(config_text, "0").filter(|n| n != TOOL_NAME && n != MOD_TOOL_NAME)?;
    list.iter().find(|p| p.usable && p.matches_steam_name(&name))
}

/// The Proton a game without a pick starts with: Steam's choice for all games, else the best one installed.
pub fn automatic<'a>(list: &'a [Proton], config_text: &str) -> Option<&'a Proton> {
    steam_default(list, config_text).or_else(|| list.iter().find(|p| p.usable))
}

/// Keeps default.proton: Steam's own choice for all games while Steam has one that is installed; otherwise the file
/// goes and the script takes the newest Proton by itself. Written only when it changes.
pub fn write_default_proton(steam_root: &Path, folder: &Path) -> Option<PathBuf> {
    let list = installed_protons(steam_root);
    let file = default_proton_file(folder);
    match steam_default(&list, &config_text(steam_root)) {
        Some(p) => {
            let want = format!("{}\nsteam\n", p.dir.to_string_lossy());
            if !fs::read_to_string(&file).is_ok_and(|t| t == want) {
                let _ = crate::write_atomic(&file, want.as_bytes());
            }
            Some(p.dir.clone())
        }
        None => {
            let _ = fs::remove_file(&file);
            None
        }
    }
}

// ---- What the tool said ----

/// The Proton the tool used at the newest launch of a game in its log, with why:
/// ("/path/Proton 11.0", "your pick in Turbine Hub for this game").
pub fn last_launch_in(log: &str, app_id: &str) -> Option<(String, String)> {
    let mark = format!(" Turbine Hub: app {app_id} runs with ");
    let l = log.lines().rev().find(|l| l.contains(&mark))?;
    let rest = &l[l.find(&mark)? + mark.len()..];
    let (dir, why) = rest.split_once(" [why: ")?;
    Some((dir.to_string(), why.strip_suffix(']').unwrap_or(why).to_string()))
}

/// One plain sentence about the newest launch through the tool: which Proton ran the game and why.
pub fn last_launch_words(folder: &Path, app_id: &str) -> Option<String> {
    let log = fs::read_to_string(launch_log(folder)).ok()?;
    let (dir, why) = last_launch_in(&log, app_id)?;
    let name = Path::new(&dir).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(dir);
    Some(format!("Last start: {name}, {why}."))
}

/// The one-time steps for the player, in order, as (number, sentence). `listed`: Steam has started since the tool
/// was written, so it is in the list; otherwise the first step says to restart Steam.
pub fn steps(game_name: &str, listed: bool) -> Vec<(String, String)> {
    let mut s = Vec::new();
    if !listed {
        s.push("Restart Steam once, so it finds the new tool: Steam button, Power, Restart Steam.".to_string());
    }
    s.push(format!("In Steam, open {game_name}, press the gear and choose Properties, then Compatibility."));
    s.push("Turn on \"Force the use of a specific Steam Play compatibility tool\".".to_string());
    s.push(format!("Pick \"{DISPLAY_NAME}\" in the list."));
    s.into_iter().enumerate().map(|(i, t)| ((i + 1).to_string(), t)).collect()
}

/// Where the frame generation wrapper lives once the Lossless Scaling plugin is set up: a script in the home
/// folder that the plugin writes.
pub fn framegen_wrapper(home: &Path) -> PathBuf {
    home.join("lsfg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_numbers_sort_the_way_they_read() {
        assert_eq!(natural_cmp("9.0", "11.0"), Ordering::Less);
        assert_eq!(natural_cmp("GE-Proton10-4", "GE-Proton10-12"), Ordering::Less);
        assert_eq!(natural_cmp("GE-Proton10-12", "GE-Proton9-27"), Ordering::Greater);
        assert_eq!(natural_cmp("11.0", "11.0"), Ordering::Equal);
        assert_eq!(natural_cmp("", "1"), Ordering::Less);
    }

    #[test]
    fn steams_settings_say_which_tool_a_game_uses() {
        let text = "\"InstallConfigStore\"\n{\n\"Software\"\n{\n\"Valve\"\n{\n\"Steam\"\n{\n\"CompatToolMapping\"\n{\n\"0\"\n{\n\"name\" \"proton_experimental\"\n}\n\"1086940\"\n{\n\"name\" \"turbine-hub\"\n\"config\" \"\"\n}\n\"1091500\"\n{\n\"name\" \"turbine-mods\"\n}\n\"7\"\n{\n\"name\" \"\"\n}\n}\n}\n}\n}\n}\n";
        assert_eq!(chosen_in(text, "1086940").as_deref(), Some(TOOL_NAME));
        assert_eq!(chosen_in(text, "1091500").as_deref(), Some(MOD_TOOL_NAME));
        assert_eq!(chosen_in(text, "7"), None);
        assert_eq!(chosen_in(text, "8"), None);
        assert_eq!(chosen_in("junk {", "7"), None);
        assert_eq!(games_using_in(text, TOOL_NAME), vec!["1086940"]);
        assert_eq!(games_using_in(text, "proton_experimental"), Vec::<String>::new());
    }

    #[test]
    fn the_log_says_which_proton_ran_a_game_last() {
        let log = "2026-11-10 10:00:00 Turbine Hub: app 7 runs with /lib/Proton 9.0 [why: the newest Proton installed (1 numbered Proton found)]\n2026-11-10 10:05:00 Turbine Hub: app 70 runs with /lib/Proton 11.0 [why: Steam's own choice for all games]\n2026-11-10 11:00:00 Turbine Hub: app 7 runs with /tools/GE-Proton10-4 [why: your pick in Turbine Hub for this game]\n";
        assert_eq!(last_launch_in(log, "7"), Some(("/tools/GE-Proton10-4".to_string(), "your pick in Turbine Hub for this game".to_string())));
        assert_eq!(last_launch_in(log, "70").unwrap().0, "/lib/Proton 11.0");
        assert_eq!(last_launch_in(log, "700"), None);
    }

    #[test]
    fn the_steps_start_with_a_steam_restart_only_when_needed() {
        let s = steps("Hades II", false);
        assert_eq!(s.len(), 4);
        assert!(s[0].1.starts_with("Restart Steam once") && s[0].0 == "1");
        let s = steps("Hades II", true);
        assert_eq!(s.len(), 3);
        assert!(s[0].1.contains("Hades II") && s[2].1.contains("\"Turbine Hub\""));
    }
}
