// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The C side of the boundary with rust/weglet-ffi.
//
// Hand-written: nothing generates this file, so a mismatch with
// rust/weglet-ffi/src/lib.rs is undefined behaviour rather than a compile
// error. rust/weglet-ffi/tests/boundary.rs binds every function by these
// signatures, which is what turns a drift into a build failure.
//
// No type crosses that is not a scalar or a C string. Do not call any of
// this directly: WegletBridge wraps it.

#ifndef WEGLET_RUST_WEGLET_FFI_H_
#define WEGLET_RUST_WEGLET_FFI_H_

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

extern "C" {

// Opaque. Created once per browser process, freed on shutdown.
typedef struct WegletState WegletState;

WegletState* weglet_state_new();
// As weglet_state_new, but loads the named profile's own data instead of
// "Default". An invalid or empty name falls back to "Default".
WegletState* weglet_state_new_for_profile(const char* profile);
void weglet_state_free(WegletState* state);

// Every char* returned below is owned by the caller and released with
// weglet_string_free -- not free(), not delete: it came from Rust's
// allocator, which on Windows is a different heap.
void weglet_string_free(char* value);

// Windows.
//
// Every tab question below takes the window it is about. Tab and window
// ids start at 1, so 0 always means "no such tab or window".
size_t weglet_window_count(const WegletState* state);
uint64_t weglet_window_id_at(const WegletState* state, size_t index);
// The new window's id, or 0 at the ceiling. Starts with one blank tab.
uint64_t weglet_open_window(WegletState* state);
// Its tabs go with it, except for the last window, which stays in the
// model with its tabs: the session restores it, and showing the profile
// again brings it back. After weglet_keep_windows_on_close, every window
// stays.
bool weglet_close_window(WegletState* state, uint64_t window);
// Every window is about to close because the browser is quitting: each one
// stays in the model, so the session keeps all of them. Undone by
// weglet_open_window.
void weglet_keep_windows_on_close(WegletState* state);
// Which window a tab is in, or 0 for no such tab.
uint64_t weglet_tab_window(const WegletState* state, uint64_t id);

// Tabs. Scoped to one window; `id` is global once you have it.
size_t weglet_tab_count(const WegletState* state, uint64_t window);
uint64_t weglet_tab_id_at(const WegletState* state, uint64_t window, size_t index);
uint64_t weglet_active_tab_id(const WegletState* state, uint64_t window);
char* weglet_tab_url(const WegletState* state, uint64_t id);
char* weglet_tab_label(const WegletState* state, uint64_t id);
bool weglet_tab_can_go_back(const WegletState* state, uint64_t id);
bool weglet_tab_can_go_forward(const WegletState* state, uint64_t id);
// Whether the tab is loading.
bool weglet_tab_loading(const WegletState* state, uint64_t id);

// Returns the new id, or 0 when the tab ceiling is reached.
uint64_t weglet_open_tab(WegletState* state, uint64_t window, const char* url);
bool weglet_close_tab(WegletState* state, uint64_t id);
bool weglet_activate_tab(WegletState* state, uint64_t id);
void weglet_cycle_tab(WegletState* state, uint64_t window, bool forward);
// One-based, as on the keyboard. 9 means the last tab.
bool weglet_activate_tab_at(WegletState* state, uint64_t window, size_t position);
bool weglet_reorder_tab(WegletState* state, uint64_t id, size_t target);

// Navigation. The engine owns each tab's back/forward list and does the
// navigating; the model keeps a copy for the tab strip and the session.
// `entries` is every entry, oldest first, separated by '\n' -- a URL the
// engine reports never contains a raw newline -- and `cursor` is which one
// is current. `new_document` is false for a pushState or a fragment
// change, which keeps the title.
void weglet_tab_history_changed(WegletState* state,
                                uint64_t id,
                                const char* entries,
                                size_t cursor,
                                bool new_document);
// The model's copy, for handing a restored tab's entries to the engine:
// how many, each one oldest first (empty when out of range), and which is
// current. 0 and empty for no such tab.
size_t weglet_tab_history_count(const WegletState* state, uint64_t id);
char* weglet_tab_history_entry_at(const WegletState* state,
                                  uint64_t id,
                                  size_t index);
size_t weglet_tab_history_cursor(const WegletState* state, uint64_t id);
void weglet_tab_title_changed(WegletState* state, uint64_t id, const char* title);
void weglet_tab_loading_changed(WegletState* state, uint64_t id, bool loading);

// Omnibox: what the user typed, resolved to a URL to load.
char* weglet_omnibox_resolve(const WegletState* state, const char* input);

// The address bar's dropdown: bookmarks and history matching `input`,
// ranked. weglet_query_omnibox_suggestions recomputes the list and
// returns its length; the three accessors below read what it stored.
// Call the query function first on every keystroke -- there is no other
// invalidation.
size_t weglet_query_omnibox_suggestions(WegletState* state, const char* input);
char* weglet_omnibox_suggestion_label_at(const WegletState* state, size_t index);
char* weglet_omnibox_suggestion_url_at(const WegletState* state, size_t index);
bool weglet_omnibox_suggestion_is_bookmark_at(const WegletState* state, size_t index);

// Security. 0 = nothing to say, 1 = warn, 2 = block.
//
// One call, not four. `title`, `reason` and `host` may each be null; a
// non-null one is always written, including on a 0 answer, where all
// three come back as empty strings. Every string written is owned by the
// caller.
//
// `host` is the host the risk is about, empty when there is none to show.
// The target URL is not returned -- the caller already has it.
//
// Judged against this profile's lists. Whether the user blocked the site
// is weglet_is_url_blocked.
uint32_t weglet_assess_navigation(const WegletState* state,
                                  const char* url,
                                  char** title,
                                  char** reason,
                                  char** host);

bool weglet_is_host_blocked(const WegletState* state, const char* host);
// The same question about a whole URL, with the wording for the notice.
//
// The host is pulled out on the Rust side: a second parser here would
// have to know that a backslash ends the authority and that the host
// follows the last '@'. False for a URL with no host.
//
// `title` and `reason` may each be null; a non-null one is always
// written, empty when the answer is false.
bool weglet_is_url_blocked(const WegletState* state,
                           const char* url,
                           char** title,
                           char** reason);

// Shortcuts: the pinned sites on the new tab page. Addressed by position.
size_t weglet_shortcut_count(const WegletState* state);
char* weglet_shortcut_title(const WegletState* state, size_t index);
char* weglet_shortcut_url(const WegletState* state, size_t index);
// False when the dock is full.
bool weglet_add_shortcut(WegletState* state, const char* title, const char* url);
bool weglet_edit_shortcut(
    WegletState* state, size_t index, const char* title, const char* url);
bool weglet_remove_shortcut(WegletState* state, size_t index);

// The line shown under the new tab page's search field.
char* weglet_new_tab_hint(const WegletState* state);

// Bookmarks. Addressed by position, like shortcuts.
size_t weglet_bookmark_count(const WegletState* state);
char* weglet_bookmark_title_at(const WegletState* state, size_t index);
char* weglet_bookmark_url_at(const WegletState* state, size_t index);
bool weglet_is_bookmarked(const WegletState* state, const char* url);
// Adds `url` if not already saved, removes it if it is. Returns whether
// the page is bookmarked after the call.
bool weglet_toggle_bookmark(WegletState* state, const char* title, const char* url);
bool weglet_remove_bookmark(WegletState* state, size_t index);

// Browsing history: address-bar submissions, newest first.
size_t weglet_history_count(const WegletState* state);
char* weglet_history_query_at(const WegletState* state, size_t index);
char* weglet_history_url_at(const WegletState* state, size_t index);
uint64_t weglet_history_visited_at_at(const WegletState* state, size_t index);
// `query` is what the user typed; `url` is where it resolved to.
void weglet_record_history(WegletState* state, const char* query, const char* url);
void weglet_clear_search_history(WegletState* state);

// Downloads, newest first. The four calls below mirror
// content::DownloadItem's own lifecycle and are made by the browser
// process, which owns the real DownloadManager.
size_t weglet_download_count(const WegletState* state);
char* weglet_download_filename_at(const WegletState* state, size_t index);
// 0 = in progress, 1 = completed, 2 = failed.
uint32_t weglet_download_status_at(const WegletState* state, size_t index);
// "1.2 MB of 4.0 MB" in progress, the final size once resolved.
char* weglet_download_size_label_at(const WegletState* state, size_t index);
// Empty unless the status is failed.
char* weglet_download_error_message_at(const WegletState* state, size_t index);
uint64_t weglet_download_started_at_at(const WegletState* state, size_t index);
// The file's own path on disk, for "reveal in folder" and "open".
char* weglet_download_path_at(const WegletState* state, size_t index);
// A download is named by its DownloadItem GUID: two downloads of one URL
// are two records. `path` is the target path, empty until the download
// manager has decided it.
void weglet_download_started(WegletState* state,
                             const char* guid,
                             const char* url,
                             const char* path);
// `total_bytes` is -1 when the server sent no Content-Length.
void weglet_download_progress(WegletState* state,
                              const char* guid,
                              const char* path,
                              uint64_t bytes_downloaded,
                              int64_t total_bytes);
void weglet_download_completed(WegletState* state,
                               const char* guid,
                               const char* path,
                               uint64_t size_bytes);
void weglet_download_failed(WegletState* state, const char* guid, const char* message);
void weglet_clear_download_history(WegletState* state);

// Threat feed: OpenPhish's public feed of known-phishing URLs, kept per
// profile as URL indicators -- see weglet-security/src/threat_feed.rs. The
// fetch itself is the browser process's job; this only parses and matches.
bool weglet_threat_feed_enabled(const WegletState* state);
void weglet_set_threat_feed_enabled(WegletState* state, bool on);
// Off by default: fetching a site's icon is itself a request to that
// site. See docs/security.md.
bool weglet_favicons_enabled(const WegletState* state);
void weglet_set_favicons_enabled(WegletState* state, bool on);
// Off by default: most downloads land in Downloads with no prompt. On,
// the browser shows a Save As dialog for every download.
bool weglet_ask_where_to_save_downloads(const WegletState* state);
void weglet_set_ask_where_to_save_downloads(WegletState* state, bool on);
// False outright when the setting is off. `title` and `reason` may each
// be null; a non-null one is always written, empty when the answer is
// false.
bool weglet_is_known_phishing(const WegletState* state,
                              const char* url,
                              char** title,
                              char** reason);
// `body` is the feed's raw text, already downloaded. False -- and the
// cache's own failure flag set -- when it does not look like a real
// feed; the previous cache is left standing either way.
bool weglet_apply_threat_feed(WegletState* state, const char* body);
uint64_t weglet_threat_feed_updated_at(const WegletState* state);
bool weglet_threat_feed_last_update_failed(const WegletState* state);

// Profiles. weglet_current_profile is this instance's own name;
// weglet_profile_count/_name_at list every profile this machine has ever
// used, alphabetical -- re-read from disk on every call, only asked for
// while the Settings page's Profiles tab is open.
char* weglet_current_profile(const WegletState* state);
size_t weglet_profile_count();
char* weglet_profile_name_at(size_t index);
// Whether `name` could become a profile directory -- checked before
// offering to create or switch to it.
bool weglet_is_valid_profile_name(const char* name);
// The existing profile `name` refers to, in its own spelling: Windows
// compares names without regard to case, so "work" is the profile "Work".
// `name` itself when no profile has it yet. Owned by the caller.
char* weglet_canonical_profile_name(const char* name);
// Deletes a profile's whole directory. Never call this with the name of
// the profile the calling process is itself running as.
bool weglet_delete_profile(const char* name);
// False and nothing touched if `old` doesn't exist, `new` already does,
// or either name is invalid.
bool weglet_rename_profile(const char* old_name, const char* new_name);
// A profile's accent colour without loading the rest of its state.
char* weglet_profile_accent_color(const char* name);
// The profile a plain launch with no --profile switch should open.
// Empty string when there is none yet.
char* weglet_last_profile();
// Records `name` as weglet_last_profile's answer from now on.
void weglet_set_last_profile(const char* name);

// Settings: search. An id the browser does not recognise leaves the
// setting unchanged rather than resetting it.
char* weglet_search_engine(const WegletState* state);
bool weglet_set_search_engine(WegletState* state, const char* id);
// The choices to list, excluding "custom" -- that is what the id field and
// the template box together mean. Read weglet_engine_count, then _id_at /
// _label_at for 0..count.
size_t weglet_engine_count(const WegletState* state);
char* weglet_engine_id_at(const WegletState* state, size_t index);
char* weglet_engine_label_at(const WegletState* state, size_t index);
char* weglet_custom_search_url(const WegletState* state);
// Accepted mid-edit; validated when it is used, in weglet_omnibox_resolve.
void weglet_set_custom_search_url(WegletState* state, const char* url);

// Settings: general.
bool weglet_restore_session(const WegletState* state);
void weglet_set_restore_session(WegletState* state, bool on);

// Settings: appearance. Shape is "pill" | "rounded" | "square".
char* weglet_accent_color(const WegletState* state);
// False for anything that is not #RRGGBB; the colour is left unchanged.
bool weglet_set_accent_color(WegletState* state, const char* color);
char* weglet_address_bar_shape(const WegletState* state);
bool weglet_set_address_bar_shape(WegletState* state, const char* shape);
// A BCP-47-ish code such as "en" or "ru"; the known set lives in
// weglet/ui/i18n/*.txt, not here. False if malformed.
char* weglet_language(const WegletState* state);
bool weglet_set_language(WegletState* state, const char* language);

// Settings: blocked hosts. Ordered; addressed by position.
size_t weglet_blocked_host_count(const WegletState* state);
char* weglet_blocked_host_at(const WegletState* state, size_t index);
// Canonicalised before storing, so two spellings of one host cannot
// disagree about whether it is blocked. False if already present.
bool weglet_block_host(WegletState* state, const char* host);
bool weglet_unblock_host(WegletState* state, size_t index);

// Settings and session.
bool weglet_save_session(const WegletState* state);

// Changing anything above marks it, it does not write it: an atomic
// write ends in fsync, and the UI thread is not the place for it. The
// browser flushes on a timer and at shutdown. weglet_flush_settings
// writes every dirty store -- settings, bookmarks, history, downloads,
// the threat-feed cache -- and returns false when something needed
// writing and could not be; whatever failed stays pending for the next
// call. With nothing pending it is a no-op returning true.
// How often to call the two above, in seconds. From the profile, clamped
// there.
uint64_t weglet_settings_flush_seconds(const WegletState* state);
uint64_t weglet_session_save_seconds(const WegletState* state);

bool weglet_settings_dirty(const WegletState* state);
bool weglet_flush_settings(WegletState* state);

}  // extern "C"

#endif  // WEGLET_RUST_WEGLET_FFI_H_
