//! Settings > Report a problem. A plain text file the player can send: the recent activity log, Turbine Hub's
//! version, its settings, the screen size and each game with how Turbine Hub plays it. Nothing secret goes in: any value
//! after a word like key, token, cookie, password or authorization is replaced, and so is anything that looks like a
//! long secret (a Nexus key, a session id).

/// Is this word (a setting's or header's name) one whose value is secret? Any name with pass, pwd, token, secret,
/// key, auth, rcon, webhook, cookie or session in it.
pub fn secret_word(w: &str) -> bool {
    let l = w.to_ascii_lowercase();
    let l = l.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    ["pass", "pwd", "token", "secret", "key", "auth", "rcon", "webhook", "cookie", "session"].iter().any(|p| l.contains(p)) || l == "sid"
}

/// A run of 24 or more letters and digits (with + = _ - .): a key or token as written. Paths stay readable.
fn long_secret(tok: &str) -> bool {
    let core = tok.trim_matches(|c: char| "\"',;()[]{}<>".contains(c));
    core.len() >= 24
        && core.chars().all(|c| c.is_ascii_alphanumeric() || "+=_-.".contains(c))
        && core.chars().any(|c| c.is_ascii_digit())
        && core.chars().any(|c| c.is_ascii_alphabetic())
}

/// One line with its secrets replaced by "(hidden)".
pub fn redact(line: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut hide_next = false;
    for part in line.split(' ') {
        if part.is_empty() {
            out.push(String::new());
            continue;
        }
        if hide_next && (part == "=" || part == ":") {
            out.push(part.to_string());
            continue;
        }
        if hide_next {
            out.push("(hidden)".into());
            // "Authorization: Bearer abc": the scheme and the value both.
            hide_next = matches!(part.to_ascii_lowercase().as_str(), "bearer" | "basic");
            continue;
        }
        if let Some(pos) = part.find(['=', ':']) {
            let (k, v) = part.split_at(pos);
            if secret_word(k) {
                if v.len() > 1 {
                    out.push(format!("{k}{}(hidden)", &v[..1]));
                } else {
                    out.push(part.to_string());
                    hide_next = true;
                }
                continue;
            }
        } else if secret_word(part) && !part.eq_ignore_ascii_case("key") {
            // "AdminPass hunter2 to hunter3": the rest of the line could all be the secret.
            out.push(part.to_string());
            out.push("(hidden)".into());
            return out.join(" ");
        }
        if long_secret(part) {
            out.push("(hidden)".into());
            continue;
        }
        out.push(part.to_string());
    }
    out.join(" ")
}

/// What goes into the report.
pub struct ProblemReport {
    pub version: String,
    pub when: String,
    pub device: Vec<String>,
    pub screen: String,
    /// "name: value" lines (the interface's settings).
    pub settings: Vec<String>,
    /// One line per game: "Name (store:id): how Turbine Hub plays it".
    pub games: Vec<String>,
    pub log: Vec<String>,
    /// The Steam launch logs (`anticheat::launch_report_lines`): the helper's log, me3's output and own log,
    /// Proton's log.
    pub launch: Vec<String>,
    /// The machine's lights and how Turbine Hub drives them (`lights::diagnostics`).
    pub lighting: Vec<String>,
    /// The last lines of Turbine Hub's own log (`applog`), private details already hidden.
    pub app_log: Vec<String>,
    /// The last lines of the crash log.
    pub crash: Vec<String>,
}

/// The report's text, every line redacted.
pub fn build(r: &ProblemReport) -> String {
    let mut t = String::new();
    t.push_str("Turbine Hub problem report\n");
    t.push_str(&format!("Turbine Hub: {}\nMade: {}\nScreen: {}\n", r.version, r.when, r.screen));
    for d in &r.device {
        t.push_str(&format!("{d}\n"));
    }
    t.push_str("\nSettings (no keys, tokens or cookies):\n");
    for s in &r.settings {
        t.push_str(&format!("  {s}\n"));
    }
    t.push_str(&format!("\nGames ({}):\n", r.games.len()));
    for g in &r.games {
        t.push_str(&format!("  {g}\n"));
    }
    t.push_str(&format!("\nRecent activity ({} lines, newest last):\n", r.log.len()));
    for l in &r.log {
        t.push_str(l);
        t.push('\n');
    }
    if !r.lighting.is_empty() {
        t.push_str("\nLighting:\n");
        for l in &r.lighting {
            t.push_str(&format!("  {l}\n"));
        }
    }
    if !r.launch.is_empty() {
        t.push_str("\nSteam launch logs (newest last):\n");
        for l in &r.launch {
            t.push_str(l);
            t.push('\n');
        }
    }
    if !r.app_log.is_empty() {
        t.push_str(&format!("\nTurbine Hub's log ({} lines, newest last):\n", r.app_log.len()));
        for l in &r.app_log {
            t.push_str(l);
            t.push('\n');
        }
    }
    if !r.crash.is_empty() {
        t.push_str("\nCrash log (newest last):\n");
        for l in &r.crash {
            t.push_str(l);
            t.push('\n');
        }
    }
    t.lines().map(redact).collect::<Vec<_>>().join("\n") + "\n"
}

/// The file name: TurbineHub-report-<stamp>.txt.
pub fn file_name(stamp: &str) -> String {
    format!("TurbineHub-report-{stamp}.txt")
}

// ---- A quieter activity log ----

/// The Nexus game answers after a scan as one line. `lines` are the per-game lines ("Nexus game for <name>:
/// <domain> (<how>)", "...: ask (n candidates)", "...: not on Nexus (your pick)"); any other line passes through
/// unchanged, in front. The games to ask about are named (at most 8), since those need the player.
pub fn nexus_summary(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let (mut by_name, mut picked, mut none) = (0usize, 0usize, 0usize);
    let mut ask: Vec<String> = Vec::new();
    for l in lines {
        let Some(rest) = l.strip_prefix("Nexus game for ") else {
            out.push(l.clone());
            continue;
        };
        if let Some((name, _)) = rest.rsplit_once(": ask (") {
            ask.push(name.to_string());
        } else if rest.ends_with(": not on Nexus (your pick)") {
            none += 1;
        } else if rest.ends_with("(your pick)") {
            picked += 1;
        } else {
            by_name += 1;
        }
    }
    let total = by_name + picked + none + ask.len();
    if total > 0 {
        let mut parts = Vec::new();
        if by_name > 0 {
            parts.push(format!("{by_name} matched by name"));
        }
        if picked > 0 {
            parts.push(format!("{picked} by your pick"));
        }
        if none > 0 {
            parts.push(format!("{none} not on Nexus (your pick)"));
        }
        if !ask.is_empty() {
            let shown: Vec<&str> = ask.iter().take(8).map(String::as_str).collect();
            let more = if ask.len() > shown.len() { format!(" and {} more", ask.len() - shown.len()) } else { String::new() };
            parts.push(format!("{} to ask about ({}{more})", ask.len(), shown.join(", ")));
        }
        out.push(format!("Nexus pages for {total} {}: {}", if total == 1 { "game" } else { "games" }, parts.join(", ")));
    }
    out
}

/// Says whether a long line is worth writing again: only when its text for this key differs from the last one
/// written in this run (the 200-character Launch Options line was written at every Play).
#[derive(Default)]
pub struct LogOnce {
    last: std::collections::HashMap<String, String>,
}

impl LogOnce {
    /// True the first time and whenever `text` differs from what was written for `key` last.
    pub fn changed(&mut self, key: &str, text: &str) -> bool {
        if self.last.get(key).is_some_and(|t| t == text) {
            return false;
        }
        self.last.insert(key.to_string(), text.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_hidden() {
        assert_eq!(redact("net apikey=abcdef123"), "net apikey=(hidden)");
        assert_eq!(redact("\"nexusKey\": \"abc\""), "\"nexusKey\": (hidden)");
        assert_eq!(redact("Cookie: sid=1"), "Cookie: (hidden)");
        assert_eq!(redact("Authorization: Bearer abc"), "Authorization: (hidden) (hidden)");
        assert_eq!(redact("token aGVsbG8gd29ybGQgaGVsbG8gd29ybGQ1"), "token (hidden)");
        assert_eq!(redact("cooppassword = hunter2"), "cooppassword (hidden)");
        assert!(redact("sha 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08").contains("(hidden)"));
        // Ordinary lines stay as they are, paths too.
        let l = "2026-11-09 03:58:28.334 launch  ELDEN RING through me3 0.13.0: /home/deck/.var/app/io.github.ravenor907.TurbineHub/data/me3";
        assert_eq!(redact(l), l);
        assert_eq!(redact("Interface size: 100%"), "Interface size: 100%");
        assert_eq!(redact("Monkey: 3"), "Monkey: (hidden)", "on any doubt, hidden");
        assert_eq!(redact("AdminPass hunter2 to hunter3"), "AdminPass (hidden)");
        assert_eq!(redact("rcon_pass=abc"), "rcon_pass=(hidden)");
        assert_eq!(redact("Press key a"), "Press key a");
    }

    #[test]
    fn report_has_every_part() {
        let r = ProblemReport {
            version: "0.58".into(),
            when: "2026-11-09 10:00".into(),
            device: vec!["Device: Handheld".into()],
            screen: "1920 x 1080, scale 2.10".into(),
            settings: vec!["Interface size: 100%".into(), "nexusKey: secretsecret".into()],
            games: vec!["ELDEN RING (steam:1245620): me3, offline with mods".into()],
            log: vec!["a line".into()],
            launch: vec!["-- steam-launch.log (last 1 lines) --".into(), "me3 ended with exit code 1 after 2 s".into()],
            lighting: vec!["ally:rgb:joystick_rings: zones 4".into()],
            app_log: vec!["2026-11-09 09:59:58.120 step    Tile Netflix: done".into()],
            crash: vec!["2026-11-09 09:00:00.000 panic in thread main: boom".into()],
        };
        let t = build(&r);
        for want in ["Turbine Hub: 0.58", "Screen: 1920 x 1080", "Interface size: 100%", "ELDEN RING (steam:1245620)", "a line", "nexusKey: (hidden)", "Steam launch logs", "Lighting:\n  ally:rgb:joystick_rings: zones 4", "me3 ended with exit code 1 after 2 s", "Turbine Hub's log (1 lines, newest last):\n2026-11-09 09:59:58.120 step    Tile Netflix: done", "Crash log (newest last):\n2026-11-09 09:00:00.000 panic in thread main: boom"] {
            assert!(t.contains(want), "{want} in {t}");
        }
        assert!(!t.contains("secretsecret"));
        assert_eq!(file_name("2026-11-09_10-00-00"), "TurbineHub-report-2026-11-09_10-00-00.txt");
    }

    #[test]
    fn a_quieter_activity_log() {
        let lines: Vec<String> = [
            "Nexus game list: 3500 games",
            "Nexus game for A Short Hike: ask (4 candidates)",
            "Nexus game for Balatro: balatro (name match)",
            "Nexus game for Diablo + Hellfire: ask (4 candidates)",
            "Nexus game for Hades: hades (your pick)",
            "Nexus game for Odd Game: not on Nexus (your pick)",
            "Nexus game for Name: with a colon: namewithacolon (name match)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            nexus_summary(&lines),
            vec![
                "Nexus game list: 3500 games".to_string(),
                "Nexus pages for 6 games: 2 matched by name, 1 by your pick, 1 not on Nexus (your pick), 2 to ask about (A Short Hike, Diablo + Hellfire)".to_string()
            ]
        );
        assert!(nexus_summary(&[]).is_empty());
        // The long Launch Options line: once, and again only when it changes.
        let mut once = LogOnce::default();
        assert!(once.changed("1245620", "if [ -x ... ]"));
        assert!(!once.changed("1245620", "if [ -x ... ]"));
        assert!(once.changed("814380", "if [ -x ... ]"), "another game");
        assert!(once.changed("1245620", ""), "the player cleared them");
        assert!(!once.changed("1245620", ""));
    }
}
