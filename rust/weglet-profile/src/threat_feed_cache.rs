// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Persisted phishing-feed indicators -- kept separate from
// settings.json since this is fetched data, not a user preference.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ThreatFeedCache {
    // Plain URL indicators -- see weglet-security's threat_feed.rs. A cache
    // written before they were kept as text holds hashes under another key,
    // which loads as empty until the next refresh.
    pub indicators: BTreeSet<String>,
    pub source_count: usize,
    // Unix seconds -- 0 means never successfully updated.
    pub updated_at: u64,
    pub last_update_failed: bool,
}

impl ThreatFeedCache {
    // An empty cache never counts as updated: settings would otherwise
    // report a recent successful update for a feed that matches nothing.
    // A cache written before indicators were kept as text loads that way.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let mut cache: Self = crate::store::load(path)?;
        if cache.indicators.is_empty() {
            cache.updated_at = 0;
            cache.source_count = 0;
        }
        Ok(cache)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        crate::store::save(path, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-threat-feed-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("threat-feed.json")
    }

    #[test]
    fn a_missing_file_is_an_empty_cache() {
        let cache = ThreatFeedCache::load(&file("missing")).unwrap();
        assert!(cache.indicators.is_empty());
        assert_eq!(cache.updated_at, 0);
        assert!(!cache.last_update_failed);
    }

    #[test]
    fn a_cache_round_trips_through_disk() {
        let path = file("roundtrip");
        let cache = ThreatFeedCache {
            indicators: ["evil.example|/login".to_string(), "bad.example|/".to_string()].into(),
            source_count: 2,
            updated_at: 1_700_000_000,
            last_update_failed: false,
        };
        cache.save(&path).unwrap();
        assert_eq!(ThreatFeedCache::load(&path).unwrap(), cache);
    }

    #[test]
    fn a_cache_of_hashes_from_an_older_weglet_loads_as_never_updated() {
        let path = file("hashes");
        crate::atomic::write(
            &path,
            r#"{"hashes": ["abc"], "source-count": 1, "updated-at": 1700000000}"#,
        )
        .unwrap();
        let cache = ThreatFeedCache::load(&path).unwrap();
        assert!(cache.indicators.is_empty());
        assert_eq!(cache.updated_at, 0);
        assert_eq!(cache.source_count, 0);
    }
}
