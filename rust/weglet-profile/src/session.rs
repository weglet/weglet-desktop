// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The open windows and tabs, saved and restored.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

// Mirrors weglet_core::RestoredWindow. See to_restore_input.
type RestoredTab = (Vec<String>, usize);
type RestoredWindow = (Vec<RestoredTab>, usize);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionTab {
    pub entries: Vec<String>,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionWindow {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Session {
    pub windows: Vec<SessionWindow>,

    // The pre-windows shape: a flat tab list and one active index. Read
    // and folded into a single window by repair(), then never written
    // again.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<SessionTab>,
    #[serde(skip_serializing_if = "is_zero")]
    pub active: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl Session {
    // A missing file is a first run. A corrupt one is an error, and the
    // caller falls back to a fresh session rather than refusing to start.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let mut session: Self = crate::store::load(path)?;
        session.repair();
        Ok(session)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        crate::store::save(path, self)
    }

    // Makes anything that came off disk safe to index with: an
    // out-of-range number would panic at startup.
    fn repair(&mut self) {
        // The pre-windows shape, folded in. Only when there is no windows
        // block at all.
        if self.windows.is_empty() && !self.tabs.is_empty() {
            self.windows.push(SessionWindow {
                tabs: std::mem::take(&mut self.tabs),
                active: self.active,
            });
        }
        self.tabs.clear();
        self.active = 0;

        // No cap here: how many tabs and windows may exist is the model's
        // rule, enforced in AppState::restore. This only makes the file
        // safe to index with.
        for window in &mut self.windows {
            window.tabs.retain(|tab| !tab.entries.is_empty());
            for tab in &mut window.tabs {
                tab.cursor = tab.cursor.min(tab.entries.len() - 1);
            }
            window.active = if window.tabs.is_empty() {
                0
            } else {
                window.active.min(window.tabs.len() - 1)
            };
        }
        // A window with no tabs cannot be shown.
        self.windows.retain(|window| !window.tabs.is_empty());
    }

    // The shape AppState::restore wants, spelled out rather than
    // imported: this crate does not depend on weglet-core.
    pub fn to_restore_input(&self) -> Vec<RestoredWindow> {
        self.windows
            .iter()
            .map(|window| {
                (
                    window
                        .tabs
                        .iter()
                        .map(|tab| (tab.entries.clone(), tab.cursor))
                        .collect(),
                    window.active,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-session-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("session.json")
    }

    fn tab(entries: &[&str], cursor: usize) -> SessionTab {
        SessionTab {
            entries: entries.iter().map(|s| s.to_string()).collect(),
            cursor,
        }
    }

    fn window(tabs: Vec<SessionTab>, active: usize) -> SessionWindow {
        SessionWindow { tabs, active }
    }

    #[test]
    fn a_missing_file_is_an_empty_session() {
        assert_eq!(Session::load(&file("missing")).unwrap(), Session::default());
    }

    #[test]
    fn a_session_round_trips_through_disk() {
        let path = file("roundtrip");
        let session = Session {
            windows: vec![window(
                vec![tab(&["https://a.example", "https://a2.example"], 1)],
                0,
            )],
            ..Session::default()
        };
        session.save(&path).unwrap();
        assert_eq!(Session::load(&path).unwrap(), session);
    }

    #[test]
    fn several_windows_round_trip_separately() {
        let path = file("windows");
        let session = Session {
            windows: vec![
                window(vec![tab(&["https://a.example"], 0)], 0),
                window(
                    vec![tab(&["https://b.example"], 0), tab(&["https://c.example"], 0)],
                    1,
                ),
            ],
            ..Session::default()
        };
        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded, session);
        assert_eq!(loaded.windows[1].active, 1);
    }

    // A session written before windows existed must not lose its tabs. It
    // was written as TOML too, so it also goes through the conversion.
    #[test]
    fn a_pre_windows_session_becomes_one_window() {
        let path = file("upgrade");
        crate::atomic::write(
            &path.with_extension("toml"),
            "active = 1\n\n[[tabs]]\nentries = [\"https://a.example\"]\ncursor = 0\n\n\
             [[tabs]]\nentries = [\"https://b.example\"]\ncursor = 0\n",
        )
        .unwrap();
        let session = Session::load(&path).unwrap();
        assert_eq!(session.windows.len(), 1);
        assert_eq!(session.windows[0].tabs.len(), 2);
        assert_eq!(session.windows[0].active, 1);
        // And the old fields are not carried forward.
        assert!(session.tabs.is_empty());
    }

    // Every one of these would panic at startup if used as written.
    #[test]
    fn out_of_range_indices_are_repaired_on_load() {
        let path = file("repair");
        Session {
            windows: vec![window(vec![tab(&["https://a.example"], 99)], 42)],
            ..Session::default()
        }
        .save(&path)
        .unwrap();

        let session = Session::load(&path).unwrap();
        assert_eq!(session.windows[0].tabs[0].cursor, 0);
        assert_eq!(session.windows[0].active, 0);
    }

    // A tab with no history has no URL to open, so it is not a tab.
    #[test]
    fn tabs_with_no_entries_are_dropped() {
        let path = file("empty-tab");
        Session {
            windows: vec![window(
                vec![tab(&[], 0), tab(&["https://a.example"], 0)],
                1,
            )],
            ..Session::default()
        }
        .save(&path)
        .unwrap();

        let session = Session::load(&path).unwrap();
        assert_eq!(session.windows[0].tabs.len(), 1);
        assert_eq!(session.windows[0].active, 0);
    }

    #[test]
    fn a_window_left_with_no_tabs_is_dropped() {
        let path = file("empty-window");
        Session {
            windows: vec![
                window(vec![tab(&[], 0)], 0),
                window(vec![tab(&["https://a.example"], 0)], 0),
            ],
            ..Session::default()
        }
        .save(&path)
        .unwrap();
        assert_eq!(Session::load(&path).unwrap().windows.len(), 1);
    }

    // The tab budget is shared, so windows cannot get around it between
    // them.
    #[test]
    fn a_malformed_file_is_an_error() {
        let path = file("malformed");
        crate::atomic::write(&path, "tabs = not-json = =").unwrap();
        assert!(matches!(Session::load(&path), Err(Error::Parse { .. })));
    }

    #[test]
    fn the_restore_shape_carries_every_window() {
        let session = Session {
            windows: vec![
                window(vec![tab(&["https://a.example"], 0)], 0),
                window(
                    vec![tab(&["https://b.example"], 0), tab(&["https://c.example"], 0)],
                    1,
                ),
            ],
            ..Session::default()
        };
        let restored = session.to_restore_input();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[1].0.len(), 2);
        assert_eq!(restored[1].1, 1);
    }

}
