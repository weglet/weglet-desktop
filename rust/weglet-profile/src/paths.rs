// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The profile directory and the files inside it.

use std::path::{Path, PathBuf};

// Where the profile lives. One place decides, so no two callers can
// disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
}

// The name a fresh install runs as, and what an invalid or missing name
// falls back to. Never rejected by is_valid_profile_name: it must always
// be a name a caller can safely construct one of these from.
pub const DEFAULT_PROFILE: &str = "Default";

// Sibling to the profile directories under Weglet/, not inside any one
// of them -- which profile was last active has to be readable before a
// profile is chosen.
const LAST_PROFILE_FILE: &str = "last-profile.txt";

// Names Windows keeps for devices, with or without an extension: a
// directory by one of these names is the device, not a directory.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

// A profile name becomes a path component directly, so it can't be
// empty, a path traversal, or contain a separator either platform
// understands. Bounded the same way a shortcut title's length isn't --
// here it matters, since the whole thing has to fit on disk and in a
// window title.
//
// Windows drops trailing dots and spaces from a path component, so "Work."
// and "Work " open the directory "Work" under a different name, and "..."
// opens the directory that holds every profile. A name is therefore
// rejected rather than trimmed: the name a caller holds is the directory
// it gets.
pub fn is_valid_profile_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default();
    !name.is_empty()
        && name.len() <= 64
        && name.trim() == name
        && !name.ends_with('.')
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
        && !RESERVED_NAMES
            .iter()
            .any(|reserved| stem.trim_end().eq_ignore_ascii_case(reserved))
}

// The name of the existing profile `name` refers to. Windows compares
// names without regard to case, so "work" opens the directory of a
// profile listed as "Work": both have to be the same profile, or the list
// offers to delete the one that is running. A name no profile has yet is
// returned as it is.
pub fn canonical_profile_name(name: &str) -> String {
    let folded = name.to_lowercase();
    Paths::list_profiles()
        .into_iter()
        .find(|existing| existing.to_lowercase() == folded)
        .unwrap_or_else(|| name.to_string())
}

impl Paths {
    // Must agree with WegletBrowserContext::ProfilePath: the engine's
    // cookies and this crate's settings share a directory. `profile`
    // falls back to DEFAULT_PROFILE when it fails is_valid_profile_name
    // -- a launch is not the place to reject a stray or corrupted value.
    pub fn discover(profile: &str) -> Result<Self, crate::Error> {
        let root = platform_root().ok_or(crate::Error::NoProfileDirectory)?;
        let name = if is_valid_profile_name(profile) {
            profile
        } else {
            DEFAULT_PROFILE
        };
        Ok(Self {
            root: root.join("Weglet").join(name),
        })
    }

    // Every profile that has ever been used on this machine: every
    // subdirectory of the Weglet root, alphabetical, falling back to
    // just DEFAULT_PROFILE when the root doesn't exist yet (a first
    // run) or can't be read.
    pub fn list_profiles() -> Vec<String> {
        let Some(root) = platform_root().map(|root| root.join("Weglet")) else {
            return vec![DEFAULT_PROFILE.to_string()];
        };
        let Ok(entries) = std::fs::read_dir(&root) else {
            return vec![DEFAULT_PROFILE.to_string()];
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| is_valid_profile_name(name))
            .collect();
        if names.is_empty() {
            return vec![DEFAULT_PROFILE.to_string()];
        }
        names.sort();
        names
    }

    // Deletes a profile's entire directory. True if it is gone afterwards,
    // whether or not it existed to begin with -- deleting something
    // already absent is not a failure. Never called on the profile a
    // process is itself running as: the browser has that directory open.
    pub fn delete_profile(name: &str) -> bool {
        let Some(dir) = Self::named_dir(name) else {
            return false;
        };
        if !dir.exists() {
            return true;
        }
        std::fs::remove_dir_all(&dir).is_ok()
    }

    // Moves a profile's directory to a new name. False, and nothing
    // touched, if either name is invalid, `old` does not exist, or `new`
    // already does -- this never overwrites another profile's data.
    pub fn rename_profile(old: &str, new: &str) -> bool {
        let (Some(old_dir), Some(new_dir)) = (Self::named_dir(old), Self::named_dir(new)) else {
            return false;
        };
        if !old_dir.exists() || new_dir.exists() {
            return false;
        }
        std::fs::rename(&old_dir, &new_dir).is_ok()
    }

    // The accent colour a profile's own settings file holds, without
    // building the rest of its state -- for showing a swatch per profile
    // in a list, alongside profiles this process is not running as.
    pub fn peek_accent_color(name: &str) -> String {
        Self::named_dir(name)
            .and_then(|dir| crate::Settings::peek(&dir.join("settings.json")).ok())
            .map(|settings| settings.accent_color)
            .unwrap_or_else(|| crate::DEFAULT_ACCENT.to_string())
    }

    // The profile the last successful switch or launch left active --
    // read on a plain launch with no --profile, so a double-click
    // reopens where the user left off. None on a first run, an unreadable
    // file, or a name that no longer passes validation.
    pub fn last_profile() -> Option<String> {
        let root = platform_root()?.join("Weglet");
        let text = std::fs::read_to_string(root.join(LAST_PROFILE_FILE)).ok()?;
        let name = text.trim();
        is_valid_profile_name(name).then(|| name.to_string())
    }

    // Records `name` as the one to reopen next time -- called on every
    // switch. Silently does nothing for an invalid name or an
    // unwritable root: this is a convenience, not data anything else
    // depends on.
    pub fn set_last_profile(name: &str) {
        if !is_valid_profile_name(name) {
            return;
        }
        let Some(dir) = platform_root().map(|root| root.join("Weglet")) else {
            return;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let _ = crate::atomic::write(&dir.join(LAST_PROFILE_FILE), name.trim());
    }

    // `None` for an invalid name -- the one thing every one of the
    // functions above must refuse before it touches a path.
    fn named_dir(name: &str) -> Option<PathBuf> {
        is_valid_profile_name(name)
            .then(|| platform_root().map(|root| root.join("Weglet").join(name.trim())))
            .flatten()
    }

    // For tests and for a future --user-data-dir.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    // Optional data files. Present means the user wants their own list;
    // absent means the compiled-in table stands. Nothing is ever written
    // to them.
    pub fn blocklist_file(&self) -> PathBuf {
        self.root.join("blocklist.txt")
    }

    pub fn brands_file(&self) -> PathBuf {
        self.root.join("brands.json")
    }

    pub fn sensitive_words_file(&self) -> PathBuf {
        self.root.join("sensitive_words.txt")
    }

    pub fn engines_file(&self) -> PathBuf {
        self.root.join("engines.json")
    }

    pub fn session_file(&self) -> PathBuf {
        self.root.join("session.json")
    }

    pub fn bookmarks_file(&self) -> PathBuf {
        self.root.join("bookmarks.json")
    }

    pub fn history_file(&self) -> PathBuf {
        self.root.join("history.json")
    }

    pub fn downloads_file(&self) -> PathBuf {
        self.root.join("downloads.json")
    }

    pub fn threat_feed_file(&self) -> PathBuf {
        self.root.join("threat-feed.json")
    }
}

#[cfg(target_os = "windows")]
fn platform_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn platform_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_sits_under_the_root() {
        let paths = Paths::at("/tmp/profile");
        for file in [paths.settings_file(), paths.session_file()] {
            assert!(file.starts_with(paths.root()), "{file:?}");
        }
    }

    #[test]
    fn the_file_names_are_stable() {
        let paths = Paths::at("/tmp/profile");
        assert!(paths.settings_file().ends_with("settings.json"));
        assert!(paths.session_file().ends_with("session.json"));
    }

    #[test]
    fn ordinary_names_are_valid() {
        for name in ["Default", "Work", "Alt Account", "Профиль", "a"] {
            assert!(is_valid_profile_name(name), "{name}");
        }
    }

    #[test]
    fn empty_or_whitespace_only_is_invalid() {
        assert!(!is_valid_profile_name(""));
        assert!(!is_valid_profile_name("   "));
    }

    #[test]
    fn a_path_component_that_would_escape_the_profile_root_is_invalid() {
        for name in [".", "..", "a/b", "a\\b", "../../etc"] {
            assert!(!is_valid_profile_name(name), "{name}");
        }
    }

    #[test]
    fn characters_windows_forbids_in_a_filename_are_invalid() {
        for name in ["a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(!is_valid_profile_name(name), "{name}");
        }
    }

    #[test]
    fn an_overlong_name_is_invalid() {
        assert!(!is_valid_profile_name(&"a".repeat(65)));
        assert!(is_valid_profile_name(&"a".repeat(64)));
    }

    // Windows would drop the trailing part and open a different directory.
    #[test]
    fn a_name_windows_would_change_on_disk_is_invalid() {
        for name in ["Work.", "Work ", " Work", "...", "a.."] {
            assert!(!is_valid_profile_name(name), "{name:?}");
        }
    }

    #[test]
    fn a_device_name_is_invalid_in_any_case_and_with_an_extension() {
        for name in ["CON", "nul", "Com1", "LPT9", "aux.txt", "NUL .x"] {
            assert!(!is_valid_profile_name(name), "{name}");
        }
        for name in ["Console", "Nullable", "COM10"] {
            assert!(is_valid_profile_name(name), "{name}");
        }
    }
}
