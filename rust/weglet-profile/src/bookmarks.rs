// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Saved pages. Distinct from Settings.shortcuts, which are dock tiles on
// the new-tab page: a bookmark here never appears there, and vice versa.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    pub saved_at: u64,
}

impl Bookmark {
    pub fn new(title: &str, url: &str) -> Self {
        Self {
            title: title.to_string(),
            url: url.to_string(),
            saved_at: unix_now(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Bookmarks {
    pub entries: Vec<Bookmark>,
}

impl Bookmarks {
    pub fn load(path: &Path) -> Result<Self, Error> {
        crate::store::load(path)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        crate::store::save(path, self)
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.entries.iter().any(|entry| entry.url == url)
    }

    // True when added, false when `url` was already saved -- toggling
    // twice must remove it rather than saving a second copy.
    pub fn add(&mut self, title: &str, url: &str) -> bool {
        if self.is_bookmarked(url) {
            return false;
        }
        self.entries.push(Bookmark::new(title, url));
        true
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.entries.remove(index);
        true
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
        let dir = std::env::temp_dir().join(format!("weglet-bookmarks-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("bookmarks.json")
    }

    #[test]
    fn a_missing_file_is_an_empty_list() {
        assert_eq!(Bookmarks::load(&file("missing")).unwrap(), Bookmarks::default());
    }

    #[test]
    fn a_bookmark_round_trips_through_disk() {
        let path = file("roundtrip");
        let mut bookmarks = Bookmarks::default();
        bookmarks.add("Rust", "https://rust-lang.org");
        bookmarks.save(&path).unwrap();
        assert_eq!(Bookmarks::load(&path).unwrap(), bookmarks);
    }

    #[test]
    fn adding_the_same_url_twice_does_not_duplicate_it() {
        let mut bookmarks = Bookmarks::default();
        assert!(bookmarks.add("Rust", "https://rust-lang.org"));
        assert!(!bookmarks.add("Rust again", "https://rust-lang.org"));
        assert_eq!(bookmarks.entries.len(), 1);
    }

    #[test]
    fn removing_by_index_drops_only_that_entry() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.add("A", "https://a.example");
        bookmarks.add("B", "https://b.example");
        assert!(bookmarks.remove(0));
        assert_eq!(bookmarks.entries.len(), 1);
        assert_eq!(bookmarks.entries[0].url, "https://b.example");
    }

    #[test]
    fn removing_an_out_of_range_index_reports_false() {
        let mut bookmarks = Bookmarks::default();
        assert!(!bookmarks.remove(0));
    }

    #[test]
    fn is_bookmarked_checks_the_url_only() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.add("A", "https://a.example");
        assert!(bookmarks.is_bookmarked("https://a.example"));
        assert!(!bookmarks.is_bookmarked("https://b.example"));
    }

    #[test]
    fn saving_a_malformed_file_is_reported() {
        let path = file("malformed");
        crate::atomic::write(&path, "entries = not-toml = =").unwrap();
        assert!(matches!(Bookmarks::load(&path), Err(Error::Parse { .. })));
    }
}
