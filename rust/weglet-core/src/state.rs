// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Windows and their tabs: what exists, what is in front, what moves where.

use crate::{Tab, TabId, WindowId, BLANK_TAB};

// The shape a restored session arrives in: for each window, its tabs as
// (history entries, cursor) pairs, and which was active. Plain tuples
// because weglet-profile builds these without depending on this crate.
pub type RestoredTab = (Vec<String>, usize);
pub type RestoredWindow = (Vec<RestoredTab>, usize);

// One browser window. Which tabs are its own is a property of the tabs,
// so all it holds is which of them is in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: WindowId,
    active: TabId,
}

impl Window {
    pub fn active(&self) -> TabId {
        self.active
    }
}

// Which windows exist, which tabs are in each, and which is in front.
//
// Tabs are kept in one list rather than one per window: their order
// within a window is the strip's order, and their identity is global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    tabs: Vec<Tab>,
    windows: Vec<Window>,
    next_tab: u64,
    next_window: u64,
}

// A page can call window.open in a loop. Counted across every window,
// because that is what the memory is.
const MAX_TABS: usize = 100;

// Likewise for windows, and lower: each is a toolbar renderer of its own
// on top of its tabs.
const MAX_WINDOWS: usize = 20;

// Ids start at 1 because 0 crosses the FFI as "no such tab or window".
const FIRST_ID: u64 = 1;

impl AppState {
    // Always starts with one window holding one tab: a browser with no
    // window has no valid representation on screen.
    pub fn new() -> Self {
        let window = WindowId::new(FIRST_ID);
        let tab = TabId::new(FIRST_ID);
        Self {
            tabs: vec![Tab::new(tab, window, BLANK_TAB.to_string())],
            windows: vec![Window {
                id: window,
                active: tab,
            }],
            next_tab: FIRST_ID + 1,
            next_window: FIRST_ID + 1,
        }
    }

    // Rebuilds from a restored session. Takes (history, cursor) pairs
    // rather than Tabs, so weglet-profile need not know how a Tab is put
    // together.
    pub fn restore(windows: Vec<RestoredWindow>) -> Self {
        let mut state = Self {
            tabs: Vec::new(),
            windows: Vec::new(),
            next_tab: FIRST_ID,
            next_window: FIRST_ID,
        };

        for (tabs, active_index) in windows.into_iter().take(MAX_WINDOWS) {
            if tabs.is_empty() {
                continue;
            }
            let window = WindowId::new(state.next_window);
            state.next_window += 1;

            let mut ids = Vec::new();
            for (entries, cursor) in tabs {
                if state.tabs.len() >= MAX_TABS {
                    break;
                }
                let id = TabId::new(state.next_tab);
                state.next_tab += 1;
                let mut tab = Tab::new(id, window, BLANK_TAB.to_string());
                tab.history = crate::History::from_entries(entries, cursor);
                state.tabs.push(tab);
                ids.push(id);
            }
            if ids.is_empty() {
                // The tab ceiling was reached partway through; a window
                // with no tabs cannot be shown.
                continue;
            }
            // Clamped, not trusted: the index comes off disk.
            let active = ids[active_index.min(ids.len() - 1)];
            state.windows.push(Window { id: window, active });
        }

        if state.windows.is_empty() {
            return Self::new();
        }
        state
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|window| window.id == id)
    }

    // The tabs of one window, in their strip order.
    pub fn tabs_in(&self, window: WindowId) -> impl Iterator<Item = &Tab> {
        self.tabs.iter().filter(move |tab| tab.window == window)
    }

    pub fn tab_count_in(&self, window: WindowId) -> usize {
        self.tabs_in(window).count()
    }

    pub fn tab_in_at(&self, window: WindowId, index: usize) -> Option<&Tab> {
        self.tabs_in(window).nth(index)
    }

    pub fn active_id(&self, window: WindowId) -> Option<TabId> {
        self.window(window).map(|window| window.active)
    }

    pub fn active_tab(&self, window: WindowId) -> Option<&Tab> {
        self.active_id(window).and_then(|id| self.tab(id))
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    // The position within the whole list, which is what removal needs.
    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    // The position within its own window, which is what the tab strip
    // shows and what reordering means.
    pub fn position_in_window(&self, id: TabId) -> Option<usize> {
        let tab = self.tab(id)?;
        self.tabs_in(tab.window).position(|other| other.id == id)
    }

    // None when the ceiling is reached, so the caller can say why.
    pub fn open_window(&mut self) -> Option<WindowId> {
        if self.windows.len() >= MAX_WINDOWS || self.tabs.len() >= MAX_TABS {
            return None;
        }
        let window = WindowId::new(self.next_window);
        self.next_window += 1;
        let tab = TabId::new(self.next_tab);
        self.next_tab += 1;
        self.tabs.push(Tab::new(tab, window, BLANK_TAB.to_string()));
        self.windows.push(Window {
            id: window,
            active: tab,
        });
        Some(window)
    }

    // True if the window existed. Its tabs go with it -- except for the last
    // window, which stays with all of its tabs: that is what the session
    // restores, and what showing the profile again brings back. A state
    // with no window has no valid representation on screen.
    pub fn close_window(&mut self, window: WindowId) -> bool {
        if self.window(window).is_none() {
            return false;
        }
        if self.windows.len() == 1 {
            return true;
        }
        self.tabs.retain(|tab| tab.window != window);
        self.windows.retain(|other| other.id != window);
        true
    }

    pub fn open_tab(&mut self, window: WindowId, url: &str) -> Option<TabId> {
        if self.tabs.len() >= MAX_TABS || self.window(window).is_none() {
            return None;
        }
        let id = TabId::new(self.next_tab);
        self.next_tab += 1;
        // After the last tab of this window, not at the end of
        // everything, so strip order is insertion order per window.
        let insert_at = self
            .tabs
            .iter()
            .rposition(|tab| tab.window == window)
            .map_or(self.tabs.len(), |position| position + 1);
        self.tabs.insert(insert_at, Tab::new(id, window, url.to_string()));
        self.set_active(window, id);
        Some(id)
    }

    // True if the tab existed. Closing the last tab of a window leaves a
    // fresh blank one rather than an empty window.
    pub fn close_tab(&mut self, id: TabId) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let window = self.tabs[index].window;
        let position = self.position_in_window(id).unwrap_or(0);
        self.tabs.remove(index);

        if self.tab_count_in(window) == 0 {
            let fresh = TabId::new(self.next_tab);
            self.next_tab += 1;
            self.tabs.push(Tab::new(fresh, window, BLANK_TAB.to_string()));
            self.set_active(window, fresh);
            return true;
        }

        if self.active_id(window) == Some(id) {
            // The one that took its place, or the new last one.
            let next = position.min(self.tab_count_in(window) - 1);
            if let Some(tab) = self.tab_in_at(window, next) {
                let tab_id = tab.id;
                self.set_active(window, tab_id);
            }
        }
        true
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        let window = tab.window;
        self.set_active(window, id);
        true
    }

    fn set_active(&mut self, window: WindowId, id: TabId) {
        if let Some(window) = self.windows.iter_mut().find(|w| w.id == window) {
            window.active = id;
        }
    }

    // Ctrl+Tab, within one window. Wraps at both ends.
    pub fn cycle(&mut self, window: WindowId, forward: bool) {
        let count = self.tab_count_in(window);
        if count < 2 {
            return;
        }
        let Some(active) = self.active_id(window) else {
            return;
        };
        let Some(index) = self.position_in_window(active) else {
            return;
        };
        let last = count - 1;
        let next = if forward {
            if index == last { 0 } else { index + 1 }
        } else if index == 0 {
            last
        } else {
            index - 1
        };
        if let Some(tab) = self.tab_in_at(window, next) {
            let id = tab.id;
            self.set_active(window, id);
        }
    }

    // Ctrl+1..9, one-based. 9 means the last tab.
    pub fn activate_by_position(&mut self, window: WindowId, position: usize) -> bool {
        let count = self.tab_count_in(window);
        if position == 0 || count == 0 {
            return false;
        }
        let index = if position >= 9 { count - 1 } else { position - 1 };
        match self.tab_in_at(window, index) {
            Some(tab) => {
                let id = tab.id;
                self.set_active(window, id);
                true
            }
            None => false,
        }
    }

    // `target` is an insertion index within the tab's own window, not the
    // tab to swap with.
    pub fn reorder(&mut self, id: TabId, target: usize) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let window = self.tabs[from].window;
        let count = self.tab_count_in(window);
        let Some(current) = self.position_in_window(id) else {
            return false;
        };
        let target = target.min(count - 1);
        if current == target {
            return false;
        }
        let tab = self.tabs.remove(from);
        // Where that position sits in the whole list, after the removal.
        let mut seen = 0;
        let mut insert_at = self.tabs.len();
        for (index, other) in self.tabs.iter().enumerate() {
            if other.window != window {
                continue;
            }
            if seen == target {
                insert_at = index;
                break;
            }
            seen += 1;
        }
        if seen < target {
            // Past the last tab of this window: go just after it.
            insert_at = self
                .tabs
                .iter()
                .rposition(|other| other.window == window)
                .map_or(self.tabs.len(), |position| position + 1);
        }
        self.tabs.insert(insert_at, tab);
        true
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The first window of a fresh state, which is what most tests use.
    fn only_window(state: &AppState) -> WindowId {
        state.windows()[0].id
    }

    fn urls(state: &AppState, window: WindowId) -> Vec<&str> {
        state.tabs_in(window).map(|tab| tab.url()).collect()
    }

    #[test]
    fn no_tab_or_window_has_id_zero() {
        let fresh = AppState::new();
        let restored = AppState::restore(vec![(vec![(vec!["https://a.example".into()], 0)], 0)]);
        for state in [fresh, restored] {
            assert!(state.tabs().iter().all(|tab| tab.id.value() != 0));
            assert!(state.windows().iter().all(|window| window.id.value() != 0));
        }
    }

    #[test]
    fn a_new_state_has_one_window_with_one_blank_active_tab() {
        let state = AppState::new();
        assert_eq!(state.windows().len(), 1);
        let window = only_window(&state);
        assert_eq!(state.tab_count_in(window), 1);
        assert_eq!(state.active_tab(window).unwrap().url(), BLANK_TAB);
    }

    #[test]
    fn opening_a_tab_makes_it_active_in_its_own_window() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let id = state.open_tab(window, "https://a.example").unwrap();
        assert_eq!(state.active_id(window), Some(id));
        assert_eq!(state.tab_count_in(window), 2);
    }

    #[test]
    fn ids_are_never_reused_after_a_close() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let first = state.open_tab(window, "https://a.example").unwrap();
        state.close_tab(first);
        let second = state.open_tab(window, "https://b.example").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn closing_the_only_tab_of_a_window_leaves_a_fresh_blank_one() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let only = state.active_id(window).unwrap();
        assert!(state.close_tab(only));
        assert_eq!(state.tab_count_in(window), 1);
        assert_eq!(state.active_tab(window).unwrap().url(), BLANK_TAB);
        assert_ne!(state.active_id(window), Some(only));
    }

    #[test]
    fn closing_the_active_tab_activates_the_one_that_took_its_place() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let a = state.active_id(window).unwrap();
        let b = state.open_tab(window, "https://b.example").unwrap();
        let c = state.open_tab(window, "https://c.example").unwrap();
        state.activate(b);
        state.close_tab(b);
        assert_eq!(state.active_id(window), Some(c));
        assert_eq!(state.tab_count_in(window), 2);
        assert!(state.tab(a).is_some());
    }

    #[test]
    fn closing_the_rightmost_tab_activates_the_new_rightmost() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let a = state.active_id(window).unwrap();
        let b = state.open_tab(window, "https://b.example").unwrap();
        state.close_tab(b);
        assert_eq!(state.active_id(window), Some(a));
    }

    #[test]
    fn closing_an_unknown_tab_reports_failure() {
        let mut state = AppState::new();
        assert!(!state.close_tab(TabId::new(999)));
    }

    #[test]
    fn cycling_wraps_at_both_ends() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let a = state.active_id(window).unwrap();
        let b = state.open_tab(window, "https://b.example").unwrap();
        state.activate(a);
        state.cycle(window, false);
        assert_eq!(state.active_id(window), Some(b));
        state.cycle(window, true);
        assert_eq!(state.active_id(window), Some(a));
    }

    #[test]
    fn position_nine_means_the_last_tab_however_many_there_are() {
        let mut state = AppState::new();
        let window = only_window(&state);
        for i in 0..4 {
            state.open_tab(window, &format!("https://{i}.example"));
        }
        let last = state.tabs_in(window).last().unwrap().id;
        assert!(state.activate_by_position(window, 9));
        assert_eq!(state.active_id(window), Some(last));
    }

    #[test]
    fn an_out_of_range_position_is_rejected() {
        let mut state = AppState::new();
        let window = only_window(&state);
        assert!(!state.activate_by_position(window, 0));
        assert!(!state.activate_by_position(window, 5));
    }

    #[test]
    fn reordering_moves_a_tab_within_its_own_window() {
        let mut state = AppState::new();
        let window = only_window(&state);
        state.open_tab(window, "https://b.example");
        let c = state.open_tab(window, "https://c.example").unwrap();
        assert!(state.reorder(c, 0));
        assert_eq!(
            urls(&state, window),
            ["https://c.example", BLANK_TAB, "https://b.example"]
        );
    }

    #[test]
    fn reordering_to_the_same_place_reports_no_change() {
        let mut state = AppState::new();
        let window = only_window(&state);
        let a = state.active_id(window).unwrap();
        assert!(!state.reorder(a, 0));
    }

    #[test]
    fn the_tab_count_is_bounded_across_every_window() {
        let mut state = AppState::new();
        let window = only_window(&state);
        while state.open_tab(window, "https://x.example").is_some() {}
        assert_eq!(state.tabs().len(), MAX_TABS);
        // And a second window cannot get around it.
        assert!(state.open_window().is_none());
    }

    // --- windows -------------------------------------------------------

    #[test]
    fn a_new_window_starts_with_its_own_blank_tab() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();
        assert_ne!(first, second);
        assert_eq!(state.windows().len(), 2);
        assert_eq!(state.tab_count_in(second), 1);
        assert_eq!(state.active_tab(second).unwrap().url(), BLANK_TAB);
    }

    // Two windows must not show the same tabs and disagree about which is
    // in front.
    #[test]
    fn each_window_has_its_own_tabs_and_its_own_active_one() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();

        let a = state.open_tab(first, "https://a.example").unwrap();
        let b = state.open_tab(second, "https://b.example").unwrap();

        assert_eq!(state.active_id(first), Some(a));
        assert_eq!(state.active_id(second), Some(b));
        assert_eq!(state.tab_count_in(first), 2);
        assert_eq!(state.tab_count_in(second), 2);
        assert!(state.tabs_in(first).all(|tab| tab.id != b));
        assert!(state.tabs_in(second).all(|tab| tab.id != a));
    }

    #[test]
    fn activating_a_tab_only_moves_its_own_windows_selection() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();
        let first_active = state.active_id(first).unwrap();
        let b = state.open_tab(second, "https://b.example").unwrap();

        state.activate(b);
        assert_eq!(state.active_id(second), Some(b));
        assert_eq!(state.active_id(first), Some(first_active));
    }

    #[test]
    fn cycling_does_not_reach_into_another_window() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();
        state.open_tab(second, "https://b.example");
        state.open_tab(second, "https://c.example");

        // One tab in the first window, so cycling there does nothing.
        let before = state.active_id(first);
        state.cycle(first, true);
        assert_eq!(state.active_id(first), before);
    }

    #[test]
    fn closing_a_window_takes_its_tabs_with_it() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();
        state.open_tab(second, "https://b.example");
        assert_eq!(state.tabs().len(), 3);

        assert!(state.close_window(second));
        assert_eq!(state.windows().len(), 1);
        assert_eq!(state.tabs().len(), 1);
        assert_eq!(state.tab_count_in(first), 1);
    }

    #[test]
    fn closing_the_last_window_keeps_it_and_its_tabs() {
        let mut state = AppState::new();
        let only = only_window(&state);
        state.open_tab(only, "https://a.example").unwrap();
        assert!(state.close_window(only));
        assert_eq!(state.windows().len(), 1);
        assert_eq!(state.windows()[0].id, only);
        assert_eq!(urls(&state, only), [BLANK_TAB, "https://a.example"]);
    }

    #[test]
    fn closing_an_unknown_window_reports_failure() {
        let mut state = AppState::new();
        assert!(!state.close_window(WindowId::new(999)));
    }

    #[test]
    fn the_window_count_is_bounded() {
        let mut state = AppState::new();
        while state.open_window().is_some() {}
        assert_eq!(state.windows().len(), MAX_WINDOWS);
    }

    #[test]
    fn a_tab_opened_in_one_window_does_not_disturb_anothers_order() {
        let mut state = AppState::new();
        let first = only_window(&state);
        let second = state.open_window().unwrap();
        state.open_tab(first, "https://a.example");
        state.open_tab(second, "https://b.example");
        state.open_tab(first, "https://c.example");

        assert_eq!(
            urls(&state, first),
            [BLANK_TAB, "https://a.example", "https://c.example"]
        );
        assert_eq!(urls(&state, second), [BLANK_TAB, "https://b.example"]);
    }

    // --- restore -------------------------------------------------------

    #[test]
    fn a_restored_session_keeps_each_tabs_position_in_its_history() {
        let state = AppState::restore(vec![(
            vec![
                (vec!["https://a.example".into(), "https://a2.example".into()], 0),
                (vec!["https://b.example".into()], 0),
            ],
            1,
        )]);
        let window = only_window(&state);
        assert_eq!(state.tab_count_in(window), 2);
        assert_eq!(state.tab_in_at(window, 0).unwrap().url(), "https://a.example");
        assert!(state.tab_in_at(window, 0).unwrap().history.can_go_forward());
        assert_eq!(state.active_tab(window).unwrap().url(), "https://b.example");
    }

    #[test]
    fn a_restored_session_keeps_its_windows_apart() {
        let state = AppState::restore(vec![
            (vec![(vec!["https://a.example".into()], 0)], 0),
            (
                vec![
                    (vec!["https://b.example".into()], 0),
                    (vec!["https://c.example".into()], 0),
                ],
                1,
            ),
        ]);
        assert_eq!(state.windows().len(), 2);
        let first = state.windows()[0].id;
        let second = state.windows()[1].id;
        assert_eq!(state.tab_count_in(first), 1);
        assert_eq!(state.tab_count_in(second), 2);
        assert_eq!(state.active_tab(second).unwrap().url(), "https://c.example");
    }

    #[test]
    fn an_empty_restored_session_becomes_a_fresh_state() {
        let state = AppState::restore(Vec::new());
        assert_eq!(state.windows().len(), 1);
        assert_eq!(state.tabs().len(), 1);
        assert_eq!(
            state.active_tab(only_window(&state)).unwrap().url(),
            BLANK_TAB
        );
    }

    // A window with no tabs cannot be shown, so it is not restored.
    #[test]
    fn a_restored_window_with_no_tabs_is_dropped() {
        let state = AppState::restore(vec![
            (Vec::new(), 0),
            (vec![(vec!["https://a.example".into()], 0)], 0),
        ]);
        assert_eq!(state.windows().len(), 1);
        assert_eq!(state.tabs().len(), 1);
    }

    #[test]
    fn an_out_of_range_active_index_is_clamped() {
        let state = AppState::restore(vec![(
            vec![(vec!["https://a.example".into()], 0)],
            99,
        )]);
        assert_eq!(
            state.active_tab(only_window(&state)).unwrap().url(),
            "https://a.example"
        );
    }

    #[test]
    fn a_restored_session_larger_than_the_cap_is_truncated() {
        let tabs: Vec<RestoredTab> = (0..MAX_TABS + 20)
            .map(|i| (vec![format!("https://{i}.example")], 0))
            .collect();
        let state = AppState::restore(vec![(tabs, 0)]);
        assert_eq!(state.tabs().len(), MAX_TABS);
    }

    #[test]
    fn more_restored_windows_than_the_cap_are_truncated() {
        let windows: Vec<RestoredWindow> = (0..MAX_WINDOWS + 5)
            .map(|i| (vec![(vec![format!("https://{i}.example")], 0)], 0))
            .collect();
        let state = AppState::restore(windows);
        assert_eq!(state.windows().len(), MAX_WINDOWS);
    }
}
