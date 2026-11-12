use std::path::PathBuf;

/// Where a game came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Store {
    Steam,
    Epic,
    Gog,
    Sideload,
}

impl Store {
    pub fn key(self) -> &'static str {
        match self {
            Store::Steam => "steam",
            Store::Epic => "epic",
            Store::Gog => "gog",
            Store::Sideload => "sideload",
        }
    }

    /// The launcher the player uses for it.
    pub fn launcher(self) -> &'static str {
        match self {
            Store::Steam => "Steam",
            Store::Epic | Store::Gog | Store::Sideload => "Heroic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Store::Steam => "Steam",
            Store::Epic => "Epic",
            Store::Gog => "GOG",
            Store::Sideload => "Added to Heroic",
        }
    }
}

/// How the game runs. It decides how mod loaders hook in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// A Windows game run through Proton (Steam) or Wine/Proton (Heroic).
    Windows,
    /// A native Linux build.
    Native,
    /// Not run yet and nothing in its folder says which.
    Unknown,
}

impl Runtime {
    pub fn key(self) -> &'static str {
        match self {
            Runtime::Windows => "windows",
            Runtime::Native => "native",
            Runtime::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    pub store: Store,
    /// The store's own ID: Steam app ID, Epic app name, GOG product ID or Heroic's sideload ID.
    pub store_id: String,
    pub name: String,
    pub install_dir: PathBuf,
    /// The library (Steam) or folder the game lives in; it tells which drive holds it.
    pub library: PathBuf,
    /// Steam build ID or Heroic version, recorded at each deploy to notice game updates.
    pub build: Option<String>,
    pub runtime: Runtime,
    /// Tall cover art (Steam's 600x900 capsule) if the launcher has it cached on the device.
    pub cover: Option<PathBuf>,
    /// Wide header art, used when there is no tall cover.
    pub header: Option<PathBuf>,
    /// The Proton prefix (Steam compatdata pfx folder) when it exists.
    pub prefix: Option<PathBuf>,
}

impl Game {
    /// Stable ID used everywhere in Turbine Hub: "steam:489830", "epic:Fortnite".
    pub fn id(&self) -> String {
        format!("{}:{}", self.store.key(), self.store_id)
    }
}

/// Result of looking for games. Problems are plain sentences for the status line; a scan never fails as a whole.
#[derive(Debug, Default)]
pub struct Scan {
    pub games: Vec<Game>,
    pub problems: Vec<String>,
}

impl Game {
    /// Plain words for where the game is stored.
    pub fn drive_label(&self) -> &'static str {
        if self.library.starts_with("/run/media") || self.library.starts_with("/media") || self.library.starts_with("/mnt") {
            "SD card or external drive"
        } else {
            "Internal storage"
        }
    }

    /// One JSON object for the interface.
    pub fn to_json(&self) -> String {
        use crate::json::quote;
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| quote(&p.to_string_lossy())).unwrap_or_else(|| "null".into());
        format!(
            "{{\"id\":{},\"store\":{},\"storeLabel\":{},\"launcher\":{},\"storeId\":{},\"name\":{},\"runtime\":{},\"drive\":{},\"installDir\":{},\"build\":{},\"cover\":{},\"header\":{}}}",
            quote(&self.id()),
            quote(self.store.key()),
            quote(self.store.label()),
            quote(self.store.launcher()),
            quote(&self.store_id),
            quote(&self.name),
            quote(self.runtime.key()),
            quote(self.drive_label()),
            quote(&self.install_dir.to_string_lossy()),
            self.build.as_deref().map(quote).unwrap_or_else(|| "null".into()),
            path(&self.cover),
            path(&self.header),
        )
    }
}

impl Scan {
    pub fn games_json(&self) -> String {
        let items: Vec<String> = self.games.iter().map(Game::to_json).collect();
        format!("[{}]", items.join(","))
    }
}

/// The rail's order, as places in `ids`: pinned games first (in pin order), then the last played (most recent
/// first), then the rest as found. Hidden games are left off, except `always` (a game asked for by name, shown even
/// when hidden; "" for none).
pub fn rail_order(ids: &[String], played: &[String], pinned: &[String], hidden: &[String], always: &str) -> Vec<usize> {
    let mut list: Vec<usize> = (0..ids.len()).filter(|&i| !hidden.contains(&ids[i]) || (!always.is_empty() && ids[i] == always)).collect();
    let rank = |i: usize| -> usize {
        let id = &ids[i];
        match (pinned.iter().position(|x| x == id), played.iter().position(|x| x == id)) {
            (Some(p), _) => p,
            (None, Some(o)) => 1000 + o,
            (None, None) => 1000 + 1_000_000 + i,
        }
    };
    list.sort_by_key(|&i| rank(i));
    list
}

/// `rail_order` for the interface's {"ids","played","pinned","hidden","always"}: a JSON list of places.
pub fn rail_order_json(input: &crate::json::Json) -> String {
    use crate::json::Json;
    let texts = |k: &str| -> Vec<String> { input.get(k).map(Json::as_array).unwrap_or(&[]).iter().filter_map(|x| x.as_str().map(str::to_string)).collect() };
    let order = rail_order(&texts("ids"), &texts("played"), &texts("pinned"), &texts("hidden"), input.str_at("always").unwrap_or_default());
    format!("[{}]", order.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
}
