//! What the background helper writes down as it runs, and what the screens make of it: battery readings (time left,
//! use per game) and sleeps (how much each used and what woke the machine). Three small files of one JSON object per
//! line under the data folder's `history`, each trimmed to its newest lines.

use crate::json::{self, Json};
use crate::ui::{arr, b, n, obj, s};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A battery reading a minute for a week.
const KEEP_SAMPLES: usize = 7 * 24 * 60;
const KEEP_SLEEPS: usize = 200;
const KEEP_SESSIONS: usize = 500;
/// A sleep shorter than this says nothing about drain per hour.
const RATE_FROM_SECS: u64 = 20 * 60;
/// A session shorter than this doesn't count toward a game's battery use.
const SESSION_FROM_SECS: u64 = 5 * 60;

fn file(data: &Path, name: &str) -> PathBuf {
    data.join("history").join(name)
}

/// Adds one line, and once the file has grown a fifth past `keep` lines cuts it back to the newest `keep`.
fn append(path: &Path, line: &str, keep: usize) {
    if let Some(d) = path.parent() {
        let _ = fs::create_dir_all(d);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(format!("{line}\n").as_bytes());
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > keep + keep / 5 {
        let _ = crate::write_atomic(path, (lines[lines.len() - keep..].join("\n") + "\n").as_bytes());
    }
}

/// Every readable line of a file, oldest first. A damaged line is skipped.
fn read(path: &Path) -> Vec<Json> {
    fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| json::parse(l).ok()).collect()
}

fn num(j: &Json, k: &str) -> Option<f64> {
    j.get(k).and_then(Json::as_f64)
}

/// One battery reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub at: u64,
    pub pct: u32,
    /// Draw in watts while on battery.
    pub watts: Option<f64>,
    pub charging: bool,
    /// The game running then ("steam:1145360"), or empty.
    pub game: String,
    /// The processor's temperature then, in degrees Celsius, where it could be read.
    pub temp: Option<f64>,
}

pub fn add_sample(data: &Path, x: &Sample) {
    let line = obj! {"t": n(x.at as f64), "pct": n(x.pct as f64), "w": x.watts.map(|w| n((w * 10.0).round() / 10.0)).unwrap_or(Json::Null), "charging": b(x.charging), "game": s(&x.game), "c": x.temp.map(|c| n(c.round())).unwrap_or(Json::Null)};
    append(&file(data, "battery.jsonl"), &line.to_text(), KEEP_SAMPLES);
}

/// The readings taken at or after `since`, oldest first.
pub fn samples(data: &Path, since: u64) -> Vec<Sample> {
    read(&file(data, "battery.jsonl")).iter().filter_map(|j| Some(Sample { at: num(j, "t")? as u64, pct: num(j, "pct")?.clamp(0.0, 100.0) as u32, watts: num(j, "w"), charging: j.get("charging").and_then(Json::as_bool).unwrap_or(false), game: j.str_at("game").unwrap_or("").to_string(), temp: num(j, "c") })).filter(|x| x.at >= since).collect()
}

/// Seconds of battery left, from readings that end now (oldest first). With the energy left in the battery
/// (`wh_now`) it is that over the average draw of the last ten minutes; without it, how fast the percentage fell
/// over the current stretch on battery. None while charging, or before there is enough to go on.
pub fn time_left(samples: &[Sample], now: u64, wh_now: Option<f64>) -> Option<u64> {
    let last = samples.last().filter(|x| !x.charging && now.saturating_sub(x.at) <= 180)?;
    // The stretch on battery that ends with the last reading, with no gap (a sleep) in it.
    let mut from = samples.len() - 1;
    while from > 0 && !samples[from - 1].charging && samples[from].at.saturating_sub(samples[from - 1].at) <= 300 {
        from -= 1;
    }
    let run = &samples[from..];
    let recent: Vec<f64> = run.iter().filter(|x| last.at - x.at <= 600).filter_map(|x| x.watts).filter(|w| *w > 0.0).collect();
    let secs = match wh_now.filter(|wh| *wh > 0.0) {
        Some(wh) if !recent.is_empty() => {
            let avg = recent.iter().sum::<f64>() / recent.len() as f64;
            (avg >= 0.5).then(|| wh / avg * 3600.0)?
        }
        _ => {
            let first = run.iter().find(|x| last.at - x.at <= 7200)?;
            let (span, drop) = (last.at - first.at, first.pct.saturating_sub(last.pct));
            (span >= 600 && drop >= 2).then(|| last.pct as f64 / (drop as f64 / span as f64))?
        }
    };
    Some((secs as u64).min(48 * 3600))
}

/// One line of a chart: the last `span` seconds cut into `buckets` equal parts, oldest first, each the average of
/// what `of` reads from the readings that fall into it. None where nothing was read (the machine slept or was off),
/// so a chart shows a gap there and never a made-up line.
pub fn trend(samples: &[Sample], now: u64, span: u64, buckets: usize, of: impl Fn(&Sample) -> Option<f64>) -> Vec<Option<f64>> {
    let mut sums = vec![(0.0, 0u32); buckets];
    let from = now.saturating_sub(span);
    for x in samples.iter().filter(|x| x.at > from && x.at <= now) {
        if let Some(v) = of(x) {
            let at = (((x.at - from - 1) as u128 * buckets as u128) / span.max(1) as u128) as usize;
            let slot = &mut sums[at.min(buckets.saturating_sub(1))];
            *slot = (slot.0 + v, slot.1 + 1);
        }
    }
    sums.into_iter().map(|(sum, count)| (count > 0).then(|| sum / count as f64)).collect()
}

/// The lowest and highest value of a chart line, when it has any.
pub fn range(line: &[Option<f64>]) -> Option<(f64, f64)> {
    let mut seen = line.iter().flatten().copied();
    let first = seen.next()?;
    Some(seen.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
}

/// A length of time in a few words: "45 min", "3 h 20 min", "2 days".
pub fn time_words(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs % 3600 / 60);
    match (h, m) {
        (0, m) => format!("{} min", m.max(1)),
        (h, _) if h >= 48 => format!("{} days", h / 24),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// One sleep, written when the machine woke.
#[derive(Debug, Clone, PartialEq)]
pub struct Sleep {
    /// When it woke.
    pub at: u64,
    pub secs: u64,
    /// The charge before and after.
    pub before: u32,
    pub after: u32,
    /// Plugged in when it woke: the charge says nothing about what the sleep used.
    pub charging: bool,
    /// What woke it, in plain words. Empty when the kernel didn't say.
    pub wakers: Vec<String>,
}

impl Sleep {
    /// Percent of the battery used per hour asleep. None when plugged in or too short to say.
    pub fn per_hour(&self) -> Option<f64> {
        (!self.charging && self.secs >= RATE_FROM_SECS && self.before >= self.after).then(|| (self.before - self.after) as f64 * 3600.0 / self.secs as f64)
    }

    /// The line under a sleep in the list.
    pub fn words(&self) -> String {
        let used = match self.per_hour() {
            _ if self.charging => "Plugged in".to_string(),
            Some(r) if r >= 0.05 => format!("{}% used, {} an hour", self.before.saturating_sub(self.after), percent(r)),
            _ if self.before > self.after => format!("{}% used", self.before - self.after),
            _ => "No battery used".to_string(),
        };
        if self.wakers.is_empty() { used } else { format!("{used} \u{b7} woken by {}", self.wakers.join(", ")) }
    }
}

/// A rate as "0.4%" or "12%".
pub fn percent(v: f64) -> String {
    if v < 10.0 { format!("{:.1}%", v) } else { format!("{:.0}%", v) }
}

pub fn add_sleep(data: &Path, x: &Sleep) {
    let line = obj! {"at": n(x.at as f64), "secs": n(x.secs as f64), "before": n(x.before as f64), "after": n(x.after as f64), "charging": b(x.charging), "wakers": arr(x.wakers.iter().map(s))};
    append(&file(data, "sleep.jsonl"), &line.to_text(), KEEP_SLEEPS);
}

/// The newest `max` sleeps, newest first.
pub fn sleeps(data: &Path, max: usize) -> Vec<Sleep> {
    let mut v: Vec<Sleep> = read(&file(data, "sleep.jsonl"))
        .iter()
        .filter_map(|j| Some(Sleep { at: num(j, "at")? as u64, secs: num(j, "secs")? as u64, before: num(j, "before")? as u32, after: num(j, "after")? as u32, charging: j.get("charging").and_then(Json::as_bool).unwrap_or(false), wakers: j.get("wakers").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect() }))
        .collect();
    v.reverse();
    v.truncate(max);
    v
}

/// What the checkup wants to know about the last sleep that was long enough to measure: percent used per hour and
/// what woke the machine.
pub fn last_sleep(data: &Path) -> Option<(f64, Vec<String>)> {
    sleeps(data, 20).into_iter().find_map(|x| Some((x.per_hour()?, x.wakers)))
}

/// One run of a game, written when it ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub game: String,
    pub name: String,
    pub start: u64,
    pub end: u64,
    /// Seconds of it the machine spent asleep with the game still open.
    pub asleep: u64,
    /// The charge at the start and the end, on a machine with a battery. What the sleeps in between used is
    /// already taken off the start, so the difference is what playing used.
    pub before: Option<u32>,
    pub after: Option<u32>,
    /// On battery from start to end.
    pub on_battery: bool,
}

impl Session {
    /// Seconds of play: the session without its sleeps.
    pub fn played(&self) -> u64 {
        self.end.saturating_sub(self.start).saturating_sub(self.asleep)
    }
}

pub fn add_session(data: &Path, x: &Session) {
    let pct = |v: Option<u32>| v.map(|p| n(p as f64)).unwrap_or(Json::Null);
    let line = obj! {"game": s(&x.game), "name": s(&x.name), "start": n(x.start as f64), "end": n(x.end as f64), "asleep": n(x.asleep as f64), "before": pct(x.before), "after": pct(x.after), "battery": b(x.on_battery)};
    append(&file(data, "sessions.jsonl"), &line.to_text(), KEEP_SESSIONS);
}

/// Every session, oldest first.
pub fn sessions(data: &Path) -> Vec<Session> {
    read(&file(data, "sessions.jsonl"))
        .iter()
        .filter_map(|j| Some(Session { game: j.str_at("game")?.to_string(), name: j.str_at("name").unwrap_or("").to_string(), start: num(j, "start")? as u64, end: num(j, "end")? as u64, asleep: num(j, "asleep").unwrap_or(0.0) as u64, before: num(j, "before").map(|v| v as u32), after: num(j, "after").map(|v| v as u32), on_battery: j.get("battery").and_then(Json::as_bool).unwrap_or(false) }))
        .collect()
}

/// Battery use of one game over its sessions on battery.
#[derive(Debug, Clone, PartialEq)]
pub struct GameUse {
    pub game: String,
    pub name: String,
    /// Percent of the battery per hour of play.
    pub per_hour: f64,
    pub sessions: usize,
    /// Seconds played on battery.
    pub secs: u64,
}

/// Battery use per game, the thirstiest first. Only sessions that ran on battery from start to end and lasted a few
/// minutes count.
pub fn use_by_game(sessions: &[Session]) -> Vec<GameUse> {
    let mut out: Vec<(GameUse, u32)> = Vec::new();
    for x in sessions.iter().filter(|x| x.on_battery && x.played() >= SESSION_FROM_SECS) {
        let (Some(before), Some(after)) = (x.before, x.after) else { continue };
        if after > before {
            continue;
        }
        let i = out.iter().position(|(g, _)| g.game == x.game).unwrap_or_else(|| {
            out.push((GameUse { game: x.game.clone(), name: x.name.clone(), per_hour: 0.0, sessions: 0, secs: 0 }, 0));
            out.len() - 1
        });
        let (g, used) = &mut out[i];
        g.sessions += 1;
        g.secs += x.played();
        *used += before - after;
        if !x.name.is_empty() {
            g.name = x.name.clone();
        }
    }
    let mut v: Vec<GameUse> = out.into_iter().map(|(mut g, used)| {
        g.per_hour = used as f64 * 3600.0 / g.secs as f64;
        g
    }).collect();
    v.sort_by(|a, b| b.per_hour.total_cmp(&a.per_hour).then_with(|| a.name.cmp(&b.name)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-history-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn sample(at: u64, pct: u32, watts: f64) -> Sample {
        Sample { at, pct, watts: Some(watts), charging: false, game: String::new(), temp: None }
    }

    #[test]
    fn a_chart_line_averages_each_part_and_leaves_gaps_empty() {
        // Four hours in four parts: readings in the first and the last hour only.
        let now = 4 * 3600;
        let run = vec![sample(600, 90, 10.0), sample(1800, 80, 10.0), Sample { temp: Some(60.0), ..sample(now - 60, 50, 10.0) }, Sample { temp: Some(70.0), ..sample(now, 40, 10.0) }];
        let pct = trend(&run, now, 4 * 3600, 4, |x| Some(x.pct as f64));
        assert_eq!(pct, vec![Some(85.0), None, None, Some(45.0)]);
        // Only the readings that have the value count.
        assert_eq!(trend(&run, now, 4 * 3600, 4, |x| x.temp), vec![None, None, None, Some(65.0)]);
        // A reading older than the span, or from the future, is left out.
        assert_eq!(trend(&run, now, 3600, 2, |x| Some(x.pct as f64)), vec![None, Some(45.0)]);
        assert_eq!(trend(&run, 1000, 1000, 1, |x| Some(x.pct as f64)), vec![Some(90.0)]);
        assert_eq!(range(&pct), Some((45.0, 85.0)));
        assert_eq!(range(&[None, None]), None);
    }

    #[test]
    fn time_left_from_the_draw_or_from_the_fall() {
        // 30 Wh left at an average of 15 W: two hours, whatever one spike says.
        let run: Vec<Sample> = (0..10).map(|i| sample(1000 + i * 60, 80, if i == 9 { 29.0 } else { 13.0 + (i % 2) as f64 * 2.0 })).collect();
        let left = time_left(&run, 1560, Some(30.0)).unwrap();
        assert!((6400..=7300).contains(&left), "{left}");
        // No energy reading: 10% fell in 30 minutes with 60% left, so three hours.
        let run: Vec<Sample> = (0..=30).map(|i| Sample { watts: None, ..sample(i * 60, 70 - (i / 3) as u32, 0.0) }).collect();
        assert_eq!(time_left(&run, 1800, None), Some(3 * 3600));
        // Not enough yet, charging, or a reading that is old: nothing is claimed.
        assert_eq!(time_left(&run[..5], 240, None), None);
        assert_eq!(time_left(&[Sample { charging: true, ..sample(0, 50, 10.0) }], 0, Some(20.0)), None);
        assert_eq!(time_left(&[sample(0, 50, 10.0)], 1000, Some(20.0)), None);
        assert_eq!(time_left(&[], 0, Some(20.0)), None);
        // One reading and the energy left is enough for a first answer.
        assert_eq!(time_left(&[sample(0, 50, 10.0)], 30, Some(20.0)), Some(7200));
        // The time before a charge or a sleep doesn't count: only the stretch since.
        let mut run = vec![sample(0, 90, 40.0), Sample { charging: true, ..sample(60, 90, 0.0) }, sample(120, 91, 10.0)];
        assert_eq!(time_left(&run, 120, Some(20.0)), Some(7200));
        run = vec![sample(0, 90, 40.0), sample(4000, 85, 10.0)];
        assert_eq!(time_left(&run, 4000, Some(20.0)), Some(7200));
        // A machine that barely draws anything isn't promised weeks.
        assert_eq!(time_left(&[sample(0, 50, 0.6)], 0, Some(90.0)), Some(48 * 3600));
        assert_eq!((time_words(59), time_words(45 * 60), time_words(3 * 3600), time_words(3 * 3600 + 20 * 60), time_words(50 * 3600)), ("1 min".into(), "45 min".into(), "3 h".into(), "3 h 20 min".into(), "2 days".into()));
    }

    #[test]
    fn samples_round_trip_and_the_file_stays_small() {
        let d = data("samples");
        // A file that has grown a fifth past its size is cut back at the next reading.
        let full = KEEP_SAMPLES + KEEP_SAMPLES / 5;
        let text: String = (0..full).map(|k| format!("{{\"t\":{k},\"pct\":50,\"w\":null,\"charging\":false,\"game\":\"\"}}\n")).collect();
        fs::create_dir_all(d.join("history")).unwrap();
        fs::write(file(&d, "battery.jsonl"), text).unwrap();
        for i in 0..5 {
            add_sample(&d, &Sample { at: (full + i) as u64, pct: 77, watts: Some(14.24), charging: false, game: "steam:1".into(), temp: Some(61.4) });
        }
        let all = samples(&d, 0);
        assert!(all.len() <= KEEP_SAMPLES + 5, "{}", all.len());
        assert_eq!(all.last().unwrap(), &Sample { at: (full + 4) as u64, pct: 77, watts: Some(14.2), charging: false, game: "steam:1".into(), temp: Some(61.0) });
        assert_eq!(samples(&d, all.last().unwrap().at).len(), 1);
        // A damaged line is skipped, the rest still reads.
        fs::write(file(&d, "battery.jsonl"), "garbage\n{\"t\":5,\"pct\":40}\n{\"pct\":1}\n").unwrap();
        assert_eq!(samples(&d, 0), vec![Sample { at: 5, pct: 40, watts: None, charging: false, game: String::new(), temp: None }]);
        let _ = fs::remove_dir_all(&d);
        assert!(samples(&d, 0).is_empty());
    }

    #[test]
    fn sleeps_say_what_they_used_and_who_woke_the_machine() {
        let d = data("sleeps");
        assert_eq!(last_sleep(&d), None);
        let long = Sleep { at: 50_000, secs: 8 * 3600, before: 80, after: 40, charging: false, wakers: vec!["Controller".into()] };
        add_sleep(&d, &long);
        add_sleep(&d, &Sleep { at: 60_000, secs: 300, before: 40, after: 39, charging: false, wakers: vec![] });
        add_sleep(&d, &Sleep { at: 70_000, secs: 7200, before: 39, after: 80, charging: true, wakers: vec![] });
        let all = sleeps(&d, 10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[2], long);
        assert_eq!(all[2].words(), "40% used, 5.0% an hour \u{b7} woken by Controller");
        assert_eq!(all[1].words(), "1% used");
        assert_eq!(all[0].words(), "Plugged in");
        assert_eq!(Sleep { at: 0, secs: 7200, before: 50, after: 50, charging: false, wakers: vec![] }.words(), "No battery used");
        // The checkup hears about the last sleep that could be measured, not the short or plugged-in ones after it.
        assert_eq!(last_sleep(&d), Some((5.0, vec!["Controller".to_string()])));
        assert_eq!(sleeps(&d, 1).len(), 1);
        assert_eq!((percent(0.44), percent(12.3)), ("0.4%".into(), "12%".into()));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn battery_use_per_game_counts_only_full_sessions_on_battery() {
        let d = data("sessions");
        let play = |game: &str, name: &str, start: u64, mins: u64, before: u32, after: u32, on_battery: bool| Session { game: game.into(), name: name.into(), start, end: start + mins * 60, asleep: 0, before: Some(before), after: Some(after), on_battery };
        for x in [play("steam:1", "Hades II", 0, 60, 90, 70, true), play("steam:1", "Hades II", 10_000, 120, 100, 54, true), play("steam:2", "Cyberpunk 2077", 20_000, 30, 80, 60, true), play("steam:2", "Cyberpunk 2077", 30_000, 90, 60, 90, false), play("steam:3", "Balatro", 40_000, 2, 50, 50, true)] {
            add_session(&d, &x);
        }
        add_session(&d, &Session { game: "steam:4".into(), name: "On a desktop".into(), start: 0, end: 4000, asleep: 0, before: None, after: None, on_battery: false });
        // A night asleep in the middle of a session isn't play: two hours of play that used 30%.
        add_session(&d, &Session { game: "steam:5".into(), name: "Hollow Knight".into(), start: 50_000, end: 50_000 + 10 * 3600, asleep: 8 * 3600, before: Some(80), after: Some(50), on_battery: true });
        let all = sessions(&d);
        assert_eq!(all.len(), 7);
        assert_eq!(all[6].played(), 7200);
        let by = use_by_game(&all);
        assert_eq!(by.iter().map(|g| (g.name.as_str(), g.per_hour.round() as u32, g.sessions, g.secs)).collect::<Vec<_>>(), vec![("Cyberpunk 2077", 40, 1, 1800), ("Hades II", 22, 2, 10_800), ("Hollow Knight", 15, 1, 7200)]);
        let _ = fs::remove_dir_all(&d);
    }
}
