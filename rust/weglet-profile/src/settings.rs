// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The profile's settings: search engines, shortcuts, block list, theme.

use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::Error;

// The built-in engines, as data. See data/engines.json for the format.
// include_str! rather than a file read at startup: Weglet ships one
// executable and nothing beside it.
const BUILT_IN_ENGINES: &str = include_str!("../data/engines.json");

// The id the user's own template is stored under. Not an engine in the
// table, and rejected if one tries to be: the template lives in
// Settings::custom_search_url.
pub const CUSTOM_ENGINE_ID: &str = "custom";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Engine {
    pub id: String,
    pub label: String,
    // Exactly one %s.
    pub template: String,
}

#[derive(Debug, Deserialize)]
struct EngineFile {
    #[serde(default)]
    engine: Vec<Engine>,
}

fn parse_engines(text: &str) -> Result<Vec<Engine>, String> {
    let engines = serde_json_lenient::from_str::<EngineFile>(text)
        .map(|file| file.engine)
        .map_err(|error| error.to_string())?;
    if engines.is_empty() {
        return Err("no engines defined".to_string());
    }
    for engine in &engines {
        if engine.id == CUSTOM_ENGINE_ID {
            return Err(format!("\"{CUSTOM_ENGINE_ID}\" is reserved"));
        }
        if engine.id.is_empty() || engine.label.is_empty() {
            return Err("an engine has no id or no label".to_string());
        }
        if !is_valid_custom_template(&engine.template) {
            return Err(format!("{}: template is not a usable URL", engine.id));
        }
    }
    let mut ids: Vec<&str> = engines.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("two engines share an id".to_string());
    }
    Ok(engines)
}

// The search engines a profile offers: the built-in table, or one the
// profile supplies in its place. Owned by the profile rather than kept in a
// static, since one process runs several profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engines(Vec<Engine>);

impl Engines {
    // The built-in file is checked by a test in this crate, so a malformed
    // one fails the build rather than the browser. Parsed once: every
    // Settings::default() asks for its first engine.
    pub fn built_in() -> Self {
        static BUILT_IN: OnceLock<Engines> = OnceLock::new();
        BUILT_IN
            .get_or_init(|| Self(parse_engines(BUILT_IN_ENGINES).unwrap_or_default()))
            .clone()
    }

    // A table read from the profile. Rejected whole if anything in it is
    // wrong: half a table is a settings page offering engines that do not
    // search.
    pub fn parse(text: &str) -> Result<Self, String> {
        parse_engines(text).map(Self)
    }

    pub fn all(&self) -> &[Engine] {
        &self.0
    }

    // The id a fresh profile is created with: the first in the table.
    pub fn default_id(&self) -> &str {
        self.0.first().map_or(CUSTOM_ENGINE_ID, |engine| engine.id.as_str())
    }

    pub fn label(&self, id: &str) -> &str {
        if id == CUSTOM_ENGINE_ID {
            return "your search engine";
        }
        self.0
            .iter()
            .find(|engine| engine.id == id)
            .map_or("your search engine", |engine| engine.label.as_str())
    }

    pub fn is_known(&self, id: &str) -> bool {
        id == CUSTOM_ENGINE_ID || self.0.iter().any(|engine| engine.id == id)
    }

    // None when there is nothing to search with: an unknown id, or Custom
    // with no usable template.
    pub fn query_url(&self, id: &str, query: &str, custom_template: &str) -> Option<String> {
        let escaped = percent_encode_query(query);
        let template = if id == CUSTOM_ENGINE_ID {
            if !is_valid_custom_template(custom_template) {
                return None;
            }
            custom_template
        } else {
            self.0
                .iter()
                .find(|engine| engine.id == id)?
                .template
                .as_str()
        };
        Some(template.replacen("%s", &escaped, 1))
    }
}

// A template needs exactly one %s and has to be an http(s) URL with a
// host. Without the host check, "%s" would turn every search into a
// navigation to whatever the user typed.
fn is_valid_custom_template(template: &str) -> bool {
    if template.matches("%s").count() != 1 {
        return false;
    }
    let Ok(url) = url::Url::parse(&template.replace("%s", "x")) else {
        return false;
    };
    (url.scheme() == "http" || url.scheme() == "https") && url.host().is_some()
}

// Written out rather than pulling in a crate. Unreserved set per RFC 3986,
// everything else escaped -- including "+", which a receiver would
// otherwise decode as a space.
fn percent_encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// A pinned site on the new tab page, identified by position: the dock is
// a list the user arranges. Default so a half-written entry becomes an
// empty one rather than failing the whole load.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Shortcut {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    // How often pending settings and the open tabs reach the disk, in
    // seconds. In the profile because the right value depends on the
    // machine. Clamped on load -- zero would write on every tick.
    pub settings_flush_seconds: u64,
    pub session_save_seconds: u64,

    // The engine's id from data/engines.json, or "custom". A String, not
    // an enum: the set of engines is data.
    pub search_engine: String,
    // Kept even when the engine is not Custom: switching away and back
    // should not lose what the user typed.
    pub custom_search_url: String,
    pub restore_session: bool,
    pub blocked_hosts: Vec<String>,
    pub shortcuts: Vec<Shortcut>,
    pub accent_color: String,
    pub address_bar_shape: AddressBarShape,
    pub threat_feed_enabled: bool,
    // Off by default: fetching a site's icon is itself a request to that
    // site, before the user has typed or clicked anything else. See
    // docs/security.md.
    pub favicons_enabled: bool,
    // Off by default: most downloads should just land in Downloads. On,
    // WegletDownloadManagerDelegate shows a native Save As dialog for
    // every download instead of picking the path itself.
    pub ask_where_to_save_downloads: bool,
    // A BCP-47-ish code ("en", "ru"). Not an enum: the known set is
    // data, in ui/i18n/*.txt, not a list this crate would have to keep
    // in step with.
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddressBarShape {
    Pill,
    Rounded,
    Square,
}

// The block list is walked for every request the blocker sees, so an
// unbounded one is a performance cliff.
const MAX_BLOCKED_HOSTS: usize = 10_000;

// Bounded so a hand-edited or corrupt file cannot make either a busy loop
// or "never".
const DEFAULT_SETTINGS_FLUSH_SECONDS: u64 = 5;
const DEFAULT_SESSION_SAVE_SECONDS: u64 = 30;
const MIN_INTERVAL_SECONDS: u64 = 1;
const MAX_INTERVAL_SECONDS: u64 = 3600;

// What the dock has room for. Past this the tiles are invisible.
pub const MAX_SHORTCUTS: usize = 8;

// The default accent lives in tokens.json and is generated into
// generated_defaults.rs.
include!("generated_defaults.rs");

impl Default for Settings {
    fn default() -> Self {
        Self {
            settings_flush_seconds: DEFAULT_SETTINGS_FLUSH_SECONDS,
            session_save_seconds: DEFAULT_SESSION_SAVE_SECONDS,
            search_engine: Engines::built_in().default_id().to_string(),
            custom_search_url: String::new(),
            restore_session: true,
            blocked_hosts: Vec::new(),
            shortcuts: Vec::new(),
            accent_color: DEFAULT_ACCENT.to_string(),
            address_bar_shape: AddressBarShape::Pill,
            threat_feed_enabled: true,
            favicons_enabled: false,
            ask_where_to_save_downloads: false,
            language: "en".to_string(),
        }
    }
}

impl Settings {
    // A missing file is a first run. Anything else is returned, so the
    // caller can say so rather than silently substituting defaults and
    // overwriting real settings on the next save.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let mut settings: Self = crate::store::load(path)?;
        settings.clamp();
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        crate::store::save(path, self)
    }

    // Another profile's settings, read without writing anything -- see
    // store::peek.
    pub fn peek(path: &Path) -> Result<Self, Error> {
        let mut settings: Self = crate::store::peek(path)?;
        settings.clamp();
        Ok(settings)
    }

    // Applied on load, not on save, so an oversized file is rewritten
    // smaller only once the user changes something.
    fn clamp(&mut self) {
        if self.blocked_hosts.len() > MAX_BLOCKED_HOSTS {
            self.blocked_hosts.truncate(MAX_BLOCKED_HOSTS);
        }
        if self.shortcuts.len() > MAX_SHORTCUTS {
            self.shortcuts.truncate(MAX_SHORTCUTS);
        }
        // A colour that will not parse is the same as one nobody chose.
        if !is_valid_hex_color(&self.accent_color) {
            self.accent_color = DEFAULT_ACCENT.to_string();
        }
        self.settings_flush_seconds = self
            .settings_flush_seconds
            .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS);
        self.session_save_seconds = self
            .session_save_seconds
            .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS);
        if !is_valid_language_code(&self.language) {
            self.language = "en".to_string();
        }
    }

    // An id no longer in the profile's table -- its engines file removed
    // it, or the settings file was edited. A stored preference nothing can
    // act on is a search box that does nothing.
    pub fn fit_engines(&mut self, engines: &Engines) {
        if !engines.is_known(&self.search_engine) {
            self.search_engine = engines.default_id().to_string();
        }
    }

    // False when the dock is full, so the caller can say so.
    pub fn add_shortcut(&mut self, title: &str, url: &str) -> bool {
        if self.shortcuts.len() >= MAX_SHORTCUTS {
            return false;
        }
        self.shortcuts.push(Shortcut {
            title: title.to_string(),
            url: url.to_string(),
        });
        true
    }

    pub fn edit_shortcut(&mut self, index: usize, title: &str, url: &str) -> bool {
        let Some(shortcut) = self.shortcuts.get_mut(index) else {
            return false;
        };
        shortcut.title = title.to_string();
        shortcut.url = url.to_string();
        true
    }

    // Index, not identity: the caller sent a position from a list it was
    // shown, which can have changed. Out of range is a no-op.
    pub fn remove_shortcut(&mut self, index: usize) -> bool {
        if index >= self.shortcuts.len() {
            return false;
        }
        self.shortcuts.remove(index);
        true
    }

    // False and unchanged for anything that is not #RRGGBB, so a
    // malformed value cannot leave the profile holding a colour nothing
    // can render.
    pub fn set_accent_color(&mut self, color: &str) -> bool {
        if !is_valid_hex_color(color) {
            return false;
        }
        self.accent_color = color.to_string();
        true
    }

    pub fn set_language(&mut self, language: &str) -> bool {
        if !is_valid_language_code(language) {
            return false;
        }
        self.language = language.to_string();
        true
    }
}

fn is_valid_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

// Shaped, not looked up: this crate does not know the set of languages
// ui/i18n/*.txt actually has -- an unknown-but-shaped code just falls
// back to English in the UI, the same as a key missing from a real
// translation does.
fn is_valid_language_code(value: &str) -> bool {
    (2..=8).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_url(id: &str, query: &str, custom_template: &str) -> Option<String> {
        Engines::built_in().query_url(id, query, custom_template)
    }

    #[test]
    fn an_engine_the_table_lacks_falls_back_to_its_default() {
        let engines = Engines::parse(
            r#"{"engine": [{"id": "only", "label": "Only", "template": "https://only.example/?q=%s"}]}"#,
        )
        .unwrap();
        let mut settings = Settings {
            search_engine: "duckduckgo".to_string(),
            ..Settings::default()
        };
        settings.fit_engines(&engines);
        assert_eq!(settings.search_engine, "only");
    }

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-settings-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn a_missing_file_is_a_first_run_not_an_error() {
        assert_eq!(Settings::load(&file("missing")).unwrap(), Settings::default());
    }

    // Both would be a busy loop at zero and "never" at a huge value, and
    // the number comes off disk.
    #[test]
    fn write_intervals_are_clamped_on_load() {
        let path = file("intervals");
        crate::atomic::write(
            &path,
            r#"{"settings-flush-seconds": 0, "session-save-seconds": 999999}"#,
        )
        .unwrap();
        let settings = Settings::load(&path).unwrap();
        assert!(settings.settings_flush_seconds >= 1);
        assert!(settings.session_save_seconds <= 3600);
    }

    #[test]
    fn settings_round_trip_through_disk() {
        let path = file("roundtrip");
        let settings = Settings {
            settings_flush_seconds: 7,
            session_save_seconds: 60,
            search_engine: "bing".to_string(),
            custom_search_url: String::new(),
            restore_session: false,
            blocked_hosts: vec!["ads.example".into()],
            shortcuts: vec![Shortcut {
                title: "Example".into(),
                url: "https://example.com".into(),
            }],
            accent_color: "#3B82F6".into(),
            address_bar_shape: AddressBarShape::Rounded,
            threat_feed_enabled: false,
            favicons_enabled: true,
            ask_where_to_save_downloads: true,
            language: "ru".into(),
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
    }

    // An unknown key is a file written by a newer version. Losing the
    // whole profile over it would be worse.
    #[test]
    fn an_unknown_key_is_ignored() {
        let path = file("unknown");
        crate::atomic::write(&path, r#"{"search-engine": "bing", "future-option": 42}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().search_engine, "bing");
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_what_is_missing() {
        let path = file("partial");
        crate::atomic::write(&path, r#"{"custom-search-url": "https://example.com"}"#).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.custom_search_url, "https://example.com");
        assert_eq!(settings.search_engine, Engines::built_in().default_id());
        assert!(settings.restore_session);
    }

    // Loud, not silent: the caller has to be able to tell the user.
    #[test]
    fn a_malformed_file_is_an_error() {
        let path = file("malformed");
        crate::atomic::write(&path, "this is not json = = =").unwrap();
        assert!(matches!(
            Settings::load(&path),
            Err(Error::Parse { .. })
        ));
    }

    #[test]
    fn an_oversized_block_list_is_clamped_on_load() {
        let path = file("clamp");
        let settings = Settings {
            blocked_hosts: (0..MAX_BLOCKED_HOSTS + 500)
                .map(|i| format!("{i}.example"))
                .collect(),
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().blocked_hosts.len(),
            MAX_BLOCKED_HOSTS
        );
    }

    #[test]
    fn the_default_accent_is_a_valid_colour() {
        assert!(is_valid_hex_color(DEFAULT_ACCENT));
    }

    #[test]
    fn a_custom_engine_needs_exactly_one_placeholder() {
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "https://example.com/s?q=%s")
            .is_some());
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "https://example.com/s").is_none());
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "https://example.com/s?q=%s&r=%s")
            .is_none());
    }

    // A template with no host turns every search into a navigation to
    // whatever the user typed.
    #[test]
    fn a_custom_engine_needs_a_navigable_host() {
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "javascript:%s").is_none());
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "%s").is_none());
        assert!(query_url(CUSTOM_ENGINE_ID, "cats", "not a url %s").is_none());
    }

    #[test]
    fn a_custom_engine_query_is_percent_encoded() {
        let url = query_url(CUSTOM_ENGINE_ID, "a b", "https://example.com/s?q=%s")
            .unwrap();
        assert_eq!(url, "https://example.com/s?q=a%20b");
    }

    #[test]
    fn setting_an_invalid_accent_colour_is_rejected() {
        let mut settings = Settings::default();
        let before = settings.accent_color.clone();
        assert!(!settings.set_accent_color("not-a-colour"));
        assert_eq!(settings.accent_color, before);
        assert!(!settings.set_accent_color("#12345"));
        assert!(!settings.set_accent_color("#1234567"));
    }

    #[test]
    fn setting_a_valid_accent_colour_is_accepted() {
        let mut settings = Settings::default();
        assert!(settings.set_accent_color("#3B82F6"));
        assert_eq!(settings.accent_color, "#3B82F6");
    }

    // A malformed colour on disk must not leave the UI holding a string
    // it cannot turn into a swatch.
    #[test]
    fn a_malformed_accent_colour_is_replaced_with_the_default_on_load() {
        let path = file("bad-accent");
        crate::atomic::write(&path, r#"{"accent-color": "purple"}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().accent_color, DEFAULT_ACCENT);
    }

    #[test]
    fn setting_an_invalid_language_is_rejected() {
        let mut settings = Settings::default();
        let before = settings.language.clone();
        assert!(!settings.set_language("!!"));
        assert_eq!(settings.language, before);
        assert!(!settings.set_language(""));
    }

    #[test]
    fn setting_a_valid_language_is_accepted() {
        let mut settings = Settings::default();
        assert!(settings.set_language("ru"));
        assert_eq!(settings.language, "ru");
    }

    #[test]
    fn a_malformed_language_code_is_replaced_with_english_on_load() {
        let path = file("bad-language");
        crate::atomic::write(&path, r#"{"language": "Not A Code!"}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, "en");
    }

    #[test]
    fn a_well_formed_language_code_survives_load() {
        let path = file("good-language");
        crate::atomic::write(&path, r#"{"language": "ru"}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, "ru");
    }

    #[test]
    fn a_shortcut_round_trips_through_disk() {
        let path = file("shortcuts");
        let mut settings = Settings::default();
        assert!(settings.add_shortcut("Example", "https://example.com"));
        settings.save(&path).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().shortcuts,
            vec![Shortcut {
                title: "Example".into(),
                url: "https://example.com".into()
            }]
        );
    }

    #[test]
    fn the_dock_is_bounded() {
        let mut settings = Settings::default();
        for i in 0..MAX_SHORTCUTS {
            assert!(settings.add_shortcut(&format!("s{i}"), "https://example.com"));
        }
        assert!(!settings.add_shortcut("one too many", "https://example.com"));
        assert_eq!(settings.shortcuts.len(), MAX_SHORTCUTS);
    }

    #[test]
    fn editing_and_removing_out_of_range_is_a_no_op() {
        let mut settings = Settings::default();
        settings.add_shortcut("Example", "https://example.com");
        assert!(!settings.edit_shortcut(9, "x", "y"));
        assert!(!settings.remove_shortcut(9));
        assert_eq!(settings.shortcuts.len(), 1);

        assert!(settings.edit_shortcut(0, "Renamed", "https://other.example"));
        assert_eq!(settings.shortcuts[0].title, "Renamed");
        assert!(settings.remove_shortcut(0));
        assert!(settings.shortcuts.is_empty());
    }

    #[test]
    fn an_oversized_dock_is_clamped_on_load() {
        let path = file("dock-clamp");
        let settings = Settings {
            shortcuts: (0..MAX_SHORTCUTS + 5)
                .map(|i| Shortcut {
                    title: format!("s{i}"),
                    url: "https://example.com".into(),
                })
                .collect(),
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().shortcuts.len(),
            MAX_SHORTCUTS
        );
    }

    #[test]
    fn the_default_engine_is_duckduckgo() {
        assert_eq!(Settings::default().search_engine, "duckduckgo");
    }

    #[test]
    fn a_query_is_percent_encoded() {
        // The template argument is unused by every built-in engine.
        assert_eq!(
            query_url("duckduckgo", "rust browser", ""),
            Some("https://duckduckgo.com/?q=rust%20browser".to_string())
        );
        // Every one of these would change the meaning of the URL.
        assert_eq!(
            query_url("google", "a&b=c#d?e", ""),
            Some("https://www.google.com/search?q=a%26b%3Dc%23d%3Fe".to_string())
        );
        // "+" must not survive: a receiver would read it as a space.
        assert_eq!(
            query_url("bing", "c++", ""),
            Some("https://www.bing.com/search?q=c%2B%2B".to_string())
        );
    }

    #[test]
    fn a_non_ascii_query_is_encoded_as_utf8_bytes() {
        assert_eq!(
            query_url("duckduckgo", "\u{43F}\u{440}\u{438}\u{432}\u{435}\u{442}", ""),
            Some("https://duckduckgo.com/?q=%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82".to_string())
        );
    }
}
