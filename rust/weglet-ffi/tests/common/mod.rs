// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Shared by every file under tests/: the bound ABI surface, and the
// per-test profile directory.
//
// Each file directly under tests/ is its own process, so this module is
// compiled once into each of them rather than shared at link time -- a
// process-global OnceLock set by a test in one file (see blocklist.rs's
// override) cannot leak into a test in another file the way it could
// leak between two tests in the same file.

use std::ffi::{c_char, CStr, CString};
use std::sync::{Mutex, MutexGuard, OnceLock};

// The public surface, bound exactly as weglet_ffi.h declares it. Each
// constant is the real function stored in a variable whose type is
// written out by hand from the header, so a drift is a compile error --
// and calling through them keeps the #[no_mangle] symbols the browser
// links against.
#[allow(non_snake_case, non_upper_case_globals, dead_code)]
pub mod abi {
    use std::ffi::c_char;
    use weglet_ffi::WegletState;

    pub const state_new: extern "C" fn() -> *mut WegletState = weglet_ffi::weglet_state_new;
    pub const state_new_for_profile: unsafe extern "C" fn(*const c_char) -> *mut WegletState =
        weglet_ffi::weglet_state_new_for_profile;
    pub const state_free: unsafe extern "C" fn(*mut WegletState) = weglet_ffi::weglet_state_free;
    pub const string_free: unsafe extern "C" fn(*mut c_char) = weglet_ffi::weglet_string_free;

    pub const current_profile: unsafe extern "C" fn(*const WegletState) -> *mut c_char =
        weglet_ffi::weglet_current_profile;
    pub const profile_count: extern "C" fn() -> usize = weglet_ffi::weglet_profile_count;
    pub const profile_name_at: extern "C" fn(usize) -> *mut c_char =
        weglet_ffi::weglet_profile_name_at;
    pub const is_valid_profile_name: unsafe extern "C" fn(*const c_char) -> bool =
        weglet_ffi::weglet_is_valid_profile_name;
    pub const canonical_profile_name: unsafe extern "C" fn(*const c_char) -> *mut c_char =
        weglet_ffi::weglet_canonical_profile_name;
    pub const delete_profile: unsafe extern "C" fn(*const c_char) -> bool =
        weglet_ffi::weglet_delete_profile;
    pub const rename_profile: unsafe extern "C" fn(*const c_char, *const c_char) -> bool =
        weglet_ffi::weglet_rename_profile;
    pub const profile_accent_color: unsafe extern "C" fn(*const c_char) -> *mut c_char =
        weglet_ffi::weglet_profile_accent_color;
    pub const last_profile: extern "C" fn() -> *mut c_char = weglet_ffi::weglet_last_profile;
    pub const set_last_profile: unsafe extern "C" fn(*const c_char) =
        weglet_ffi::weglet_set_last_profile;

    pub const window_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_window_count;
    pub const window_id_at: unsafe extern "C" fn(*const WegletState, usize) -> u64 =
        weglet_ffi::weglet_window_id_at;
    pub const open_window: unsafe extern "C" fn(*mut WegletState) -> u64 =
        weglet_ffi::weglet_open_window;
    pub const close_window: unsafe extern "C" fn(*mut WegletState, u64) -> bool =
        weglet_ffi::weglet_close_window;
    pub const save_session: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_save_session;
    pub const keep_windows_on_close: unsafe extern "C" fn(*mut WegletState) =
        weglet_ffi::weglet_keep_windows_on_close;
    pub const tab_window: unsafe extern "C" fn(*const WegletState, u64) -> u64 =
        weglet_ffi::weglet_tab_window;

    pub const tab_count: unsafe extern "C" fn(*const WegletState, u64) -> usize =
        weglet_ffi::weglet_tab_count;
    pub const tab_id_at: unsafe extern "C" fn(*const WegletState, u64, usize) -> u64 =
        weglet_ffi::weglet_tab_id_at;
    pub const active_tab_id: unsafe extern "C" fn(*const WegletState, u64) -> u64 =
        weglet_ffi::weglet_active_tab_id;
    pub const tab_url: unsafe extern "C" fn(*const WegletState, u64) -> *mut c_char =
        weglet_ffi::weglet_tab_url;
    pub const tab_label: unsafe extern "C" fn(*const WegletState, u64) -> *mut c_char =
        weglet_ffi::weglet_tab_label;
    pub const open_tab: unsafe extern "C" fn(*mut WegletState, u64, *const c_char) -> u64 =
        weglet_ffi::weglet_open_tab;
    pub const close_tab: unsafe extern "C" fn(*mut WegletState, u64) -> bool =
        weglet_ffi::weglet_close_tab;
    pub const tab_history_changed: unsafe extern "C" fn(
        *mut WegletState,
        u64,
        *const c_char,
        usize,
        bool,
    ) = weglet_ffi::weglet_tab_history_changed;
    pub const tab_can_go_back: unsafe extern "C" fn(*const WegletState, u64) -> bool =
        weglet_ffi::weglet_tab_can_go_back;
    pub const tab_can_go_forward: unsafe extern "C" fn(*const WegletState, u64) -> bool =
        weglet_ffi::weglet_tab_can_go_forward;
    pub const tab_title_changed: unsafe extern "C" fn(*mut WegletState, u64, *const c_char) =
        weglet_ffi::weglet_tab_title_changed;
    pub const tab_history_count: unsafe extern "C" fn(*const WegletState, u64) -> usize =
        weglet_ffi::weglet_tab_history_count;
    pub const tab_history_entry_at: unsafe extern "C" fn(
        *const WegletState,
        u64,
        usize,
    ) -> *mut c_char = weglet_ffi::weglet_tab_history_entry_at;
    pub const tab_history_cursor: unsafe extern "C" fn(*const WegletState, u64) -> usize =
        weglet_ffi::weglet_tab_history_cursor;

    pub const omnibox_resolve: unsafe extern "C" fn(
        *const WegletState,
        *const c_char,
    ) -> *mut c_char = weglet_ffi::weglet_omnibox_resolve;

    pub const query_omnibox_suggestions: unsafe extern "C" fn(
        *mut WegletState,
        *const c_char,
    ) -> usize = weglet_ffi::weglet_query_omnibox_suggestions;
    pub const omnibox_suggestion_label_at: unsafe extern "C" fn(
        *const WegletState,
        usize,
    ) -> *mut c_char = weglet_ffi::weglet_omnibox_suggestion_label_at;
    pub const omnibox_suggestion_url_at: unsafe extern "C" fn(
        *const WegletState,
        usize,
    ) -> *mut c_char = weglet_ffi::weglet_omnibox_suggestion_url_at;
    pub const omnibox_suggestion_is_bookmark_at: unsafe extern "C" fn(*const WegletState, usize) -> bool =
        weglet_ffi::weglet_omnibox_suggestion_is_bookmark_at;

    pub const assess_navigation: unsafe extern "C" fn(
        *const WegletState,
        *const c_char,
        *mut *mut c_char,
        *mut *mut c_char,
        *mut *mut c_char,
    ) -> u32 = weglet_ffi::weglet_assess_navigation;

    pub const block_host: unsafe extern "C" fn(*mut WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_block_host;
    pub const unblock_host: unsafe extern "C" fn(*mut WegletState, usize) -> bool =
        weglet_ffi::weglet_unblock_host;
    pub const blocked_host_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_blocked_host_count;
    pub const blocked_host_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_blocked_host_at;
    pub const is_url_blocked: unsafe extern "C" fn(
        *const WegletState,
        *const c_char,
        *mut *mut c_char,
        *mut *mut c_char,
    ) -> bool = weglet_ffi::weglet_is_url_blocked;
    pub const is_host_blocked: unsafe extern "C" fn(*const WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_is_host_blocked;

    pub const set_accent_color: unsafe extern "C" fn(*mut WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_set_accent_color;
    pub const accent_color: unsafe extern "C" fn(*const WegletState) -> *mut c_char =
        weglet_ffi::weglet_accent_color;
    pub const set_language: unsafe extern "C" fn(*mut WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_set_language;
    pub const language: unsafe extern "C" fn(*const WegletState) -> *mut c_char =
        weglet_ffi::weglet_language;
    pub const settings_dirty: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_settings_dirty;
    pub const flush_settings: unsafe extern "C" fn(*mut WegletState) -> bool =
        weglet_ffi::weglet_flush_settings;

    pub const engine_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_engine_count;
    pub const engine_id_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_engine_id_at;
    pub const set_search_engine: unsafe extern "C" fn(*mut WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_set_search_engine;

    pub const bookmark_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_bookmark_count;
    pub const bookmark_title_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_bookmark_title_at;
    pub const bookmark_url_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_bookmark_url_at;
    pub const is_bookmarked: unsafe extern "C" fn(*const WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_is_bookmarked;
    pub const toggle_bookmark: unsafe extern "C" fn(
        *mut WegletState,
        *const c_char,
        *const c_char,
    ) -> bool = weglet_ffi::weglet_toggle_bookmark;
    pub const remove_bookmark: unsafe extern "C" fn(*mut WegletState, usize) -> bool =
        weglet_ffi::weglet_remove_bookmark;

    pub const history_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_history_count;
    pub const history_query_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_history_query_at;
    pub const history_url_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_history_url_at;
    pub const history_visited_at_at: unsafe extern "C" fn(*const WegletState, usize) -> u64 =
        weglet_ffi::weglet_history_visited_at_at;
    pub const record_history: unsafe extern "C" fn(*mut WegletState, *const c_char, *const c_char) =
        weglet_ffi::weglet_record_history;
    pub const clear_search_history: unsafe extern "C" fn(*mut WegletState) =
        weglet_ffi::weglet_clear_search_history;

    pub const download_count: unsafe extern "C" fn(*const WegletState) -> usize =
        weglet_ffi::weglet_download_count;
    pub const download_filename_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_download_filename_at;
    pub const download_status_at: unsafe extern "C" fn(*const WegletState, usize) -> u32 =
        weglet_ffi::weglet_download_status_at;
    pub const download_size_label_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_download_size_label_at;
    pub const download_error_message_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_download_error_message_at;
    pub const download_path_at: unsafe extern "C" fn(*const WegletState, usize) -> *mut c_char =
        weglet_ffi::weglet_download_path_at;
    pub const download_started: unsafe extern "C" fn(
        *mut WegletState,
        *const c_char,
        *const c_char,
        *const c_char,
    ) = weglet_ffi::weglet_download_started;
    pub const download_progress: unsafe extern "C" fn(
        *mut WegletState,
        *const c_char,
        *const c_char,
        u64,
        i64,
    ) = weglet_ffi::weglet_download_progress;
    pub const download_completed: unsafe extern "C" fn(
        *mut WegletState,
        *const c_char,
        *const c_char,
        u64,
    ) = weglet_ffi::weglet_download_completed;
    pub const download_failed: unsafe extern "C" fn(*mut WegletState, *const c_char, *const c_char) =
        weglet_ffi::weglet_download_failed;
    pub const clear_download_history: unsafe extern "C" fn(*mut WegletState) =
        weglet_ffi::weglet_clear_download_history;

    pub const threat_feed_enabled: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_threat_feed_enabled;
    pub const set_threat_feed_enabled: unsafe extern "C" fn(*mut WegletState, bool) =
        weglet_ffi::weglet_set_threat_feed_enabled;
    pub const favicons_enabled: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_favicons_enabled;
    pub const set_favicons_enabled: unsafe extern "C" fn(*mut WegletState, bool) =
        weglet_ffi::weglet_set_favicons_enabled;
    pub const ask_where_to_save_downloads: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_ask_where_to_save_downloads;
    pub const set_ask_where_to_save_downloads: unsafe extern "C" fn(*mut WegletState, bool) =
        weglet_ffi::weglet_set_ask_where_to_save_downloads;
    pub const is_known_phishing: unsafe extern "C" fn(
        *const WegletState,
        *const c_char,
        *mut *mut c_char,
        *mut *mut c_char,
    ) -> bool = weglet_ffi::weglet_is_known_phishing;
    pub const apply_threat_feed: unsafe extern "C" fn(*mut WegletState, *const c_char) -> bool =
        weglet_ffi::weglet_apply_threat_feed;
    pub const threat_feed_updated_at: unsafe extern "C" fn(*const WegletState) -> u64 =
        weglet_ffi::weglet_threat_feed_updated_at;
    pub const threat_feed_last_update_failed: unsafe extern "C" fn(*const WegletState) -> bool =
        weglet_ffi::weglet_threat_feed_last_update_failed;
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub struct Fixture {
    _guard: MutexGuard<'static, ()>,
    pub dir: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// A private profile directory, held for the duration of one test.
pub fn guard(name: &str) -> Fixture {
    let mutex = lock();
    // A panicking test must not poison the lock for every later one.
    let g = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("weglet-ffi-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", &dir);
    std::env::set_var("LOCALAPPDATA", &dir);
    Fixture { _guard: g, dir }
}

// Takes ownership the way WegletBridge::TakeString does, so a leak here
// would be a leak there.
#[allow(dead_code)]
pub fn take(raw: *mut c_char) -> String {
    assert!(!raw.is_null(), "the boundary must never return null");
    let value = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    unsafe { abi::string_free(raw) };
    value
}

#[allow(dead_code)]
pub fn c(value: &str) -> CString {
    CString::new(value).unwrap()
}

// What the browser reports after a page commits in tab `id`: the engine's
// back/forward list, here one entry long, with `url` current.
#[allow(dead_code)]
pub fn navigate(state: *mut weglet_ffi::WegletState, id: u64, url: &str) {
    unsafe { abi::tab_history_changed(state, id, c(url).as_ptr(), 0, true) };
}
