// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Everything Weglet keeps on disk. Every write goes through
// atomic::write; every read treats the file as untrusted input.

mod atomic;
mod bookmarks;
mod browsing_history;
mod downloads;
mod error;
mod paths;
mod session;
mod settings;
mod threat_feed_cache;

pub use atomic::write as write_atomic;
pub use bookmarks::{Bookmark, Bookmarks};
pub use browsing_history::{BrowsingHistory, HistoryEntry};
pub use downloads::{
    folder_to_reveal, format_bytes, progress_label, DownloadRecord, DownloadStatus, Downloads,
};
pub use error::Error;
pub use paths::{is_valid_profile_name, Paths, DEFAULT_PROFILE};
pub use session::{Session, SessionTab, SessionWindow};
pub use settings::{
    default_engine_id, engine_label, engines, is_known_engine, query_url,
    set_engines_override, AddressBarShape, Engine, Settings, Shortcut,
    CUSTOM_ENGINE_ID, DEFAULT_ACCENT, MAX_SHORTCUTS,
};
pub use threat_feed_cache::ThreatFeedCache;
