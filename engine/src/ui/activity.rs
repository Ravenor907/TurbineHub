//! The activity list on the Veteran dashboard: one line per thing Turbine Hub did, newest first, kept in the
//! player's own folder.
use std::path::Path;

const KEEP: usize = 200;

fn file(folder: &Path) -> std::path::PathBuf {
    folder.join("activity.log")
}

/// Adds one line (tabs and line ends in it become spaces), keeping the newest `KEEP`.
pub fn note(folder: &Path, at: u64, text: &str) {
    let mut lines: Vec<String> = std::fs::read_to_string(file(folder)).unwrap_or_default().lines().map(String::from).collect();
    lines.push(format!("{at}\t{}", text.replace(['\t', '\n', '\r'], " ")));
    let from = lines.len().saturating_sub(KEEP);
    let _ = crate::write_atomic(&file(folder), (lines[from..].join("\n") + "\n").as_bytes());
}

/// The newest `max` lines, newest first, as (when, words).
pub fn read(folder: &Path, max: usize) -> Vec<(u64, String)> {
    let text = std::fs::read_to_string(file(folder)).unwrap_or_default();
    let mut v: Vec<(u64, String)> = text.lines().filter_map(|l| l.split_once('\t')).filter_map(|(t, w)| Some((t.parse().ok()?, w.to_string()))).collect();
    v.reverse();
    v.truncate(max);
    v
}

/// How long ago, in a few characters.
pub fn ago(now: u64, at: u64) -> String {
    let d = now.saturating_sub(at);
    match d {
        0..60 => "now".into(),
        60..3600 => format!("{} min", d / 60),
        3600..86_400 => format!("{} h", d / 3600),
        _ => format!("{} d", d / 86_400),
    }
}
