// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// A profile's own blocklist.txt replaces the built-in list for that
// profile only -- see weglet-security/src/lists.rs.

mod common;
use common::{abi, c, guard};

// The blocklist.txt a profile may carry. Present means the user's own
// list replaces the built-in one, not adds to it.
#[test]
fn a_profile_can_supply_its_own_blocklist() {
    let f = guard("override");
    let profile = f.dir.join("Weglet").join("Default");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("blocklist.txt"),
        "# mine\nnot-doubleclick.example\n",
    )
    .unwrap();

    let state = abi::state_new();
    // Replaces rather than extends: the user removed the built-in entries
    // for a reason, and silently keeping entries they removed would
    // defeat it.
    assert!(unsafe {
        abi::is_url_blocked(
            state,
            c("https://not-doubleclick.example/").as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    });
    assert!(!unsafe { abi::is_host_blocked(state, c("stats.doubleclick.net").as_ptr()) });
    unsafe { abi::state_free(state) };
}

// One process runs several profiles: a list one of them supplies must not
// reach the others.
#[test]
fn another_profile_in_the_same_process_keeps_the_built_in_list() {
    let f = guard("override-isolated");
    let profile = f.dir.join("Weglet").join("Default");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("blocklist.txt"), "not-doubleclick.example\n").unwrap();

    let with_override = abi::state_new();
    let other = unsafe { abi::state_new_for_profile(c("Work").as_ptr()) };
    assert!(unsafe { abi::is_host_blocked(with_override, c("not-doubleclick.example").as_ptr()) });
    assert!(!unsafe { abi::is_host_blocked(other, c("not-doubleclick.example").as_ptr()) });
    assert!(unsafe { abi::is_host_blocked(other, c("stats.doubleclick.net").as_ptr()) });
    unsafe {
        abi::state_free(with_override);
        abi::state_free(other);
    }
}
