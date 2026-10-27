// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The boundary driven the way C++ drives it: create a state, call entry
// points in sequence, free it. A mistake here is undefined behaviour in
// the browser process rather than a failed assertion.
//
// Every test goes through `guard()`: the profile path comes from an
// environment variable, and cargo runs tests as threads in one process.
//
// A test that installs a profile blocklist override lives in its own
// file (blocklist_override.rs), not here -- see tests/common/mod.rs for
// why.

mod common;
use common::{abi, c, guard, take};

#[test]
fn a_fresh_state_has_one_blank_tab() {
    let _f = guard("fresh");
    let state = abi::state_new();
    let w = unsafe { abi::window_id_at(state, 0) };
    assert!(!state.is_null());
    assert_eq!(unsafe { abi::tab_count(state, w) }, 1);
    let id = unsafe { abi::active_tab_id(state, w) };
    assert_eq!(take(unsafe { abi::tab_url(state, id) }), "about:blank");
    assert_eq!(take(unsafe { abi::tab_label(state, id) }), "New Tab");
    unsafe { abi::state_free(state) };
}

#[test]
fn tabs_open_navigate_and_close() {
    let _f = guard("tabs");
    let state = abi::state_new();
    let w = unsafe { abi::window_id_at(state, 0) };

    let id = unsafe { abi::open_tab(state, w, c("https://a.example").as_ptr()) };
    assert_ne!(id, 0);
    assert_eq!(unsafe { abi::tab_count(state, w) }, 2);
    assert_eq!(unsafe { abi::active_tab_id(state, w) }, id);

    unsafe { abi::tab_navigated(state, id, c("https://b.example").as_ptr()) };
    assert_eq!(take(unsafe { abi::tab_url(state, id) }), "https://b.example");
    assert_eq!(
        take(unsafe { abi::tab_go_back(state, id) }),
        "https://a.example"
    );

    assert!(unsafe { abi::close_tab(state, id) });
    assert_eq!(unsafe { abi::tab_count(state, w) }, 1);
    unsafe { abi::state_free(state) };
}

// Every one of these would be a read past the end if the entry point
// trusted its argument.
#[test]
fn out_of_range_indices_are_answered_not_trusted() {
    let _f = guard("range");
    let state = abi::state_new();
    let w = unsafe { abi::window_id_at(state, 0) };
    assert_eq!(unsafe { abi::tab_id_at(state, w, 999) }, 0);
    assert_eq!(take(unsafe { abi::tab_url(state, 999) }), "");
    assert_eq!(take(unsafe { abi::tab_label(state, 999) }), "");
    assert_eq!(take(unsafe { abi::blocked_host_at(state, 999) }), "");
    assert!(!unsafe { abi::close_tab(state, 999) });
    assert!(!unsafe { abi::unblock_host(state, 999) });
    assert_eq!(take(unsafe { abi::engine_id_at(999) }), "");
    unsafe { abi::state_free(state) };
}

// A null handle is C++ having a bug, and must not be this side crashing.
#[test]
fn a_null_handle_is_a_defined_no_op() {
    let _f = guard("null");
    let null = std::ptr::null_mut();
    assert_eq!(unsafe { abi::tab_count(null, 0) }, 0);
    assert_eq!(unsafe { abi::active_tab_id(null, 0) }, 0);
    assert_eq!(take(unsafe { abi::tab_url(null, 0) }), "");
    assert_eq!(unsafe { abi::open_tab(null, 0, c("https://a.example").as_ptr()) }, 0);
    assert!(!unsafe { abi::is_url_blocked(null, c("https://a.example").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    assert_eq!(take(unsafe { abi::omnibox_resolve(null, c("x").as_ptr()) }), "");
    unsafe { abi::state_free(null) };
}

#[test]
fn the_omnibox_resolves_through_the_configured_engine() {
    let _f = guard("omnibox");
    let state = abi::state_new();
    assert_eq!(
        take(unsafe { abi::omnibox_resolve(state, c("example.com").as_ptr()) }),
        "https://example.com"
    );
    assert_eq!(
        take(unsafe { abi::omnibox_resolve(state, c("rust browser").as_ptr()) }),
        "https://duckduckgo.com/?q=rust%20browser"
    );

    // Every id the engine list offers has to be one set_search_engine
    // accepts.
    for index in 0..abi::engine_count() {
        let id = take(unsafe { abi::engine_id_at(index) });
        assert!(
            unsafe { abi::set_search_engine(state, c(&id).as_ptr()) },
            "engine {id} is offered but not accepted"
        );
    }
    assert!(!unsafe { abi::set_search_engine(state, c("nonesuch").as_ptr()) });
    unsafe { abi::state_free(state) };
}

#[test]
fn one_assessment_fills_every_out_param() {
    let _f = guard("risk");
    let (mut title, mut reason, mut host) =
        (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());

    let level = unsafe {
        abi::assess_navigation(
            c("https://gooogle.com/").as_ptr(),
            &mut title,
            &mut reason,
            &mut host,
        )
    };
    assert_eq!(level, 2);
    assert!(take(title).contains("Google"));
    assert!(!take(reason).is_empty());
    assert_eq!(take(host), "gooogle.com");
}

// The out-params are written even when there is no risk, so a caller
// cannot read whatever the pointer held.
#[test]
fn a_safe_url_still_writes_empty_strings() {
    let _f = guard("risk-none");
    let (mut title, mut reason, mut host) =
        (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
    let level = unsafe {
        abi::assess_navigation(
            c("https://example.com/").as_ptr(),
            &mut title,
            &mut reason,
            &mut host,
        )
    };
    assert_eq!(level, 0);
    assert_eq!(take(title), "");
    assert_eq!(take(reason), "");
    assert_eq!(take(host), "");
}

// A caller that wants only the level passes nothing to fill.
#[test]
fn null_out_params_are_allowed() {
    let _f = guard("risk-null");
    let level = unsafe {
        abi::assess_navigation(
            c("https://gooogle.com/").as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(level, 2);
}

#[test]
fn a_blocked_host_is_blocked_however_the_url_spells_it() {
    let _f = guard("block");
    let state = abi::state_new();

    assert!(unsafe { abi::block_host(state, c("EVIL.example.").as_ptr()) });
    assert_eq!(unsafe { abi::blocked_host_count(state) }, 1);
    // Stored canonicalised, so the settings page shows one spelling.
    assert_eq!(take(unsafe { abi::blocked_host_at(state, 0) }), "evil.example");
    // The same host twice is not two entries.
    assert!(!unsafe { abi::block_host(state, c("evil.example").as_ptr()) });

    for url in [
        "https://evil.example/",
        "https://sub.evil.example/path",
        "http://EVIL.EXAMPLE:8080/",
        // The host is what follows the last '@', so this is evil.example.
        "https://example.com@evil.example/",
    ] {
        assert!(
            unsafe { abi::is_url_blocked(state, c(url).as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) },
            "{url} should be blocked"
        );
    }
    assert!(!unsafe { abi::is_url_blocked(state, c("https://example.com/").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });

    // Nothing without a host can be blocked, or a page of ours could be.
    for url in ["about:blank", "chrome://weglet/settings.html", ""] {
        assert!(
            !unsafe { abi::is_url_blocked(state, c(url).as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) },
            "{url} has no host and must not match"
        );
    }

    // The wording comes back with the answer, so the notice page and the
    // heuristics are fed from one place.
    let (mut title, mut reason) = (std::ptr::null_mut(), std::ptr::null_mut());
    assert!(unsafe {
        abi::is_url_blocked(
            state,
            c("https://evil.example/").as_ptr(),
            &mut title,
            &mut reason,
        )
    });
    assert!(!take(title).is_empty());
    assert!(take(reason).contains("block list"));

    // And empty rather than untouched when nothing is blocked.
    let (mut title, mut reason) = (std::ptr::null_mut(), std::ptr::null_mut());
    assert!(!unsafe {
        abi::is_url_blocked(
            state,
            c("https://example.com/").as_ptr(),
            &mut title,
            &mut reason,
        )
    });
    assert_eq!(take(title), "");
    assert_eq!(take(reason), "");

    assert!(unsafe { abi::unblock_host(state, 0) });
    assert!(!unsafe { abi::is_url_blocked(state, c("https://evil.example/").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });

    // The built-in list is in force regardless of the user's.
    assert!(unsafe { abi::is_host_blocked(state, c("stats.doubleclick.net").as_ptr()) });
    unsafe { abi::state_free(state) };
}

#[test]
fn windows_hold_their_own_tabs_and_their_own_active_one() {
    let _f = guard("windows");
    let state = abi::state_new();
    let first = unsafe { abi::window_id_at(state, 0) };
    assert_eq!(unsafe { abi::window_count(state) }, 1);

    let second = unsafe { abi::open_window(state) };
    assert_ne!(second, first);
    assert_eq!(unsafe { abi::window_count(state) }, 2);

    let a = unsafe { abi::open_tab(state, first, c("https://a.example").as_ptr()) };
    let b = unsafe { abi::open_tab(state, second, c("https://b.example").as_ptr()) };

    assert_eq!(unsafe { abi::active_tab_id(state, first) }, a);
    assert_eq!(unsafe { abi::active_tab_id(state, second) }, b);
    assert_eq!(unsafe { abi::tab_count(state, first) }, 2);
    assert_eq!(unsafe { abi::tab_count(state, second) }, 2);
    assert_eq!(unsafe { abi::tab_window(state, a) }, first);
    assert_eq!(unsafe { abi::tab_window(state, b) }, second);

    // Closing one takes its tabs and leaves the other alone.
    assert!(unsafe { abi::close_window(state, second) });
    assert_eq!(unsafe { abi::window_count(state) }, 1);
    assert_eq!(unsafe { abi::tab_count(state, first) }, 2);
    assert_eq!(unsafe { abi::active_tab_id(state, first) }, a);

    // An unknown window answers rather than reaching past the end.
    assert_eq!(unsafe { abi::tab_count(state, 9999) }, 0);
    assert_eq!(unsafe { abi::active_tab_id(state, 9999) }, 0);
    assert_eq!(unsafe { abi::open_tab(state, 9999, c("https://x.example").as_ptr()) }, 0);
    assert!(!unsafe { abi::close_window(state, 9999) });

    unsafe { abi::state_free(state) };
}

// The whole point of deferring: changing a setting must not write.
#[test]
fn settings_are_marked_dirty_and_written_only_on_flush() {
    let f = guard("flush");
    let state = abi::state_new();
    let settings_file = f.dir.join("Weglet").join("Default").join("settings.toml");

    assert!(!unsafe { abi::settings_dirty(state) });
    assert!(unsafe { abi::set_accent_color(state, c("#3B82F6").as_ptr()) });
    assert_eq!(take(unsafe { abi::accent_color(state) }), "#3B82F6");

    assert!(unsafe { abi::settings_dirty(state) });
    assert!(!settings_file.exists(), "the change must not have touched the disk");

    assert!(unsafe { abi::flush_settings(state) });
    assert!(settings_file.exists());
    assert!(!unsafe { abi::settings_dirty(state) });

    // Nothing pending: a flush is a cheap no-op, so the browser can call
    // it on a timer without thinking about it.
    assert!(unsafe { abi::flush_settings(state) });

    unsafe { abi::state_free(state) };

    // And it really round-trips: a second state reads back the colour.
    let state = abi::state_new();
    assert_eq!(take(unsafe { abi::accent_color(state) }), "#3B82F6");
    unsafe { abi::state_free(state) };
}

// A rejected value must leave neither the setting nor the dirty flag
// changed, or the browser writes the file to save nothing.
#[test]
fn a_rejected_setting_does_not_mark_anything_dirty() {
    let _f = guard("reject");
    let state = abi::state_new();
    let before = take(unsafe { abi::accent_color(state) });
    assert!(!unsafe { abi::set_accent_color(state, c("not-a-colour").as_ptr()) });
    assert_eq!(take(unsafe { abi::accent_color(state) }), before);
    assert!(!unsafe { abi::settings_dirty(state) });
    unsafe { abi::state_free(state) };
}

#[test]
fn language_round_trips_through_flush() {
    let _f = guard("language");
    let state = abi::state_new();

    assert_eq!(take(unsafe { abi::language(state) }), "en");
    assert!(unsafe { abi::set_language(state, c("ru").as_ptr()) });
    assert_eq!(take(unsafe { abi::language(state) }), "ru");
    assert!(unsafe { abi::flush_settings(state) });
    unsafe { abi::state_free(state) };

    let state = abi::state_new();
    assert_eq!(take(unsafe { abi::language(state) }), "ru");
    unsafe { abi::state_free(state) };
}

#[test]
fn an_invalid_language_is_rejected() {
    let _f = guard("language-reject");
    let state = abi::state_new();
    let before = take(unsafe { abi::language(state) });
    assert!(!unsafe { abi::set_language(state, c("!!").as_ptr()) });
    assert_eq!(take(unsafe { abi::language(state) }), before);
    assert!(!unsafe { abi::settings_dirty(state) });
    unsafe { abi::state_free(state) };
}

#[test]
fn toggling_a_bookmark_adds_it_and_toggling_again_removes_it() {
    let _f = guard("bookmarks");
    let state = abi::state_new();

    assert!(!unsafe { abi::is_bookmarked(state, c("https://a.example").as_ptr()) });
    assert!(unsafe {
        abi::toggle_bookmark(state, c("A").as_ptr(), c("https://a.example").as_ptr())
    });
    assert_eq!(unsafe { abi::bookmark_count(state) }, 1);
    assert!(unsafe { abi::is_bookmarked(state, c("https://a.example").as_ptr()) });
    assert_eq!(take(unsafe { abi::bookmark_title_at(state, 0) }), "A");
    assert_eq!(
        take(unsafe { abi::bookmark_url_at(state, 0) }),
        "https://a.example"
    );

    assert!(!unsafe {
        abi::toggle_bookmark(state, c("A").as_ptr(), c("https://a.example").as_ptr())
    });
    assert_eq!(unsafe { abi::bookmark_count(state) }, 0);
    unsafe { abi::state_free(state) };
}

#[test]
fn removing_a_bookmark_by_index_drops_only_that_one() {
    let _f = guard("bookmarks-remove");
    let state = abi::state_new();
    unsafe {
        abi::toggle_bookmark(state, c("A").as_ptr(), c("https://a.example").as_ptr());
        abi::toggle_bookmark(state, c("B").as_ptr(), c("https://b.example").as_ptr());
    }
    assert!(unsafe { abi::remove_bookmark(state, 0) });
    assert_eq!(unsafe { abi::bookmark_count(state) }, 1);
    assert_eq!(
        take(unsafe { abi::bookmark_url_at(state, 0) }),
        "https://b.example"
    );
    assert!(!unsafe { abi::remove_bookmark(state, 99) });
    unsafe { abi::state_free(state) };
}

#[test]
fn bookmarks_round_trip_through_a_flush_and_a_fresh_state() {
    let f = guard("bookmarks-flush");
    let state = abi::state_new();
    unsafe {
        abi::toggle_bookmark(state, c("A").as_ptr(), c("https://a.example").as_ptr());
        abi::flush_settings(state);
        abi::state_free(state);
    }
    assert!(f
        .dir
        .join("Weglet")
        .join("Default")
        .join("bookmarks.toml")
        .exists());

    let state = abi::state_new();
    assert_eq!(unsafe { abi::bookmark_count(state) }, 1);
    unsafe { abi::state_free(state) };
}

#[test]
fn recorded_history_is_newest_first_and_clear_empties_it() {
    let _f = guard("history");
    let state = abi::state_new();
    unsafe {
        abi::record_history(state, c("cats").as_ptr(), c("https://ddg.example/?q=cats").as_ptr());
        abi::record_history(state, c("dogs").as_ptr(), c("https://ddg.example/?q=dogs").as_ptr());
    }
    assert_eq!(unsafe { abi::history_count(state) }, 2);
    assert_eq!(take(unsafe { abi::history_query_at(state, 0) }), "dogs");
    assert_eq!(take(unsafe { abi::history_query_at(state, 1) }), "cats");
    assert!(unsafe { abi::history_visited_at_at(state, 0) } > 0);

    unsafe { abi::clear_search_history(state) };
    assert_eq!(unsafe { abi::history_count(state) }, 0);
    unsafe { abi::state_free(state) };
}

#[test]
fn omnibox_suggestions_rank_a_bookmark_above_a_history_entry() {
    let _f = guard("omnibox-suggestions");
    let state = abi::state_new();
    unsafe {
        abi::toggle_bookmark(state, c("Rust Lang").as_ptr(), c("https://rust-lang.org").as_ptr());
        abi::record_history(state, c("rust book").as_ptr(), c("https://doc.rust-lang.org/book").as_ptr());
    }
    let count = unsafe { abi::query_omnibox_suggestions(state, c("rust").as_ptr()) };
    assert_eq!(count, 2);
    assert!(unsafe { abi::omnibox_suggestion_is_bookmark_at(state, 0) });
    assert!(!unsafe { abi::omnibox_suggestion_is_bookmark_at(state, 1) });
    assert_eq!(
        take(unsafe { abi::omnibox_suggestion_label_at(state, 0) }),
        "Rust Lang"
    );
    assert_eq!(
        take(unsafe { abi::omnibox_suggestion_url_at(state, 0) }),
        "https://rust-lang.org"
    );
    unsafe { abi::state_free(state) };
}

#[test]
fn omnibox_suggestions_are_empty_for_a_blank_query_and_out_of_range_reads_are_defined() {
    let _f = guard("omnibox-suggestions-empty");
    let state = abi::state_new();
    unsafe {
        abi::toggle_bookmark(state, c("Rust Lang").as_ptr(), c("https://rust-lang.org").as_ptr());
    }
    assert_eq!(unsafe { abi::query_omnibox_suggestions(state, c("").as_ptr()) }, 0);
    assert_eq!(take(unsafe { abi::omnibox_suggestion_label_at(state, 0) }), "");
    assert_eq!(take(unsafe { abi::omnibox_suggestion_url_at(state, 0) }), "");
    assert!(!unsafe { abi::omnibox_suggestion_is_bookmark_at(state, 0) });
    unsafe { abi::state_free(state) };
}

#[test]
fn a_download_moves_from_in_progress_to_completed() {
    let _f = guard("downloads");
    let state = abi::state_new();
    let url = c("https://example.com/a.zip");

    unsafe { abi::download_started(state, url.as_ptr(), c("/tmp/a.zip").as_ptr()) };
    assert_eq!(unsafe { abi::download_count(state) }, 1);
    assert_eq!(unsafe { abi::download_status_at(state, 0) }, 0);

    unsafe { abi::download_progress(state, url.as_ptr(), 1024, 4096) };
    assert_eq!(
        take(unsafe { abi::download_size_label_at(state, 0) }),
        "1.0 KB of 4.0 KB"
    );

    unsafe { abi::download_completed(state, url.as_ptr(), 4096) };
    assert_eq!(unsafe { abi::download_status_at(state, 0) }, 1);
    assert_eq!(take(unsafe { abi::download_size_label_at(state, 0) }), "4.0 KB");
    unsafe { abi::state_free(state) };
}

#[test]
fn a_failed_download_carries_the_real_error_message() {
    let _f = guard("downloads-failed");
    let state = abi::state_new();
    let url = c("https://example.com/b.zip");
    unsafe {
        abi::download_started(state, url.as_ptr(), c("/tmp/b.zip").as_ptr());
        abi::download_failed(state, url.as_ptr(), c("Server returned an error (HTTP 404)").as_ptr());
    }
    assert_eq!(unsafe { abi::download_status_at(state, 0) }, 2);
    assert_eq!(
        take(unsafe { abi::download_error_message_at(state, 0) }),
        "Server returned an error (HTTP 404)"
    );
    unsafe { abi::clear_download_history(state) };
    assert_eq!(unsafe { abi::download_count(state) }, 0);
    unsafe { abi::state_free(state) };
}

#[test]
fn a_download_still_in_progress_when_the_browser_closes_is_orphaned_on_restart() {
    let f = guard("downloads-orphan");
    let state = abi::state_new();
    unsafe {
        abi::download_started(state, c("https://example.com/c.zip").as_ptr(), c("/tmp/c.zip").as_ptr());
        abi::flush_settings(state);
        abi::state_free(state);
    }
    assert!(f
        .dir
        .join("Weglet")
        .join("Default")
        .join("downloads.toml")
        .exists());

    let state = abi::state_new();
    assert_eq!(unsafe { abi::download_status_at(state, 0) }, 2);
    assert_eq!(
        take(unsafe { abi::download_error_message_at(state, 0) }),
        "Interrupted"
    );
    unsafe { abi::state_free(state) };
}

#[test]
fn threat_feed_enabled_defaults_to_on_and_the_setting_persists() {
    let _f = guard("threat-feed-setting");
    let state = abi::state_new();
    assert!(unsafe { abi::threat_feed_enabled(state) });
    unsafe { abi::set_threat_feed_enabled(state, false) };
    assert!(!unsafe { abi::threat_feed_enabled(state) });
    assert!(unsafe { abi::settings_dirty(state) });
    unsafe { abi::state_free(state) };
}

#[test]
fn favicons_enabled_defaults_to_off_and_the_setting_persists() {
    let _f = guard("favicons-setting");
    let state = abi::state_new();
    assert!(!unsafe { abi::favicons_enabled(state) });
    unsafe { abi::set_favicons_enabled(state, true) };
    assert!(unsafe { abi::favicons_enabled(state) });
    assert!(unsafe { abi::settings_dirty(state) });
    unsafe { abi::state_free(state) };
}

#[test]
fn ask_where_to_save_downloads_defaults_to_off_and_the_setting_persists() {
    let _f = guard("ask-where-to-save-downloads-setting");
    let state = abi::state_new();
    assert!(!unsafe { abi::ask_where_to_save_downloads(state) });
    unsafe { abi::set_ask_where_to_save_downloads(state, true) };
    assert!(unsafe { abi::ask_where_to_save_downloads(state) });
    assert!(unsafe { abi::settings_dirty(state) });
    unsafe { abi::state_free(state) };
}

#[test]
fn a_named_profile_has_its_own_settings_separate_from_default() {
    let _f = guard("named-profile");
    let work = unsafe { abi::state_new_for_profile(c("Work").as_ptr()) };
    assert_eq!(take(unsafe { abi::current_profile(work) }), "Work");
    unsafe {
        abi::set_accent_color(work, c("#123456").as_ptr());
        abi::flush_settings(work);
        abi::state_free(work);
    }

    // Reopening "Work" sees what was saved.
    let work_again = unsafe { abi::state_new_for_profile(c("Work").as_ptr()) };
    assert_eq!(take(unsafe { abi::accent_color(work_again) }), "#123456");
    unsafe { abi::state_free(work_again) };

    // The default profile never saw it -- separate directories.
    let default = abi::state_new();
    assert_eq!(take(unsafe { abi::current_profile(default) }), "Default");
    assert_ne!(take(unsafe { abi::accent_color(default) }), "#123456");
    unsafe { abi::state_free(default) };
}

#[test]
fn an_invalid_or_missing_profile_name_falls_back_to_default() {
    let _f = guard("invalid-profile-name");
    for name in ["../escape", "", "a/b"] {
        let state = unsafe { abi::state_new_for_profile(c(name).as_ptr()) };
        assert_eq!(take(unsafe { abi::current_profile(state) }), "Default", "{name}");
        unsafe { abi::state_free(state) };
    }
    let state = unsafe { abi::state_new_for_profile(std::ptr::null()) };
    assert_eq!(take(unsafe { abi::current_profile(state) }), "Default");
    unsafe { abi::state_free(state) };
}

#[test]
fn is_valid_profile_name_matches_the_crate_rule() {
    assert!(unsafe { abi::is_valid_profile_name(c("Work").as_ptr()) });
    assert!(!unsafe { abi::is_valid_profile_name(c("").as_ptr()) });
    assert!(!unsafe { abi::is_valid_profile_name(c("a/b").as_ptr()) });
    assert!(!unsafe { abi::is_valid_profile_name(std::ptr::null()) });
}

#[test]
fn a_deleted_profile_no_longer_lists_and_deleting_a_missing_one_is_not_an_error() {
    let f = guard("delete-profile");
    std::fs::create_dir_all(f.dir.join("Weglet").join("Temp")).unwrap();
    assert_eq!(abi::profile_count(), 1);
    assert!(unsafe { abi::delete_profile(c("Temp").as_ptr()) });
    assert_eq!(abi::profile_count(), 1); // falls back to just Default
    assert_eq!(take(abi::profile_name_at(0)), "Default");
    // Already gone -- still reports success.
    assert!(unsafe { abi::delete_profile(c("Temp").as_ptr()) });
    // An invalid name is refused outright.
    assert!(!unsafe { abi::delete_profile(c("../escape").as_ptr()) });
}

#[test]
fn a_renamed_profile_keeps_its_data_under_the_new_name() {
    let f = guard("rename-profile");
    let old = unsafe { abi::state_new_for_profile(c("Old").as_ptr()) };
    unsafe {
        abi::set_accent_color(old, c("#ABCDEF").as_ptr());
        abi::flush_settings(old);
        abi::state_free(old);
    }
    assert!(unsafe { abi::rename_profile(c("Old").as_ptr(), c("New").as_ptr()) });
    assert!(!f.dir.join("Weglet").join("Old").exists());
    let new = unsafe { abi::state_new_for_profile(c("New").as_ptr()) };
    assert_eq!(take(unsafe { abi::accent_color(new) }), "#ABCDEF");
    unsafe { abi::state_free(new) };
}

#[test]
fn a_rename_never_overwrites_an_existing_profile() {
    let f = guard("rename-collision");
    std::fs::create_dir_all(f.dir.join("Weglet").join("A")).unwrap();
    std::fs::create_dir_all(f.dir.join("Weglet").join("B")).unwrap();
    assert!(!unsafe { abi::rename_profile(c("A").as_ptr(), c("B").as_ptr()) });
    assert!(f.dir.join("Weglet").join("A").exists());
}

#[test]
fn profile_accent_color_is_read_without_switching_to_it() {
    let _f = guard("peek-accent");
    let other = unsafe { abi::state_new_for_profile(c("Other").as_ptr()) };
    unsafe {
        abi::set_accent_color(other, c("#112233").as_ptr());
        abi::flush_settings(other);
        abi::state_free(other);
    }
    assert_eq!(
        take(unsafe { abi::profile_accent_color(c("Other").as_ptr()) }),
        "#112233"
    );
    // A profile that has never been opened still answers, with the default.
    assert!(!take(unsafe { abi::profile_accent_color(c("NeverOpened").as_ptr()) }).is_empty());
}

#[test]
fn last_profile_round_trips_and_defaults_to_empty() {
    let _f = guard("last-profile");
    assert_eq!(take(abi::last_profile()), "");
    unsafe { abi::set_last_profile(c("Work").as_ptr()) };
    assert_eq!(take(abi::last_profile()), "Work");
    // An invalid name does not overwrite what was already recorded.
    unsafe { abi::set_last_profile(c("../escape").as_ptr()) };
    assert_eq!(take(abi::last_profile()), "Work");
}

#[test]
fn listed_profiles_are_every_directory_actually_on_disk_sorted() {
    let f = guard("list-profiles");
    std::fs::create_dir_all(f.dir.join("Weglet").join("Work")).unwrap();
    std::fs::create_dir_all(f.dir.join("Weglet").join("Personal")).unwrap();

    let count = abi::profile_count();
    assert_eq!(count, 2);
    let names: Vec<String> = (0..count)
        .map(|i| take(abi::profile_name_at(i)))
        .collect();
    assert_eq!(names, vec!["Personal".to_string(), "Work".to_string()]);
}
