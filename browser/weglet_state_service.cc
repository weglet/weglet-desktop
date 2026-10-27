// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Tracks the open pages and pushes state to the ones a change affects.

#include "weglet/browser/weglet_state_service.h"

#include <algorithm>
#include <memory>
#include <string>
#include <utility>

#include "base/functional/bind.h"
#include "base/strings/string_number_conversions.h"
#include "base/task/single_thread_task_runner.h"
#include "content/public/browser/browser_context.h"
#include "content/public/browser/web_contents.h"
#include "content/public/browser/web_ui.h"
#include "url/gurl.h"
#include "url/origin.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_permission_delegate.h"
#include "weglet/browser/weglet_security_guard.h"

namespace weglet {
namespace {

// The address is the key; the contents are never read.
const char kUserDataKey[] = "weglet_state_service";

base::ListValue TabsToValue(const std::vector<WegletBridge::TabInfo>& tabs) {
  // base::ListValue and base::DictValue are top-level classes in this
  // version, not the nested Value::List and Value::Dict.
  base::ListValue list;
  for (const WegletBridge::TabInfo& info : tabs) {
    base::DictValue entry;
    // Ids cross as strings: a JavaScript number is a double and cannot
    // hold every u64 exactly.
    entry.Set("id", base::NumberToString(info.id));
    entry.Set("url", info.url);
    entry.Set("label", info.label);
    entry.Set("canGoBack", info.can_go_back);
    entry.Set("canGoForward", info.can_go_forward);
    entry.Set("loading", info.loading);
    list.Append(base::Value(std::move(entry)));
  }
  return list;
}

}  // namespace

// static
void WegletStateService::CreateForBrowserContext(
    content::BrowserContext* browser_context,
    WegletBridge* bridge,
    WegletSecurityGuard* guard,
    WegletPermissionDelegate* permission_delegate,
    WindowsSpellChecker* spell_checker) {
  browser_context->SetUserData(
      kUserDataKey, std::make_unique<WegletStateService>(
                        bridge, guard, permission_delegate, spell_checker));
}

// static
// static
void WegletStateService::RemoveFromBrowserContext(
    content::BrowserContext* browser_context) {
  browser_context->RemoveUserData(kUserDataKey);
}

// static
WegletStateService* WegletStateService::FromBrowserContext(
    content::BrowserContext* browser_context) {
  if (!browser_context) {
    return nullptr;
  }
  return static_cast<WegletStateService*>(
      browser_context->GetUserData(kUserDataKey));
}

WegletStateService::WegletStateService(WegletBridge* bridge,
                                      WegletSecurityGuard* guard,
                                      WegletPermissionDelegate* permission_delegate,
                                      WindowsSpellChecker* spell_checker)
    : bridge_(bridge),
      guard_(guard),
      permission_delegate_(permission_delegate),
      spell_checker_(spell_checker) {}

WegletStateService::~WegletStateService() = default;

void WegletStateService::AddPage(content::WebUI* web_ui,
                                 contract::PageKind kind,
                                 uint64_t window) {
  if (!web_ui || kind == contract::PageKind::kOther) {
    return;
  }
  pages_.push_back(Entry{web_ui, kind, window});
}

void WegletStateService::RemovePage(content::WebUI* web_ui) {
  std::erase_if(pages_, [web_ui](const Entry& entry) {
    return entry.web_ui == web_ui;
  });
}

// static
bool WegletStateService::Affects(uint32_t changes, contract::PageKind kind) {
  if ((changes & (kLanguage | kFavicons | kTheme)) != 0) {
    // Every page's push carries these (see SendTo), so every page
    // showing anything should repaint -- a popup with nothing pending
    // still declines, same as always, since BuildFor returns nullopt.
    return true;
  }
  switch (kind) {
    case contract::PageKind::kToolbar:
      return (changes & kTabs) != 0;
    case contract::PageKind::kNewtab:
      // Settings too: the line under the search field names the
      // configured engine.
      return (changes & (kShortcuts | kSettings)) != 0;
    case contract::PageKind::kSettings:
      return (changes & kSettings) != 0;
    case contract::PageKind::kSecurityNotice:
    case contract::PageKind::kSaveShortcut:
    case contract::PageKind::kMenu:
    case contract::PageKind::kContextMenu:
      // Pushed when the page asks: what any of these shows belongs to
      // one moment (a stopped navigation, a button click, the popup
      // opening) and never changes under it.
      return false;
    case contract::PageKind::kFindBar:
      // Unlike those: a find session's match count changes repeatedly
      // while the bar stays open, on every reply from the renderer.
      return (changes & kFindResult) != 0;
    case contract::PageKind::kPermissionPrompt:
    case contract::PageKind::kSiteInfo:
    case contract::PageKind::kToast:
      return false;
    case contract::PageKind::kOmniboxSuggestions:
      // Like the find bar: new rows on every keystroke, while it stays
      // open.
      return (changes & kOmniboxSuggestions) != 0;
    case contract::PageKind::kHistory:
      return (changes & kHistory) != 0;
    case contract::PageKind::kBookmarks:
      return (changes & kBookmarks) != 0;
    case contract::PageKind::kOther:
      return false;
  }
  return false;
}

void WegletStateService::RequestOmniboxFocus(uint64_t window) {
  omnibox_focus_pending_.insert(window);
  Notify(kTabs);
}

void WegletStateService::SetPendingShortcut(content::WebContents* contents,
                                            PendingShortcut shortcut) {
  if (!contents) {
    return;
  }
  pending_shortcuts_[contents] = std::move(shortcut);
}

void WegletStateService::ClearPendingShortcut(content::WebContents* contents) {
  pending_shortcuts_.erase(contents);
}

void WegletStateService::SetPendingContextMenu(
    content::WebContents* contents,
    std::vector<PendingContextMenuItem> items) {
  if (!contents) {
    return;
  }
  pending_context_menus_[contents] = std::move(items);
}

void WegletStateService::ClearPendingContextMenu(content::WebContents* contents) {
  pending_context_menus_.erase(contents);
}

void WegletStateService::SetPendingFindResult(content::WebContents* contents,
                                              PendingFindResult result) {
  if (!contents) {
    return;
  }
  pending_find_results_[contents] = std::move(result);
}

void WegletStateService::ClearPendingFindResult(content::WebContents* contents) {
  pending_find_results_.erase(contents);
}

void WegletStateService::SetPendingPermissionPrompt(content::WebContents* contents,
                                                     PendingPermissionPrompt prompt) {
  if (!contents) {
    return;
  }
  pending_permission_prompts_[contents] = std::move(prompt);
}

void WegletStateService::ClearPendingPermissionPrompt(content::WebContents* contents) {
  pending_permission_prompts_.erase(contents);
}

void WegletStateService::SetPendingToasts(content::WebContents* contents,
                                          std::vector<PendingToast> toasts) {
  if (!contents) {
    return;
  }
  pending_toasts_[contents] = std::move(toasts);
}

void WegletStateService::ClearPendingToasts(content::WebContents* contents) {
  pending_toasts_.erase(contents);
}

void WegletStateService::SetPendingOmniboxSuggestions(
    content::WebContents* contents, PendingOmniboxSuggestions suggestions) {
  if (!contents) {
    return;
  }
  pending_omnibox_suggestions_[contents] = std::move(suggestions);
}

void WegletStateService::ClearPendingOmniboxSuggestions(content::WebContents* contents) {
  pending_omnibox_suggestions_.erase(contents);
}

std::optional<base::DictValue> WegletStateService::BuildFor(
    const Entry& entry,
    content::WebContents* contents) {
  switch (entry.kind) {
    case contract::PageKind::kToolbar: {
      // Its own window's tabs, not every tab that exists.
      const std::vector<WegletBridge::TabInfo> tabs = bridge_->Tabs(entry.window);
      const uint64_t active_id = bridge_->ActiveTabId(entry.window);
      base::DictValue state;
      state.Set("tabs", base::Value(TabsToValue(tabs)));
      state.Set("activeId", base::NumberToString(active_id));
      // Consumed here: leaving it set would pull focus back on the next
      // title change of any tab.
      state.Set("focusOmnibox",
                omnibox_focus_pending_.erase(entry.window) > 0);
      bool bookmarked = false;
      for (const WegletBridge::TabInfo& tab : tabs) {
        if (tab.id == active_id) {
          bookmarked = bridge_->IsBookmarked(tab.url);
          break;
        }
      }
      state.Set("bookmarked", bookmarked);
      return state;
    }
    case contract::PageKind::kNewtab: {
      base::ListValue shortcuts;
      for (const WegletBridge::Shortcut& shortcut : bridge_->Shortcuts()) {
        base::DictValue item;
        item.Set("title", shortcut.title);
        item.Set("url", shortcut.url);
        shortcuts.Append(base::Value(std::move(item)));
      }
      base::DictValue state;
      state.Set("shortcuts", base::Value(std::move(shortcuts)));
      state.Set("hint", bridge_->NewTabHint());
      return state;
    }
    case contract::PageKind::kSettings: {
      const WegletBridge::SettingsSnapshot snapshot = bridge_->Settings();
      base::ListValue engines;
      for (const WegletBridge::EngineChoice& engine : snapshot.engines) {
        base::DictValue item;
        item.Set("id", engine.id);
        item.Set("label", engine.label);
        engines.Append(base::Value(std::move(item)));
      }
      base::ListValue blocked;
      for (const std::string& host : snapshot.blocked_hosts) {
        blocked.Append(base::Value(host));
      }
      base::ListValue profiles;
      for (const std::string& profile : snapshot.profiles) {
        profiles.Append(base::Value(profile));
      }
      base::ListValue profile_colors;
      for (const std::string& color : snapshot.profile_colors) {
        profile_colors.Append(base::Value(color));
      }
      base::DictValue state;
      state.Set("engines", base::Value(std::move(engines)));
      state.Set("searchEngine", snapshot.search_engine);
      state.Set("customSearchUrl", snapshot.custom_search_url);
      state.Set("restoreSession", snapshot.restore_session);
      state.Set("blockedHosts", base::Value(std::move(blocked)));
      state.Set("accentColor", snapshot.accent_color);
      state.Set("addressBarShape", snapshot.address_bar_shape);
      state.Set("threatFeedEnabled", snapshot.threat_feed_enabled);
      state.Set("threatFeedUpdatedAt",
                static_cast<double>(snapshot.threat_feed_updated_at));
      state.Set("threatFeedLastUpdateFailed",
                snapshot.threat_feed_last_update_failed);
      state.Set("askWhereToSaveDownloads", snapshot.ask_where_to_save_downloads);
      state.Set("profiles", base::Value(std::move(profiles)));
      state.Set("profileColors", base::Value(std::move(profile_colors)));
      state.Set("activeProfile", snapshot.active_profile);
      return state;
    }
    case contract::PageKind::kSecurityNotice: {
      if (!guard_) {
        return std::nullopt;
      }
      const WegletSecurityGuard::Notice* notice =
          guard_->PendingNotice(contents);
      if (!notice) {
        return std::nullopt;
      }
      base::DictValue state;
      // Sent as the word the page checks: the level decides whether it
      // offers a way through at all.
      state.Set("level", notice->blocking ? "block" : "warning");
      state.Set("title", notice->title);
      state.Set("reason", notice->reason);
      state.Set("host", notice->host);
      state.Set("target", notice->target.spec());
      return state;
    }
    case contract::PageKind::kMenu:
      // No state of its own -- the language every push carries (see
      // SendTo) is the whole reason this page asks for one at all.
      return base::DictValue();
    case contract::PageKind::kContextMenu: {
      auto found = pending_context_menus_.find(contents);
      if (found == pending_context_menus_.end()) {
        return std::nullopt;
      }
      base::ListValue items;
      for (const PendingContextMenuItem& item : found->second) {
        base::DictValue item_value;
        item_value.Set("id", item.id);
        item_value.Set("enabled", item.enabled);
        if (!item.label.empty()) {
          item_value.Set("label", item.label);
        }
        items.Append(base::Value(std::move(item_value)));
      }
      base::DictValue state;
      state.Set("items", base::Value(std::move(items)));
      return state;
    }
    case contract::PageKind::kFindBar: {
      auto found = pending_find_results_.find(contents);
      if (found == pending_find_results_.end()) {
        return std::nullopt;
      }
      base::DictValue state;
      state.Set("query", found->second.query);
      state.Set("matchCount", found->second.match_count);
      state.Set("activeOrdinal", found->second.active_ordinal);
      return state;
    }
    case contract::PageKind::kPermissionPrompt: {
      auto found = pending_permission_prompts_.find(contents);
      if (found == pending_permission_prompts_.end()) {
        return std::nullopt;
      }
      base::ListValue types;
      for (const std::string& type : found->second.types) {
        types.Append(base::Value(type));
      }
      base::DictValue state;
      state.Set("origin", found->second.origin);
      state.Set("types", base::Value(std::move(types)));
      return state;
    }
    case contract::PageKind::kSiteInfo: {
      std::string url;
      const uint64_t active_id = bridge_->ActiveTabId(entry.window);
      for (const WegletBridge::TabInfo& tab : bridge_->Tabs(entry.window)) {
        if (tab.id == active_id) {
          url = tab.url;
          break;
        }
      }
      const GURL gurl(url);
      const bool is_internal = gurl.SchemeIs("chrome") && gurl.host() == "weglet";
      base::DictValue state;
      state.Set("isInternal", is_internal);
      if (!is_internal) {
        const url::Origin origin = url::Origin::Create(gurl);
        base::ListValue permissions;
        for (const WegletPermissionDelegate::Decision& decision :
            permission_delegate_->DecisionsFor(origin)) {
          base::DictValue item;
          item.Set("id", decision.id);
          item.Set("status", decision.status);
          permissions.Append(base::Value(std::move(item)));
        }
        state.Set("origin", origin.Serialize());
        state.Set("permissions", base::Value(std::move(permissions)));
      }
      return state;
    }
    case contract::PageKind::kSaveShortcut: {
      auto found = pending_shortcuts_.find(contents);
      if (found == pending_shortcuts_.end()) {
        return std::nullopt;
      }
      base::DictValue state;
      state.Set("url", found->second.url);
      state.Set("suggestedName", found->second.suggested_name);
      return state;
    }
    case contract::PageKind::kHistory: {
      base::ListValue search_entries;
      for (const WegletBridge::HistoryEntry& history_entry : bridge_->SearchHistory()) {
        base::DictValue item;
        item.Set("query", history_entry.query);
        item.Set("url", history_entry.url);
        item.Set("visitedAt", static_cast<double>(history_entry.visited_at));
        search_entries.Append(base::Value(std::move(item)));
      }
      base::ListValue downloads;
      for (const WegletBridge::DownloadEntry& download : bridge_->Downloads()) {
        base::DictValue item;
        item.Set("filename", download.filename);
        // 0 = in progress, 1 = completed, 2 = failed -- see
        // weglet_download_status_at.
        item.Set("status", download.status == 0   ? "in-progress"
                            : download.status == 1 ? "complete"
                                                    : "failed");
        item.Set("errorMessage", download.error_message);
        item.Set("sizeLabel", download.size_label);
        item.Set("startedAt", static_cast<double>(download.started_at));
        downloads.Append(base::Value(std::move(item)));
      }
      base::DictValue state;
      state.Set("searchEntries", base::Value(std::move(search_entries)));
      state.Set("downloads", base::Value(std::move(downloads)));
      return state;
    }
    case contract::PageKind::kBookmarks: {
      base::ListValue entries;
      for (const WegletBridge::BookmarkEntry& bookmark : bridge_->Bookmarks()) {
        base::DictValue item;
        item.Set("title", bookmark.title);
        item.Set("url", bookmark.url);
        entries.Append(base::Value(std::move(item)));
      }
      base::DictValue state;
      state.Set("entries", base::Value(std::move(entries)));
      return state;
    }
    case contract::PageKind::kToast: {
      auto found = pending_toasts_.find(contents);
      if (found == pending_toasts_.end()) {
        return std::nullopt;
      }
      base::ListValue items;
      for (const PendingToast& toast : found->second) {
        base::DictValue item_value;
        item_value.Set("id", toast.id);
        item_value.Set("key", toast.key);
        items.Append(base::Value(std::move(item_value)));
      }
      base::DictValue state;
      state.Set("items", base::Value(std::move(items)));
      return state;
    }
    case contract::PageKind::kOmniboxSuggestions: {
      auto found = pending_omnibox_suggestions_.find(contents);
      if (found == pending_omnibox_suggestions_.end()) {
        return std::nullopt;
      }
      base::ListValue items;
      for (const PendingOmniboxSuggestion& suggestion : found->second.items) {
        base::DictValue item_value;
        item_value.Set("label", suggestion.label);
        item_value.Set("url", suggestion.url);
        item_value.Set("isBookmark", suggestion.is_bookmark);
        items.Append(base::Value(std::move(item_value)));
      }
      base::DictValue state;
      state.Set("items", base::Value(std::move(items)));
      state.Set("highlightedIndex", found->second.highlighted_index);
      return state;
    }
    case contract::PageKind::kOther:
      return std::nullopt;
  }
  return std::nullopt;
}

void WegletStateService::SendTo(const Entry& entry) {
  if (!entry.web_ui || !entry.web_ui->CanCallJavascript()) {
    return;
  }
  const std::string_view function = contract::PushFunctionFor(entry.kind);
  if (function.empty()) {
    return;
  }
  std::optional<base::DictValue> state =
      BuildFor(entry, entry.web_ui->GetWebContents());
  if (!state.has_value()) {
    return;
  }
  // Carried on every push -- see the kLanguage comment on Change -- so
  // no page kind has to ask for these separately.
  state->Set("language", bridge_->Language());
  state->Set("faviconsEnabled", bridge_->FaviconsEnabled());
  state->Set("accentColor", bridge_->AccentColor());
  state->Set("addressBarShape", bridge_->AddressBarShape());
  // The name comes from the generated contract, which the page's own
  // TypeScript installs from.
  entry.web_ui->CallJavascriptFunctionUnsafe(
      function, base::Value(std::move(*state)));
}

void WegletStateService::PushTo(content::WebUI* web_ui) {
  for (const Entry& entry : pages_) {
    if (entry.web_ui == web_ui) {
      SendTo(entry);
      return;
    }
  }
}

void WegletStateService::Notify(uint32_t changes) {
  if (changes == kNone) {
    return;
  }
  pending_ |= changes;
  if (flush_scheduled_) {
    return;
  }
  flush_scheduled_ = true;
  // Coalesced to the end of the current task: one navigation fires
  // DidStartLoading, DidFinishNavigation, TitleWasSet and DidStopLoading.
  // WeakPtr because the profile can go away between the post and the run.
  base::SingleThreadTaskRunner::GetCurrentDefault()->PostTask(
      FROM_HERE, base::BindOnce(&WegletStateService::FlushNow,
                                weak_factory_.GetWeakPtr()));
}

void WegletStateService::FlushNow() {
  flush_scheduled_ = false;
  const uint32_t changes = pending_;
  pending_ = kNone;
  if (changes == kNone) {
    return;
  }
  // Copied: sending calls into a renderer, which can destroy a page and
  // mutate pages_ underneath the loop.
  const std::vector<Entry> snapshot = pages_;
  for (const Entry& entry : snapshot) {
    if (!Affects(changes, entry.kind)) {
      continue;
    }
    // SendTo checks the WebUI, but the entry may have been removed by an
    // earlier iteration.
    const bool present = std::any_of(
        pages_.begin(), pages_.end(), [&entry](const Entry& live) {
          return live.web_ui == entry.web_ui;
        });
    if (present) {
      SendTo(entry);
    }
  }
}

}  // namespace weglet