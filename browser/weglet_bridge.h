// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The C++ side of the Rust FFI: tabs, history, settings, security.

#ifndef WEGLET_BROWSER_WEGLET_BRIDGE_H_
#define WEGLET_BROWSER_WEGLET_BRIDGE_H_

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "base/functional/callback_forward.h"
#include "base/memory/raw_ptr_exclusion.h"
#include "base/memory/scoped_refptr.h"
#include "base/sequence_checker.h"
#include "base/time/time.h"

class GURL;

namespace network {
class SharedURLLoaderFactory;
class SimpleURLLoader;
}  // namespace network

// The opaque handle from rust/weglet_ffi.h, forward-declared at global
// scope where that header puts it. Declaring it inside namespace weglet
// would create a second, unrelated type.
extern "C" {
struct WegletState;
}

namespace weglet {

// What the browser knows about its own tabs, held in Rust.
//
// Nothing outside weglet_bridge.cc includes weglet_ffi.h, so the raw
// pointers and manual frees are confined to one file.
class WegletBridge {
 public:
  // 0 = nothing to say, 1 = warn, 2 = block. Mirrors the integers the
  // Rust side returns; see weglet_assess_navigation.
  enum class Risk {
    kNone = 0,
    kWarning = 1,
    kBlock = 2,
  };

  struct TabInfo {
    uint64_t id = 0;
    std::string url;
    // What to show in the tab strip: the page title, or its host until a
    // title arrives, or a name for one of our own pages.
    std::string label;
    bool can_go_back = false;
    bool can_go_forward = false;
    bool loading = false;
  };

  struct Shortcut {
    std::string title;
    std::string url;
  };

  struct BookmarkEntry {
    std::string title;
    std::string url;
  };

  struct HistoryEntry {
    std::string query;
    std::string url;
    uint64_t visited_at = 0;
  };

  // 0 = in progress, 1 = completed, 2 = failed -- mirrors
  // weglet_download_status_at exactly, so nothing here re-decides what
  // the Rust side already decided.
  struct DownloadEntry {
    std::string filename;
    uint32_t status = 0;
    std::string size_label;
    std::string error_message;
    uint64_t started_at = 0;
    std::string path;
  };

  struct EngineChoice {
    std::string id;
    std::string label;
  };

  struct SettingsSnapshot {
    std::vector<EngineChoice> engines;
    std::string search_engine;
    std::string custom_search_url;
    bool restore_session = false;
    std::string accent_color;
    // "pill" | "rounded" | "square".
    std::string address_bar_shape;
    std::vector<std::string> blocked_hosts;
    bool threat_feed_enabled = false;
    uint64_t threat_feed_updated_at = 0;
    bool threat_feed_last_update_failed = false;
    bool ask_where_to_save_downloads = false;
    std::vector<std::string> profiles;
    std::string active_profile;
    // Parallel to `profiles`.
    std::vector<std::string> profile_colors;
  };

  WegletBridge();
  // As the default constructor, but loads `profile`'s own data instead
  // of "Default" -- see WegletBrowserContext, which is expected to be
  // pointed at the same profile so cookies and settings agree on where
  // they both live.
  explicit WegletBridge(const std::string& profile);
  WegletBridge(const WegletBridge&) = delete;
  WegletBridge& operator=(const WegletBridge&) = delete;
  ~WegletBridge();

  // Whether `name` could become a profile directory -- no path
  // separators, nothing a filesystem would reject. Static: callable
  // before any WegletBridge exists, to resolve --profile at startup.
  static bool IsValidProfileName(const std::string& name);

  // This instance's own profile name -- "Default" if none was given or
  // the one given was invalid.
  std::string CurrentProfile() const;
  // Every profile this machine has ever used, alphabetical.
  std::vector<std::string> ListProfiles() const;

  // All static: none of these need a live WegletState.
  static bool DeleteProfile(const std::string& name);
  static bool RenameProfile(const std::string& old_name, const std::string& new_name);
  static std::string PeekAccentColor(const std::string& name);
  static std::string LastProfile();
  static void SetLastProfile(const std::string& name);

  // Windows. Every tab question is asked about one.
  std::vector<uint64_t> Windows() const;
  // 0 when the window ceiling is reached.
  uint64_t OpenWindow();
  bool CloseWindow(uint64_t window);
  // Which window a tab is in, or 0 for no such tab.
  uint64_t TabWindow(uint64_t id) const;

  // Tabs, within one window. An id is global once you have it.
  std::vector<TabInfo> Tabs(uint64_t window) const;
  uint64_t ActiveTabId(uint64_t window) const;
  std::string TabUrl(uint64_t id) const;

  // 0 when the tab ceiling is reached.
  uint64_t OpenTab(uint64_t window, const std::string& url);
  bool CloseTab(uint64_t id);
  bool ActivateTab(uint64_t id);
  void CycleTab(uint64_t window, bool forward);
  // One-based, as on the keyboard. 9 means the last tab.
  bool ActivateTabAt(uint64_t window, size_t position);
  bool ReorderTab(uint64_t id, size_t target);

  // The engine's back/forward list for a tab, after it changed. The engine
  // owns it; the model keeps a copy for the tab strip and the session.
  // `new_document` is false for a pushState or a fragment change.
  struct BackForwardList {
    BackForwardList();
    BackForwardList(const BackForwardList&);
    BackForwardList& operator=(const BackForwardList&);
    ~BackForwardList();

    std::vector<std::string> entries;
    size_t cursor = 0;
  };
  void BackForwardListChanged(uint64_t id,
                              const BackForwardList& list,
                              bool new_document);
  // The model's copy, for a restored tab the engine has not seen yet.
  BackForwardList TabBackForwardList(uint64_t id) const;
  void TitleChanged(uint64_t id, const std::string& title);
  void LoadingChanged(uint64_t id, bool loading);


  // Shortcuts: the pinned sites on the new tab page.
  std::vector<Shortcut> Shortcuts() const;
  std::string NewTabHint() const;
  // False when the dock is full.
  bool AddShortcut(const std::string& title, const std::string& url);
  bool EditShortcut(size_t index, const std::string& title, const std::string& url);
  bool RemoveShortcut(size_t index);

  // Bookmarks.
  std::vector<BookmarkEntry> Bookmarks() const;
  bool IsBookmarked(const std::string& url) const;
  // Adds `url` if not already saved, removes it if it is. Returns
  // whether the page is bookmarked after the call.
  bool ToggleBookmark(const std::string& title, const std::string& url);
  bool RemoveBookmark(size_t index);

  // Browsing history: address-bar submissions, newest first.
  std::vector<HistoryEntry> SearchHistory() const;
  // `query` is what the user typed; `url` is where it resolved to.
  void RecordHistory(const std::string& query, const std::string& url);
  void ClearSearchHistory();

  // Downloads, newest first. The four Download* calls mirror
  // content::DownloadItem's own lifecycle -- see the .cc for who calls
  // them.
  std::vector<DownloadEntry> Downloads() const;
  void DownloadStarted(const std::string& url, const std::string& path);
  // `total_bytes` is -1 when the server sent no Content-Length.
  void DownloadProgress(const std::string& url, uint64_t bytes_downloaded, int64_t total_bytes);
  void DownloadCompleted(const std::string& url, uint64_t size_bytes);
  void DownloadFailed(const std::string& url, const std::string& message);
  void ClearDownloadHistory();

  // OpenPhish's public feed of known-phishing URLs. See CheckThreatFeed.
  bool ThreatFeedEnabled() const;
  void SetThreatFeedEnabled(bool on);
  // `body` is the feed's raw text; see RefreshThreatFeed. False when it
  // doesn't look like a real feed -- the previous cache is left standing.
  bool ApplyThreatFeed(const std::string& body);
  uint64_t ThreatFeedUpdatedAt() const;
  bool ThreatFeedLastUpdateFailed() const;

  // Off by default: fetching a site's icon is itself a request to that
  // site. See docs/security.md and WegletFaviconFetcher.
  bool FaviconsEnabled() const;
  void SetFaviconsEnabled(bool on);

  // Off by default: most downloads land in Downloads with no prompt. On,
  // WegletDownloadManagerDelegate shows a Save As dialog for every one.
  bool AskWhereToSaveDownloads() const;
  void SetAskWhereToSaveDownloads(bool on);

  // `on_done` runs once with whether the fetch+parse succeeded. Only one
  // fetch is ever in flight; a new call replaces the previous one.
  void RefreshThreatFeed(scoped_refptr<network::SharedURLLoaderFactory> factory,
                        base::OnceCallback<void(bool)> on_done);

  // What the user typed, resolved to a URL. Search terms become a search
  // URL for the configured engine.
  std::string ResolveOmnibox(const std::string& input) const;

  struct OmniboxSuggestion {
    std::string label;
    std::string url;
    bool is_bookmark = false;
  };

  // The address bar's dropdown: bookmarks and history matching `input`,
  // ranked. Not const -- recomputes and caches the list Rust-side; see
  // weglet_query_omnibox_suggestions.
  std::vector<OmniboxSuggestion> QueryOmniboxSuggestions(const std::string& input);

  // Advisory: a kWarning may be dismissed, a kBlock may not.
  struct RiskAssessment {
    Risk level = Risk::kNone;
    std::string title;
    std::string reason;
    // Empty when there is no host to show.
    std::string host;
  };

  // One call: each of the four it replaced re-ran the whole assessment.
  RiskAssessment AssessNavigation(const GURL& url) const;
  // Just the level, skipping the three strings.
  Risk AssessNavigationLevel(const GURL& url) const;

  bool IsHostBlocked(const std::string& host) const;

  // The user's block list and the built-in one, asked about a whole URL
  // (the host is pulled out Rust-side). Shaped like AssessNavigation.
  RiskAssessment CheckBlockList(const GURL& url) const;
  bool IsUrlBlocked(const GURL& url) const;

  // Shaped like CheckBlockList. kNone when the setting is off -- checked
  // here so only one place can disagree with the toggle.
  RiskAssessment CheckThreatFeed(const GURL& url) const;

  // Everything the settings page renders, in one call.
  SettingsSnapshot Settings() const;
  bool SetSearchEngine(const std::string& id);
  void SetCustomSearchUrl(const std::string& url);
  void SetRestoreSession(bool on);
  bool SetAccentColor(const std::string& color);
  bool SetAddressBarShape(const std::string& shape);
  // Cheap, unlike Settings() (which rebuilds the engine list too), so
  // every page's push can carry these without the full snapshot cost.
  std::string Language() const;
  bool SetLanguage(const std::string& language);
  // "#RRGGBB".
  std::string AccentColor() const;
  // "pill" | "rounded" | "square".
  std::string AddressBarShape() const;
  // False if the host was already blocked or could not be canonicalised.
  bool BlockHost(const std::string& host);
  bool UnblockHost(size_t index);

  // False when the session could not be written.
  bool SaveSession() const;

  // Settings changes are marked, not written -- an atomic write ends in
  // fsync, not for the UI thread. Flushed on a timer and at shutdown.
  bool SettingsDirty() const;
  bool FlushSettings();

  // From the profile, bounded there.
  base::TimeDelta SettingsFlushInterval() const;
  base::TimeDelta SessionSaveInterval() const;

 private:
  // Takes a string from Rust and frees it.
  static std::string TakeString(char* owned);

  void OnThreatFeedFetched(scoped_refptr<network::SharedURLLoaderFactory> factory,
                           base::OnceCallback<void(bool)> on_done,
                           std::optional<std::string> body);

  // Alive only while a fetch is in flight.
  std::unique_ptr<network::SimpleURLLoader> threat_feed_loader_;

  // Opaque handle into the Rust side.
  // RAW_PTR_EXCLUSION: this points into Rust's allocator, not
  // PartitionAlloc, and is freed by weglet_state_free.
  RAW_PTR_EXCLUSION ::WegletState* state_ = nullptr;

  // The Rust side has no locking: its state belongs to one thread.
  SEQUENCE_CHECKER(sequence_checker_);
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_BRIDGE_H_
