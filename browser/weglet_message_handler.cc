// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Receives and validates the messages a page sends with chrome.send().

#include "weglet/browser/weglet_message_handler.h"

#include "base/files/file_path.h"
#include "base/functional/bind.h"
#include "base/functional/callback_helpers.h"
#include "base/logging.h"
#include "base/no_destructor.h"
#include "base/strings/string_number_conversions.h"
#include "base/task/thread_pool.h"
#include "base/time/time.h"
#include "build/build_config.h"
#include "content/public/browser/browser_context.h"
#include "content/public/browser/browsing_data_remover.h"
#include "content/public/browser/storage_partition.h"
#include "content/public/browser/web_contents.h"
#include "content/public/browser/web_ui.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_state_service.h"
#include "weglet/browser/weglet_window.h"

#if BUILDFLAG(IS_WIN)
#include "ui/base/win/shell.h"
#endif

namespace weglet {
namespace {

// Read only after Validate has passed.
const std::string& EmptyString() {
  static const base::NoDestructor<std::string> empty;
  return *empty;
}

// The shell calls below block, so they run on a thread that allows it
// rather than the UI thread this handler itself runs on.
void RevealInFolder(const base::FilePath& path) {
#if BUILDFLAG(IS_WIN)
  base::ThreadPool::PostTask(
      FROM_HERE, {base::MayBlock()},
      base::BindOnce(base::IgnoreResult(&ui::win::OpenFolderViaShell),
                     path.DirName()));
#else
  LOG(WARNING) << "revealDownload: no shell integration on this platform";
#endif
}

void OpenWithDefaultApp(const base::FilePath& path) {
#if BUILDFLAG(IS_WIN)
  base::ThreadPool::PostTask(
      FROM_HERE, {base::MayBlock()},
      base::BindOnce(base::IgnoreResult(&ui::win::OpenFileViaShell), path));
#else
  LOG(WARNING) << "openDownload: no shell integration on this platform";
#endif
}

}  // namespace

WegletMessageHandler::WegletMessageHandler(WegletWindow* window,
                                          WegletBridge* bridge,
                                          WegletStateService* state)
    : window_(window), bridge_(bridge), state_(state) {}

WegletMessageHandler::~WegletMessageHandler() = default;

void WegletMessageHandler::RegisterMessages() {
  // content crashes the browser process on an unregistered chrome.send(),
  // so every name in the contract needs a callback here.
  for (const contract::MessageSpec& spec : contract::kMessages) {
    web_ui()->RegisterMessageCallback(
        spec.name, base::BindRepeating(&WegletMessageHandler::OnMessage,
                                       base::Unretained(this), &spec));
  }
}

// static
bool WegletMessageHandler::Validate(const contract::MessageSpec& spec,
                                    const base::ListValue& args) {
  if (args.size() != spec.arity) {
    LOG(WARNING) << spec.name << " sent " << args.size()
                 << " arguments, expected " << spec.arity << " -- ignored";
    return false;
  }
  for (size_t index = 0; index < spec.arity; ++index) {
    const base::Value& value = args[index];
    bool ok = false;
    switch (spec.args[index]) {
      case contract::ArgKind::kString:
        ok = value.GetIfString() != nullptr;
        break;
      case contract::ArgKind::kNumber:
        // Every number in the contract is a position in a list the page
        // was shown.
        ok = value.GetIfInt().value_or(-1) >= 0;
        break;
      case contract::ArgKind::kBoolean:
        ok = value.GetIfBool().has_value();
        break;
      case contract::ArgKind::kTabId: {
        // Ids cross as strings: a JavaScript number is a double and
        // cannot hold every u64 exactly.
        const std::string* text = value.GetIfString();
        uint64_t ignored = 0;
        ok = text && base::StringToUint64(*text, &ignored);
        break;
      }
    }
    if (!ok) {
      LOG(WARNING) << spec.name << " argument " << index
                   << " is not the type the contract declares -- ignored";
      return false;
    }
  }
  return true;
}

void WegletMessageHandler::OnMessage(const contract::MessageSpec* spec,
                                     const base::ListValue& args) {
  if (!Validate(*spec, args)) {
    return;
  }
  if (!spec->implemented) {
    // Registered, not acted on. contract.json says which messages these
    // are.
    LOG(WARNING) << "chrome.send(\"" << spec->name
                 << "\") is registered but not implemented yet";
    return;
  }
  Dispatch(spec->name, args);
}

// static
const std::string& WegletMessageHandler::StringAt(const base::ListValue& args,
                                                  size_t index) {
  const std::string* value = args[index].GetIfString();
  return value ? *value : EmptyString();
}

// static
uint64_t WegletMessageHandler::TabIdAt(const base::ListValue& args,
                                       size_t index) {
  uint64_t id = 0;
  base::StringToUint64(StringAt(args, index), &id);
  return id;
}

// static
size_t WegletMessageHandler::IndexAt(const base::ListValue& args,
                                     size_t index) {
  return static_cast<size_t>(args[index].GetIfInt().value_or(0));
}

// static
bool WegletMessageHandler::BoolAt(const base::ListValue& args, size_t index) {
  return args[index].GetIfBool().value_or(false);
}

void WegletMessageHandler::Dispatch(std::string_view name,
                                    const base::ListValue& args) {
  // A page asking for a repaint after its own reload, or because it just
  // registered. Answered directly: the page is waiting for it.
  if (name == "requestState") {
    if (state_) {
      state_->PushTo(web_ui());
    }
    return;
  }

  if (name == "addShortcut") {
    if (bridge_->AddShortcut(StringAt(args, 0), StringAt(args, 1))) {
      state_->Notify(WegletStateService::kShortcuts);
    }
    return;
  }
  if (name == "editShortcut") {
    if (bridge_->EditShortcut(IndexAt(args, 0), StringAt(args, 1),
                              StringAt(args, 2))) {
      state_->Notify(WegletStateService::kShortcuts);
    }
    return;
  }
  if (name == "removeShortcut") {
    if (bridge_->RemoveShortcut(IndexAt(args, 0))) {
      state_->Notify(WegletStateService::kShortcuts);
    }
    return;
  }
  if (name == "setSearchEngine") {
    if (bridge_->SetSearchEngine(StringAt(args, 0))) {
      state_->Notify(WegletStateService::kSettings);
    }
    return;
  }
  if (name == "setCustomSearchUrl") {
    bridge_->SetCustomSearchUrl(StringAt(args, 0));
    state_->Notify(WegletStateService::kSettings);
    return;
  }
  if (name == "setRestoreSession") {
    bridge_->SetRestoreSession(BoolAt(args, 0));
    state_->Notify(WegletStateService::kSettings);
    return;
  }
  if (name == "setAccentColor") {
    if (bridge_->SetAccentColor(StringAt(args, 0))) {
      state_->Notify(WegletStateService::kTheme);
    }
    return;
  }
  if (name == "setAddressBarShape") {
    if (bridge_->SetAddressBarShape(StringAt(args, 0))) {
      state_->Notify(WegletStateService::kTheme);
    }
    return;
  }
  if (name == "setLanguage") {
    if (bridge_->SetLanguage(StringAt(args, 0))) {
      state_->Notify(WegletStateService::kLanguage);
    }
    return;
  }
  if (name == "blockHost") {
    if (bridge_->BlockHost(StringAt(args, 0))) {
      state_->Notify(WegletStateService::kSettings);
    }
    return;
  }
  if (name == "unblockHost") {
    if (bridge_->UnblockHost(IndexAt(args, 0))) {
      state_->Notify(WegletStateService::kSettings);
    }
    return;
  }
  if (name == "reorderTab") {
    if (bridge_->ReorderTab(TabIdAt(args, 0), IndexAt(args, 1))) {
      state_->Notify(WegletStateService::kTabs);
    }
    return;
  }
  if (name == "removeBookmark") {
    if (bridge_->RemoveBookmark(IndexAt(args, 0))) {
      state_->Notify(WegletStateService::kBookmarks);
    }
    return;
  }
  if (name == "clearSearchHistory") {
    bridge_->ClearSearchHistory();
    state_->Notify(WegletStateService::kHistory);
    return;
  }
  if (name == "clearDownloadHistory") {
    bridge_->ClearDownloadHistory();
    state_->Notify(WegletStateService::kHistory);
    return;
  }
  if (name == "revealDownload") {
    const std::vector<WegletBridge::DownloadEntry> downloads = bridge_->Downloads();
    const size_t index = IndexAt(args, 0);
    if (index < downloads.size()) {
      RevealInFolder(base::FilePath::FromUTF8Unsafe(downloads[index].path));
    }
    return;
  }
  if (name == "openDownload") {
    const std::vector<WegletBridge::DownloadEntry> downloads = bridge_->Downloads();
    const size_t index = IndexAt(args, 0);
    if (index < downloads.size()) {
      OpenWithDefaultApp(base::FilePath::FromUTF8Unsafe(downloads[index].path));
    }
    return;
  }
  if (name == "setThreatFeedEnabled") {
    bridge_->SetThreatFeedEnabled(BoolAt(args, 0));
    state_->Notify(WegletStateService::kSettings);
    return;
  }
  if (name == "setFaviconsEnabled") {
    bridge_->SetFaviconsEnabled(BoolAt(args, 0));
    state_->Notify(WegletStateService::kFavicons);
    return;
  }
  if (name == "setAskWhereToSaveDownloads") {
    bridge_->SetAskWhereToSaveDownloads(BoolAt(args, 0));
    state_->Notify(WegletStateService::kSettings);
    return;
  }
  if (name == "switchProfile") {
    if (window_) {
      window_->RequestProfileSwitch(StringAt(args, 0));
    }
    return;
  }
  if (name == "openProfileWindow") {
    if (window_) {
      window_->RequestOpenProfileWindow(StringAt(args, 0));
    }
    return;
  }
  if (name == "renameProfile") {
    const std::string old_name = StringAt(args, 0);
    const std::string new_name = StringAt(args, 1);
    if (old_name != bridge_->CurrentProfile() &&
        WegletBridge::RenameProfile(old_name, new_name)) {
      state_->Notify(WegletStateService::kSettings);
    }
    return;
  }
  if (name == "deleteProfile") {
    const std::string profile = StringAt(args, 0);
    if (profile != bridge_->CurrentProfile() && WegletBridge::DeleteProfile(profile)) {
      state_->Notify(WegletStateService::kSettings);
    }
    return;
  }
  if (name == "createProfileShortcut") {
    if (window_) {
      window_->CreateProfileShortcut(StringAt(args, 0));
    }
    return;
  }
  if (name == "refreshThreatFeed") {
    content::BrowserContext* browser_context =
        web_ui()->GetWebContents()->GetBrowserContext();
    bridge_->RefreshThreatFeed(
        browser_context->GetDefaultStoragePartition()
            ->GetURLLoaderFactoryForBrowserProcess(),
        base::BindOnce(
            [](base::WeakPtr<WegletStateService> state, bool ok) {
              // The result itself is not carried in the push: the
              // settings snapshot already has updatedAt/lastUpdateFailed,
              // which is the same answer with a timestamp attached.
              if (state) {
                state->Notify(WegletStateService::kSettings);
              }
            },
            state_->GetWeakPtr()));
    return;
  }
  if (name == "clearBrowsingData") {
    bridge_->ClearSearchHistory();
    bridge_->ClearDownloadHistory();
    content::BrowserContext* browser_context =
        web_ui()->GetWebContents()->GetBrowserContext();
    browser_context->GetBrowsingDataRemover()->Remove(
        base::Time(), base::Time::Max(),
        content::BrowsingDataRemover::DATA_TYPE_COOKIES |
            content::BrowsingDataRemover::DATA_TYPE_CACHE,
        content::BrowsingDataRemover::ORIGIN_TYPE_UNPROTECTED_WEB);
    state_->Notify(WegletStateService::kHistory);
    return;
  }

  // --- tabs, navigation and the notice -------------------------------
  //
  // A page open outside a window has no window to ask.
  if (!window_) {
    LOG(WARNING) << name << " needs a window and this page is not in one";
    return;
  }
  if (name == "toggleBookmark") {
    const uint64_t active_id = bridge_->ActiveTabId(window_->window_id());
    for (const WegletBridge::TabInfo& tab : bridge_->Tabs(window_->window_id())) {
      if (tab.id == active_id) {
        bridge_->ToggleBookmark(tab.label, tab.url);
        // Both: the toolbar's star lives in the tabs push, the list on
        // the bookmarks page lives in its own.
        state_->Notify(WegletStateService::kTabs | WegletStateService::kBookmarks);
        break;
      }
    }
    return;
  }
  if (name == "navigate") {
    window_->NavigateActiveTabFromOmnibox(StringAt(args, 0));
  } else if (name == "queryOmniboxSuggestions") {
    window_->QueryOmniboxSuggestions(StringAt(args, 0),
                                     static_cast<int>(IndexAt(args, 1)),
                                     static_cast<int>(IndexAt(args, 2)));
  } else if (name == "hideOmniboxSuggestions") {
    window_->HideOmniboxSuggestions();
  } else if (name == "moveOmniboxHighlightNext") {
    window_->MoveOmniboxHighlight(/*forward=*/true);
  } else if (name == "moveOmniboxHighlightPrevious") {
    window_->MoveOmniboxHighlight(/*forward=*/false);
  } else if (name == "acceptOmniboxSuggestion") {
    window_->AcceptOmniboxSuggestion(static_cast<int>(IndexAt(args, 0)));
  } else if (name == "goBack") {
    window_->GoBack();
  } else if (name == "goForward") {
    window_->GoForward();
  } else if (name == "reload") {
    window_->Reload();
  } else if (name == "newTab") {
    window_->OpenNewTab();
  } else if (name == "minimizeWindow") {
    window_->MinimizeWindow();
  } else if (name == "toggleMaximizeWindow") {
    window_->ToggleMaximizeWindow();
  } else if (name == "closeWindow") {
    window_->CloseWindow();
  } else if (name == "closeTab") {
    window_->CloseTab(TabIdAt(args, 0));
  } else if (name == "activateTab") {
    window_->ActivateTab(TabIdAt(args, 0));
  } else if (name == "toggleMenu") {
    window_->ToggleMenu();
  } else if (name == "hideMenu") {
    window_->HideMenu();
  } else if (name == "securityDismiss") {
    window_->DismissSecurityNotice(web_ui()->GetWebContents());
  } else if (name == "securityProceed") {
    window_->ProceedPastSecurityNotice(web_ui()->GetWebContents());
  } else if (name == "openSaveShortcutPopup") {
    window_->OpenSaveShortcutPopup(static_cast<int>(IndexAt(args, 0)),
                                   static_cast<int>(IndexAt(args, 1)));
  } else if (name == "hideSaveShortcutPopup") {
    window_->HideSaveShortcutPopup();
  } else if (name == "contextMenuAction") {
    window_->HandleContextMenuAction(StringAt(args, 0));
  } else if (name == "hideContextMenu") {
    window_->HideContextMenu();
  } else if (name == "findInPage") {
    window_->FindInPage(StringAt(args, 0));
  } else if (name == "findNext") {
    window_->FindNext(true);
  } else if (name == "findPrevious") {
    window_->FindNext(false);
  } else if (name == "closeFindBar") {
    window_->HideFindBar();
  } else if (name == "answerPermissionPrompt") {
    window_->AnswerPermissionPrompt(BoolAt(args, 0));
  } else if (name == "toggleSiteInfo") {
    window_->ToggleSiteInfo(static_cast<int>(IndexAt(args, 0)),
                           static_cast<int>(IndexAt(args, 1)));
  } else if (name == "hideSiteInfo") {
    window_->HideSiteInfo();
  } else if (name == "setPermissionDecision") {
    window_->SetPermissionDecision(StringAt(args, 0), BoolAt(args, 1));
  } else if (name == "showToast") {
    window_->ShowToast(StringAt(args, 0));
  } else if (name == "dismissToast") {
    window_->DismissToast(static_cast<int>(IndexAt(args, 0)));
  } else {
    // Reachable only if contract.json gained a message marked implemented
    // with nothing added here. The page will look like it did nothing.
    LOG(ERROR) << name
               << " is marked implemented in contract.json but has no case "
                  "in WegletMessageHandler::Dispatch";
  }
}

}  // namespace weglet
