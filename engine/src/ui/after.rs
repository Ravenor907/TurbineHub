//! What a dialog's work leaves behind once it worked: the steps that need the interface's own state (a report, the
//! setup file, the first-run steps), and what the screens know about the host (installed apps, updates), brought up
//! to date at once so a page never shows the state from before the change. On the made-up machine the steps that
//! only touch its own folders run for real, so its pages change the way a real machine's do.
use super::plan::{Plan, Step};
use super::{Ui, games, settings, store};
use crate::json::quote;
use crate::saves::{self, Job, Ran};
use std::path::PathBuf;

impl Ui {
    /// Called once every step of a confirmed dialog worked.
    pub fn finish(&mut self, plan: &Plan) -> Result<(), String> {
        let now = crate::now();
        for st in &plan.steps {
            match st {
                Step::Report { game } => {
                    let dir = games::reports_dir(self);
                    let text = games::report_text(self, game, &now.to_string());
                    std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(crate::report::file_name(&now.to_string())), text)).map_err(|e| format!("Couldn't write the report: {e}"))?;
                }
                Step::SaveSetup => {
                    let file = settings::setup_file(self);
                    std::fs::create_dir_all(self.folder()).and_then(|_| std::fs::write(&file, settings::setup_json(self))).map_err(|e| format!("Couldn't save the setup: {e}"))?;
                }
                Step::Done { setup, brought } => {
                    let mut s = self.settings();
                    if let Some(x) = setup.filter(|x| !s.setup_done.iter().any(|d| d == x)) {
                        s.setup_done.push(x.to_string());
                    }
                    if let Some(x) = brought.filter(|x| !s.brought.iter().any(|d| d == x)) {
                        s.brought.push(x.to_string());
                    }
                    self.save(&s);
                }
                Step::SetupFinished { boost } => {
                    let mut s = self.settings();
                    s.setup_done = crate::settings::SETUP_STEPS.iter().map(|x| x.to_string()).collect();
                    s.game_boost = s.game_boost || *boost;
                    self.save(&s);
                }
                Step::Choice { key, on } => {
                    let mut s = self.settings();
                    super::plan::set_choice(&mut s, key, *on);
                    self.save(&s);
                }
                Step::Host(line) => self.took(line),
                _ => {}
            }
        }
        if !plan.done.is_empty() {
            super::note_activity(&self.folder(), now, &plan.done);
        }
        Ok(())
    }

    /// What a flatpak line that worked changed: an app installed, updated or removed.
    fn took(&mut self, line: &[String]) {
        let Some(args) = line.strip_prefix(&["flatpak-spawn".to_string(), "--host".to_string(), "flatpak".to_string()][..]) else { return };
        let ids: Vec<&String> = args.iter().skip(1).filter(|a| !a.starts_with('-') && a.as_str() != "flathub").collect();
        let ours = |id: &str| store::APPS.iter().any(|a| a.id == id);
        match args.first().map(String::as_str) {
            Some("install") => {
                for id in ids.into_iter().filter(|id| ours(id)) {
                    if !self.installed.contains(id) {
                        self.installed.push(id.clone());
                    }
                    self.updates.retain(|u| u != id);
                    store::note_made(&self.folder(), id, true);
                }
            }
            Some("update") if ids.is_empty() => self.updates.clear(),
            Some("update") => self.updates.retain(|u| !ids.contains(&u)),
            Some("uninstall") => {
                for id in ids.into_iter().filter(|id| ours(id)) {
                    self.installed.retain(|a| a != id);
                    self.updates.retain(|u| u != id);
                    store::note_made(&self.folder(), id, false);
                }
            }
            _ => return,
        }
        self.facts.updates = store::APPS.iter().filter(|a| self.updates.iter().any(|u| u == a.id)).map(|a| a.name.to_string()).collect();
        // What the background helper wrote down before this no longer counts.
        self.updates_own = true;
    }

    /// A confirmed dialog on the made-up machine: the steps that touch only its own folders run (the launch tool,
    /// Proton picks, save snapshots with a stand-in for Ludusavi); nothing reaches the real system. Then the same
    /// bookkeeping as on a real machine.
    pub fn play_sample(&mut self, plan: &Plan) -> Result<(), String> {
        if !self.sample {
            return Err("Only on the made-up machine.".into());
        }
        let (home, folder, data) = (self.ctx.home.clone(), self.folder(), self.data.clone());
        for st in &plan.steps {
            match st {
                Step::Tool { .. } | Step::ProtonPick { .. } | Step::ProtonUndo { .. } => super::launch::run_step(st, &home, &folder).unwrap_or(Ok(()))?,
                Step::Saves(job @ (Job::Backup { .. } | Job::Restore { .. } | Job::Undo { .. })) => {
                    let places = saves::Places { folder: &folder, data: &data, home: &home };
                    saves::run(job, &places, crate::now(), &|_: String| {}, &mut stand_in_ludusavi(job))?;
                }
                _ => {}
            }
        }
        self.finish(plan)
    }
}

/// Answers Ludusavi's commands the way Ludusavi does, writing a small save into each backup folder it is given.
fn stand_in_ludusavi(job: &Job) -> impl FnMut(&[String]) -> Result<Ran, String> {
    let name = match job {
        Job::Backup { game } | Job::Restore { game, .. } | Job::Undo { game } => game.name.clone(),
        _ => String::new(),
    };
    move |line: &[String]| {
        let title = line.iter().position(|a| a == "--").and_then(|i| line.get(i + 1)).cloned().unwrap_or_else(|| name.clone());
        if line.iter().any(|a| a == "backup")
            && let Some(dir) = line.iter().position(|a| a == "--path").and_then(|i| line.get(i + 1))
        {
            let dir = PathBuf::from(dir).join("drive-0/saves");
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("save.dat"), crate::now().to_string())).map_err(|e| e.to_string())?;
        }
        let bytes = 3_100_000;
        Ok(Ran { code: 0, stdout: format!("{{\"overall\":{{\"processedBytes\":{bytes}}},\"games\":{{{}:{{\"decision\":\"Processed\",\"files\":{{\"/saves/save.dat\":{{\"bytes\":{bytes}}}}}}}}}}}", quote(&title)), stderr: String::new() })
    }
}
