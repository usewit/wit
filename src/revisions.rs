//! A book's history: named revisions, kept for good, and daily copies of the
//! working draft since the latest one.
//!
//! The source file is the working draft and saves itself as you write. When
//! you name a revision (v1, v2…), a frozen copy goes to `.wit/revisions/`.
//! Between revisions, the first save of a new writing day keeps a copy of the
//! draft as it stood at the end of the last one, in `.wit/daily/`; only the
//! latest ten are kept. Naming a revision clears them and the count starts over.

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many daily copies are kept.
const DAILY_KEEP: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub n: u32,
    pub name: String,
    /// Unix time in milliseconds.
    pub ms: u64,
}

/// One entry in the history list.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// "v3" for a named revision, "2026-10-12" for a daily copy.
    pub id: String,
    pub kind: &'static str,
    /// The revision number; 0 for a daily copy.
    pub n: u32,
    pub name: String,
    pub ms: u64,
    pub words: usize,
}

pub struct History {
    dir: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn local_date(ms: u64) -> Option<NaiveDate> {
    Local.timestamp_millis_opt(ms as i64).single().map(|t| t.date_naive())
}

fn is_date_id(id: &str) -> bool {
    NaiveDate::parse_from_str(id, "%Y-%m-%d").is_ok()
}

impl History {
    /// `wit_dir` is the book's `.wit` folder.
    pub fn new(wit_dir: PathBuf) -> Self {
        History { dir: wit_dir }
    }

    fn named_dir(&self) -> PathBuf {
        self.dir.join("revisions")
    }
    fn index_path(&self) -> PathBuf {
        self.named_dir().join("index.json")
    }
    fn daily_dir(&self) -> PathBuf {
        self.dir.join("daily")
    }

    /// Named revisions, oldest first.
    pub fn revisions(&self) -> Vec<Revision> {
        let mut v: Vec<Revision> = std::fs::read_to_string(self.index_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        v.retain(|r| self.named_dir().join(format!("v{}.md", r.n)).is_file());
        v.sort_by_key(|r| r.n);
        v
    }

    pub fn latest(&self) -> Option<Revision> {
        self.revisions().pop()
    }

    /// Freeze `text` as the next named revision and clear the daily copies.
    pub fn name(&self, text: &str, name: &str) -> Result<Revision> {
        let mut all = self.revisions();
        let n = all.last().map_or(1, |r| r.n + 1);
        let name = name.trim();
        let rev = Revision { n, name: if name.is_empty() { format!("Revision {n}") } else { name.to_string() }, ms: now_ms() };
        std::fs::create_dir_all(self.named_dir())?;
        write_atomic(&self.named_dir().join(format!("v{n}.md")), text)?;
        all.push(rev.clone());
        write_atomic(&self.index_path(), &serde_json::to_string_pretty(&all)?)?;
        if self.daily_dir().is_dir() {
            std::fs::remove_dir_all(self.daily_dir())?;
        }
        Ok(rev)
    }

    /// Called before the working draft is overwritten by a save. On the first
    /// save of a new day, keep the draft as it stood when last written to.
    pub fn before_save(&self, draft: &str, draft_modified: Option<SystemTime>) -> Result<()> {
        let Some(modified) = draft_modified else { return Ok(()) };
        let day = DateTime::<Local>::from(modified).date_naive();
        if day >= Local::now().date_naive() {
            return Ok(());
        }
        let path = self.daily_dir().join(format!("{day}.md"));
        if path.exists() {
            return Ok(());
        }
        // Nothing new since the latest revision: that copy is already kept.
        if let Some(r) = self.latest() {
            if self.read(&format!("v{}", r.n)).is_ok_and(|t| t == draft) {
                return Ok(());
            }
        }
        std::fs::create_dir_all(self.daily_dir())?;
        write_atomic(&path, draft)?;
        self.prune_daily()
    }

    fn daily_days(&self) -> Vec<String> {
        let mut days: Vec<String> = std::fs::read_dir(self.daily_dir())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_suffix(".md").map(String::from))
            .filter(|d| is_date_id(d))
            .collect();
        days.sort();
        days
    }

    fn prune_daily(&self) -> Result<()> {
        let days = self.daily_days();
        for d in &days[..days.len().saturating_sub(DAILY_KEEP)] {
            std::fs::remove_file(self.daily_dir().join(format!("{d}.md")))?;
        }
        Ok(())
    }

    /// Everything that can be looked at or brought back, newest first.
    pub fn entries(&self) -> Vec<Entry> {
        let words = |id: &str| self.read(id).map(|t| t.split_whitespace().count()).unwrap_or(0);
        let mut v: Vec<Entry> = self
            .revisions()
            .into_iter()
            .map(|r| {
                let id = format!("v{}", r.n);
                Entry { words: words(&id), id, kind: "named", n: r.n, name: r.name, ms: r.ms }
            })
            .collect();
        for day in self.daily_days() {
            // Listed at the end of the day it holds.
            let ms = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(23, 59, 59))
                .and_then(|t| Local.from_local_datetime(&t).earliest())
                .map_or(0, |t| t.timestamp_millis() as u64);
            v.push(Entry { words: words(&day), id: day, kind: "daily", n: 0, name: String::new(), ms });
        }
        v.sort_by(|a, b| b.ms.cmp(&a.ms));
        v
    }

    pub fn read(&self, id: &str) -> Result<String> {
        let path = if let Some(n) = id.strip_prefix('v').filter(|n| n.parse::<u32>().is_ok()) {
            self.named_dir().join(format!("v{n}.md"))
        } else if is_date_id(id) {
            self.daily_dir().join(format!("{id}.md"))
        } else {
            return Err(anyhow!("no such revision"));
        };
        Ok(std::fs::read_to_string(path)?)
    }

    /// Before named revisions there was a copy every few minutes in
    /// `.wit/history/`. Keep the last of each day as a daily copy.
    pub fn migrate(&self) -> Result<()> {
        let old = self.dir.join("history");
        if !old.is_dir() {
            return Ok(());
        }
        let mut last_per_day: std::collections::BTreeMap<NaiveDate, (u64, PathBuf)> = Default::default();
        for e in std::fs::read_dir(&old)?.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            let Some(n) = file.strip_suffix(".md").and_then(|s| s.rsplit('-').next()).and_then(|n| n.parse::<u64>().ok()) else {
                continue;
            };
            // Older snapshots were named in seconds, newer ones in milliseconds.
            let ms = if n < 100_000_000_000 { n * 1000 } else { n };
            let Some(day) = local_date(ms) else { continue };
            if last_per_day.get(&day).is_none_or(|(m, _)| ms > *m) {
                last_per_day.insert(day, (ms, e.path()));
            }
        }
        std::fs::create_dir_all(self.daily_dir())?;
        // Today's text is still the working draft.
        let today = Local::now().date_naive();
        for (day, (_, path)) in last_per_day.iter().filter(|(d, _)| **d < today) {
            let dest = self.daily_dir().join(format!("{day}.md"));
            if !dest.exists() {
                std::fs::copy(path, &dest)?;
            }
        }
        self.prune_daily()?;
        std::fs::remove_dir_all(&old)?;
        Ok(())
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("wit-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
