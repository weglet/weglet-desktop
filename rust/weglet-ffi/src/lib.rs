// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The only place C++ and Rust touch.
//
// No Rust type crosses the boundary: C++ sees an opaque handle and C
// strings. Every string handed over is freed with weglet_string_free.
// Nothing panics across the boundary. A null or unknown handle is a
// no-op with a defined return.

// Private except for `weglet_string_free`, re-exported so
// tests/boundary.rs can bind it by the name the browser links against.
mod string;

pub use string::weglet_string_free;

use std::ffi::c_char;

use weglet_core::{AppState, OmniboxAction, TabId, WindowId};
use weglet_profile::{
    Bookmarks, BrowsingHistory, DownloadStatus, Downloads, Engines, Paths, Session, SessionTab,
    SessionWindow, Settings, ThreatFeedCache,
};

use crate::string::{into_c_string, str_from_c};

// Repeated per entry point because clippy wants it there, and because a
// caller reading one signature should not have to go looking.
macro_rules! safety_doc {
    () => {
        "# Safety\n\n\
         `state` must be null or a live pointer from [`weglet_state_new`] \
         that has not been passed to [`weglet_state_free`]. Every string \
         argument must be null or NUL-terminated and valid for the call. \
         All calls must come from the browser process's UI thread and only \
         that thread. Any returned string is owned by the caller and freed \
         with `weglet_string_free`."
    };
}

// Everything the browser knows, behind one pointer. C++ holds it for the
// life of the process and passes it back on every call.
pub struct WegletState {
    state: AppState,
    settings: Settings,
    // This profile's search engines and security lists: built in, or
    // replaced by the profile's own files. Per profile, never a static:
    // one process runs several profiles.
    engines: Engines,
    lists: weglet_security::Lists,
    bookmarks: Bookmarks,
    history: BrowsingHistory,
    downloads: Downloads,
    threat_feed: ThreatFeedCache,
    paths: Option<Paths>,
    // The name actually used to discover `paths` -- weglet_profile::DEFAULT_PROFILE
    // when the caller passed nothing or an invalid name, never the raw
    // input. See weglet_current_profile.
    profile_name: String,
    // Recomputed by weglet_query_omnibox_suggestions on every keystroke;
    // never persisted. Not `dirty`-tracked like the stores above -- it is
    // derived from them, not a store of its own.
    omnibox_suggestions: Vec<weglet_core::OmniboxSuggestion>,
    // Set while every window closes because the browser is quitting: each
    // one stays in the model, tabs and all, so the session keeps all of
    // them rather than only the one that happened to close last.
    keep_windows_on_close: bool,
    // Set when the matching store changed and has not reached the disk
    // yet. An atomic write ends in fsync, and the moment something
    // changed -- a click, a navigation, a download progress callback --
    // is not the place for it. weglet_flush_settings runs on a timer and
    // at shutdown and flushes every dirty store, not settings alone.
    settings_dirty: bool,
    bookmarks_dirty: bool,
    history_dirty: bool,
    downloads_dirty: bool,
    threat_feed_dirty: bool,
}

// Shared by weglet_state_new (always DEFAULT_PROFILE) and
// weglet_state_new_for_profile (whatever the browser resolved from the
// command line). `profile` is assumed already resolved -- see
// weglet_state_new_for_profile's own fallback.
fn build_state(profile: &str) -> WegletState {
    let paths = Paths::discover(profile).ok();
    let profile_name = profile.to_string();

    // Optional per-profile data files, read before anything asks a
    // question that depends on them. Present means the user's own list
    // replaces the built-in one. A malformed file is ignored, never fatal.
    let (engines, lists) = match &paths {
        Some(paths) => load_overrides(paths),
        None => (Engines::built_in(), weglet_security::Lists::built_in()),
    };

    // A settings file that will not parse must not stop the browser from
    // opening. The C++ side logs it; here the fallback is defaults.
    let mut settings = paths
        .as_ref()
        .and_then(|paths| Settings::load(&paths.settings_file()).ok())
        .unwrap_or_default();
    settings.fit_engines(&engines);

    let state = if settings.restore_session {
        paths
            .as_ref()
            .and_then(|paths| Session::load(&paths.session_file()).ok())
            .map(|session| AppState::restore(session.to_restore_input()))
            .unwrap_or_default()
    } else {
        AppState::new()
    };

    let bookmarks = paths
        .as_ref()
        .and_then(|paths| Bookmarks::load(&paths.bookmarks_file()).ok())
        .unwrap_or_default();
    let history = paths
        .as_ref()
        .and_then(|paths| BrowsingHistory::load(&paths.history_file()).ok())
        .unwrap_or_default();
    let mut downloads = paths
        .as_ref()
        .and_then(|paths| Downloads::load(&paths.downloads_file()).ok())
        .unwrap_or_default();
    // The browser closed mid-download last time; there is no in-progress
    // download left to report back to, on this or any run.
    let downloads_dirty = downloads.fail_orphaned_in_progress() > 0;
    let threat_feed = paths
        .as_ref()
        .and_then(|paths| ThreatFeedCache::load(&paths.threat_feed_file()).ok())
        .unwrap_or_default();

    WegletState {
        state,
        settings,
        engines,
        lists,
        bookmarks,
        history,
        downloads,
        threat_feed,
        paths,
        profile_name,
        omnibox_suggestions: Vec::new(),
        keep_windows_on_close: false,
        settings_dirty: false,
        bookmarks_dirty: false,
        history_dirty: false,
        downloads_dirty,
        threat_feed_dirty: false,
    }
}

// SAFETY: the caller must be the browser process's UI thread, and only
// that thread. Nothing here is synchronised.
#[no_mangle]
pub extern "C" fn weglet_state_new() -> *mut WegletState {
    Box::into_raw(Box::new(build_state(weglet_profile::DEFAULT_PROFILE)))
}

// As weglet_state_new, but for a named profile -- see
// WegletBrowserContext's own --profile switch. An invalid or empty name
// falls back to DEFAULT_PROFILE rather than failing the launch.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_state_new_for_profile(profile: *const c_char) -> *mut WegletState {
    // SAFETY: contract of str_from_c.
    let requested = unsafe { str_from_c(profile) }.unwrap_or_default();
    let name = if weglet_profile::is_valid_profile_name(requested) {
        requested.trim()
    } else {
        weglet_profile::DEFAULT_PROFILE
    };
    Box::into_raw(Box::new(build_state(name)))
}

// ---------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------

// The name this instance actually runs as -- see WegletState::profile_name.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_current_profile(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let text = unsafe { as_ref(state) }
        .map(|state| state.profile_name.clone())
        .unwrap_or_default();
    into_c_string(text)
}

// Every profile this machine has ever used, alphabetical. Re-lists the
// filesystem on every call: only asked for when the Settings page's
// Profiles tab is open, not on any hot path.
#[no_mangle]
pub extern "C" fn weglet_profile_count() -> usize {
    weglet_profile::Paths::list_profiles().len()
}

#[no_mangle]
pub extern "C" fn weglet_profile_name_at(index: usize) -> *mut c_char {
    into_c_string(
        weglet_profile::Paths::list_profiles()
            .get(index)
            .cloned()
            .unwrap_or_default(),
    )
}

// Whether `name` could become a profile directory: non-empty, no path
// separators, nothing a filesystem would reject. Checked before offering
// to create or switch to it -- see WegletBridge::IsValidProfileName.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_is_valid_profile_name(name: *const c_char) -> bool {
    // SAFETY: contract of str_from_c.
    unsafe { str_from_c(name) }.is_some_and(weglet_profile::is_valid_profile_name)
}

// The existing profile `name` refers to, in its own spelling, or `name`
// itself when no profile has it yet. Empty for a null or non-UTF-8 name.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_canonical_profile_name(name: *const c_char) -> *mut c_char {
    // SAFETY: contract of str_from_c.
    let name = unsafe { str_from_c(name) }
        .map(weglet_profile::canonical_profile_name)
        .unwrap_or_default();
    into_c_string(name)
}

// Deletes a profile's whole directory. Never call this with the name of
// the profile the calling process is itself running as -- see
// WegletBridge::DeleteProfile.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_delete_profile(name: *const c_char) -> bool {
    // SAFETY: contract of str_from_c.
    unsafe { str_from_c(name) }.is_some_and(weglet_profile::Paths::delete_profile)
}

// False and nothing touched if `old` doesn't exist, `new` already does,
// or either name is invalid.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_rename_profile(old: *const c_char, new: *const c_char) -> bool {
    // SAFETY: contract of str_from_c, both arguments.
    let (Some(old), Some(new)) = (unsafe { str_from_c(old) }, unsafe { str_from_c(new) }) else {
        return false;
    };
    weglet_profile::Paths::rename_profile(old, new)
}

// A profile's accent colour without loading the rest of its state -- for
// a swatch per row in a list that includes profiles this process is not
// running as.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_profile_accent_color(name: *const c_char) -> *mut c_char {
    // SAFETY: contract of str_from_c.
    let text = unsafe { str_from_c(name) }
        .map(weglet_profile::Paths::peek_accent_color)
        .unwrap_or_default();
    into_c_string(text)
}

// The profile a plain launch with no --profile should open -- whatever
// the last switch or launch left active. Empty when there is none yet.
#[no_mangle]
pub extern "C" fn weglet_last_profile() -> *mut c_char {
    into_c_string(weglet_profile::Paths::last_profile().unwrap_or_default())
}

// Records `name` as weglet_last_profile's answer from now on. A no-op
// for an invalid name.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_last_profile(name: *const c_char) {
    // SAFETY: contract of str_from_c.
    if let Some(name) = unsafe { str_from_c(name) } {
        weglet_profile::Paths::set_last_profile(name);
    }
}

// SAFETY: `state` must come from weglet_state_new and must not be used
// afterwards. Passing null is allowed and does nothing.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_state_free(state: *mut WegletState) {
    if state.is_null() {
        return;
    }
    // SAFETY: checked non-null above, and the contract says this pointer
    // came from Box::into_raw in weglet_state_new.
    drop(unsafe { Box::from_raw(state) });
}

/// Turns a raw handle into a reference, or None. Every entry point starts
/// here so a null from C++ is a defined no-op rather than a crash.
///
/// # Safety
///
/// `state` must be null or a live pointer from `weglet_state_new`.
unsafe fn as_ref<'a>(state: *const WegletState) -> Option<&'a WegletState> {
    // SAFETY: null-checked; validity is the caller's contract.
    unsafe { state.as_ref() }
}

/// # Safety
///
/// As [`as_ref`], and no other reference to the state may be live.
unsafe fn as_mut<'a>(state: *mut WegletState) -> Option<&'a mut WegletState> {
    // SAFETY: null-checked; validity and exclusivity are the caller's
    // contract.
    unsafe { state.as_mut() }
}

// ---------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------

// Every tab question below takes the window it is about.

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_window_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.state.windows().len())
}

// The id at `index`, or 0 when out of range.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_window_id_at(state: *const WegletState, index: usize) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.windows().get(index))
        .map_or(0, |window| window.id.value())
}

// The new window's id, or 0 when the ceiling is reached. It starts with
// one blank tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_open_window(state: *mut WegletState) -> u64 {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return 0;
    };
    let Some(window) = state.state.open_window() else {
        return 0;
    };
    state.keep_windows_on_close = false;
    window.value()
}

// Its tabs go with it, except for the last window, which stays in the
// model with its tabs: the session restores it, and showing the profile
// again brings it back. After weglet_keep_windows_on_close, every window
// stays.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_close_window(state: *mut WegletState, window: u64) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    let window = WindowId::new(window);
    if state.keep_windows_on_close {
        return state.state.window(window).is_some();
    }
    state.state.close_window(window)
}

// Every window is about to close because the browser is quitting: each
// one stays in the model, so the session keeps all of them. Undone by
// weglet_open_window.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_keep_windows_on_close(state: *mut WegletState) {
    // SAFETY: contract of as_mut.
    if let Some(state) = unsafe { as_mut(state) } {
        state.keep_windows_on_close = true;
    }
}

// Which window a tab is in, or 0 for no such tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_window(state: *const WegletState, id: u64) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .map_or(0, |tab| tab.window.value())
}

// ---------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_count(state: *const WegletState, window: u64) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| {
        state.state.tab_count_in(WindowId::new(window))
    })
}

// The id at `index`, or 0 if out of range.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_id_at(
    state: *const WegletState,
    window: u64,
    index: usize,
) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab_in_at(WindowId::new(window), index))
        .map_or(0, |tab| tab.id.value())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_active_tab_id(state: *const WegletState, window: u64) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.active_id(WindowId::new(window)))
        .map_or(0, |id| id.value())
}

// Owned by the caller. Free with weglet_string_free.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_url(state: *const WegletState, id: u64) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let url = unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .map(|tab| tab.url().to_string())
        .unwrap_or_default();
    into_c_string(url)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_label(state: *const WegletState, id: u64) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let label = unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .map(|tab| tab.label().to_string())
        .unwrap_or_default();
    into_c_string(label)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_can_go_back(state: *const WegletState, id: u64) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .is_some_and(|tab| tab.history.can_go_back())
}

// Whether this tab is loading.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_loading(state: *const WegletState, id: u64) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .is_some_and(|tab| tab.loading)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_can_go_forward(state: *const WegletState, id: u64) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .is_some_and(|tab| tab.history.can_go_forward())
}

// The new tab's id, or 0 when the tab ceiling is reached. `url` may be
// null, which opens a blank tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_open_tab(
    state: *mut WegletState,
    window: u64,
    url: *const c_char,
) -> u64 {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return 0;
    };
    // SAFETY: contract of str_from_c.
    let url = unsafe { str_from_c(url) }.unwrap_or(weglet_core::BLANK_TAB);
    state
        .state
        .open_tab(WindowId::new(window), url)
        .map_or(0, |id| id.value())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_close_tab(state: *mut WegletState, id: u64) -> bool {
    // SAFETY: contract of as_mut.
    unsafe { as_mut(state) }.is_some_and(|state| state.state.close_tab(TabId::new(id)))
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_activate_tab(state: *mut WegletState, id: u64) -> bool {
    // SAFETY: contract of as_mut.
    unsafe { as_mut(state) }.is_some_and(|state| state.state.activate(TabId::new(id)))
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_cycle_tab(state: *mut WegletState, window: u64, forward: bool) {
    // SAFETY: contract of as_mut.
    if let Some(state) = unsafe { as_mut(state) } {
        state.state.cycle(WindowId::new(window), forward);
    }
}

// One-based, as on the keyboard. 9 means the last tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_activate_tab_at(
    state: *mut WegletState,
    window: u64,
    position: usize,
) -> bool {
    // SAFETY: contract of as_mut.
    unsafe { as_mut(state) }
        .is_some_and(|state| state.state.activate_by_position(WindowId::new(window), position))
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_reorder_tab(
    state: *mut WegletState,
    id: u64,
    target: usize,
) -> bool {
    // SAFETY: contract of as_mut.
    unsafe { as_mut(state) }.is_some_and(|state| state.state.reorder(TabId::new(id), target))
}

// ---------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------

// Between the entries of a back/forward list going into the model. A URL
// the engine reports never contains a raw newline, so the separator cannot
// occur inside an entry. One string rather than one call per entry: the
// list replaces the tab's copy whole, and a half-sent list would be a tab
// whose cursor points at an entry that is not there yet.
const HISTORY_SEPARATOR: char = '\n';

// The engine's back/forward list for a tab, after it changed: every entry
// oldest first, separated by HISTORY_SEPARATOR, and which one is current.
// The engine owns the list and does the navigating; the tab keeps a copy
// for the tab strip and the session. An empty entry is dropped and the
// cursor moved to match. `new_document` is false for a pushState or a
// fragment change, which keeps the title.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_history_changed(
    state: *mut WegletState,
    id: u64,
    entries: *const c_char,
    cursor: usize,
    new_document: bool,
) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c.
    let Some(entries) = (unsafe { str_from_c(entries) }) else {
        return;
    };
    if let Some(tab) = state.state.tab_mut(TabId::new(id)) {
        let entries = entries
            .split(HISTORY_SEPARATOR)
            .map(str::to_string)
            .collect();
        tab.sync_history(entries, cursor, new_document);
    }
}

// The tab's copy of its back/forward list, for handing a restored tab's
// entries to the engine. 0 for no such tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_history_count(state: *const WegletState, id: u64) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .map_or(0, |tab| tab.history.entries().len())
}

// The entry at `index`, oldest first, or an empty string when out of range.
// Owned by the caller.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_history_entry_at(
    state: *const WegletState,
    id: u64,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let entry = unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .and_then(|tab| tab.history.entries().get(index))
        .cloned()
        .unwrap_or_default();
    into_c_string(entry)
}

// Which of those entries is current. 0 for no such tab.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_history_cursor(state: *const WegletState, id: u64) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.state.tab(TabId::new(id)))
        .map_or(0, |tab| tab.history.cursor())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_title_changed(
    state: *mut WegletState,
    id: u64,
    title: *const c_char,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c.
    let title = unsafe { str_from_c(title) }.unwrap_or_default();
    if let Some(tab) = state.state.tab_mut(TabId::new(id)) {
        tab.title = title.to_string();
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_tab_loading_changed(
    state: *mut WegletState,
    id: u64,
    loading: bool,
) {
    // SAFETY: contract of as_mut.
    if let Some(state) = unsafe { as_mut(state) } {
        if let Some(tab) = state.state.tab_mut(TabId::new(id)) {
            tab.loading = loading;
        }
    }
}

// ---------------------------------------------------------------------
// Omnibox
// ---------------------------------------------------------------------

// What the user typed, turned into a URL to load. Owned by the caller.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_omnibox_resolve(
    state: *const WegletState,
    input: *const c_char,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let Some(state) = (unsafe { as_ref(state) }) else {
        return into_c_string(String::new());
    };
    // SAFETY: contract of str_from_c.
    let Some(input) = (unsafe { str_from_c(input) }) else {
        return into_c_string(String::new());
    };

    let url = match weglet_core::parse_omnibox(input) {
        OmniboxAction::Navigate(url) => url,
        OmniboxAction::Search(query) => state
            .engines
            .query_url(
                &state.settings.search_engine,
                &query,
                &state.settings.custom_search_url,
            )
            // A broken custom engine, or an id an override removed, falls
            // back to the default: Enter has to go somewhere.
            .unwrap_or_else(|| {
                state
                    .engines
                    .query_url(state.engines.default_id(), &query, "")
                    .expect("the default engine always produces a URL")
            }),
    };
    into_c_string(url)
}

// Recomputes the address bar's dropdown from bookmarks and history and
// stores it on `state`, returning how many there are. The three
// accessors below read that stored list -- call this first on every
// keystroke; it is cheap (a profile has at most a few thousand entries)
// and there is no other invalidation to get wrong.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_query_omnibox_suggestions(
    state: *mut WegletState,
    input: *const c_char,
) -> usize {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return 0;
    };
    // SAFETY: contract of str_from_c.
    let Some(input) = (unsafe { str_from_c(input) }) else {
        state.omnibox_suggestions.clear();
        return 0;
    };

    let bookmark_sources: Vec<weglet_core::OmniboxSuggestionSource> = state
        .bookmarks
        .entries
        .iter()
        .map(|entry| weglet_core::OmniboxSuggestionSource {
            label: &entry.title,
            url: &entry.url,
        })
        .collect();
    let history_sources: Vec<weglet_core::OmniboxSuggestionSource> = state
        .history
        .entries
        .iter()
        .map(|entry| weglet_core::OmniboxSuggestionSource {
            label: &entry.query,
            url: &entry.url,
        })
        .collect();
    state.omnibox_suggestions =
        weglet_core::suggest_omnibox(input, &bookmark_sources, &history_sources);
    state.omnibox_suggestions.len()
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_omnibox_suggestion_label_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let text = unsafe { as_ref(state) }
        .and_then(|state| state.omnibox_suggestions.get(index))
        .map(|suggestion| suggestion.label.clone())
        .unwrap_or_default();
    into_c_string(text)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_omnibox_suggestion_url_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let text = unsafe { as_ref(state) }
        .and_then(|state| state.omnibox_suggestions.get(index))
        .map(|suggestion| suggestion.url.clone())
        .unwrap_or_default();
    into_c_string(text)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_omnibox_suggestion_is_bookmark_at(
    state: *const WegletState,
    index: usize,
) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.omnibox_suggestions.get(index))
        .is_some_and(|suggestion| suggestion.kind == weglet_core::OmniboxSuggestionKind::Bookmark)
}

// ---------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------

// 0 = nothing to say, 1 = warn, 2 = block (ints, not an enum, so the
// layout across the boundary stays fixed). `title`/`reason`/`host` may
// each be null; a non-null one is always filled, even when the answer
// is 0. A pure function of the URL; the user's own block list is
// weglet_is_host_blocked.
///
/// # Safety
///
/// `url` must be null or NUL-terminated. Each of `title`, `reason` and
/// `host` must be null or a writable `char*`; any string written is
/// owned by the caller and freed with `weglet_string_free`.
#[no_mangle]
pub unsafe extern "C" fn weglet_assess_navigation(
    state: *const WegletState,
    url: *const c_char,
    title: *mut *mut c_char,
    reason: *mut *mut c_char,
    host: *mut *mut c_char,
) -> u32 {
    // SAFETY: each pointer is null-checked before it is written, and the
    // caller's contract says a non-null one is writable.
    let write = |slot: *mut *mut c_char, value: String| {
        if !slot.is_null() {
            unsafe { *slot = into_c_string(value) };
        }
    };

    // SAFETY: contract of as_ref and str_from_c.
    let risk = unsafe { as_ref(state) }.and_then(|state| {
        unsafe { str_from_c(url) }
            .and_then(|url| weglet_security::assess_navigation(&state.lists, url))
    });

    let Some(risk) = risk else {
        write(title, String::new());
        write(reason, String::new());
        write(host, String::new());
        return 0;
    };

    let level = match risk.level {
        weglet_security::RiskLevel::Warning => 1,
        weglet_security::RiskLevel::Block => 2,
    };
    write(title, risk.title);
    write(reason, risk.reason);
    // Empty when the assessment has no host to show. The target URL is
    // not returned: the caller passed it in.
    write(host, risk.normalized_host.unwrap_or_default());
    level
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_is_host_blocked(
    state: *const WegletState,
    host: *const c_char,
) -> bool {
    // SAFETY: contract of as_ref.
    let Some(state) = (unsafe { as_ref(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let Some(host) = (unsafe { str_from_c(host) }) else {
        return false;
    };
    is_blocked(state, host)
}

// Same question, whole URL, with the wording for the notice. The host is
// pulled out on this side -- a second parser in C++ wouldn't know a
// backslash ends the authority and the host follows the last '@', which
// is what makes "google.com@evil.example" evil.example. False for
// anything with no host (about:blank, chrome://weglet/). `title`/`reason`
// may each be null; a non-null one is always written, empty when false.
///
/// # Safety
///
/// `url` must be null or NUL-terminated. `title` and `reason` must each be
/// null or a writable `char*`; any string written is owned by the caller
/// and freed with `weglet_string_free`.
#[no_mangle]
pub unsafe extern "C" fn weglet_is_url_blocked(
    state: *const WegletState,
    url: *const c_char,
    title: *mut *mut c_char,
    reason: *mut *mut c_char,
) -> bool {
    // SAFETY: each pointer is null-checked before it is written.
    let write = |slot: *mut *mut c_char, value: &str| {
        if !slot.is_null() {
            unsafe { *slot = into_c_string(value.to_string()) };
        }
    };

    // SAFETY: contract of as_ref and str_from_c.
    let blocked = unsafe { as_ref(state) }.is_some_and(|state| {
        unsafe { str_from_c(url) }
            .and_then(weglet_url::host)
            .is_some_and(|host| is_blocked(state, host))
    });

    // Written either way, so a caller cannot read whatever the pointer
    // held before the call.
    if blocked {
        write(title, weglet_security::USER_BLOCK_TITLE);
        write(reason, weglet_security::USER_BLOCK_REASON);
    } else {
        write(title, "");
        write(reason, "");
    }
    blocked
}

fn is_blocked(state: &WegletState, host: &str) -> bool {
    weglet_security::matches_user_blocklist(host, &state.settings.blocked_hosts)
        || weglet_security::is_blocked_host(&state.lists, host)
}

// ---------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_shortcut_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.settings.shortcuts.len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_shortcut_title(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let title = unsafe { as_ref(state) }
        .and_then(|state| state.settings.shortcuts.get(index))
        .map(|shortcut| shortcut.title.clone())
        .unwrap_or_default();
    into_c_string(title)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_shortcut_url(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let url = unsafe { as_ref(state) }
        .and_then(|state| state.settings.shortcuts.get(index))
        .map(|shortcut| shortcut.url.clone())
        .unwrap_or_default();
    into_c_string(url)
}

// False when the dock is full, so the caller can say so.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_add_shortcut(
    state: *mut WegletState,
    title: *const c_char,
    url: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let (Some(title), Some(url)) =
        (unsafe { str_from_c(title) }, unsafe { str_from_c(url) })
    else {
        return false;
    };
    // Stored as typed and resolved when the tile is clicked, the same way
    // an address bar entry is, so "example.com" works here too.
    if !state.settings.add_shortcut(title, url) {
        return false;
    }
    state.persist_settings();
    true
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_edit_shortcut(
    state: *mut WegletState,
    index: usize,
    title: *const c_char,
    url: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let (Some(title), Some(url)) =
        (unsafe { str_from_c(title) }, unsafe { str_from_c(url) })
    else {
        return false;
    };
    if !state.settings.edit_shortcut(index, title, url) {
        return false;
    }
    state.persist_settings();
    true
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_remove_shortcut(
    state: *mut WegletState,
    index: usize,
) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    if !state.settings.remove_shortcut(index) {
        return false;
    }
    state.persist_settings();
    true
}

// The line under the new tab page's search field. Composed here because
// the engine is a profile setting and the page does not read the profile.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_new_tab_hint(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let hint = unsafe { as_ref(state) }
        .map(|state| {
            let engine = state.engines.label(&state.settings.search_engine);
            format!("Searches go to {engine}")
        })
        .unwrap_or_default();
    into_c_string(hint)
}

// ---------------------------------------------------------------------
// Bookmarks
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_bookmark_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.bookmarks.entries.len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_bookmark_title_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let title = unsafe { as_ref(state) }
        .and_then(|state| state.bookmarks.entries.get(index))
        .map(|bookmark| bookmark.title.clone())
        .unwrap_or_default();
    into_c_string(title)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_bookmark_url_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let url = unsafe { as_ref(state) }
        .and_then(|state| state.bookmarks.entries.get(index))
        .map(|bookmark| bookmark.url.clone())
        .unwrap_or_default();
    into_c_string(url)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_is_bookmarked(
    state: *const WegletState,
    url: *const c_char,
) -> bool {
    // SAFETY: contract of as_ref and str_from_c.
    let Some(state) = (unsafe { as_ref(state) }) else {
        return false;
    };
    let Some(url) = (unsafe { str_from_c(url) }) else {
        return false;
    };
    state.bookmarks.is_bookmarked(url)
}

// Adds `url` if it is not already saved, removes it if it is. Returns
// whether the page is bookmarked after the call, so the caller can
// repaint the toolbar's star from one answer rather than asking twice.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_toggle_bookmark(
    state: *mut WegletState,
    title: *const c_char,
    url: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    let (Some(title), Some(url)) = (unsafe { str_from_c(title) }, unsafe { str_from_c(url) })
    else {
        return false;
    };
    let now_bookmarked = if state.bookmarks.is_bookmarked(url) {
        let index = state
            .bookmarks
            .entries
            .iter()
            .position(|entry| entry.url == url);
        if let Some(index) = index {
            state.bookmarks.remove(index);
        }
        false
    } else {
        state.bookmarks.add(title, url);
        true
    };
    state.persist_bookmarks();
    now_bookmarked
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_remove_bookmark(state: *mut WegletState, index: usize) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    if !state.bookmarks.remove(index) {
        return false;
    }
    state.persist_bookmarks();
    true
}

// ---------------------------------------------------------------------
// Browsing history
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_history_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.history.entries.len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_history_query_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let query = unsafe { as_ref(state) }
        .and_then(|state| state.history.entries.get(index))
        .map(|entry| entry.query.clone())
        .unwrap_or_default();
    into_c_string(query)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_history_url_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let url = unsafe { as_ref(state) }
        .and_then(|state| state.history.entries.get(index))
        .map(|entry| entry.url.clone())
        .unwrap_or_default();
    into_c_string(url)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_history_visited_at_at(
    state: *const WegletState,
    index: usize,
) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.history.entries.get(index))
        .map_or(0, |entry| entry.visited_at)
}

// Called once a typed address resolves -- to a search results page, or
// to itself when it was already a URL. `query` is what the user typed,
// `url` is where it went.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_record_history(
    state: *mut WegletState,
    query: *const c_char,
    url: *const c_char,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    let (Some(query), Some(url)) = (unsafe { str_from_c(query) }, unsafe { str_from_c(url) })
    else {
        return;
    };
    state.history.record(query, url);
    state.persist_history();
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_clear_search_history(state: *mut WegletState) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    state.history.clear();
    state.persist_history();
}

// ---------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.downloads.records.len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_filename_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let filename = unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .map(|record| record.filename.clone())
        .unwrap_or_default();
    into_c_string(filename)
}

// 0 = in progress, 1 = completed, 2 = failed.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_status_at(
    state: *const WegletState,
    index: usize,
) -> u32 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .map_or(0, |record| match record.status {
            DownloadStatus::InProgress => 0,
            DownloadStatus::Completed => 1,
            DownloadStatus::Failed => 2,
        })
}

// "1.2 MB of 4.0 MB" in progress, the final size once complete -- one
// line the page shows as-is, the same way weglet-security composes the
// notice's wording rather than handing the page numbers to format.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_size_label_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let label = unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .map(|record| match record.status {
            DownloadStatus::InProgress => {
                weglet_profile::progress_label(record.bytes_downloaded, record.total_bytes)
            }
            _ => weglet_profile::format_bytes(record.bytes_downloaded),
        })
        .unwrap_or_default();
    into_c_string(label)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_error_message_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let message = unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .and_then(|record| record.error_message.clone())
        .unwrap_or_default();
    into_c_string(message)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_started_at_at(
    state: *const WegletState,
    index: usize,
) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .map_or(0, |record| record.started_at)
}

// The file's own path on disk, for "reveal in folder" and "open".
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_path_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let path = unsafe { as_ref(state) }
        .and_then(|state| state.downloads.records.get(index))
        .map(|record| record.path.clone())
        .unwrap_or_default();
    into_c_string(path)
}

// The four calls below mirror content::DownloadItem's own lifecycle:
// started once, then either progress* any number of times or one of the
// two terminal calls. The browser process is the caller for all of
// them -- it owns the real DownloadManager, this only keeps a record of
// what it reported.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_started(
    state: *mut WegletState,
    guid: *const c_char,
    url: *const c_char,
    path: *const c_char,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c, every argument.
    let (Some(guid), Some(url), Some(path)) = (
        unsafe { str_from_c(guid) },
        unsafe { str_from_c(url) },
        unsafe { str_from_c(path) },
    ) else {
        return;
    };
    state.downloads.started(guid, url, std::path::Path::new(path));
    state.persist_downloads();
}

// `path` is the target path, empty until the download manager has decided
// it. `total_bytes` is passed as -1 when the server sent no
// Content-Length.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_progress(
    state: *mut WegletState,
    guid: *const c_char,
    path: *const c_char,
    bytes_downloaded: u64,
    total_bytes: i64,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c, both arguments.
    let (Some(guid), Some(path)) = (unsafe { str_from_c(guid) }, unsafe { str_from_c(path) })
    else {
        return;
    };
    let total = (total_bytes >= 0).then_some(total_bytes as u64);
    if state
        .downloads
        .update_progress(guid, std::path::Path::new(path), bytes_downloaded, total)
    {
        state.persist_downloads();
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_completed(
    state: *mut WegletState,
    guid: *const c_char,
    path: *const c_char,
    size_bytes: u64,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c, both arguments.
    let (Some(guid), Some(path)) = (unsafe { str_from_c(guid) }, unsafe { str_from_c(path) })
    else {
        return;
    };
    if state
        .downloads
        .mark_completed(guid, std::path::Path::new(path), size_bytes)
    {
        state.persist_downloads();
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_download_failed(
    state: *mut WegletState,
    guid: *const c_char,
    message: *const c_char,
) {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c, both arguments.
    let (Some(guid), Some(message)) = (unsafe { str_from_c(guid) }, unsafe { str_from_c(message) })
    else {
        return;
    };
    if state.downloads.mark_failed(guid, message.to_string()) {
        state.persist_downloads();
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_clear_download_history(state: *mut WegletState) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    state.downloads.clear();
    state.persist_downloads();
}

// ---------------------------------------------------------------------
// Threat feed
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_threat_feed_enabled(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.settings.threat_feed_enabled)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_threat_feed_enabled(state: *mut WegletState, on: bool) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    state.settings.threat_feed_enabled = on;
    state.persist_settings();
}

// ---------------------------------------------------------------------
// Favicons
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_favicons_enabled(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.settings.favicons_enabled)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_favicons_enabled(state: *mut WegletState, on: bool) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    state.settings.favicons_enabled = on;
    state.persist_settings();
}

// ---------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_ask_where_to_save_downloads(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.settings.ask_where_to_save_downloads)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_ask_where_to_save_downloads(state: *mut WegletState, on: bool) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    state.settings.ask_where_to_save_downloads = on;
    state.persist_settings();
}

// Whether the URL matches a known-phishing indicator. Off entirely when
// the setting is off -- checked here rather than left to the caller, so
// there is exactly one place that can disagree with the toggle.
//
// `title` and `reason` may each be null; a non-null one is always
// written, empty when the answer is false. Shaped like
// weglet_is_url_blocked for the same reason: the notice page renders
// whatever wording it is handed.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_is_known_phishing(
    state: *const WegletState,
    url: *const c_char,
    title: *mut *mut c_char,
    reason: *mut *mut c_char,
) -> bool {
    let write = |slot: *mut *mut c_char, value: &str| {
        if !slot.is_null() {
            // SAFETY: contract of the caller -- a non-null slot is
            // writable.
            unsafe { *slot = into_c_string(value.to_string()) };
        }
    };

    // SAFETY: contract of as_ref and str_from_c.
    let matched = (unsafe { as_ref(state) }).is_some_and(|state| {
        state.settings.threat_feed_enabled
            && (unsafe { str_from_c(url) }).is_some_and(|url| {
                weglet_security::is_known_phishing(url, &state.threat_feed.indicators)
            })
    });

    if matched {
        write(title, weglet_security::KNOWN_PHISHING_TITLE);
        write(reason, weglet_security::KNOWN_PHISHING_REASON);
    } else {
        write(title, "");
        write(reason, "");
    }
    matched
}

// `body` is the feed's raw text, already downloaded by the browser (see
// threat_feed.rs for why the fetch itself isn't here). On success,
// replaces the live indicator set and the persisted cache; on failure,
// leaves both standing and just marks the cache's failure flag, so a
// broken feed doesn't erase yesterday's good one.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_apply_threat_feed(
    state: *mut WegletState,
    body: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    let Some(body) = (unsafe { str_from_c(body) }) else {
        return false;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match weglet_security::parse_feed(body) {
        Ok(parsed) => {
            state.threat_feed.indicators = parsed.indicators;
            state.threat_feed.source_count = parsed.source_count;
            state.threat_feed.updated_at = now;
            state.threat_feed.last_update_failed = false;
            state.persist_threat_feed();
            true
        }
        Err(_) => {
            state.threat_feed.last_update_failed = true;
            state.persist_threat_feed();
            false
        }
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_threat_feed_updated_at(state: *const WegletState) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.threat_feed.updated_at)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_threat_feed_last_update_failed(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.threat_feed.last_update_failed)
}

// ---------------------------------------------------------------------
// Settings: search
// ---------------------------------------------------------------------

// "duckduckgo" | "google" | "bing" | "custom". A string rather than an
// int: the settings page has to display and compare it.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_search_engine(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let id = unsafe { as_ref(state) }
        .map(|state| state.settings.search_engine.clone())
        .unwrap_or_default();
    into_c_string(id)
}


// False for an id the browser does not recognise, which leaves the
// setting as it was rather than resetting it.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_search_engine(
    state: *mut WegletState,
    id: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut and str_from_c.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let Some(id) = (unsafe { str_from_c(id) }) else {
        return false;
    };
    // Checked against the table rather than a match arm per engine: an id
    // it does not contain is a page sending something no settings screen
    // offered.
    if !state.engines.is_known(id) {
        return false;
    }
    state.settings.search_engine = id.to_string();
    state.persist_settings();
    true
}

// The number of engines the settings page can offer; read it, then
// weglet_engine_id_at / weglet_engine_label_at for 0..count. "custom" is
// not in the list -- it is what the template box means.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_engine_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.engines.all().len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_engine_id_at(state: *const WegletState, index: usize) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let id = unsafe { as_ref(state) }
        .and_then(|state| state.engines.all().get(index))
        .map(|engine| engine.id.clone())
        .unwrap_or_default();
    into_c_string(id)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_engine_label_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let label = unsafe { as_ref(state) }
        .and_then(|state| state.engines.all().get(index))
        .map(|engine| engine.label.clone())
        .unwrap_or_default();
    into_c_string(label)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_custom_search_url(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let url = unsafe { as_ref(state) }
        .map(|state| state.settings.custom_search_url.clone())
        .unwrap_or_default();
    into_c_string(url)
}

// Accepted even when it will not validate as a template yet: the user is
// often mid-edit. Validated where it is used, in weglet_omnibox_resolve.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_custom_search_url(
    state: *mut WegletState,
    url: *const c_char,
) {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return;
    };
    // SAFETY: contract of str_from_c.
    let Some(url) = (unsafe { str_from_c(url) }) else {
        return;
    };
    state.settings.custom_search_url = url.to_string();
    state.persist_settings();
}

// ---------------------------------------------------------------------
// Settings: general
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_restore_session(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.settings.restore_session)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_restore_session(state: *mut WegletState, on: bool) {
    // SAFETY: contract of as_mut.
    if let Some(state) = unsafe { as_mut(state) } {
        state.settings.restore_session = on;
        state.persist_settings();
    }
}

// ---------------------------------------------------------------------
// Settings: appearance
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_accent_color(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let color = unsafe { as_ref(state) }
        .map(|state| state.settings.accent_color.clone())
        .unwrap_or_default();
    into_c_string(color)
}

// False for anything that is not #RRGGBB, so the colour is never left as
// something the UI cannot turn into a swatch.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_accent_color(
    state: *mut WegletState,
    color: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let Some(color) = (unsafe { str_from_c(color) }) else {
        return false;
    };
    if !state.settings.set_accent_color(color) {
        return false;
    }
    state.persist_settings();
    true
}

// "pill" | "rounded" | "square".
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_address_bar_shape(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let shape = unsafe { as_ref(state) }
        .map(|state| address_bar_shape_id(state.settings.address_bar_shape))
        .unwrap_or("pill");
    into_c_string(shape.to_string())
}

fn address_bar_shape_id(shape: weglet_profile::AddressBarShape) -> &'static str {
    match shape {
        weglet_profile::AddressBarShape::Pill => "pill",
        weglet_profile::AddressBarShape::Rounded => "rounded",
        weglet_profile::AddressBarShape::Square => "square",
    }
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_address_bar_shape(
    state: *mut WegletState,
    shape: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let parsed = match unsafe { str_from_c(shape) } {
        Some("pill") => weglet_profile::AddressBarShape::Pill,
        Some("rounded") => weglet_profile::AddressBarShape::Rounded,
        Some("square") => weglet_profile::AddressBarShape::Square,
        _ => return false,
    };
    state.settings.address_bar_shape = parsed;
    state.persist_settings();
    true
}

// A BCP-47-ish code such as "en" or "ru". Not an enum on this side of the
// FFI either -- see weglet_profile::Settings::language for why.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_language(state: *const WegletState) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let language = unsafe { as_ref(state) }
        .map(|state| state.settings.language.clone())
        .unwrap_or_else(|| "en".to_string());
    into_c_string(language)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_set_language(
    state: *mut WegletState,
    language: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let Some(language) = (unsafe { str_from_c(language) }) else {
        return false;
    };
    if !state.settings.set_language(language) {
        return false;
    }
    state.persist_settings();
    true
}

// ---------------------------------------------------------------------
// Settings: blocked hosts
// ---------------------------------------------------------------------

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_blocked_host_count(state: *const WegletState) -> usize {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(0, |state| state.settings.blocked_hosts.len())
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_blocked_host_at(
    state: *const WegletState,
    index: usize,
) -> *mut c_char {
    // SAFETY: contract of as_ref.
    let host = unsafe { as_ref(state) }
        .and_then(|state| state.settings.blocked_hosts.get(index))
        .cloned()
        .unwrap_or_default();
    into_c_string(host)
}

// Canonicalised before storing -- see weglet_security::canonical_host --
// so "EXAMPLE.com." and "example.com" are one entry, not two that
// disagree about whether the site is blocked.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_block_host(
    state: *mut WegletState,
    host: *const c_char,
) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    // SAFETY: contract of str_from_c.
    let Some(host) = (unsafe { str_from_c(host) }) else {
        return false;
    };
    let canonical = weglet_security::canonical_host(host);
    if canonical.is_empty() || state.settings.blocked_hosts.iter().any(|h| h == &canonical) {
        return false;
    }
    state.settings.blocked_hosts.push(canonical);
    state.persist_settings();
    true
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_unblock_host(state: *mut WegletState, index: usize) -> bool {
    // SAFETY: contract of as_mut.
    let Some(state) = (unsafe { as_mut(state) }) else {
        return false;
    };
    if index >= state.settings.blocked_hosts.len() {
        return false;
    }
    state.settings.blocked_hosts.remove(index);
    state.persist_settings();
    true
}

// ---------------------------------------------------------------------
// Settings and session
// ---------------------------------------------------------------------

// Writes the session to disk. False when there was nowhere to write or
// the write failed; the caller logs it.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_save_session(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    let Some(state) = (unsafe { as_ref(state) }) else {
        return false;
    };
    if !state.settings.restore_session {
        return true;
    }
    let Some(paths) = &state.paths else {
        return false;
    };

    session_of(&state.state).save(&paths.session_file()).is_ok()
}

fn session_of(state: &AppState) -> Session {
    Session {
        windows: state
            .windows()
            .iter()
            .map(|window| SessionWindow {
                tabs: state
                    .tabs_in(window.id)
                    .map(|tab| SessionTab {
                        entries: tab.history.entries().to_vec(),
                        cursor: tab.history.cursor(),
                    })
                    .collect(),
                active: state.position_in_window(window.active()).unwrap_or(0),
            })
            .collect(),
        ..Session::default()
    }
}

// Reads the four optional files a profile may carry. Errors are dropped:
// weglet_state_new has no way to report them, and a malformed file leaves
// the built-in list standing.
fn load_overrides(paths: &Paths) -> (Engines, weglet_security::Lists) {
    // A brands or engines table written by hand for an older Weglet is
    // TOML; it is converted once, like the stores. One that will not
    // convert is left in place and ignored, never fatal.
    for file in [paths.brands_file(), paths.engines_file()] {
        let _ = weglet_profile::migrate_legacy_toml(&file);
    }
    let mut lists = weglet_security::Lists::built_in();
    if let Ok(text) = std::fs::read_to_string(paths.blocklist_file()) {
        lists.replace_blocklist(&text);
    }
    if let Ok(text) = std::fs::read_to_string(paths.brands_file()) {
        let _ = lists.replace_brands(&text);
    }
    if let Ok(text) = std::fs::read_to_string(paths.sensitive_words_file()) {
        lists.replace_sensitive_words(&text);
    }
    let engines = std::fs::read_to_string(paths.engines_file())
        .ok()
        .and_then(|text| Engines::parse(&text).ok())
        .unwrap_or_else(Engines::built_in);
    (engines, lists)
}

impl WegletState {
    // Marks settings as needing a write. Does not touch the disk.
    fn persist_settings(&mut self) {
        self.settings_dirty = true;
    }

    fn persist_bookmarks(&mut self) {
        self.bookmarks_dirty = true;
    }

    fn persist_history(&mut self) {
        self.history_dirty = true;
    }

    fn persist_downloads(&mut self) {
        self.downloads_dirty = true;
    }

    fn persist_threat_feed(&mut self) {
        self.threat_feed_dirty = true;
    }

    // Every dirty store, each independent: one failing to write does not
    // stop the others, and each stays dirty on its own failure so the
    // next flush retries just that one.
    fn flush_all(&mut self) -> bool {
        let Some(paths) = self.paths.clone() else {
            // Nowhere to write. Left dirty rather than cleared, so a
            // later call still reports the failure.
            return !(self.settings_dirty
                || self.bookmarks_dirty
                || self.history_dirty
                || self.downloads_dirty
                || self.threat_feed_dirty);
        };
        let mut ok = true;

        if self.settings_dirty {
            match self.settings.save(&paths.settings_file()) {
                Ok(()) => self.settings_dirty = false,
                Err(_) => ok = false,
            }
        }
        if self.bookmarks_dirty {
            match self.bookmarks.save(&paths.bookmarks_file()) {
                Ok(()) => self.bookmarks_dirty = false,
                Err(_) => ok = false,
            }
        }
        if self.history_dirty {
            match self.history.save(&paths.history_file()) {
                Ok(()) => self.history_dirty = false,
                Err(_) => ok = false,
            }
        }
        if self.downloads_dirty {
            match self.downloads.save(&paths.downloads_file()) {
                Ok(()) => self.downloads_dirty = false,
                Err(_) => ok = false,
            }
        }
        if self.threat_feed_dirty {
            match self.threat_feed.save(&paths.threat_feed_file()) {
                Ok(()) => self.threat_feed_dirty = false,
                Err(_) => ok = false,
            }
        }
        ok
    }
}

// Writes every pending change -- settings, bookmarks, history, downloads,
// the threat-feed cache. False when something needed writing and could
// not be written. Called on a timer and at shutdown; a no-op when
// nothing changed.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_flush_settings(state: *mut WegletState) -> bool {
    // SAFETY: contract of as_mut.
    unsafe { as_mut(state) }.is_some_and(|state| state.flush_all())
}

// True when settings have changed and not yet been written, so the
// browser can skip a timer tick.
// How often the browser should call weglet_flush_settings and
// weglet_save_session, in seconds. From the profile, clamped on load.
#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_settings_flush_seconds(state: *const WegletState) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(5, |state| state.settings.settings_flush_seconds)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_session_save_seconds(state: *const WegletState) -> u64 {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.map_or(30, |state| state.settings.session_save_seconds)
}

#[doc = safety_doc!()]
#[no_mangle]
pub unsafe extern "C" fn weglet_settings_dirty(state: *const WegletState) -> bool {
    // SAFETY: contract of as_ref.
    unsafe { as_ref(state) }.is_some_and(|state| state.settings_dirty)
}
