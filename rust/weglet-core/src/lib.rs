// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The browser's model: tabs, windows, history, the omnibox. No IO, no
// engine, no platform.

mod history;
mod omnibox;
mod state;
mod tab;

pub use history::History;
pub use omnibox::{
    parse as parse_omnibox, suggest as suggest_omnibox, Action as OmniboxAction,
    Suggestion as OmniboxSuggestion, SuggestionKind as OmniboxSuggestionKind,
    SuggestionSource as OmniboxSuggestionSource, MAX_SUGGESTIONS as MAX_OMNIBOX_SUGGESTIONS,
};
pub use state::{AppState, RestoredTab, RestoredWindow, Window};
pub use tab::{Tab, TabId, WindowId};

// Generated from contract.json by rust/build_rust.py; C++ generates the
// same four addresses from the same file.
include!("generated_addresses.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_addresses_are_recognised() {
        for url in [
            BLANK_TAB,
            SETTINGS_ADDRESS,
            HISTORY_ADDRESS,
            BOOKMARKS_ADDRESS,
        ] {
            assert!(is_internal_address(url), "{url}");
        }
    }

    // A prefix or suffix test would be a privilege hole: these addresses
    // decide which pages get profile data pushed into them.
    #[test]
    fn a_lookalike_address_is_not_one_of_ours() {
        for url in [
            "weglet://settings.evil.example",
            "weglet://settingsx",
            "https://weglet.example/settings",
            "https://evil.example/weglet://settings",
            "weglet://",
            "",
        ] {
            assert!(!is_internal_address(url), "{url}");
        }
    }
}
