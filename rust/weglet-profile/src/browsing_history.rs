// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// A persistent log of address-bar submissions -- distinct from
// weglet-core::History, which is one tab's back/forward stack and never
// touches disk. Backs the "Search history" tab on the History page.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HistoryEntry {
    // What the user actually typed -- a search phrase, or an address.
    pub query: String,
    // Where it resolved to -- the search engine's results URL, or the
    // same address if it was typed directly.
    pub url: String,
    pub visited_at: u64,
}

impl HistoryEntry {
    pub fn new(query: &str, url: &str) -> Self {
        Self {
            query: query.to_string(),
            url: url.to_string(),
            visited_at: unix_now(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BrowsingHistory {
    pub entries: Vec<HistoryEntry>,
}

// A personal browser's history doesn't need to grow forever. Capped here
// the same way downloads.rs caps its own list: it keeps the file small
// enough to always rewrite wholesale on save.
const MAX_ENTRIES: usize = 2000;

impl BrowsingHistory {
    pub fn load(path: &Path) -> Result<Self, Error> {
        crate::store::load(path)
    }

    pub fn save(&mut self, path: &Path) -> Result<(), Error> {
        if self.entries.len() > MAX_ENTRIES {
            self.entries.sort_by_key(|entry| std::cmp::Reverse(entry.visited_at));
            self.entries.truncate(MAX_ENTRIES);
        }
        crate::store::save(path, self)
    }

    // Newest first: how the History page shows them, and where a rewrite
    // caps them from.
    pub fn record(&mut self, query: &str, url: &str) {
        self.entries.insert(0, HistoryEntry::new(query, url));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-history-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("history.json")
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        assert_eq!(
            BrowsingHistory::load(&file("missing")).unwrap(),
            BrowsingHistory::default()
        );
    }

    #[test]
    fn an_entry_round_trips_through_disk() {
        let path = file("roundtrip");
        let mut history = BrowsingHistory::default();
        history.record("cats", "https://duckduckgo.com/?q=cats");
        history.save(&path).unwrap();
        assert_eq!(BrowsingHistory::load(&path).unwrap(), history);
    }

    #[test]
    fn recording_puts_the_newest_entry_first() {
        let mut history = BrowsingHistory::default();
        history.record("first", "https://a.example");
        history.record("second", "https://b.example");
        assert_eq!(history.entries[0].query, "second");
        assert_eq!(history.entries[1].query, "first");
    }

    #[test]
    fn saving_more_than_the_cap_keeps_only_the_newest() {
        let path = file("capped");
        let mut history = BrowsingHistory {
            entries: (0..(MAX_ENTRIES + 10) as u64)
                .map(|i| HistoryEntry {
                    query: format!("q{i}"),
                    url: format!("https://example.com/{i}"),
                    visited_at: i,
                })
                .collect(),
        };
        history.save(&path).unwrap();
        let loaded = BrowsingHistory::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), MAX_ENTRIES);
        assert!(loaded.entries.iter().all(|e| e.visited_at >= 10));
    }

    #[test]
    fn clear_empties_the_list() {
        let mut history = BrowsingHistory::default();
        history.record("cats", "https://duckduckgo.com/?q=cats");
        history.clear();
        assert!(history.entries.is_empty());
    }
}
