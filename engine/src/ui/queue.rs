//! Work that runs while the player keeps browsing: one line of jobs, one at a time, in the order they were asked
//! for. An install, an update, a removal, a copy from the Windows drive and a long copy in Files wait here for
//! their turn; the top bar says what is being worked on and how many wait, the row of the thing says Working, and
//! a note says how each one ended and how long it took.
//!
//! The rules are decided here, so nothing else has to know them:
//! - What may wait in the line at all (`covered` says what may not): anything that asks for the password, changes
//!   a system setting through the helper, sets the helper up, replaces Turbine Hub itself or writes Steam's own
//!   files still runs alone behind the cover, with its time limits, as before.
//! - Never two jobs that touch the same thing (`keys_of`, `clash`): the second one is refused in plain words.
//! - The app runs a plan's steps (it owns the threads and the host); Files runs its own copy. Both report back
//!   here, and the next job starts only when the one before has ended.
use super::plan::{Plan, Step};
use super::{Ui, b, n, obj, s};
use crate::helper;
use crate::json::Json;
use std::collections::VecDeque;
use std::time::Instant;

/// How many jobs may wait at once. More than this is a slip of the thumb, not a plan.
pub const MOST_WAITING: usize = 12;

/// What a job is made of.
pub enum Work {
    /// A confirmed dialog's steps: the app runs them.
    Plan(Box<Plan>),
    /// A long copy, move or emptying in Files: the explorer runs it on its own thread.
    Files(Box<super::files::Waiting>),
}

/// One job in the line.
pub struct Entry {
    pub id: u64,
    /// The thing it is about, as the bar names it ("Discord", "12 files to Pictures").
    pub what: String,
    /// What it touches (see `keys_of`).
    pub keys: Vec<String>,
    pub work: Work,
}

/// Where a thing stands in the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Working,
    Waiting,
}

struct Running {
    entry: Entry,
    since: Instant,
    /// What it is doing now, when it said ("Copying Elden Ring").
    words: String,
    /// How far it is, 0 to 1, when that is known.
    part: Option<f64>,
}

#[derive(Default)]
pub struct Queue {
    next: u64,
    running: Option<Running>,
    waiting: VecDeque<Entry>,
}

/// The arguments of a flatpak command line that the host runs, after `flatpak-spawn --host flatpak`.
fn flatpak_args(line: &[String]) -> Option<&[String]> {
    line.strip_prefix(&["flatpak-spawn".to_string(), "--host".to_string(), "flatpak".to_string()][..])
}

/// Is this line a job for the privileged helper (a system change)?
fn helper_line(line: &[String]) -> bool {
    line.iter().any(|a| a == helper::BIN)
}

/// May this step wait in the line and run while the player browses? Only what changes nothing of the system and
/// can be stopped cleanly: Flathub installs, updates and removals, and copies into the player's own folders. The
/// markers that only note what was done ride along.
fn free_step(st: &Step) -> bool {
    match st {
        Step::Host(line) => !helper_line(line) && flatpak_args(line).and_then(|a| a.first()).is_some_and(|verb| ["install", "update", "uninstall"].contains(&verb.as_str())),
        // A GE-Proton download: half a gigabyte fetched, checked and unpacked into a folder of its own in Steam's
        // folder for tools. None of Steam's own files change, so it needs no cover.
        Step::CopyGame { .. } | Step::ProtonGet { .. } | Step::Done { .. } => true,
        // Everything else stays behind the cover: among it a storage cleaning and the library pictures, which
        // touch Steam's folders, and taking a Proton out again.
        _ => false,
    }
}

/// Must this plan run alone behind the cover? Yes when it asks for a password, or when any step is a system
/// change, the helper's own setup, Turbine Hub's update or a write to Steam's files; and when it has nothing to
/// run at all (a plan of notes is done at once).
pub fn covered(plan: &Plan) -> bool {
    plan.ask != super::plan::Ask::None || plan.steps.is_empty() || !plan.steps.iter().all(free_step) || plan.steps.iter().all(|st| matches!(st, Step::Done { .. }))
}

/// What a plan touches, one key each: "app:<id>" for a Flathub app ("app:*" for every app at once), "path:<folder>"
/// for a folder being written, "proton:<build>" for a Proton build being fetched or removed ("proton:*" for the
/// newest, whichever that is), "system" for a change through the helper, "self" for Turbine Hub's own update. A
/// storage cleaning touches every app (it has Flatpak drop unused runtimes) and every Proton (it may remove one).
pub fn keys_of(plan: &Plan) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut put = |k: String| {
        if !out.contains(&k) {
            out.push(k);
        }
    };
    for st in &plan.steps {
        match st {
            Step::Host(line) if helper_line(line) => put("system".into()),
            Step::Host(line) => {
                let Some(args) = flatpak_args(line) else { continue };
                let ids: Vec<&String> = args.iter().skip(1).filter(|a| !a.starts_with('-') && a.as_str() != "flathub").collect();
                match args.first().map(String::as_str) {
                    Some("update") if ids.is_empty() => put("app:*".into()),
                    Some("install" | "update" | "uninstall" | "override") => ids.iter().for_each(|id| put(format!("app:{id}"))),
                    _ => {}
                }
            }
            Step::CopyGame { pairs, .. } => pairs.iter().for_each(|(_, to)| put(path_key(to))),
            Step::ProtonGet { tag } => put(format!("proton:{}", if tag.is_empty() { "*" } else { tag.as_str() })),
            Step::ProtonRemove { name } => put(format!("proton:{name}")),
            Step::Storage(_) => {
                put("app:*".into());
                put("proton:*".into());
            }
            Step::HelperInstall | Step::SetPassword => put("system".into()),
            Step::SelfUpdate => put("self".into()),
            _ => {}
        }
    }
    out
}

/// The key of a folder or file a job reads or writes.
pub fn path_key(p: &std::path::Path) -> String {
    format!("path:{}", p.display())
}

/// Do two keys name the same thing? The same key; every app against any one app (and every Proton against any one
/// Proton); a folder against anything in it.
pub fn clash(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    for kind in ["app:", "proton:"] {
        if let (Some(x), Some(y)) = (a.strip_prefix(kind), b.strip_prefix(kind)) {
            return x == "*" || y == "*";
        }
    }
    if let (Some(x), Some(y)) = (a.strip_prefix("path:"), b.strip_prefix("path:")) {
        let (x, y) = (std::path::Path::new(x), std::path::Path::new(y));
        return x.starts_with(y) || y.starts_with(x);
    }
    false
}

impl Queue {
    pub fn is_empty(&self) -> bool {
        self.running.is_none() && self.waiting.is_empty()
    }
    /// How many jobs there are, the running one included.
    pub fn count(&self) -> usize {
        self.waiting.len() + usize::from(self.running.is_some())
    }
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
    pub fn running_id(&self) -> Option<u64> {
        self.running.as_ref().map(|r| r.entry.id)
    }
    /// The name of what is being worked on.
    pub fn running_what(&self) -> Option<&str> {
        self.running.as_ref().map(|r| r.entry.what.as_str())
    }

    /// The job that already touches one of these keys, and where it stands.
    fn holder(&self, keys: &[String]) -> Option<(&Entry, Place)> {
        let hit = |e: &Entry| e.keys.iter().any(|k| keys.iter().any(|x| clash(k, x)));
        self.running.as_ref().map(|r| (&r.entry, Place::Working)).filter(|(e, _)| hit(e)).or_else(|| self.waiting.iter().find(|e| hit(e)).map(|e| (e, Place::Waiting)))
    }

    /// Where the thing with this key stands, when a job touches it.
    pub fn place(&self, key: &str) -> Option<Place> {
        self.holder(&[key.to_string()]).map(|(_, p)| p)
    }

    /// Why a job that touches these keys can't be taken now, in plain words. None when it can.
    pub fn refusal(&self, keys: &[String]) -> Option<String> {
        if let Some((e, place)) = self.holder(keys) {
            return Some(match place {
                Place::Working => format!("{} is being worked on right now. Wait for it to finish, then try again.", e.what),
                Place::Waiting => format!("{} is waiting for its turn already. It can't be asked for twice.", e.what),
            });
        }
        (self.waiting.len() >= MOST_WAITING).then(|| format!("{MOST_WAITING} things are waiting already. Let some of them finish first."))
    }

    /// Puts a job at the end of the line. Refused (with the reason) when it touches what another job touches.
    pub fn add(&mut self, what: &str, keys: Vec<String>, work: Work) -> Result<u64, String> {
        if let Some(why) = self.refusal(&keys) {
            return Err(why);
        }
        self.next += 1;
        self.waiting.push_back(Entry { id: self.next, what: what.to_string(), keys, work });
        Ok(self.next)
    }

    /// The next job's turn: it leaves the line and counts as running. None while one runs or nothing waits.
    fn begin(&mut self) -> Option<&Entry> {
        if self.running.is_some() {
            return None;
        }
        let entry = self.waiting.pop_front()?;
        self.running = Some(Running { entry, since: Instant::now(), words: String::new(), part: None });
        self.running.as_ref().map(|r| &r.entry)
    }

    /// Is the next job in line one of Files' own (the explorer starts those itself)?
    fn files_next(&self) -> bool {
        self.running.is_none() && matches!(self.waiting.front().map(|e| &e.work), Some(Work::Files(_)))
    }

    /// The running job says what it is doing and, when it knows, how far it is.
    pub fn say(&mut self, id: u64, words: &str, part: Option<f64>) {
        if let Some(r) = self.running.as_mut().filter(|r| r.entry.id == id) {
            if !words.is_empty() {
                r.words = words.to_string();
            }
            r.part = part.map(|p| p.clamp(0.0, 1.0)).or(r.part);
        }
    }

    /// The running job ended: it leaves, with how many whole seconds it took.
    fn end(&mut self, id: u64) -> Option<(Entry, u64)> {
        if self.running_id() != Some(id) {
            return None;
        }
        self.running.take().map(|r| (r.entry, r.since.elapsed().as_secs()))
    }

    /// Takes a job that still waits out of the line (Stop on a copy that hasn't started).
    pub fn drop_waiting(&mut self, id: u64) -> bool {
        let before = self.waiting.len();
        self.waiting.retain(|e| e.id != id);
        before != self.waiting.len()
    }

    /// Empties the line (Turbine Hub is closing). Answers how many jobs waited; the running one is the caller's to
    /// stop.
    pub fn clear_waiting(&mut self) -> usize {
        let c = self.waiting.len();
        self.waiting.clear();
        c
    }

    /// The first waiting job of Files, if any (its id).
    pub fn waiting_files(&self) -> Option<u64> {
        self.waiting.iter().find(|e| matches!(e.work, Work::Files(_))).map(|e| e.id)
    }

    /// The bar's words: "Working: Discord (2 more waiting)". "" when nothing runs.
    pub fn words(&self) -> String {
        let Some(r) = &self.running else { return String::new() };
        match self.waiting.len() {
            0 => format!("Working: {}", r.entry.what),
            c => format!("Working: {} ({c} more waiting)", r.entry.what),
        }
    }

    /// The same in a few letters, for a bar with little room: "Discord +2".
    pub fn short(&self) -> String {
        let Some(r) = &self.running else { return String::new() };
        match self.waiting.len() {
            0 => r.entry.what.clone(),
            c => format!("{} +{c}", r.entry.what),
        }
    }

    /// What the question at closing says: "2 things are still running. Close anyway?". "" with nothing in line.
    pub fn closing_words(&self) -> String {
        match self.count() {
            0 => String::new(),
            1 => format!("{} is still being worked on. Close anyway? It is stopped cleanly.", self.running_what().unwrap_or("Something")),
            c => format!("{c} things are still running. Close anyway? The one being worked on is stopped cleanly and the rest are dropped."),
        }
    }
}

/// How long a job took, the way the note says it: "(12 s)", "(3 min)". "" for under a second.
pub fn took_words(secs: u64) -> String {
    match secs {
        0 => String::new(),
        1..=119 => format!(" ({secs} s)"),
        _ => format!(" ({} min)", (secs + 30) / 60),
    }
}

/// The note when a job ends: its own words with the time it took, or what went wrong, named.
pub fn end_words(what: &str, done: &str, result: &Result<(), String>, secs: u64) -> String {
    match result {
        Ok(()) if done.is_empty() => format!("{what}: done{}", took_words(secs)),
        Ok(()) => format!("{}{}", done.trim_end_matches('.'), took_words(secs)),
        Err(why) => format!("{what} didn't work: {why}"),
    }
}

/// The name of the thing a plan is about: the first thing its dialog lists, or its title without the question.
pub fn what_of(plan: &Plan) -> String {
    if !plan.what.is_empty() {
        return plan.what.clone();
    }
    plan.title.trim_end_matches('?').to_string()
}

impl Ui {
    /// Does this plan run alone behind the cover (true), or wait in the line while the player browses?
    pub fn runs_covered(&self, plan: &Plan) -> bool {
        covered(plan)
    }

    /// Why a plan can't start now, with the line as it is: something in it touches the same thing, or the plan
    /// replaces Turbine Hub itself while work still runs.
    pub fn work_refusal(&self, plan: &Plan) -> Option<String> {
        if !self.queue.is_empty() && plan.steps.iter().any(|st| matches!(st, Step::SelfUpdate)) {
            return Some("Turbine Hub updates itself once the work that is running has finished. Try again then.".into());
        }
        self.queue.refusal(&keys_of(plan))
    }

    /// A confirmed plan joins the line. Answers what the window shows: a note that it started or waits, or why it
    /// was refused.
    pub fn enqueue(&mut self, plan: Plan) -> Json {
        if let Some(why) = self.work_refusal(&plan) {
            return obj! {"toast": s(why), "refresh": b(true)};
        }
        let what = what_of(&plan);
        let ahead = self.queue.count();
        match self.queue.add(&what, keys_of(&plan), Work::Plan(Box::new(plan))) {
            Ok(_) => obj! {
                "queued": b(true),
                "refresh": b(true),
                "toast": s(match ahead {
                    0 => format!("{what}: working on it. Keep going, the top bar shows how it is doing."),
                    1 => format!("{what} waits for its turn: 1 thing is ahead of it."),
                    c => format!("{what} waits for its turn: {c} things are ahead of it."),
                }),
            },
            Err(why) => obj! {"toast": s(why), "refresh": b(true)},
        }
    }

    /// The next job's turn, when nothing runs. A plan is handed to the app to run (its id and its steps); a copy
    /// in Files is started here and nothing is handed out.
    pub fn queue_next(&mut self) -> Option<(u64, Plan)> {
        let (id, plan) = match self.queue.begin().map(|e| (e.id, &e.work))? {
            (id, Work::Plan(p)) => (id, Some((**p).clone())),
            (id, Work::Files(_)) => (id, None),
        };
        if let Some(plan) = plan {
            return Some((id, plan));
        }
        // The copy itself: taken out of the running entry (it runs once) and started.
        let work = self.queue.running.as_mut().map(|r| std::mem::replace(&mut r.entry.work, Work::Plan(Box::new(Plan::new("", "", "")))));
        if let Some(Work::Files(w)) = work
            && let Err(why) = super::files::start_waiting(self, id, *w)
        {
            // It couldn't be started: it leaves the line, and Files says why at its next look.
            self.queue.end(id);
            self.files_failed(why);
        }
        None
    }

    /// Starts the next job when it is one of Files' own (the explorer calls this as it adds one, so a copy with
    /// nothing ahead of it starts at once). A plan at the front waits for the app.
    pub(crate) fn queue_kick(&mut self) {
        if self.queue.files_next() {
            let _ = self.queue_next();
        }
    }

    /// Is a job of the line being worked on right now?
    pub fn queue_working(&self) -> bool {
        self.queue.running_id().is_some()
    }

    /// The running job says what it is doing.
    pub fn queue_say(&mut self, id: u64, words: &str, part: Option<f64>) {
        self.queue.say(id, words, part);
    }

    /// A plan from the line ended. What the engine keeps is brought up to date when it worked (`finish`), and the
    /// answer is the note for the window: `{"ok", "toast"}`. None when no such job runs.
    pub fn queue_done(&mut self, id: u64, result: Result<(), String>) -> Option<Json> {
        let (entry, secs) = self.queue.end(id)?;
        let Work::Plan(plan) = entry.work else { return None };
        let result = result.and_then(|()| if self.sample { self.play_sample(&plan) } else { self.finish(&plan) });
        // A made-up machine's jobs take as long as the pictures say.
        let secs = if self.sample { SAMPLE_SECS } else { secs };
        Some(obj! {"ok": b(result.is_ok()), "toast": s(end_words(&entry.what, &plan.done, &result, secs))})
    }

    /// Files' copy ended (the explorer saw it): it leaves the line. Answers how long it took, in words.
    pub(crate) fn queue_files_done(&mut self, id: u64) -> String {
        self.queue.end(id).map(|(_, secs)| took_words(secs)).unwrap_or_default()
    }

    /// Turbine Hub is closing: nothing waits any more, and a copy of Files that runs is told to stop (what was
    /// half copied is removed). Answers the id of a running plan, which the app stops itself.
    pub fn queue_stop(&mut self) -> Option<u64> {
        self.queue.clear_waiting();
        super::files::stop_job(self);
        self.queue.running_id()
    }

    /// What the top bar shows beside the tabs, as JSON: the battery (only where there is one), whether the clock
    /// shows, the work in the line and the updates that are ready.
    pub fn status(&self) -> String {
        let r = self.readings();
        let running = self.queue.running.as_ref();
        let updates = self.updates.len() + self.facts.plugin_updates.len() + usize::from(self.update_ready().is_some());
        obj! {
            "battery": n(r.battery_pct.map(|p| p.min(100) as f64).unwrap_or(-1.0)),
            "charging": b(r.battery_pct.is_some() && r.charging),
            // A Desktop Mode window sits under the desktop's own clock; everywhere else Steam's is hidden.
            "clock": b(!self.window),
            // A picture's clock always says the same.
            "clockText": s(if self.sample { "20:41" } else { "" }),
            "work": s(self.queue.words()),
            "workShort": s(self.queue.short()),
            "detail": s(running.map(|x| x.words.as_str()).unwrap_or("")),
            "part": n(running.and_then(|x| x.part).unwrap_or(-1.0)),
            "jobs": n(self.queue.count() as f64),
            "closing": s(self.queue.closing_words()),
            "updates": n(updates as f64),
        }
        .to_text()
    }

    /// A made-up machine with work in the line: Discord being installed and `waiting` more behind it.
    pub fn sample_queue(&mut self, waiting: usize) {
        let ids = ["com.discordapp.Discord", "com.spotify.Client", "com.obsproject.Studio", "org.mozilla.firefox", "com.moonlight_stream.Moonlight"];
        for id in ids.iter().take(waiting + 1) {
            self.installed.retain(|x| x != id);
            if let Some(Json::Obj(_)) = super::store::act(self, "store", id, "")
                && let Some(plan) = self.confirm()
            {
                let _ = self.enqueue(plan);
            }
        }
        let _ = self.queue_next();
        let id = self.queue.running_id().unwrap_or(0);
        self.queue.say(id, "Installing Discord", None);
    }
}

/// How long every job takes on a made-up machine.
pub const SAMPLE_SECS: u64 = 12;

/// The made-up machine's own press: the job that is being worked on ends now (the pictures and the build's checks
/// use it; a real machine's jobs end when the host says so).
pub(crate) fn act(ui: &mut Ui, action: &str, _arg: &str, value: &str) -> Option<Json> {
    if action != "queue-finish" || !ui.sample {
        return None;
    }
    let id = ui.queue.running_id()?;
    let result = if value.is_empty() { Ok(()) } else { Err(value.to_string()) };
    let mut j = ui.queue_done(id, result).unwrap_or_else(|| obj! {});
    let _ = ui.queue_next();
    j = super::with(j, "refresh", b(true));
    Some(j)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::plan::Ask;

    fn flatpak(args: &[&str]) -> Step {
        Step::Host(["flatpak-spawn", "--host", "flatpak"].iter().chain(args).map(|x| x.to_string()).collect())
    }
    fn install(id: &str) -> Plan {
        let mut p = Plan::new(&format!("Install {id}?"), "", "Install");
        p.steps.push(flatpak(&["install", "-y", "--noninteractive", "flathub", id]));
        p.done = format!("{id} installed");
        p.what = id.to_string();
        p
    }
    fn add(q: &mut Queue, p: Plan) -> Result<u64, String> {
        let (what, keys) = (what_of(&p), keys_of(&p));
        q.add(&what, keys, Work::Plan(Box::new(p)))
    }

    #[test]
    fn installs_updates_removals_and_copies_wait_in_the_line() {
        assert!(!covered(&install("a.b.C")));
        let mut p = Plan::new("Update everything?", "", "Update all");
        p.steps.push(flatpak(&["update", "-y", "--noninteractive"]));
        assert!(!covered(&p));
        let mut p = Plan::new("Remove X?", "", "Remove");
        p.steps.push(flatpak(&["uninstall", "-y", "--noninteractive", "a.b.C"]));
        assert!(!covered(&p));
        let mut p = Plan::new("Copy 1 game?", "", "Copy");
        p.steps.push(Step::CopyGame { name: "Elden Ring".into(), pairs: vec![("/win/ER".into(), "/home/deck/games/ER".into())] });
        p.steps.push(Step::Done { setup: None, brought: Some("windows") });
        assert!(!covered(&p));
        // A GE-Proton download is long and touches none of Steam's own files.
        let mut p = Plan::new("Get GE-Proton10-4?", "", "Get");
        p.steps.push(Step::ProtonGet { tag: "GE-Proton10-4".into() });
        assert!(!covered(&p));
        assert!(p.needs_internet());
    }

    #[test]
    fn a_proton_download_never_meets_its_own_removal_or_a_cleaning() {
        let get = |tag: &str| {
            let mut p = Plan::new("Get?", "", "Get");
            p.steps.push(Step::ProtonGet { tag: tag.into() });
            p
        };
        assert_eq!(keys_of(&get("GE-Proton10-4")), ["proton:GE-Proton10-4"]);
        // The newest, whichever it is when it runs, may be any build.
        assert_eq!(keys_of(&get("")), ["proton:*"]);
        let mut remove = Plan::new("Remove?", "", "Remove");
        remove.steps.push(Step::ProtonRemove { name: "GE-Proton10-4".into() });
        let mut clean = Plan::new("Free space?", "", "Free");
        clean.steps.push(Step::Storage(crate::storage::Job::Undo));
        assert_eq!(keys_of(&clean), ["app:*", "proton:*"]);
        // Removing, cleaning and the library pictures run alone behind the cover.
        assert!(covered(&remove) && covered(&clean));
        for st in [Step::TileArt, Step::TileArtUndo] {
            let mut p = Plan::new("x", "", "y");
            p.steps.push(st);
            assert!(covered(&p));
        }
        let mut q = Queue::default();
        add(&mut q, get("GE-Proton10-4")).unwrap();
        // The same build twice, its removal, the newest (it may be that one) and a cleaning are all refused;
        // another build and an app are taken.
        assert!(add(&mut q, get("GE-Proton10-4")).is_err());
        assert!(q.refusal(&keys_of(&remove)).is_some());
        assert!(q.refusal(&keys_of(&get(""))).is_some());
        assert!(q.refusal(&keys_of(&clean)).is_some());
        assert!(add(&mut q, get("GE-Proton10-3")).is_ok());
        assert!(add(&mut q, install("Discord")).is_ok());
        // A cleaning is refused while any app is in the line, too.
        let mut q = Queue::default();
        add(&mut q, install("Discord")).unwrap();
        assert!(q.refusal(&keys_of(&clean)).is_some());
    }

    #[test]
    fn the_password_the_helper_steams_files_and_the_update_stay_behind_the_cover() {
        // Asking for a password.
        let mut p = install("a.b.C");
        p.ask = Ask::Current;
        assert!(covered(&p));
        // A system change through the helper, alone or mixed with an install.
        let mut p = install("a.b.C");
        p.steps.push(Step::Host(helper::job("decky-install", None)));
        assert!(covered(&p));
        // The helper's own setup, a new password, Turbine Hub's update.
        for st in [Step::HelperInstall, Step::SetPassword, Step::SelfUpdate] {
            let mut p = Plan::new("x", "", "y");
            p.steps.push(st);
            assert!(covered(&p));
        }
        // Writing Steam's files: a tile (with the browser's install before it), the launch tool, the own entry.
        for st in [Step::Tile { name: "Netflix".into(), url: "https://www.netflix.com".into() }, Step::TileRemove { url: "u".into() }, Step::Tool { install: true }, Step::SelfTile { add: true }, Step::FilesTile { add: true }, Step::SelfArt] {
            let mut p = install("com.google.Chrome");
            p.steps.push(st);
            assert!(covered(&p));
        }
        // Another host command (an override, a harmless write) isn't known to be safe to leave running.
        let mut p = Plan::new("x", "", "y");
        p.steps.push(flatpak(&["override", "--user", "--filesystem=/x", "a.b.C"]));
        assert!(covered(&p));
        // Nothing to run, or only notes: done at once, as before.
        assert!(covered(&Plan::new("x", "", "y")));
        let mut p = Plan::new("x", "", "y");
        p.steps.push(Step::Done { setup: Some("games"), brought: None });
        assert!(covered(&p));
    }

    #[test]
    fn a_plan_names_what_it_touches() {
        assert_eq!(keys_of(&install("a.b.C")), ["app:a.b.C"]);
        let mut p = Plan::new("Update everything?", "", "Update all");
        p.steps.push(flatpak(&["update", "-y", "--noninteractive"]));
        assert_eq!(keys_of(&p), ["app:*"]);
        let mut p = Plan::new("x", "", "y");
        p.steps.push(flatpak(&["install", "-y", "--noninteractive", "flathub", "com.google.Chrome"]));
        p.steps.push(flatpak(&["override", "--user", "--filesystem=/run/udev:ro", "com.google.Chrome"]));
        p.steps.push(Step::Host(helper::job("verify", None)));
        p.steps.push(Step::SelfUpdate);
        p.steps.push(Step::CopyGame { name: "g".into(), pairs: vec![("/a".into(), "/lib/g".into())] });
        assert_eq!(keys_of(&p), ["app:com.google.Chrome", "system", "self", "path:/lib/g"]);
    }

    #[test]
    fn the_same_thing_clashes_and_other_things_dont() {
        assert!(clash("app:a", "app:a"));
        assert!(!clash("app:a", "app:b"));
        // Updating everything touches every app.
        assert!(clash("app:*", "app:b") && clash("app:b", "app:*"));
        assert!(!clash("app:*", "system"));
        // A folder and anything inside it; never a folder that only starts with the same letters.
        assert!(clash("path:/home/deck/Games", "path:/home/deck/Games/Elden Ring"));
        assert!(clash("path:/home/deck/Games/Elden Ring", "path:/home/deck/Games"));
        assert!(!clash("path:/home/deck/Games", "path:/home/deck/Games2"));
        assert!(!clash("path:/a", "app:/a"));
    }

    #[test]
    fn jobs_run_one_at_a_time_in_the_order_asked() {
        let mut q = Queue::default();
        assert!(q.is_empty() && q.words().is_empty() && q.closing_words().is_empty());
        let a = add(&mut q, install("Discord")).unwrap();
        let b2 = add(&mut q, install("Spotify")).unwrap();
        let c = add(&mut q, install("OBS")).unwrap();
        assert_eq!((q.count(), q.waiting(), q.running_id()), (3, 3, None));
        assert_eq!(q.begin().map(|e| e.id), Some(a));
        // While one runs nothing else starts.
        assert!(q.begin().is_none());
        assert_eq!(q.words(), "Working: Discord (2 more waiting)");
        assert_eq!(q.short(), "Discord +2");
        assert_eq!(q.place("app:Discord"), Some(Place::Working));
        assert_eq!(q.place("app:Spotify"), Some(Place::Waiting));
        assert_eq!(q.place("app:Firefox"), None);
        // Only the running one can end, and only once.
        assert!(q.end(b2).is_none());
        assert_eq!(q.end(a).map(|(e, _)| e.what), Some("Discord".to_string()));
        assert!(q.end(a).is_none());
        assert_eq!(q.begin().map(|e| e.id), Some(b2));
        assert_eq!(q.words(), "Working: Spotify (1 more waiting)");
        q.end(b2);
        assert_eq!(q.begin().map(|e| e.id), Some(c));
        assert_eq!(q.words(), "Working: OBS");
        assert_eq!(q.short(), "OBS");
        q.end(c);
        assert!(q.is_empty() && q.begin().is_none());
    }

    #[test]
    fn two_jobs_never_touch_the_same_thing() {
        let mut q = Queue::default();
        add(&mut q, install("Discord")).unwrap();
        add(&mut q, install("Spotify")).unwrap();
        q.begin();
        // The one being worked on, and the one that waits: both refused, each in its own words.
        let why = add(&mut q, install("Discord")).unwrap_err();
        assert!(why.starts_with("Discord is being worked on right now"), "{why}");
        let why = add(&mut q, install("Spotify")).unwrap_err();
        assert!(why.starts_with("Spotify is waiting for its turn already"), "{why}");
        // Update all touches every app, so it waits for none of them.
        let mut all = Plan::new("Update everything?", "", "Update all");
        all.steps.push(flatpak(&["update", "-y", "--noninteractive"]));
        assert!(add(&mut q, all.clone()).is_err());
        assert_eq!(q.count(), 2);
        // Something else is taken.
        assert!(add(&mut q, install("OBS")).is_ok());
        // With Update all in line, no single app is taken either.
        let mut q = Queue::default();
        add(&mut q, all).unwrap();
        assert!(add(&mut q, install("Discord")).is_err());
        // Two copies into the same folder, or one into a folder inside the other's.
        let copy = |to: &str| {
            let mut p = Plan::new("Copy?", "", "Copy");
            p.steps.push(Step::CopyGame { name: to.into(), pairs: vec![("/win/x".into(), to.into())] });
            p
        };
        let mut q = Queue::default();
        add(&mut q, copy("/lib/common/ER")).unwrap();
        assert!(add(&mut q, copy("/lib/common/ER")).is_err());
        assert!(add(&mut q, copy("/lib/common/ER/sub")).is_err());
        assert!(add(&mut q, copy("/lib/common/BG3")).is_ok());
    }

    #[test]
    fn the_line_has_an_end() {
        let mut q = Queue::default();
        add(&mut q, install("first")).unwrap();
        q.begin();
        for i in 0..MOST_WAITING {
            add(&mut q, install(&format!("app{i}"))).unwrap();
        }
        let why = add(&mut q, install("one-more")).unwrap_err();
        assert!(why.contains("waiting already"), "{why}");
    }

    #[test]
    fn progress_belongs_to_the_running_job_and_closing_empties_the_line() {
        let mut q = Queue::default();
        let a = add(&mut q, install("Discord")).unwrap();
        let b2 = add(&mut q, install("Spotify")).unwrap();
        q.begin();
        q.say(b2, "not mine", Some(0.9));
        assert!(q.running.as_ref().unwrap().words.is_empty());
        q.say(a, "Installing Discord", Some(1.7));
        assert_eq!(q.running.as_ref().unwrap().words, "Installing Discord");
        assert_eq!(q.running.as_ref().unwrap().part, Some(1.0));
        // Words alone keep how far it was.
        q.say(a, "Almost there", None);
        assert_eq!(q.running.as_ref().unwrap().part, Some(1.0));
        assert_eq!(q.closing_words(), "2 things are still running. Close anyway? The one being worked on is stopped cleanly and the rest are dropped.");
        assert!(q.drop_waiting(b2) && !q.drop_waiting(b2));
        assert_eq!(q.closing_words(), "Discord is still being worked on. Close anyway? It is stopped cleanly.");
        add(&mut q, install("OBS")).unwrap();
        assert_eq!(q.clear_waiting(), 1);
        assert_eq!((q.count(), q.running_id()), (1, Some(a)));
    }

    #[test]
    fn the_note_says_how_it_ended_and_how_long_it_took() {
        assert_eq!(end_words("Discord", "Discord installed", &Ok(()), 12), "Discord installed (12 s)");
        assert_eq!(end_words("Discord", "Discord installed", &Ok(()), 0), "Discord installed");
        assert_eq!(end_words("Everything", "Everything is up to date.", &Ok(()), 200), "Everything is up to date (3 min)");
        assert_eq!(end_words("Discord", "", &Ok(()), 3), "Discord: done (3 s)");
        assert_eq!(end_words("Discord", "Discord installed", &Err("No internet.".into()), 4), "Discord didn't work: No internet.");
        assert_eq!(what_of(&Plan::new("Install Discord?", "", "Install")), "Install Discord");
    }
}
