// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Whether an address is worth warning about. Advisory: the heuristics are
// wrong in both directions and nothing depends on them being right. Their
// measured rates live in tests/corpus.rs.

mod blocklist;
mod lists;
mod risk;
mod threat_feed;

pub use blocklist::{
    canonical_host, is_blocked_host, matches_user_blocklist, USER_BLOCK_REASON, USER_BLOCK_TITLE,
};
pub use lists::Lists;
pub use risk::{assess_navigation, NavigationRisk, RiskLevel};
pub use threat_feed::{
    is_known_phishing, is_known_phishing_now, parse_feed, set_live_hashes, ParsedFeed,
    KNOWN_PHISHING_REASON, KNOWN_PHISHING_TITLE,
};
