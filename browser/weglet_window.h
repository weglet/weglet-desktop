// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// A browser window: the widget, its toolbar, its tabs, its accelerators.

#ifndef WEGLET_BROWSER_WEGLET_WINDOW_H_
#define WEGLET_BROWSER_WEGLET_WINDOW_H_

#include <cstdint>
#include <map>
#include <memory>
#include <string>
#include <vector>

#include "base/functional/callback.h"
#include "base/memory/raw_ptr.h"
#include "base/memory/scoped_refptr.h"
#include "base/memory/weak_ptr.h"
#include "content/public/browser/file_select_listener.h"
#include "content/public/browser/media_stream_request.h"
#include "content/public/browser/web_contents_delegate.h"
#include "content/public/common/page_zoom.h"
#include "third_party/blink/public/mojom/choosers/file_chooser.mojom-forward.h"
#include "ui/base/accelerators/accelerator.h"
#include "ui/base/window_open_disposition.h"
#include "ui/gfx/geometry/rect.h"
#include "ui/views/controls/webview/unhandled_keyboard_event_handler.h"
#include "ui/views/widget/widget_delegate.h"
#include "url/gurl.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_state_service.h"

namespace content {
class BrowserContext;
struct ContextMenuParams;
class RenderFrameHost;
class WebContents;
}  // namespace content

namespace views {
class FrameView;
class View;
class WebView;
class Widget;
}  // namespace views

namespace weglet {

class WegletBridge;
class WegletContextMenu;
class WegletFindBar;
class WegletFrameView;
class WegletPermissionPrompt;
class WegletSecurityGuard;
class WegletMenuPopup;
class WegletShortcutPopup;
class WegletSiteInfoPopup;
class WegletStateService;
class WegletTabObserver;
class WegletToastPopup;
class WegletOmniboxSuggestionsPopup;

// One browser window: a toolbar above, the active tab's page below. The
// toolbar is a WebContents showing chrome://weglet/toolbar.html, not
// native views, so the whole interface shares one design system.
//
// Not a views::WidgetDelegateView: being delegate and view at once makes
// ownership ambiguous. The delegate is this object; the contents view is
// a plain views::View it hands over.
class WegletWindow : public views::WidgetDelegate,
                     public content::WebContentsDelegate,
                     public ui::AcceleratorTarget {
 public:
  // The window a WebContents belongs to, or nullptr.
  static WegletWindow* FromWebContents(content::WebContents* contents);

  // Creates a window for a `window_id` the model already knows about,
  // shows it, and opens the tabs the model says are in it.
  static void CreateAndShow(content::BrowserContext* browser_context,
                            WegletBridge* bridge,
                            uint64_t window_id);

  // Opens a window the model does not have yet. Null when the window
  // ceiling is reached.
  static void OpenNewWindow(content::BrowserContext* browser_context,
                            WegletBridge* bridge);

  uint64_t window_id() const { return window_id_; }
  content::BrowserContext* browser_context() const { return browser_context_; }

  static int window_count();
  static void SetQuitClosure(base::OnceClosure quit);

  // Closes every open window -- see RequestProfileSwitch. Each Close()
  // is asynchronous, so this returns before window_count() reaches 0.
  static void CloseAllWindows();

  // Set once by WegletBrowserMainParts, which owns the WegletBridge a
  // clean switch has to check the new name against and flush before
  // handing off.
  static void SetProfileSwitchHandler(
      base::RepeatingCallback<void(const std::string&)> handler);
  // Forwards to the handler above, if one was set. A no-op otherwise --
  // there is no profile-switch handler outside a real browser process.
  void RequestProfileSwitch(const std::string& profile_name);

  // Set once by WegletBrowserMainParts. Opens a new window into
  // `profile_name` in this same process, building its context and
  // bridge first if this is the first window into it this run.
  static void SetOpenProfileWindowHandler(
      base::RepeatingCallback<void(const std::string&)> handler);
  void RequestOpenProfileWindow(const std::string& profile_name);

  // Writes a desktop .lnk that launches this exe with
  // --profile=<profile_name>, off the UI thread -- shortcut creation is
  // blocking COM/file work, which the UI thread disallows post-startup.
  // Reports the result as a toast once it's done.
  void CreateProfileShortcut(const std::string& profile_name);

  WegletWindow(const WegletWindow&) = delete;
  WegletWindow& operator=(const WegletWindow&) = delete;
  ~WegletWindow() override;

  // Called by a page's message handler, for the things that are a
  // window's business: which tab is in front and where it goes.
  void NavigateActiveTabFromOmnibox(const std::string& input);

  // The address bar's dropdown. `anchor_left`/`anchor_bottom` are the
  // address bar's own on-screen rect, like OpenSaveShortcutPopup's.
  void QueryOmniboxSuggestions(const std::string& input, int anchor_left, int anchor_bottom);
  void HideOmniboxSuggestions();
  void MoveOmniboxHighlight(bool forward);
  void AcceptOmniboxSuggestion(int index);

  void GoBack();
  void GoForward();
  void Reload();
  void OpenNewTab();
  void CloseTab(uint64_t id);
  void ActivateTab(uint64_t id);

  // The toolbar's own minimize/maximize/close buttons. There is no OS
  // titlebar to do this instead -- see Init's remove_standard_frame.
  void MinimizeWindow();
  void ToggleMaximizeWindow();
  void CloseWindow();

  // Ctrl+/Ctrl-/Ctrl+0, on whichever tab is in front.
  void ZoomActiveTab(content::PageZoom zoom);

  // Puts the notice page in the tab whose navigation was stopped. The
  // throttle has already recorded why with the guard.
  void ShowSecurityNotice(content::WebContents* contents);

  // The notice's two answers. Proceed refuses outright on a hard block --
  // decided here, not by a page running in a renderer.
  void DismissSecurityNotice(content::WebContents* contents);
  void ProceedPastSecurityNotice(content::WebContents* contents);

  // The "keep" button's on-screen rect, in the toolbar page's coordinates --
  // its only way to say where it put the button. Prefills from the active tab.
  void OpenSaveShortcutPopup(int anchor_right, int anchor_bottom);
  void HideSaveShortcutPopup();

  // The toolbar's "⋮" button. Anchored to the window's top-right corner,
  // not the button's rect -- unlike the save-shortcut popup, there's only
  // ever one place this can open.
  void ToggleMenu();
  void HideMenu();

  // The right-click menu's action buttons, and losing focus/Escape.
  void HandleContextMenuAction(const std::string& id);
  void HideContextMenu();

  // Ctrl+F. Unlike ToggleMenu, a second Ctrl+F while already open just
  // refocuses the field rather than closing it -- see WegletFindBar.
  void OpenFindBar();
  void HideFindBar();
  void FindInPage(const std::string& query);
  void FindNext(bool forward);

  // F12 / Ctrl+Shift+I: opens DevTools for the active tab as a new tab of
  // its own. `CloseDevToolsTab` is WegletDevToolsBindings' way of telling
  // us the inspected page's agent host went away.
  // `inspect_x`/`inspect_y` jump straight to the element under those
  // frame-local coordinates once attached; -1/-1 (the default) opens on
  // whatever panel was last open instead.
  void OpenDevTools(int inspect_x = -1, int inspect_y = -1);
  void CloseDevToolsTab(content::WebContents* devtools_contents);

  // Ctrl+S and Ctrl+U, on the active tab.
  void SavePage();
  void ViewSource();

  // A camera/mic/location/notifications request -- see
  // WegletPermissionDelegate. `types` are ids like "camera"; `callback`
  // runs with true for Allow, false for Block.
  void ShowPermissionPrompt(const std::string& origin,
                            std::vector<std::string> types,
                            base::OnceCallback<void(bool)> callback);
  void AnswerPermissionPrompt(bool allow);

  // The toolbar's site-info button, and a permission changed from its
  // dropdown.
  void ToggleSiteInfo(int anchor_right, int anchor_bottom);
  void HideSiteInfo();

  // A brief notice under the address bar -- link copied, bookmark
  // added/removed. `text_key` is an i18n key, resolved by toast.ts.
  void ShowToast(const std::string& text_key);
  // The toast's own close button, or its 5s on-page timer -- see toast.ts.
  void DismissToast(int id);
  void SetPermissionDecision(const std::string& id, bool allow);

  // Gives `contents` a channel to this window, same as every tab and the
  // toolbar get. For a page that is neither: the shortcut popup.
  void RegisterWebContents(content::WebContents* contents);

  // Called by the per-tab observer when the engine reports a change.
  void OnTabNavigated(uint64_t id, const GURL& url, bool same_document);
  void OnTabTitleChanged(uint64_t id, const std::string& title);
  void OnTabLoadingChanged(uint64_t id, bool loading);

  // ui::AcceleratorTarget:
  bool AcceleratorPressed(const ui::Accelerator& accelerator) override;
  bool CanHandleAccelerators() const override;

  // views::WidgetDelegate:
  views::Widget* GetWidget() override;
  const views::Widget* GetWidget() const override;
  // No OS caption or border: WegletFrameView answers hit-testing for
  // the whole window instead. See Init's remove_standard_frame.
  std::unique_ptr<views::FrameView> CreateFrameView(
      views::Widget* widget) override;

  // content::WebContentsDelegate:
  void CloseContents(content::WebContents* source) override;
  // A link or a navigation the page itself cannot start: in the tab it came
  // from, or in a new one, as `params.disposition` says.
  content::WebContents* OpenURLFromTab(
      content::WebContents* source,
      const content::OpenURLParams& params,
      base::OnceCallback<void(content::NavigationHandle&)>
          navigation_handle_callback) override;
  // window.open: content has already created `new_contents`, with its
  // opener, and this makes it a tab.
  content::WebContents* AddNewContents(
      content::WebContents* source,
      std::unique_ptr<content::WebContents> new_contents,
      const GURL& target_url,
      WindowOpenDisposition disposition,
      const blink::mojom::WindowFeatures& window_features,
      bool user_gesture,
      bool* was_blocked) override;
  bool HandleContextMenu(content::RenderFrameHost& render_frame_host,
                         const content::ContextMenuParams& params) override;
  // A key the active WebContents (toolbar or tab) didn't want -- the only
  // way a kNormalPriority accelerator reaches the FocusManager at all,
  // since focus always sits inside one WebContents or the other here.
  bool HandleKeyboardEvent(content::WebContents* source,
                           const input::NativeWebKeyboardEvent& event) override;
  void FindReply(content::WebContents* source,
                 int request_id,
                 int number_of_matches,
                 const gfx::Rect& selection_rect,
                 int active_match_ordinal,
                 bool final_update) override;
  // getUserMedia()'s own path, separate from the generic Permissions API
  // -- see WegletPermissionDelegate::RequestMediaTypes.
  void RequestMediaAccessPermission(content::WebContents* web_contents,
                                    const content::MediaStreamRequest& request,
                                    content::MediaResponseCallback callback) override;
  bool CheckMediaAccessPermission(content::RenderFrameHost* render_frame_host,
                                  const url::Origin& security_origin,
                                  blink::mojom::MediaStreamType type) override;
  void RunFileChooser(content::RenderFrameHost* render_frame_host,
                      scoped_refptr<content::FileSelectListener> listener,
                      const blink::mojom::FileChooserParams& params) override;
  // The toolbar's -webkit-app-region: drag regions -- ignored for any
  // other `contents`, since a tab showing an arbitrary site has no
  // business resizing the window's drag target.
  void DraggableRegionsChanged(
      const std::vector<blink::mojom::DraggableRegionPtr>& regions,
      content::WebContents* contents) override;

 private:
  // Per-tab state. The observer turns engine events into bridge calls and
  // lives exactly as long as the contents it watches.
  struct Tab {
    Tab();
    Tab(Tab&&);
    Tab& operator=(Tab&&);
    ~Tab();

    std::unique_ptr<content::WebContents> contents;
    std::unique_ptr<WegletTabObserver> observer;
  };

  WegletWindow(content::BrowserContext* browser_context,
               WegletBridge* bridge,
               uint64_t window_id);

  void Init();
  void OnWindowClosing();

  // Which tab a WebContents is, or 0 if it is not one of this window's.
  uint64_t TabIdFor(content::WebContents* contents) const;

  // The active tab's WebContents, or nullptr if the model has none.
  content::WebContents* ActiveTabContents() const;

  // The Windows native spellchecker's suggestions arrive asynchronously,
  // well after the context menu is already showing -- see HandleContextMenu.
  // `generation` guards against a reply for a menu that has since closed
  // or moved to a different word.
  void OnSpellingSuggestions(
      int generation,
      const std::vector<std::u16string>& per_language_suggestions_flat);

  // Ctrl+L: focus the toolbar and ask it for the address bar.
  void FocusOmnibox();

  // Registers every shortcut with the focus manager, once the widget
  // exists.
  void RegisterAccelerators();

  // Tells the state service the tab model changed.
  void Notify();

  static void QuitIfLastWindow();

  // Builds a WebContents for `id` and loads the model's URL in it: on
  // startup for the restored active tab, and lazily for the others.
  content::WebContents* EnsureContentsFor(uint64_t id);

  // Makes `contents` the WebContents of tab `id`, without loading anything:
  // the caller, or content itself for window.open, starts the navigation.
  content::WebContents* AttachContents(
      uint64_t id,
      std::unique_ptr<content::WebContents> contents);

  // Opens a tab in the model for `url` and attaches `contents` to it. A
  // background disposition leaves the current tab in front. Null at the
  // tab ceiling, with `contents` dropped.
  content::WebContents* AddTab(const GURL& url,
                               WindowOpenDisposition disposition,
                               std::unique_ptr<content::WebContents> contents);

  // Points the visible area at the active tab's contents, creating them if
  // this is the first time that tab is shown.
  void ShowActiveTab();

  // The URL to load for an address the model holds. "weglet://settings"
  // and the rest never reach the engine.
  static GURL ResolveForEngine(const std::string& address);

  // Shared by NavigateActiveTabFromOmnibox's two paths (typed text and
  // an accepted suggestion): loads an already-resolved URL in the active
  // tab. `resolved` is never re-parsed as a search.
  void NavigateActiveTabToResolvedUrl(const std::string& resolved);

  // (Re)shows omnibox_suggestions_popup_ from omnibox_suggestions_,
  // omnibox_highlighted_ and the remembered anchor. Creates the popup on
  // first use.
  void ShowOmniboxSuggestionsPopup();

  static int window_count_;

  const raw_ptr<content::BrowserContext> browser_context_;
  const raw_ptr<WegletBridge> bridge_;
  // Which window in the model this is. Every tab question is scoped to it.
  const uint64_t window_id_;
  // Owned by the browser context, which outlives every window.
  const raw_ptr<WegletStateService> state_service_;
  // Owned by the browser context, like the state service.
  const raw_ptr<WegletSecurityGuard> security_guard_;

  // CLIENT_OWNS_WIDGET. Declared before the contents it holds so it is
  // torn down first.
  std::unique_ptr<views::Widget> widget_;

  // Declared after widget_: both are parented to it, and have to be torn
  // down first, not last.
  std::unique_ptr<WegletShortcutPopup> shortcut_popup_;
  std::unique_ptr<WegletMenuPopup> menu_popup_;
  std::unique_ptr<WegletContextMenu> context_menu_;
  std::unique_ptr<WegletFindBar> find_bar_;
  std::unique_ptr<WegletPermissionPrompt> permission_prompt_;
  std::unique_ptr<WegletSiteInfoPopup> site_info_popup_;
  std::unique_ptr<WegletToastPopup> toast_popup_;
  std::unique_ptr<WegletOmniboxSuggestionsPopup> omnibox_suggestions_popup_;

  // Oldest first, capped at kMaxToasts by ShowToast.
  std::vector<WegletStateService::PendingToast> toasts_;
  int next_toast_id_ = 1;

  // The address bar's current dropdown, and which row (if any) is
  // highlighted for Enter/arrow-key selection. Cleared by
  // HideOmniboxSuggestions. `omnibox_anchor_*` is the address bar's own
  // rect, remembered so MoveOmniboxHighlight can re-show at the same spot.
  std::vector<WegletBridge::OmniboxSuggestion> omnibox_suggestions_;
  int omnibox_highlighted_ = -1;
  int omnibox_anchor_left_ = 0;
  int omnibox_anchor_bottom_ = 0;

  views::UnhandledKeyboardEventHandler unhandled_keyboard_event_handler_;

  // The find session in progress, if any. Climbs for the window's whole
  // lifetime rather than resetting per session, so a reply from a session
  // that just ended can never be mistaken for the new one.
  int find_request_id_ = 0;
  std::u16string find_query_;

  // Owned by the widget's view tree.
  raw_ptr<views::View> contents_view_ = nullptr;
  raw_ptr<views::WebView> toolbar_view_ = nullptr;
  raw_ptr<views::WebView> page_view_ = nullptr;
  // Owned by widget_'s NonClientView, created via CreateFrameView.
  raw_ptr<WegletFrameView> frame_view_ = nullptr;

  std::unique_ptr<content::WebContents> toolbar_contents_;
  std::map<uint64_t, Tab> tabs_;

  // What the open context menu acts on -- the tab it was opened over, and
  // the link under the cursor if any. Cleared on hide. Weak: the tab can
  // close while the menu is open, from window.close() for one.
  base::WeakPtr<content::WebContents> pending_context_menu_contents_;
  GURL pending_context_menu_link_;
  gfx::Point pending_context_menu_point_;
  gfx::Point pending_context_menu_screen_point_;
  // Indexed by the trailing number in a "spellingSuggestion:<n>" item id --
  // the words themselves don't round-trip through the item id/i18n system,
  // just their display text (see HandleContextMenu).
  std::vector<std::u16string> pending_context_menu_suggestions_;
  // Bumped on every HandleContextMenu call, so a suggestion fetch that's
  // still in flight when the menu closes (or reopens elsewhere) can tell
  // it's stale and skip updating a menu it no longer describes.
  int context_menu_generation_ = 0;

  base::WeakPtrFactory<WegletWindow> weak_factory_{this};
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_WINDOW_H_