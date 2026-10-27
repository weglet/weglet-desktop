// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Download history, persisted like bookmarks and browsing history. The
// browser (content::DownloadManager) owns an in-progress download's live
// bytes; this holds the record once it's told us started/progressed/ended.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DownloadRecord {
    // content::DownloadItem's GUID: what tells two downloads of the same
    // URL apart. Empty in a record written before it was kept, which no
    // update ever matches -- such a record is finished anyway.
    pub guid: String,
    pub url: String,
    pub filename: String,
    pub path: String,
    pub status: DownloadStatus,
    // Unix seconds -- plain enough to sort by, no timezone handling
    // needed for a "started 3 minutes ago" style display.
    pub started_at: u64,
    // Live while InProgress, final once Completed. Whatever was written
    // before a failure isn't meaningful, so Failed doesn't update it.
    pub bytes_downloaded: u64,
    // None when the server sent no Content-Length (chunked transfer, or
    // just omitted).
    pub total_bytes: Option<u64>,
    // Set only when status is Failed -- content::DownloadItem's own
    // interrupt reason, not a guess.
    pub error_message: Option<String>,
}

impl Default for DownloadRecord {
    fn default() -> Self {
        Self {
            guid: String::new(),
            url: String::new(),
            filename: String::new(),
            path: String::new(),
            status: DownloadStatus::InProgress,
            started_at: 0,
            bytes_downloaded: 0,
            total_bytes: None,
            error_message: None,
        }
    }
}

impl DownloadRecord {
    pub fn started(guid: &str, url: &str, path: &Path) -> Self {
        let mut record = Self {
            guid: guid.to_string(),
            url: url.to_string(),
            filename: String::new(),
            path: String::new(),
            status: DownloadStatus::InProgress,
            started_at: unix_now(),
            bytes_downloaded: 0,
            total_bytes: None,
            error_message: None,
        };
        record.set_path(path);
        record
    }

    // The target path, once the download manager has decided it: often
    // still empty when the download starts. The filename falls back to the
    // URL until then.
    fn set_path(&mut self, path: &Path) {
        if path.as_os_str().is_empty() {
            if self.filename.is_empty() {
                self.filename = self.url.clone();
            }
            return;
        }
        self.path = path.to_string_lossy().into_owned();
        if let Some(name) = path.file_name() {
            self.filename = name.to_string_lossy().into_owned();
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Downloads {
    pub records: Vec<DownloadRecord>,
}

// Older entries beyond this are dropped on save -- a personal browser's
// download history doesn't need to grow forever, and capping here keeps
// the file small enough to always rewrite wholesale.
const MAX_RECORDS: usize = 200;

impl Downloads {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn save(&mut self, path: &Path) -> Result<(), Error> {
        if self.records.len() > MAX_RECORDS {
            // Newest-first isn't guaranteed by the caller, so sort before
            // truncating rather than assuming which end is oldest.
            self.records.sort_by_key(|record| std::cmp::Reverse(record.started_at));
            self.records.truncate(MAX_RECORDS);
        }
        let text = toml::to_string_pretty(self).map_err(|source| Error::Serialise {
            path: path.to_path_buf(),
            source,
        })?;
        crate::atomic::write(path, &text)
    }

    pub fn started(&mut self, guid: &str, url: &str, path: &Path) {
        self.records.insert(0, DownloadRecord::started(guid, url, path));
    }

    // The still-in-progress record for `guid`. None for an unknown GUID or
    // a record already resolved -- a stray late callback, say.
    fn in_progress(&mut self, guid: &str) -> Option<&mut DownloadRecord> {
        if guid.is_empty() {
            return None;
        }
        self.records
            .iter_mut()
            .find(|record| record.guid == guid && record.status == DownloadStatus::InProgress)
    }

    // A new byte count and, once it is known, the target path, from
    // DownloadItem's own progress callback. False if no in-progress record
    // has this GUID.
    pub fn update_progress(
        &mut self,
        guid: &str,
        path: &Path,
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
    ) -> bool {
        let Some(record) = self.in_progress(guid) else {
            return false;
        };
        record.set_path(path);
        record.bytes_downloaded = bytes_downloaded;
        record.total_bytes = total_bytes;
        true
    }

    pub fn mark_completed(&mut self, guid: &str, path: &Path, size_bytes: u64) -> bool {
        let Some(record) = self.in_progress(guid) else {
            return false;
        };
        record.set_path(path);
        record.status = DownloadStatus::Completed;
        record.bytes_downloaded = size_bytes;
        true
    }

    pub fn mark_failed(&mut self, guid: &str, message: String) -> bool {
        let Some(record) = self.in_progress(guid) else {
            return false;
        };
        record.status = DownloadStatus::Failed;
        record.error_message = Some(message);
        true
    }

    // Any record still InProgress at load time is orphaned -- the
    // browser closed mid-download -- and would otherwise show
    // "Downloading..." forever. Called once, right after load.
    pub fn fail_orphaned_in_progress(&mut self) -> usize {
        let mut count = 0;
        for record in &mut self.records {
            if record.status == DownloadStatus::InProgress {
                record.status = DownloadStatus::Failed;
                record.error_message = Some("Interrupted".to_string());
                count += 1;
            }
        }
        count
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

// "2.3 MB", "180.0 KB", "512 B" -- one decimal place above the smallest
// unit, matching how file managers and browsers typically show this.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }
    format!("{value:.1} {}", UNITS[unit_index])
}

// What a download row's size line reads while still in flight: "1.2 MB
// of 4.0 MB", or just the running total when the server sent no
// Content-Length.
pub fn progress_label(bytes_downloaded: u64, total_bytes: Option<u64>) -> String {
    match total_bytes {
        Some(total) => format!("{} of {}", format_bytes(bytes_downloaded), format_bytes(total)),
        None => format_bytes(bytes_downloaded),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

// The path a "reveal in folder" action should open -- the download's own
// directory, falling back to its own path if it somehow has no parent
// (e.g. already just a bare filename).
pub fn folder_to_reveal(record: &DownloadRecord) -> PathBuf {
    parent_dir(Path::new(&record.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-downloads-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("downloads.toml")
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        assert_eq!(Downloads::load(&file("missing")).unwrap(), Downloads::default());
    }

    #[test]
    fn a_record_round_trips_through_disk() {
        let path = file("roundtrip");
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        downloads.save(&path).unwrap();
        assert_eq!(Downloads::load(&path).unwrap(), downloads);
    }

    #[test]
    fn started_derives_the_filename_from_the_path() {
        let record = DownloadRecord::started("g-1", "https://example.com/x", Path::new("/dl/report.pdf"));
        assert_eq!(record.filename, "report.pdf");
        assert_eq!(record.status, DownloadStatus::InProgress);
    }

    #[test]
    fn update_progress_sets_the_live_byte_counts() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        assert!(downloads.update_progress("g-1", Path::new("/tmp/a.zip"), 1024, Some(4096)));
        assert_eq!(downloads.records[0].bytes_downloaded, 1024);
        assert_eq!(downloads.records[0].total_bytes, Some(4096));
        assert_eq!(downloads.records[0].status, DownloadStatus::InProgress);
    }

    #[test]
    fn mark_completed_updates_the_matching_in_progress_record() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        assert!(downloads.mark_completed("g-1", Path::new("/tmp/a.zip"), 2048));
        assert_eq!(downloads.records[0].status, DownloadStatus::Completed);
        assert_eq!(downloads.records[0].bytes_downloaded, 2048);
    }

    #[test]
    fn mark_failed_sets_status_and_the_real_error_message() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        assert!(downloads.mark_failed(
            "g-1",
            "Server returned an error (HTTP 404)".to_string()
        ));
        assert_eq!(downloads.records[0].status, DownloadStatus::Failed);
        assert_eq!(
            downloads.records[0].error_message.as_deref(),
            Some("Server returned an error (HTTP 404)")
        );
    }

    #[test]
    fn fail_orphaned_in_progress_marks_only_in_progress_records() {
        let mut downloads = Downloads {
            records: vec![
                DownloadRecord::started("g-1", "https://example.com/a", Path::new("/tmp/a")),
                DownloadRecord {
                    status: DownloadStatus::Completed,
                    ..DownloadRecord::started("g-2", "https://example.com/b", Path::new("/tmp/b"))
                },
            ],
        };
        let count = downloads.fail_orphaned_in_progress();
        assert_eq!(count, 1);
        assert_eq!(downloads.records[0].status, DownloadStatus::Failed);
        assert_eq!(downloads.records[0].error_message.as_deref(), Some("Interrupted"));
        assert_eq!(downloads.records[1].status, DownloadStatus::Completed);
    }

    // The same file downloaded twice at once: each callback reaches its
    // own record, not whichever of the two shares the URL.
    #[test]
    fn two_downloads_of_one_url_keep_their_own_progress() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        downloads.started("g-2", "https://example.com/a.zip", Path::new("/tmp/a (1).zip"));
        assert!(downloads.update_progress("g-1", Path::new("/tmp/a.zip"), 100, Some(1000)));
        assert!(downloads.update_progress("g-2", Path::new("/tmp/a (1).zip"), 700, Some(1000)));
        assert!(downloads.mark_completed("g-1", Path::new("/tmp/a.zip"), 1000));
        // Newest first: index 0 is g-2.
        assert_eq!(downloads.records[0].status, DownloadStatus::InProgress);
        assert_eq!(downloads.records[0].bytes_downloaded, 700);
        assert_eq!(downloads.records[1].status, DownloadStatus::Completed);
        assert_eq!(downloads.records[1].bytes_downloaded, 1000);
    }

    // The target path is often still empty when a download starts.
    #[test]
    fn the_path_arrives_with_a_later_update() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new(""));
        assert_eq!(downloads.records[0].filename, "https://example.com/a.zip");
        assert!(downloads.update_progress("g-1", Path::new("/tmp/a.zip"), 1, None));
        assert_eq!(downloads.records[0].path, "/tmp/a.zip");
        assert_eq!(downloads.records[0].filename, "a.zip");
    }

    #[test]
    fn mark_completed_on_no_match_reports_false() {
        let mut downloads = Downloads::default();
        assert!(!downloads.mark_completed("g-unknown", Path::new(""), 0));
    }

    // A record from before GUIDs were kept has an empty one; no update
    // with an empty GUID may land on it.
    #[test]
    fn an_empty_guid_matches_no_record() {
        let mut downloads = Downloads {
            records: vec![DownloadRecord::started("", "https://example.com/a", Path::new("/tmp/a"))],
        };
        assert!(!downloads.mark_completed("", Path::new("/tmp/a"), 1));
    }

    #[test]
    fn format_bytes_picks_a_sensible_unit() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2_355_098), "2.2 MB");
        assert_eq!(format_bytes(184_320), "180.0 KB");
    }

    #[test]
    fn progress_label_shows_the_total_when_known() {
        assert_eq!(progress_label(1024, Some(4096)), "1.0 KB of 4.0 KB");
        assert_eq!(progress_label(1024, None), "1.0 KB");
    }

    #[test]
    fn saving_more_than_the_cap_keeps_only_the_newest() {
        let path = file("capped");
        let mut downloads = Downloads {
            records: (0..(MAX_RECORDS + 10) as u64)
                .map(|i| DownloadRecord {
                    guid: format!("g-{i}"),
                    url: format!("https://example.com/{i}"),
                    filename: format!("{i}.bin"),
                    path: format!("/tmp/{i}.bin"),
                    status: DownloadStatus::Completed,
                    started_at: i,
                    bytes_downloaded: 0,
                    total_bytes: None,
                    error_message: None,
                })
                .collect(),
        };
        downloads.save(&path).unwrap();
        let loaded = Downloads::load(&path).unwrap();
        assert_eq!(loaded.records.len(), MAX_RECORDS);
        assert!(loaded.records.iter().all(|r| r.started_at >= 10));
    }

    #[test]
    fn clear_empties_the_list() {
        let mut downloads = Downloads::default();
        downloads.started("g-1", "https://example.com/a.zip", Path::new("/tmp/a.zip"));
        downloads.clear();
        assert!(downloads.records.is_empty());
    }

    #[test]
    fn folder_to_reveal_is_the_downloads_parent_directory() {
        let record = DownloadRecord {
            path: "/home/user/Downloads/report.pdf".to_string(),
            ..Default::default()
        };
        assert_eq!(folder_to_reveal(&record), Path::new("/home/user/Downloads"));
    }
}
