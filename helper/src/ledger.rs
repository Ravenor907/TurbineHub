//! The record of every change the helper made: what, what was there before, when, and on which SteamOS. It is the
//! single source of truth: the files under /etc are written from it, Undo reads "before" from it, and the boot unit
//! puts /sys values back from it. Kept at /var/lib/turbinehub/ledger.json, readable by everyone, written by root.

use crate::json::{self, Json, quote};
use crate::policy::Setting;

/// One recorded change.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Set(Setting),
    /// Decky Loader installed or repaired from this official release tag.
    Decky { tag: String },
    /// AllyHub's system files removed (each backed up first, so Undo puts them back).
    AllyHubRemoved,
}

impl Change {
    pub fn key(&self) -> String {
        match self {
            Change::Set(s) => s.key(),
            Change::Decky { .. } => "decky".into(),
            Change::AllyHubRemoved => "allyhub".into(),
        }
    }

    pub fn words(&self) -> String {
        match self {
            Change::Set(s) => s.words(),
            Change::Decky { tag } => format!("Decky Loader {tag} installed"),
            Change::AllyHubRemoved => "AllyHub's system files removed".into(),
        }
    }

    fn to_json(&self) -> String {
        match self {
            Change::Set(s) => s.to_json(),
            Change::Decky { tag } => format!("{{\"kind\":\"decky\",\"tag\":{}}}", quote(tag)),
            Change::AllyHubRemoved => "{\"kind\":\"allyhub-removed\"}".into(),
        }
    }

    fn from_json(j: &Json) -> Result<Change, String> {
        if j.str_at("kind") == Some("allyhub-removed") {
            return Ok(Change::AllyHubRemoved);
        }
        if j.str_at("kind") == Some("decky") {
            let tag = j.str_at("tag").ok_or("tag is missing")?;
            if !crate::ops::decky_tag_ok(tag) {
                return Err("Bad Decky tag".into());
            }
            return Ok(Change::Decky { tag: tag.into() });
        }
        Setting::from_json(j).map(Change::Set)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub change: Change,
    /// What was there before, as named values ("value" for a /sys or kernel value, "enabled" for a service, the
    /// backup files' names for Decky).
    pub before: Vec<(String, String)>,
    /// Seconds since 1970.
    pub applied: u64,
    /// SteamOS's VERSION_ID when it was applied.
    pub steamos: String,
}

impl Entry {
    pub fn before(&self, k: &str) -> Option<&str> {
        self.before.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    pub next: u64,
    pub entries: Vec<Entry>,
}

impl Ledger {
    pub fn parse(text: &str) -> Result<Ledger, String> {
        if text.trim().is_empty() {
            return Ok(Ledger { next: 1, entries: Vec::new() });
        }
        let j = json::parse(text).map_err(|e| format!("The change record is damaged ({e})"))?;
        let mut l = Ledger { next: j.get("next").and_then(Json::as_f64).unwrap_or(1.0) as u64, entries: Vec::new() };
        for e in j.get("entries").map(Json::as_array).unwrap_or(&[]) {
            let id = e.get("id").and_then(Json::as_f64).ok_or("An entry has no id")? as u64;
            let change = Change::from_json(e.get("change").ok_or("An entry has no change")?)?;
            let before = e.get("before").map(Json::entries).unwrap_or(&[]).iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
            let applied = e.get("applied").and_then(Json::as_f64).unwrap_or(0.0) as u64;
            let steamos = e.str_at("steamos").unwrap_or("").to_string();
            l.next = l.next.max(id + 1);
            l.entries.push(Entry { id, change, before, applied, steamos });
        }
        Ok(l)
    }

    pub fn to_text(&self) -> String {
        let entries: Vec<String> = self
            .entries
            .iter()
            .map(|e| {
                let before: Vec<String> = e.before.iter().map(|(k, v)| format!("{}:{}", quote(k), quote(v))).collect();
                format!(
                    "{{\"id\":{},\"words\":{},\"change\":{},\"before\":{{{}}},\"applied\":{},\"steamos\":{}}}",
                    e.id,
                    quote(&e.change.words()),
                    e.change.to_json(),
                    before.join(","),
                    e.applied,
                    quote(&e.steamos)
                )
            })
            .collect();
        format!("{{\"version\":1,\"next\":{},\"entries\":[\n{}\n]}}\n", self.next.max(1), entries.join(",\n"))
    }

    pub fn find_key(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.change.key() == key)
    }

    /// Records a change. One with the same key as an earlier one replaces it but keeps the earlier "before", so
    /// Undo always goes back to how things were before Turbine Hub first touched it.
    pub fn record(&mut self, change: Change, before: Vec<(String, String)>, applied: u64, steamos: &str) -> u64 {
        let key = change.key();
        if let Some(e) = self.entries.iter_mut().find(|e| e.change.key() == key) {
            e.change = change;
            e.applied = applied;
            e.steamos = steamos.into();
            return e.id;
        }
        let id = self.next.max(1);
        self.next = id + 1;
        self.entries.push(Entry { id, change, before, applied, steamos: steamos.into() });
        id
    }

    pub fn remove(&mut self, id: u64) -> Option<Entry> {
        let i = self.entries.iter().position(|e| e.id == id)?;
        Some(self.entries.remove(i))
    }

    pub fn settings(&self) -> Vec<&Setting> {
        self.entries
            .iter()
            .filter_map(|e| match &e.change {
                Change::Set(s) => Some(s),
                Change::Decky { .. } | Change::AllyHubRemoved => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Change {
        Change::Set(Setting::from_json(&json::parse(text).unwrap()).unwrap())
    }

    #[test]
    fn round_trips_and_keeps_the_first_before() {
        let mut l = Ledger::parse("").unwrap();
        let a = l.record(s(r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#), vec![("value".into(), "60".into())], 10, "3.8.13");
        let b = l.record(Change::Decky { tag: "v3.1.0".into() }, vec![], 11, "3.8.13");
        let a2 = l.record(s(r#"{"kind":"sysctl","key":"vm.swappiness","value":150}"#), vec![("value".into(), "100".into())], 12, "3.8.14");
        assert_eq!((a, b, a2), (1, 2, 1));
        assert_eq!(l.find_key("sysctl:vm.swappiness").unwrap().before("value"), Some("60"));
        let back = Ledger::parse(&l.to_text()).unwrap();
        assert_eq!(back, l);
        assert!(l.to_text().contains("\"words\":\"Kernel setting vm.swappiness set to 150\""));
        let mut l = back;
        assert_eq!(l.remove(1).unwrap().id, 1);
        assert_eq!(l.record(s(r#"{"kind":"module","name":"ntsync"}"#), vec![], 13, ""), 3);
    }

    #[test]
    fn a_tampered_record_is_refused() {
        assert!(Ledger::parse(r#"{"entries":[{"id":1,"change":{"kind":"sysctl","key":"kernel.core_pattern","value":1}}]}"#).is_err());
        assert!(Ledger::parse(r#"{"entries":[{"id":1,"change":{"kind":"decky","tag":"../../x"}}]}"#).is_err());
        assert!(Ledger::parse("{").is_err());
    }
}
