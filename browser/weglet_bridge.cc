// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The C++ side of the Rust FFI: tabs, history, settings, security.

#include "weglet/browser/weglet_bridge.h"

#include "base/check.h"
#include "base/functional/bind.h"
#include "base/strings/string_util.h"
#include "net/traffic_annotation/network_traffic_annotation.h"
#include "services/network/public/cpp/resource_request.h"
#include "services/network/public/cpp/shared_url_loader_factory.h"
#include "services/network/public/cpp/simple_url_loader.h"
#include "url/gurl.h"
#include "weglet/rust/weglet_ffi.h"

namespace weglet {

namespace {

// OpenPhish's public feed -- see weglet-security/src/threat_feed.rs for
// what happens to it once it arrives. Never sent anywhere that would
// reveal a URL the user visited: this is the one request that goes out
// unconditionally, and it is always the same request.
constexpr char kThreatFeedUrl[] =
    "https://raw.githubusercontent.com/openphish/public_feed/refs/heads/"
    "main/feed.txt";
constexpr size_t kMaxThreatFeedBytes = 2 * 1024 * 1024;

net::NetworkTrafficAnnotationTag ThreatFeedTrafficAnnotation() {
  return net::DefineNetworkTrafficAnnotation("weglet_threat_feed", R"(
      semantics {
        sender: "Weglet Threat Feed"
        description:
          "Downloads OpenPhish's public feed of known-phishing URLs. "
          "Weglet keeps only salted-free SHA-256 hashes of the feed's own "
          "entries and checks browsing URLs against them locally -- the "
          "request carries no information about what the user is "
          "browsing."
        trigger:
          "The user opens Weglet with phishing protection on, or presses "
          "Refresh now in Settings."
        data: "None. The request has no parameters and no user data."
        destination: OTHER
        internal {
          contacts {
            email: "security@weglet.example"
          }
        }
        user_data {
          type: NONE
        }
        last_reviewed: "2026-01-01"
      }
      policy {
        cookies_allowed: NO
        setting: "Disable phishing protection in Settings > Security."
        policy_exception_justification:
          "Not yet controlled by an enterprise policy."
      })");
}

}  // namespace

WegletBridge::WegletBridge() : state_(weglet_state_new()) {
  // Failing here means failing at startup, where a message can still
  // reach the screen.
  CHECK(state_) << "could not create the Weglet state";
}

WegletBridge::WegletBridge(const std::string& profile)
    : state_(weglet_state_new_for_profile(profile.c_str())) {
  CHECK(state_) << "could not create the Weglet state";
}

WegletBridge::~WegletBridge() {
  weglet_state_free(state_);
  state_ = nullptr;
}

// static
bool WegletBridge::IsValidProfileName(const std::string& name) {
  return weglet_is_valid_profile_name(name.c_str());
}

// static
std::string WegletBridge::CanonicalProfileName(const std::string& name) {
  return TakeString(weglet_canonical_profile_name(name.c_str()));
}

std::string WegletBridge::CurrentProfile() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_current_profile(state_));
}

// static
bool WegletBridge::DeleteProfile(const std::string& name) {
  return weglet_delete_profile(name.c_str());
}

// static
bool WegletBridge::RenameProfile(const std::string& old_name, const std::string& new_name) {
  return weglet_rename_profile(old_name.c_str(), new_name.c_str());
}

// static
std::string WegletBridge::PeekAccentColor(const std::string& name) {
  return TakeString(weglet_profile_accent_color(name.c_str()));
}

// static
std::string WegletBridge::LastProfile() {
  return TakeString(weglet_last_profile());
}

// static
void WegletBridge::SetLastProfile(const std::string& name) {
  weglet_set_last_profile(name.c_str());
}

std::vector<std::string> WegletBridge::ListProfiles() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_profile_count();
  std::vector<std::string> profiles;
  profiles.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    profiles.push_back(TakeString(weglet_profile_name_at(index)));
  }
  return profiles;
}

// static
std::string WegletBridge::TakeString(char* owned) {
  if (!owned) {
    return std::string();
  }
  std::string copy(owned);
  weglet_string_free(owned);
  return copy;
}

std::vector<uint64_t> WegletBridge::Windows() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_window_count(state_);
  std::vector<uint64_t> windows;
  windows.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    windows.push_back(weglet_window_id_at(state_, index));
  }
  return windows;
}

uint64_t WegletBridge::OpenWindow() {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_open_window(state_);
}

bool WegletBridge::CloseWindow(uint64_t window) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_close_window(state_, window);
}

void WegletBridge::KeepWindowsOnClose() {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_keep_windows_on_close(state_);
}

uint64_t WegletBridge::TabWindow(uint64_t id) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_tab_window(state_, id);
}

std::vector<WegletBridge::TabInfo> WegletBridge::Tabs(uint64_t window) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);

  const size_t count = weglet_tab_count(state_, window);
  std::vector<TabInfo> tabs;
  tabs.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    TabInfo info;
    info.id = weglet_tab_id_at(state_, window, index);
    info.url = TakeString(weglet_tab_url(state_, info.id));
    info.label = TakeString(weglet_tab_label(state_, info.id));
    info.can_go_back = weglet_tab_can_go_back(state_, info.id);
    info.can_go_forward = weglet_tab_can_go_forward(state_, info.id);
    info.loading = weglet_tab_loading(state_, info.id);
    tabs.push_back(std::move(info));
  }
  return tabs;
}

uint64_t WegletBridge::ActiveTabId(uint64_t window) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_active_tab_id(state_, window);
}

std::string WegletBridge::TabUrl(uint64_t id) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_tab_url(state_, id));
}

uint64_t WegletBridge::OpenTab(uint64_t window, const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_open_tab(state_, window, url.c_str());
}

bool WegletBridge::CloseTab(uint64_t id) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_close_tab(state_, id);
}

bool WegletBridge::ActivateTab(uint64_t id) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_activate_tab(state_, id);
}

void WegletBridge::CycleTab(uint64_t window, bool forward) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_cycle_tab(state_, window, forward);
}

bool WegletBridge::ActivateTabAt(uint64_t window, size_t position) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_activate_tab_at(state_, window, position);
}

bool WegletBridge::ReorderTab(uint64_t id, size_t target) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_reorder_tab(state_, id, target);
}

WegletBridge::BackForwardList::BackForwardList() = default;
WegletBridge::BackForwardList::BackForwardList(const BackForwardList&) = default;
WegletBridge::BackForwardList& WegletBridge::BackForwardList::operator=(
    const BackForwardList&) = default;
WegletBridge::BackForwardList::~BackForwardList() = default;

// The separator weglet_ffi.h gives for a back/forward list.
constexpr char kBackForwardSeparator[] = "\n";

void WegletBridge::BackForwardListChanged(uint64_t id,
                                          const BackForwardList& list,
                                          bool new_document) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const std::string entries =
      base::JoinString(list.entries, kBackForwardSeparator);
  weglet_tab_history_changed(state_, id, entries.c_str(), list.cursor,
                             new_document);
}

WegletBridge::BackForwardList WegletBridge::TabBackForwardList(
    uint64_t id) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  BackForwardList list;
  const size_t count = weglet_tab_history_count(state_, id);
  list.entries.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    list.entries.push_back(
        TakeString(weglet_tab_history_entry_at(state_, id, index)));
  }
  list.cursor = weglet_tab_history_cursor(state_, id);
  return list;
}

void WegletBridge::TitleChanged(uint64_t id, const std::string& title) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_tab_title_changed(state_, id, title.c_str());
}

void WegletBridge::LoadingChanged(uint64_t id, bool loading) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_tab_loading_changed(state_, id, loading);
}

std::vector<WegletBridge::Shortcut> WegletBridge::Shortcuts() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_shortcut_count(state_);
  std::vector<Shortcut> shortcuts;
  shortcuts.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    shortcuts.push_back({
        TakeString(weglet_shortcut_title(state_, index)),
        TakeString(weglet_shortcut_url(state_, index)),
    });
  }
  return shortcuts;
}

std::string WegletBridge::NewTabHint() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_new_tab_hint(state_));
}

bool WegletBridge::AddShortcut(const std::string& title, const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_add_shortcut(state_, title.c_str(), url.c_str());
}

bool WegletBridge::EditShortcut(size_t index,
                                const std::string& title,
                                const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_edit_shortcut(state_, index, title.c_str(), url.c_str());
}

bool WegletBridge::RemoveShortcut(size_t index) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_remove_shortcut(state_, index);
}

std::vector<WegletBridge::BookmarkEntry> WegletBridge::Bookmarks() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_bookmark_count(state_);
  std::vector<BookmarkEntry> bookmarks;
  bookmarks.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    bookmarks.push_back({
        TakeString(weglet_bookmark_title_at(state_, index)),
        TakeString(weglet_bookmark_url_at(state_, index)),
    });
  }
  return bookmarks;
}

bool WegletBridge::IsBookmarked(const std::string& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_is_bookmarked(state_, url.c_str());
}

bool WegletBridge::ToggleBookmark(const std::string& title, const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_toggle_bookmark(state_, title.c_str(), url.c_str());
}

bool WegletBridge::RemoveBookmark(size_t index) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_remove_bookmark(state_, index);
}

std::vector<WegletBridge::HistoryEntry> WegletBridge::SearchHistory() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_history_count(state_);
  std::vector<HistoryEntry> entries;
  entries.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    entries.push_back({
        TakeString(weglet_history_query_at(state_, index)),
        TakeString(weglet_history_url_at(state_, index)),
        weglet_history_visited_at_at(state_, index),
    });
  }
  return entries;
}

void WegletBridge::RecordHistory(const std::string& query, const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_record_history(state_, query.c_str(), url.c_str());
}

void WegletBridge::ClearSearchHistory() {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_clear_search_history(state_);
}

std::vector<WegletBridge::DownloadEntry> WegletBridge::Downloads() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_download_count(state_);
  std::vector<DownloadEntry> downloads;
  downloads.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    downloads.push_back({
        TakeString(weglet_download_filename_at(state_, index)),
        weglet_download_status_at(state_, index),
        TakeString(weglet_download_size_label_at(state_, index)),
        TakeString(weglet_download_error_message_at(state_, index)),
        weglet_download_started_at_at(state_, index),
        TakeString(weglet_download_path_at(state_, index)),
    });
  }
  return downloads;
}

void WegletBridge::DownloadStarted(const std::string& guid,
                                   const std::string& url,
                                   const std::string& path) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_download_started(state_, guid.c_str(), url.c_str(), path.c_str());
}

void WegletBridge::DownloadProgress(const std::string& guid,
                                    const std::string& path,
                                    uint64_t bytes_downloaded,
                                    int64_t total_bytes) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_download_progress(state_, guid.c_str(), path.c_str(), bytes_downloaded,
                           total_bytes);
}

void WegletBridge::DownloadCompleted(const std::string& guid,
                                     const std::string& path,
                                     uint64_t size_bytes) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_download_completed(state_, guid.c_str(), path.c_str(), size_bytes);
}

void WegletBridge::DownloadFailed(const std::string& guid,
                                  const std::string& message) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_download_failed(state_, guid.c_str(), message.c_str());
}

void WegletBridge::ClearDownloadHistory() {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_clear_download_history(state_);
}

bool WegletBridge::ThreatFeedEnabled() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_threat_feed_enabled(state_);
}

void WegletBridge::SetThreatFeedEnabled(bool on) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_set_threat_feed_enabled(state_, on);
}

bool WegletBridge::FaviconsEnabled() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_favicons_enabled(state_);
}

void WegletBridge::SetFaviconsEnabled(bool on) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_set_favicons_enabled(state_, on);
}

bool WegletBridge::AskWhereToSaveDownloads() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_ask_where_to_save_downloads(state_);
}

void WegletBridge::SetAskWhereToSaveDownloads(bool on) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_set_ask_where_to_save_downloads(state_, on);
}

bool WegletBridge::ApplyThreatFeed(const std::string& body) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_apply_threat_feed(state_, body.c_str());
}

uint64_t WegletBridge::ThreatFeedUpdatedAt() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_threat_feed_updated_at(state_);
}

bool WegletBridge::ThreatFeedLastUpdateFailed() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_threat_feed_last_update_failed(state_);
}

void WegletBridge::RefreshThreatFeed(
    scoped_refptr<network::SharedURLLoaderFactory> factory,
    base::OnceCallback<void(bool)> on_done) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  auto request = std::make_unique<network::ResourceRequest>();
  request->url = GURL(kThreatFeedUrl);
  request->credentials_mode = network::mojom::CredentialsMode::kOmit;

  // Replaces whatever the previous fetch was doing; its loader is
  // destroyed, which cancels it -- there is only ever one feed to have.
  threat_feed_loader_ =
      network::SimpleURLLoader::Create(std::move(request), ThreatFeedTrafficAnnotation());
  network::SimpleURLLoader* loader = threat_feed_loader_.get();
  // Unretained: this owns the loader, so the callback cannot outlive it.
  // `factory` is bound to keep it alive for the request's duration.
  loader->DownloadToString(
      factory.get(),
      base::BindOnce(&WegletBridge::OnThreatFeedFetched, base::Unretained(this),
                     factory, std::move(on_done)),
      kMaxThreatFeedBytes);
}

void WegletBridge::OnThreatFeedFetched(
    scoped_refptr<network::SharedURLLoaderFactory> factory,
    base::OnceCallback<void(bool)> on_done,
    std::optional<std::string> body) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  threat_feed_loader_.reset();
  if (!body) {
    std::move(on_done).Run(false);
    return;
  }
  std::move(on_done).Run(ApplyThreatFeed(*body));
}

WegletBridge::RiskAssessment WegletBridge::CheckThreatFeed(const GURL& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  char* title = nullptr;
  char* reason = nullptr;
  const bool matched =
      weglet_is_known_phishing(state_, url.spec().c_str(), &title, &reason);

  RiskAssessment assessment;
  assessment.level = matched ? Risk::kBlock : Risk::kNone;
  assessment.title = TakeString(title);
  assessment.reason = TakeString(reason);
  assessment.host = std::string(url.host());
  return assessment;
}

std::string WegletBridge::ResolveOmnibox(const std::string& input) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_omnibox_resolve(state_, input.c_str()));
}

std::vector<WegletBridge::OmniboxSuggestion> WegletBridge::QueryOmniboxSuggestions(
    const std::string& input) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  const size_t count = weglet_query_omnibox_suggestions(state_, input.c_str());
  std::vector<OmniboxSuggestion> suggestions;
  suggestions.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    suggestions.push_back({
        TakeString(weglet_omnibox_suggestion_label_at(state_, index)),
        TakeString(weglet_omnibox_suggestion_url_at(state_, index)),
        weglet_omnibox_suggestion_is_bookmark_at(state_, index),
    });
  }
  return suggestions;
}

namespace {

// weglet_assess_navigation's return codes. Unrecognised = "nothing to
// say": a mismatch must not stop pages from loading.
WegletBridge::Risk RiskFromLevel(uint32_t level) {
  switch (level) {
    case 2:
      return WegletBridge::Risk::kBlock;
    case 1:
      return WegletBridge::Risk::kWarning;
    default:
      return WegletBridge::Risk::kNone;
  }
}

}  // namespace

WegletBridge::RiskAssessment WegletBridge::AssessNavigation(
    const GURL& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  char* title = nullptr;
  char* reason = nullptr;
  char* host = nullptr;
  const uint32_t level =
      weglet_assess_navigation(state_, url.spec().c_str(), &title, &reason, &host);

  RiskAssessment assessment;
  assessment.level = RiskFromLevel(level);
  // The Rust side writes every non-null out-param even on a "nothing to
  // say" answer, so all three have to be freed.
  assessment.title = TakeString(title);
  assessment.reason = TakeString(reason);
  assessment.host = TakeString(host);
  return assessment;
}

WegletBridge::Risk WegletBridge::AssessNavigationLevel(const GURL& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  // Nulls: the caller wants the decision, not the wording.
  return RiskFromLevel(weglet_assess_navigation(state_, url.spec().c_str(),
                                                nullptr, nullptr, nullptr));
}

bool WegletBridge::IsHostBlocked(const std::string& host) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_is_host_blocked(state_, host.c_str());
}

WegletBridge::SettingsSnapshot WegletBridge::Settings() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  SettingsSnapshot snapshot;

  const size_t engine_count = weglet_engine_count(state_);
  snapshot.engines.reserve(engine_count);
  for (size_t index = 0; index < engine_count; ++index) {
    snapshot.engines.push_back({
        TakeString(weglet_engine_id_at(state_, index)),
        TakeString(weglet_engine_label_at(state_, index)),
    });
  }

  snapshot.search_engine = TakeString(weglet_search_engine(state_));
  snapshot.custom_search_url = TakeString(weglet_custom_search_url(state_));
  snapshot.restore_session = weglet_restore_session(state_);
  snapshot.accent_color = TakeString(weglet_accent_color(state_));
  snapshot.address_bar_shape = TakeString(weglet_address_bar_shape(state_));
  snapshot.ask_where_to_save_downloads = weglet_ask_where_to_save_downloads(state_);

  const size_t blocked_count = weglet_blocked_host_count(state_);
  snapshot.blocked_hosts.reserve(blocked_count);
  for (size_t index = 0; index < blocked_count; ++index) {
    snapshot.blocked_hosts.push_back(
        TakeString(weglet_blocked_host_at(state_, index)));
  }

  snapshot.threat_feed_enabled = weglet_threat_feed_enabled(state_);
  snapshot.threat_feed_updated_at = weglet_threat_feed_updated_at(state_);
  snapshot.threat_feed_last_update_failed =
      weglet_threat_feed_last_update_failed(state_);

  snapshot.active_profile = TakeString(weglet_current_profile(state_));
  const size_t profile_count = weglet_profile_count();
  snapshot.profiles.reserve(profile_count);
  snapshot.profile_colors.reserve(profile_count);
  for (size_t index = 0; index < profile_count; ++index) {
    std::string name = TakeString(weglet_profile_name_at(index));
    snapshot.profile_colors.push_back(
        name == snapshot.active_profile ? snapshot.accent_color : PeekAccentColor(name));
    snapshot.profiles.push_back(std::move(name));
  }

  return snapshot;
}

bool WegletBridge::SetSearchEngine(const std::string& id) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_set_search_engine(state_, id.c_str());
}

void WegletBridge::SetCustomSearchUrl(const std::string& url) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_set_custom_search_url(state_, url.c_str());
}

void WegletBridge::SetRestoreSession(bool on) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  weglet_set_restore_session(state_, on);
}

bool WegletBridge::SetAccentColor(const std::string& color) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_set_accent_color(state_, color.c_str());
}

bool WegletBridge::SetAddressBarShape(const std::string& shape) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_set_address_bar_shape(state_, shape.c_str());
}

std::string WegletBridge::Language() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_language(state_));
}

bool WegletBridge::SetLanguage(const std::string& language) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_set_language(state_, language.c_str());
}

std::string WegletBridge::AccentColor() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_accent_color(state_));
}

std::string WegletBridge::AddressBarShape() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return TakeString(weglet_address_bar_shape(state_));
}

bool WegletBridge::BlockHost(const std::string& host) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_block_host(state_, host.c_str());
}

bool WegletBridge::UnblockHost(size_t index) {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_unblock_host(state_, index);
}

WegletBridge::RiskAssessment WegletBridge::CheckBlockList(
    const GURL& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  char* title = nullptr;
  char* reason = nullptr;
  const bool blocked =
      weglet_is_url_blocked(state_, url.spec().c_str(), &title, &reason);

  RiskAssessment assessment;
  assessment.level = blocked ? Risk::kBlock : Risk::kNone;
  // Both out-params are written either way, so both have to be freed.
  assessment.title = TakeString(title);
  assessment.reason = TakeString(reason);
  assessment.host = std::string(url.host());
  return assessment;
}

bool WegletBridge::IsUrlBlocked(const GURL& url) const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  // Nulls: the caller wants the decision, not the wording.
  return weglet_is_url_blocked(state_, url.spec().c_str(), nullptr, nullptr);
}

bool WegletBridge::SaveSession() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_save_session(state_);
}

bool WegletBridge::SettingsDirty() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_settings_dirty(state_);
}

base::TimeDelta WegletBridge::SettingsFlushInterval() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return base::Seconds(weglet_settings_flush_seconds(state_));
}

base::TimeDelta WegletBridge::SessionSaveInterval() const {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return base::Seconds(weglet_session_save_seconds(state_));
}

bool WegletBridge::FlushSettings() {
  DCHECK_CALLED_ON_VALID_SEQUENCE(sequence_checker_);
  return weglet_flush_settings(state_);
}

}  // namespace weglet