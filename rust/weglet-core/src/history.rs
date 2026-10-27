// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Back/forward history for one tab: a copy of the engine's own list, kept
// for the tab strip and the session. The engine owns the list and does the
// navigating; this never decides where Back goes. The cursor is the only
// record of where the tab is; the tab keeps no second copy of the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    entries: Vec<String>,
    cursor: usize,
}

// Bounded: every entry is a URL held for the life of the tab, and the list
// comes in from the engine and from a session file on disk.
const MAX_ENTRIES: usize = 256;

impl History {
    pub fn new(url: String) -> Self {
        Self {
            entries: vec![url],
            cursor: 0,
        }
    }

    // From the engine's list or a restored session. The cursor is clamped,
    // not trusted: it comes off disk, and out of range would panic.
    pub fn from_entries(mut entries: Vec<String>, cursor: usize) -> Self {
        if entries.is_empty() {
            entries.push(crate::BLANK_TAB.to_string());
        }
        let cursor = cursor.min(entries.len() - 1);
        // Over the cap, the entries kept are a window that still holds the
        // current one, oldest dropped first.
        let excess = entries.len().saturating_sub(MAX_ENTRIES);
        let start = excess.min(cursor);
        entries.truncate(start + MAX_ENTRIES);
        entries.drain(..start);
        Self {
            entries,
            cursor: cursor - start,
        }
    }

    pub fn current(&self) -> &str {
        // Every path that touches entries keeps at least one.
        &self.entries[self.cursor]
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn can_go_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(urls: &[&str], cursor: usize) -> History {
        History::from_entries(urls.iter().map(|s| s.to_string()).collect(), cursor)
    }

    fn numbered(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("https://{i}.example")).collect()
    }

    #[test]
    fn a_new_history_sits_on_its_only_entry() {
        let history = History::new("https://example.com".into());
        assert_eq!(history.current(), "https://example.com");
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn back_and_forward_follow_the_cursor() {
        let history = at(&["https://a.example", "https://b.example", "https://c.example"], 1);
        assert_eq!(history.current(), "https://b.example");
        assert!(history.can_go_back());
        assert!(history.can_go_forward());
    }

    #[test]
    fn an_out_of_range_cursor_is_clamped_rather_than_trusted() {
        let history = at(&["https://a.example", "https://b.example"], 99);
        assert_eq!(history.current(), "https://b.example");
    }

    #[test]
    fn an_empty_list_becomes_a_blank_tab() {
        let history = History::from_entries(Vec::new(), 0);
        assert_eq!(history.current(), crate::BLANK_TAB);
    }

    #[test]
    fn a_list_longer_than_the_cap_keeps_the_newest_entries() {
        let history = History::from_entries(numbered(MAX_ENTRIES + 10), MAX_ENTRIES + 9);
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.entries()[0], "https://10.example");
        assert_eq!(history.current(), format!("https://{}.example", MAX_ENTRIES + 9));
    }

    #[test]
    fn a_list_longer_than_the_cap_keeps_an_early_current_entry() {
        let history = History::from_entries(numbered(MAX_ENTRIES + 10), 3);
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.current(), "https://3.example");
    }
}
