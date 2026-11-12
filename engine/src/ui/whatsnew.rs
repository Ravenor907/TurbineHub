//! What's new: after Turbine Hub was updated, the notes of the version that is running now, shown once. The notes
//! come from the releases feed on GitHub (the same one the update check reads for test builds); the app fetches
//! them once per version and keeps them in the cache folder. Settings, Updates has a row that shows them again.
use super::plan::toast;
use super::{Ui, arr, b, obj, s};
use crate::json::Json;
use std::path::{Path, PathBuf};

/// The most lines of notes the screen shows; a longer list ends with where the rest is.
const MOST_LINES: usize = 12;
const MOST_CHARS: usize = 220;

/// Where the version whose notes were last shown is kept: one line in Turbine Hub's folder.
fn seen_file(folder: &Path) -> PathBuf {
    folder.join("whats-new-seen")
}

/// Where a version's notes are kept once fetched.
pub fn notes_file(cache: &Path, version: &str) -> PathBuf {
    let safe: String = version.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    cache.join(format!("whats-new-{safe}.txt"))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&nbsp;", " ").replace("&amp;", "&")
}

/// Plain lines out of a piece of HTML: a new line at every paragraph, list item, heading and break; every other
/// tag is dropped. Lines that only carry a checksum (64 hex digits) are left out: they are for the installer.
fn plain_lines(html: &str) -> Vec<String> {
    let mut text = String::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[at + 1..at + end].trim_start_matches('/').to_ascii_lowercase();
        let name: String = tag.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if ["p", "li", "br", "div", "ul", "ol", "tr", "h1", "h2", "h3", "h4", "h5", "h6"].contains(&name.as_str()) {
            text.push('\n');
        }
        rest = &rest[at + end + 1..];
    }
    text.push_str(rest);
    let checksum = |l: &str| l.split(|c: char| !c.is_ascii_hexdigit()).any(|run| run.len() == 64);
    unescape(&text).lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty() && !checksum(l)).collect()
}

/// The notes of the release with this tag ("v0.4") out of GitHub's releases feed, as plain lines. None when the
/// feed has no such release or its notes are empty.
pub fn notes_from_feed(feed: &str, tag: &str) -> Option<Vec<String>> {
    let mark = format!("/releases/tag/{tag}\"");
    for entry in feed.split("<entry>").skip(1) {
        let entry = entry.split("</entry>").next().unwrap_or("");
        if !entry.contains(&mark) {
            continue;
        }
        let from = entry.find("<content")?;
        let body = &entry[from..];
        let body = &body[body.find('>')? + 1..];
        let body = &body[..body.find("</content>")?];
        // The feed carries the notes as escaped HTML.
        let lines = plain_lines(&unescape(body));
        return (!lines.is_empty()).then_some(lines);
    }
    None
}

/// Keeps a version's notes for the screen.
pub fn keep(cache: &Path, version: &str, lines: &[String]) {
    let _ = std::fs::create_dir_all(cache);
    let _ = crate::write_atomic(&notes_file(cache, version), lines.join("\n").as_bytes());
}

fn kept(ui: &Ui) -> Vec<String> {
    std::fs::read_to_string(notes_file(&ui.cache, &ui.ctx.version)).unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

fn seen(ui: &Ui) -> Option<String> {
    std::fs::read_to_string(seen_file(&ui.folder())).ok().map(|t| t.trim().to_string())
}

fn mark_seen(ui: &Ui) {
    let _ = std::fs::create_dir_all(ui.folder());
    let _ = crate::write_atomic(&seen_file(&ui.folder()), ui.ctx.version.as_bytes());
}

/// The notes as the one dialog shows them.
fn screen(ui: &Ui, lines: &[String]) -> Json {
    let cut = |l: &String| if l.chars().count() > MOST_CHARS { format!("{}...", l.chars().take(MOST_CHARS).collect::<String>().trim_end()) } else { l.clone() };
    let mut items: Vec<Json> = lines.iter().take(MOST_LINES).map(|l| obj! {"title": s(cut(l)), "line": s("")}).collect();
    if lines.len() > MOST_LINES {
        items.push(obj! {"title": s("And more"), "line": s("The rest is on the release's page on GitHub.")});
    }
    obj! {"dialog": obj! {
        "title": s(format!("What's new in Turbine Hub {}", ui.ctx.version)),
        "text": s("Turbine Hub was updated. This is what changed."),
        "items": arr(items),
        "confirm": s(""),
        "cancel": s("Close"),
        "ask": s(""),
        "danger": b(false),
        "guide": b(true),
    }}
}

impl Ui {
    /// Should the app fetch this version's notes? Only after an update (another version's notes were the last
    /// ones seen) and only while they aren't kept yet. A first start just notes the version: nothing is new then.
    pub fn whats_new_wanted(&self) -> bool {
        match seen(self) {
            None => {
                if !self.sample {
                    mark_seen(self);
                }
                false
            }
            Some(v) => v != self.ctx.version && kept(self).is_empty(),
        }
    }

    /// The What's new screen, when it is due: this version's notes are here and weren't shown yet. Shown once.
    pub fn whats_new_due(&mut self) -> Option<Json> {
        if seen(self).is_none_or(|v| v == self.ctx.version) {
            return None;
        }
        let lines = kept(self);
        if lines.is_empty() {
            return None;
        }
        mark_seen(self);
        Some(screen(self, &lines))
    }

    /// A made-up machine that was just updated, with the new version's notes fetched.
    pub fn sample_whats_new(&mut self) {
        let _ = std::fs::create_dir_all(self.folder());
        let _ = std::fs::write(seen_file(&self.folder()), "0.0");
        keep(&self.cache, &self.ctx.version, &["Store rows show their state and their button: Install, Open or Update, one press.".to_string(), "Installs, updates and copies run while you keep browsing. The top bar shows the line.".to_string(), "The top bar shows the battery and the clock.".to_string(), "Files: long copies wait their turn with everything else.".to_string()]);
    }
}

/// The row of Settings, Updates that shows the notes again.
pub(crate) fn row(ui: &Ui) -> Json {
    let line = if kept(ui).is_empty() { "The notes of this version show here once they were fetched".to_string() } else { format!("What changed in Turbine Hub {}", ui.ctx.version) };
    super::with(super::with(super::row("whats-new", "", "button", "What's new", &line), "button", s("Read")), "icon", s("info"))
}

pub(crate) fn act(ui: &mut Ui, action: &str, _arg: &str, _value: &str) -> Option<Json> {
    match action {
        // The row in Settings: the notes again, whenever asked.
        "whats-new" => {
            let lines = kept(ui);
            if lines.is_empty() {
                return Some(toast(if ui.offline { "The notes for this version aren't here yet. They are fetched once the internet is there." } else { "The notes for this version aren't here yet. Look again in a moment." }));
            }
            mark_seen(ui);
            Some(screen(ui, &lines))
        }
        // The window asks at its start and when the notes arrive: the screen once, or nothing.
        "whats-new-due" => Some(ui.whats_new_due().unwrap_or_else(|| obj! {})),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>tag:github.com,2008:Repository/1/v0.5</id>
    <link rel="alternate" type="text/html" href="https://github.com/Ravenor907/TurbineHub/releases/tag/v0.5"/>
    <title>Turbine Hub 0.5</title>
    <content type="html">&lt;h2&gt;New&lt;/h2&gt;&lt;ul&gt;&lt;li&gt;Store rows show Install &amp;amp; Open&lt;/li&gt;&lt;li&gt;Work runs   while you browse&lt;/li&gt;&lt;/ul&gt;&lt;p&gt;SHA-256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef&lt;/p&gt;</content>
  </entry>
  <entry>
    <link rel="alternate" type="text/html" href="https://github.com/Ravenor907/TurbineHub/releases/tag/v0.4"/>
    <content type="html">&lt;p&gt;Older notes&lt;br&gt;Second line&lt;/p&gt;</content>
  </entry>
  <entry>
    <link rel="alternate" type="text/html" href="https://github.com/Ravenor907/TurbineHub/releases/tag/v0.3"/>
    <content type="html"></content>
  </entry>
</feed>"#;

    #[test]
    fn the_notes_of_one_release_come_out_of_the_feed_as_plain_lines() {
        assert_eq!(notes_from_feed(FEED, "v0.5").unwrap(), ["New", "Store rows show Install & Open", "Work runs while you browse"]);
        assert_eq!(notes_from_feed(FEED, "v0.4").unwrap(), ["Older notes", "Second line"]);
        // Empty notes, a release the feed doesn't have, and something that isn't a feed.
        assert_eq!(notes_from_feed(FEED, "v0.3"), None);
        assert_eq!(notes_from_feed(FEED, "v0.6"), None);
        assert_eq!(notes_from_feed("not a feed", "v0.5"), None);
        // v0.4 never matches v0.45.
        assert_eq!(notes_from_feed(&FEED.replace("v0.4\"", "v0.45\""), "v0.4"), None);
    }

    #[test]
    fn whats_new_shows_once_after_an_update_and_never_on_a_first_start() {
        let dir = std::env::temp_dir().join(format!("turbinehub-whatsnew-{}", std::process::id()));
        let mut u = Ui::sample(&dir, crate::ui::HANDHELD, Some(crate::settings::Mode::Guided));
        // A first start (nothing seen yet): nothing is wanted and nothing shows.
        assert!(!u.whats_new_wanted());
        assert!(u.whats_new_due().is_none());
        let j = crate::json::parse(&u.act("whats-new", "", "")).unwrap();
        assert!(j.str_at("toast").unwrap().starts_with("The notes for this version aren't here yet"));
        // After an update the notes are wanted until they are kept, then shown once.
        std::fs::create_dir_all(u.folder()).unwrap();
        std::fs::write(seen_file(&u.folder()), "0.0").unwrap();
        assert!(u.whats_new_wanted());
        assert!(u.whats_new_due().is_none());
        keep(&u.cache, &u.ctx.version.clone(), &["One".to_string(), "Two".to_string()]);
        assert!(!u.whats_new_wanted());
        let d = u.whats_new_due().expect("due once the notes are here");
        let d = d.get("dialog").unwrap();
        assert_eq!(d.str_at("title"), Some(format!("What's new in Turbine Hub {}", u.ctx.version).as_str()));
        assert_eq!(d.get("items").unwrap().as_array().len(), 2);
        assert_eq!(d.str_at("confirm"), Some(""));
        assert!(u.whats_new_due().is_none());
        assert_eq!(crate::json::parse(&u.act("whats-new-due", "", "")).unwrap(), obj! {});
        // The row in Settings shows them again whenever asked.
        let again = crate::json::parse(&u.act("whats-new", "", "")).unwrap();
        assert!(again.get("dialog").is_some());
        // A long list is cut, and says where the rest is.
        let many: Vec<String> = (0..30).map(|i| format!("Line {i}")).collect();
        keep(&u.cache, &u.ctx.version.clone(), &many);
        let d = crate::json::parse(&u.act("whats-new", "", "")).unwrap();
        assert_eq!(d.get("dialog").unwrap().get("items").unwrap().as_array().len(), MOST_LINES + 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
