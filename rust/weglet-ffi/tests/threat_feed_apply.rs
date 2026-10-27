// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// weglet_apply_threat_feed installs the parsed indicators into the
// profile's own cache, and the profile matches against it.

mod common;
use common::{abi, c, guard};

const MIN_INDICATORS: usize = 50;

fn feed_body() -> String {
    let mut body = String::new();
    for i in 0..(MIN_INDICATORS + 5) {
        body.push_str(&format!("https://evil{i}.example/login\n"));
    }
    body
}

#[test]
fn a_real_looking_feed_is_applied_and_then_matches() {
    let f = guard("threat-feed-apply");
    let state = abi::state_new();
    let body = feed_body();

    assert!(!unsafe { abi::is_known_phishing(state, c("https://evil0.example/login").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    assert!(unsafe { abi::apply_threat_feed(state, c(&body).as_ptr()) });
    assert!(unsafe { abi::is_known_phishing(state, c("https://evil0.example/login").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    assert!(!unsafe { abi::is_known_phishing(state, c("https://safe.example/").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    assert!(unsafe { abi::threat_feed_updated_at(state) } > 0);
    assert!(!unsafe { abi::threat_feed_last_update_failed(state) });

    unsafe {
        abi::flush_settings(state);
        abi::state_free(state);
    }
    assert!(f
        .dir
        .join("Weglet")
        .join("Default")
        .join("threat-feed.json")
        .exists());
}

#[test]
fn a_broken_feed_is_rejected_and_marked_failed_without_erasing_a_good_cache() {
    let _f = guard("threat-feed-broken");
    let state = abi::state_new();

    assert!(unsafe { abi::apply_threat_feed(state, c(&feed_body()).as_ptr()) });
    let good_updated_at = unsafe { abi::threat_feed_updated_at(state) };

    // Too few indicators to be a real feed.
    assert!(!unsafe { abi::apply_threat_feed(state, c("https://one.example/login\n").as_ptr()) });
    assert!(unsafe { abi::threat_feed_last_update_failed(state) });
    // The good cache's timestamp -- and the indicators behind it -- stand.
    assert_eq!(unsafe { abi::threat_feed_updated_at(state) }, good_updated_at);
    assert!(unsafe { abi::is_known_phishing(state, c("https://evil0.example/login").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    unsafe { abi::state_free(state) };
}

#[test]
fn disabling_the_feed_stops_it_matching_without_clearing_it() {
    let _f = guard("threat-feed-disabled");
    let state = abi::state_new();
    unsafe {
        abi::apply_threat_feed(state, c(&feed_body()).as_ptr());
        abi::set_threat_feed_enabled(state, false);
    }
    assert!(!unsafe { abi::is_known_phishing(state, c("https://evil0.example/login").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    unsafe { abi::set_threat_feed_enabled(state, true) };
    assert!(unsafe { abi::is_known_phishing(state, c("https://evil0.example/login").as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) });
    unsafe { abi::state_free(state) };
}
