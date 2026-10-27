// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Tracks the open pages and pushes state to the ones a change affects.

#ifndef WEGLET_BROWSER_WEGLET_STATE_SERVICE_H_
#define WEGLET_BROWSER_WEGLET_STATE_SERVICE_H_

#include <cstdint>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <vector>

#include "base/memory/raw_ptr.h"
#include "base/memory/weak_ptr.h"
#include "base/supports_user_data.h"
#include "base/values.h"
#include "weglet/ui/generated_contract.h"

namespace content {
class BrowserContext;
class WebContents;
class WebUI;
}  // namespace content

class WindowsSpellChecker;

namespace weglet {

class WegletBridge;
class WegletPermissionDelegate;
class WegletSecurityGuard;

// Who tells the pages that something changed. Attached to the
// BrowserContext, not a window: the pages outlive any one window. A
// page says what kind it is when it registers, so nothing re-derives it
// from the URL per push. Changes are coalesced into one flush per turn
// of the message loop and reach only the pages that care --
// DidStartLoading, DidStopLoading and TitleWasSet fire constantly.
class WegletStateService : public base::SupportsUserData::Data {
 public:
  // What changed. A bitmask: several changes between two flushes arrive
  // as one.
  enum Change : uint32_t {
    kNone = 0,
    // Which tabs exist, which is active, where each one is.
    kTabs = 1 << 0,
    // The new tab page's dock.
    kShortcuts = 1 << 1,
    // Anything in the profile's settings.
    kSettings = 1 << 2,
    kBookmarks = 1 << 3,
    // Search history and downloads: one push, since the History page
    // shows both as tabs of the same panel.
    kHistory = 1 << 4,
    // Not page-specific like the flags above: every page's push carries
    // the current language (see SendTo), so a change reaches every page
    // showing right now, not just Settings.
    kLanguage = 1 << 5,
    // Same reasoning as kLanguage: whether favicons may load.
    kFavicons = 1 << 6,
    // Same reasoning as kLanguage: the accent colour and address bar
    // shape (see applyTheme in theme.ts).
    kTheme = 1 << 7,
    // A find-in-page reply arrived for the tab the find bar is open on.
    kFindResult = 1 << 8,
  };

  // Creates the service and hands it to `browser_context`, which owns it.
  // `bridge` must outlive the context.
  static void CreateForBrowserContext(content::BrowserContext* browser_context,
                                      WegletBridge* bridge,
                                      WegletSecurityGuard* guard,
                                      WegletPermissionDelegate* permission_delegate,
                                      WindowsSpellChecker* spell_checker);
  // Null if nothing was created for this context.
  static WegletStateService* FromBrowserContext(
      content::BrowserContext* browser_context);

  WegletStateService(WegletBridge* bridge, WegletSecurityGuard* guard,
                     WegletPermissionDelegate* permission_delegate,
                     WindowsSpellChecker* spell_checker);
  WegletStateService(const WegletStateService&) = delete;
  WegletStateService& operator=(const WegletStateService&) = delete;
  ~WegletStateService() override;

  // Called by a page's WebUI controller when it is created and destroyed.
  // `kind` is decided once, from the URL. `window` is which window's tabs
  // this page shows, and is meaningful only for the toolbar.
  void AddPage(content::WebUI* web_ui,
               contract::PageKind kind,
               uint64_t window = 0);
  void RemovePage(content::WebUI* web_ui);

  // Asks this window's toolbar to put the caret in the address bar.
  // Focus lives in the DOM, so Ctrl+L is a one-shot flag on the
  // toolbar's next state, cleared by the push that carries it.
  void RequestOmniboxFocus(uint64_t window);

  // Sends this page its state now, for one that just registered or asked
  // again after a reload.
  void PushTo(content::WebUI* web_ui);

  // Records what changed and schedules one flush. Cheap to call often.
  void Notify(uint32_t changes);

  // The model these pages show, so a controller can hand it to the
  // message handler.
  WegletBridge* bridge() { return bridge_; }
  // For the toolbar's site-info popup -- see WegletWindow::SetPermissionDecision.
  WegletPermissionDelegate* permission_delegate() { return permission_delegate_; }
  // For fetching real replacement words for a misspelling -- see
  // WegletWindow::HandleContextMenu.
  WindowsSpellChecker* spell_checker() { return spell_checker_; }

  // Sends every pending change immediately, for a caller that needs it on
  // screen before it returns.
  void FlushNow();

  // For a callback that outlives the turn it was posted from -- an async
  // fetch's completion, say -- and must not touch this after the
  // BrowserContext (and this with it) is gone.
  base::WeakPtr<WegletStateService> GetWeakPtr() {
    return weak_factory_.GetWeakPtr();
  }

  // What the save-shortcut popup is open for -- remembered here, same
  // as a pending security notice, since the page asks for it as soon
  // as its script runs.
  struct PendingShortcut {
    std::string url;
    std::string suggested_name;
  };
  void SetPendingShortcut(content::WebContents* contents, PendingShortcut shortcut);
  void ClearPendingShortcut(content::WebContents* contents);

  // Same reasoning as PendingShortcut: which items the context menu
  // popup is open with, decided by WegletWindow from where the click
  // landed.
  struct PendingContextMenuItem {
    std::string id;
    bool enabled = true;
    // Empty means the page looks the label up by id in its own i18n table
    // (the usual case). Set for items whose text isn't a fixed string --
    // a spelling suggestion's own word, for instance.
    std::string label;
  };
  void SetPendingContextMenu(content::WebContents* contents,
                            std::vector<PendingContextMenuItem> items);
  void ClearPendingContextMenu(content::WebContents* contents);

  // The find bar's own pending state: the last reply for the tab it is
  // open on. Same reasoning as PendingShortcut -- asked for as soon as
  // the bar's page runs, and again on every reply.
  struct PendingFindResult {
    std::string query;
    int match_count = 0;
    int active_ordinal = 0;
  };
  void SetPendingFindResult(content::WebContents* contents, PendingFindResult result);
  void ClearPendingFindResult(content::WebContents* contents);

  // The permission prompt's own pending state: same reasoning as
  // PendingShortcut -- asked for as soon as the popup's page runs.
  struct PendingPermissionPrompt {
    std::string origin;
    std::vector<std::string> types;
  };
  void SetPendingPermissionPrompt(content::WebContents* contents,
                                  PendingPermissionPrompt prompt);
  void ClearPendingPermissionPrompt(content::WebContents* contents);

  // The toast popup's own pending state: up to WegletWindow's own cap
  // (2) of {id, i18n key} pairs, oldest first. Same reasoning as
  // PendingShortcut.
  struct PendingToast {
    int id = 0;
    std::string key;
  };
  void SetPendingToasts(content::WebContents* contents, std::vector<PendingToast> toasts);
  void ClearPendingToasts(content::WebContents* contents);

  // The address bar's dropdown: bookmarks and history matching what is
  // typed, ranked by WegletBridge::QueryOmniboxSuggestions, plus which
  // one (if any) is highlighted for Enter/arrow-key selection.
  struct PendingOmniboxSuggestion {
    std::string label;
    std::string url;
    bool is_bookmark = false;
  };
  struct PendingOmniboxSuggestions {
    std::vector<PendingOmniboxSuggestion> items;
    int highlighted_index = -1;
  };
  void SetPendingOmniboxSuggestions(content::WebContents* contents,
                                    PendingOmniboxSuggestions suggestions);
  void ClearPendingOmniboxSuggestions(content::WebContents* contents);

 private:
  struct Entry {
    // Owned by the WebContents. Removed in RemovePage before it goes.
    raw_ptr<content::WebUI> web_ui = nullptr;
    contract::PageKind kind = contract::PageKind::kOther;
    uint64_t window = 0;
  };

  // True when a page of this kind shows anything affected by `changes`.
  static bool Affects(uint32_t changes, contract::PageKind kind);

  // The payload for a page of this kind, or nullopt. `contents` matters
  // only to the security notice. Not const: building the toolbar's state
  // consumes a pending focus request.
  std::optional<base::DictValue> BuildFor(const Entry& entry,
                                          content::WebContents* contents);

  void SendTo(const Entry& entry);

  // Windows whose toolbar should take focus on its next push.
  std::set<uint64_t> omnibox_focus_pending_;

  // At most one entry in practice -- one popup open at a time -- but
  // keyed by contents like PendingNotice, not assumed.
  std::map<content::WebContents*, PendingShortcut> pending_shortcuts_;
  std::map<content::WebContents*, std::vector<PendingContextMenuItem>>
      pending_context_menus_;
  std::map<content::WebContents*, PendingFindResult> pending_find_results_;
  std::map<content::WebContents*, PendingPermissionPrompt> pending_permission_prompts_;
  std::map<content::WebContents*, std::vector<PendingToast>> pending_toasts_;
  std::map<content::WebContents*, PendingOmniboxSuggestions> pending_omnibox_suggestions_;

  const raw_ptr<WegletBridge> bridge_;
  const raw_ptr<WegletSecurityGuard> guard_;
  const raw_ptr<WegletPermissionDelegate> permission_delegate_;
  const raw_ptr<WindowsSpellChecker> spell_checker_;

  std::vector<Entry> pages_;

  // What has changed since the last flush, and whether one is scheduled.
  uint32_t pending_ = kNone;
  bool flush_scheduled_ = false;

  base::WeakPtrFactory<WegletStateService> weak_factory_{this};
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_STATE_SERVICE_H_
