// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// A tab: its window, its history, and what the tab strip shows for it.

use crate::History;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(u64);

impl TabId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

// Which window a tab is in. An id rather than an index, because a window
// in the middle can close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    // The window it belongs to. Never two, never none.
    pub window: WindowId,
    pub history: History,
    // What the page called itself. Empty until the engine reports one, so
    // `label` falls back to the host.
    pub title: String,
    pub loading: bool,
}

impl Tab {
    pub fn new(id: TabId, window: WindowId, url: String) -> Self {
        Self {
            id,
            window,
            history: History::new(url),
            title: String::new(),
            loading: false,
        }
    }

    // Reads through to the history cursor: no second copy of the current
    // URL, so the address bar and the loaded page cannot disagree.
    pub fn url(&self) -> &str {
        self.history.current()
    }

    // Takes the engine's back/forward list after a navigation. A new
    // document drops the old title, which belongs to the old page; a
    // pushState or a fragment change keeps it.
    pub fn sync_history(&mut self, entries: Vec<String>, cursor: usize, new_document: bool) {
        self.history = History::from_entries(entries, cursor);
        if new_document {
            self.title.clear();
        }
    }

    // What to show in the tab strip. Our own pages get a name, everything
    // else its title, and its host until the title arrives.
    pub fn label(&self) -> &str {
        match self.url() {
            crate::BLANK_TAB => "New Tab",
            crate::SETTINGS_ADDRESS => "Settings",
            crate::HISTORY_ADDRESS => "History",
            crate::BOOKMARKS_ADDRESS => "Bookmarks",
            url => {
                if self.title.is_empty() {
                    weglet_url::display_host(url)
                } else {
                    &self.title
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str) -> Tab {
        Tab::new(TabId::new(1), WindowId::new(0), url.to_string())
    }

    fn entries(urls: &[&str]) -> Vec<String> {
        urls.iter().map(|url| url.to_string()).collect()
    }

    #[test]
    fn a_tabs_url_comes_from_its_history() {
        let mut tab = tab("https://a.example");
        assert_eq!(tab.url(), "https://a.example");
        tab.sync_history(entries(&["https://a.example", "https://b.example"]), 1, true);
        assert_eq!(tab.url(), "https://b.example");
        assert_eq!(tab.history.current(), tab.url());
    }

    // Every one of our pages needs a name here; a missing arm shows the
    // raw host.
    #[test]
    fn every_internal_address_has_a_readable_name() {
        for (url, expected) in [
            (crate::BLANK_TAB, "New Tab"),
            (crate::SETTINGS_ADDRESS, "Settings"),
            (crate::HISTORY_ADDRESS, "History"),
            (crate::BOOKMARKS_ADDRESS, "Bookmarks"),
        ] {
            assert_eq!(tab(url).label(), expected, "{url}");
        }
    }

    #[test]
    fn a_web_page_shows_its_host_until_a_title_arrives() {
        let mut tab = tab("https://sub.example.com/deep/path");
        assert_eq!(tab.label(), "sub.example.com");
        tab.title = "Real Title".into();
        assert_eq!(tab.label(), "Real Title");
    }

    #[test]
    fn a_new_document_drops_the_previous_pages_title() {
        let mut tab = tab("https://a.example");
        tab.title = "Page A".into();
        tab.sync_history(entries(&["https://a.example", "https://b.example"]), 1, true);
        assert_eq!(tab.label(), "b.example");
    }

    #[test]
    fn a_same_document_change_keeps_the_title() {
        let mut tab = tab("https://a.example");
        tab.title = "Page A".into();
        tab.sync_history(entries(&["https://a.example", "https://a.example/#b"]), 1, false);
        assert_eq!(tab.label(), "Page A");
    }
}
