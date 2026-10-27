// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// A browser window: the widget, its toolbar, its tabs, its accelerators.

#include "weglet/browser/weglet_window.h"

#include <algorithm>
#include <optional>
#include <string_view>
#include <utility>
#include <vector>

#include "base/base_paths.h"
#include "base/command_line.h"
#include "base/functional/bind.h"
#include "base/logging.h"
#include "base/no_destructor.h"
#include "base/path_service.h"
#include "base/strings/string_number_conversions.h"
#include "base/strings/utf_string_conversions.h"
#include "base/task/single_thread_task_runner.h"
#include "base/task/thread_pool.h"
#include "base/values.h"
#include "base/win/shortcut.h"
#include "build/build_config.h"
#include "components/spellcheck/browser/windows_spell_checker.h"
#include "components/spellcheck/common/spellcheck_common.h"
#include "components/zoom/page_zoom.h"
#include "components/zoom/zoom_controller.h"
#include "content/public/browser/context_menu_params.h"
#include "content/public/browser/media_capture_devices.h"
#include "content/public/browser/navigation_controller.h"
#include "content/public/browser/navigation_entry.h"
#include "content/public/browser/restore_type.h"
#include "services/network/public/cpp/shared_url_loader_factory.h"
#include "content/public/browser/render_frame_host.h"
#include "content/public/browser/render_widget_host.h"
#include "content/public/browser/render_widget_host_view.h"
#include "content/public/browser/web_contents.h"
#include "content/public/common/stop_find_action.h"
#include "content/public/common/referrer.h"
#include "content/public/common/url_constants.h"
#include "content/public/browser/web_contents_user_data.h"
#include "content/public/browser/web_ui.h"
#include "third_party/blink/public/mojom/frame/find_in_page.mojom.h"
#include "third_party/blink/public/mojom/mediastream/media_stream.mojom.h"
#include "third_party/blink/public/mojom/page/draggable_region.mojom.h"
#include "ui/base/clipboard/scoped_clipboard_writer.h"
#include "ui/base/page_transition_types.h"
#include "ui/gfx/geometry/rect.h"
#include "ui/base/accelerators/accelerator_manager.h"
#include "ui/events/keycodes/keyboard_codes.h"
#include "ui/views/controls/webview/webview.h"
#include "ui/views/focus/focus_manager.h"
#include "ui/views/layout/box_layout.h"
#include "ui/views/view.h"
#include "ui/views/widget/desktop_aura/desktop_native_widget_aura.h"
#include "ui/views/widget/widget.h"
#include "url/gurl.h"
#include "url/origin.h"
#include "url/url_constants.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_context_menu.h"
#include "weglet/browser/weglet_devtools_bindings.h"
#include "weglet/browser/weglet_devtools_ui.h"
#include "weglet/browser/weglet_file_select_helper.h"
#include "weglet/browser/weglet_find_bar.h"
#include "weglet/browser/weglet_frame_view.h"
#include "weglet/browser/weglet_permission_delegate.h"
#include "weglet/browser/weglet_permission_prompt.h"
#include "weglet/browser/weglet_site_info_popup.h"
#include "weglet/browser/weglet_menu_popup.h"
#include "weglet/browser/weglet_security_guard.h"
#include "weglet/browser/weglet_shortcut_popup.h"
#include "weglet/browser/weglet_state_service.h"
#include "weglet/browser/weglet_tab_observer.h"
#include "weglet/browser/weglet_omnibox_suggestions_popup.h"
#include "weglet/browser/weglet_toast_popup.h"
#include "weglet/common/weglet_host.h"
#include "weglet/ui/generated_contract.h"
#include "weglet/ui/weglet_tokens.h"

namespace weglet {
namespace {

base::OnceClosure& QuitClosure() {
  static base::NoDestructor<base::OnceClosure> quit;
  return *quit;
}

base::RepeatingCallback<void(const std::string&)>& ProfileSwitchHandler() {
  static base::NoDestructor<base::RepeatingCallback<void(const std::string&)>> handler;
  return *handler;
}

base::RepeatingCallback<void(const std::string&)>& OpenProfileWindowHandler() {
  static base::NoDestructor<base::RepeatingCallback<void(const std::string&)>> handler;
  return *handler;
}

// Every live window, in creation order. Used only by CloseAllWindows --
// see RequestProfileSwitch.
std::vector<WegletWindow*>& AllWindowsRegistry() {
  static base::NoDestructor<std::vector<WegletWindow*>> windows;
  return *windows;
}

// Attaches a window to each of its WebContents so the WebUI factory can find
// it. A holder, since WebContentsUserData would otherwise own the window --
// a WidgetDelegate with its own lifetime.
class WindowHolder : public content::WebContentsUserData<WindowHolder> {
 public:
  WindowHolder(content::WebContents* contents, WegletWindow* window)
      : content::WebContentsUserData<WindowHolder>(*contents),
        window_(window) {}

  WegletWindow* window() const { return window_; }

 private:
  friend class content::WebContentsUserData<WindowHolder>;

  const raw_ptr<WegletWindow> window_;

  WEB_CONTENTS_USER_DATA_KEY_DECL();
};

WEB_CONTENTS_USER_DATA_KEY_IMPL(WindowHolder);

}  // namespace

int WegletWindow::window_count_ = 0;

WegletWindow::Tab::Tab() = default;
WegletWindow::Tab::Tab(Tab&&) = default;
WegletWindow::Tab& WegletWindow::Tab::operator=(Tab&&) = default;
WegletWindow::Tab::~Tab() = default;

// static
int WegletWindow::window_count() {
  return window_count_;
}

// static
void WegletWindow::SetQuitClosure(base::OnceClosure quit) {
  QuitClosure() = std::move(quit);
}

// static
void WegletWindow::QuitIfLastWindow() {
  if (window_count_ == 0 && QuitClosure()) {
    std::move(QuitClosure()).Run();
  }
}

// static
void WegletWindow::CloseAllWindows() {
  // Copied first: each CloseWindow() is asynchronous (views::Widget::Close
  // posts the real teardown), so this never mutates the registry it is
  // iterating.
  const std::vector<WegletWindow*> windows = AllWindowsRegistry();
  for (WegletWindow* window : windows) {
    window->CloseWindow();
  }
}

// static
void WegletWindow::SetProfileSwitchHandler(
    base::RepeatingCallback<void(const std::string&)> handler) {
  ProfileSwitchHandler() = std::move(handler);
}

namespace {

bool CreateProfileShortcutBlocking(const std::string& profile_name) {
  if (!WegletBridge::IsValidProfileName(profile_name)) {
    return false;
  }
  base::FilePath desktop;
  if (!base::PathService::Get(base::DIR_USER_DESKTOP, &desktop)) {
    return false;
  }
  const base::FilePath exe = base::CommandLine::ForCurrentProcess()->GetProgram();

  base::CommandLine shortcut_args(exe);
  shortcut_args.AppendSwitchUTF8("profile", profile_name);

  base::win::ShortcutProperties properties;
  properties.set_target(exe);
  properties.set_working_dir(exe.DirName());
  properties.set_arguments(shortcut_args.GetArgumentsString());
  properties.set_description(base::UTF8ToWide("Weglet — " + profile_name));

  const base::FilePath shortcut_path =
      desktop.Append(base::FilePath::FromUTF8Unsafe("Weglet - " + profile_name + ".lnk"));
  return base::win::CreateOrUpdateShortcutLink(
      shortcut_path, properties, base::win::ShortcutOperation::kCreateAlways);
}

}  // namespace

void WegletWindow::CreateProfileShortcut(const std::string& profile_name) {
  base::ThreadPool::CreateCOMSTATaskRunner({base::MayBlock()})
      ->PostTaskAndReplyWithResult(
          FROM_HERE, base::BindOnce(&CreateProfileShortcutBlocking, profile_name),
          base::BindOnce(
              [](base::WeakPtr<WegletWindow> window, bool ok) {
                if (window) {
                  window->ShowToast(ok ? "toast.shortcutCreated" : "toast.shortcutFailed");
                }
              },
              weak_factory_.GetWeakPtr()));
}

void WegletWindow::RequestProfileSwitch(const std::string& profile_name) {
  for (const WegletBridge::DownloadEntry& download : bridge_->Downloads()) {
    if (download.status == 0) {
      ShowToast("toast.downloadsInProgress");
      return;
    }
  }
  if (ProfileSwitchHandler()) {
    ProfileSwitchHandler().Run(profile_name);
  }
}

// static
void WegletWindow::SetOpenProfileWindowHandler(
    base::RepeatingCallback<void(const std::string&)> handler) {
  OpenProfileWindowHandler() = std::move(handler);
}

void WegletWindow::RequestOpenProfileWindow(const std::string& profile_name) {
  if (OpenProfileWindowHandler()) {
    OpenProfileWindowHandler().Run(profile_name);
  }
}

// static
WegletWindow* WegletWindow::FromWebContents(content::WebContents* contents) {
  if (!contents) {
    return nullptr;
  }
  WindowHolder* holder = WindowHolder::FromWebContents(contents);
  return holder ? holder->window() : nullptr;
}

// static
void WegletWindow::CreateAndShow(content::BrowserContext* browser_context,
                                 WegletBridge* bridge,
                                 uint64_t window_id) {
  // Self-owned; see OnWindowClosing.
  auto* window = new WegletWindow(browser_context, bridge, window_id);
  window->Init();
}

// static
void WegletWindow::OpenNewWindow(content::BrowserContext* browser_context,
                                 WegletBridge* bridge) {
  const uint64_t id = bridge->OpenWindow();
  if (id == 0) {
    // The model's own ceiling. Reported rather than silently ignored.
    LOG(WARNING) << "window limit reached";
    return;
  }
  CreateAndShow(browser_context, bridge, id);
}

WegletWindow::WegletWindow(content::BrowserContext* browser_context,
                           WegletBridge* bridge,
                           uint64_t window_id)
    : browser_context_(browser_context),
      bridge_(bridge),
      window_id_(window_id),
      state_service_(
          WegletStateService::FromBrowserContext(browser_context)),
      security_guard_(
          WegletSecurityGuard::FromBrowserContext(browser_context)) {
  ++window_count_;
  AllWindowsRegistry().push_back(this);
}

WegletWindow::~WegletWindow() {
  --window_count_;
  std::erase(AllWindowsRegistry(), this);

  // Parented to widget_ -- torn down before it, not by relying on
  // declaration order alone.
  shortcut_popup_.reset();
  menu_popup_.reset();
  context_menu_.reset();
  find_bar_.reset();
  permission_prompt_.reset();
  site_info_popup_.reset();
  toast_popup_.reset();
  omnibox_suggestions_popup_.reset();

  // The WebViews hold bare pointers to their contents. Detach while the
  // views are alive, drop the widget, then drop the contents.
  if (toolbar_view_) {
    toolbar_view_->SetWebContents(nullptr);
    toolbar_view_ = nullptr;
  }
  if (page_view_) {
    page_view_->SetWebContents(nullptr);
    page_view_ = nullptr;
  }
  contents_view_ = nullptr;
  frame_view_ = nullptr;
  widget_.reset();

  // Observers before contents: an observer's destructor touches the
  // contents it watches.
  for (auto& [id, tab] : tabs_) {
    if (security_guard_) {
      security_guard_->Forget(tab.contents.get());
    }
    tab.observer.reset();
  }
  tabs_.clear();
  toolbar_contents_.reset();

  // The model stops knowing about this window, and its tabs go with it.
  if (bridge_) {
    bridge_->CloseWindow(window_id_);
  }

  QuitIfLastWindow();
}

void WegletWindow::Init() {
  SetCanResize(true);
  SetCanMaximize(true);
  SetCanMinimize(true);
  // "Weglet" alone for Default -- the common case -- so only a second or
  // later profile's windows carry a visible label telling them apart.
  const std::string profile = bridge_->CurrentProfile();
  SetTitle(profile == "Default" ? u"Weglet"
                                : base::UTF8ToUTF16("Weglet — " + profile));

  // Vertical stack: toolbar at its own height, page taking the rest. The
  // heights come from tokens.json through the generated header.
  auto container = std::make_unique<views::View>();
  auto* layout = container->SetLayoutManager(std::make_unique<views::BoxLayout>(
      views::BoxLayout::Orientation::kVertical));

  auto toolbar = std::make_unique<views::WebView>(browser_context_);
  // Tab strip, gap, toolbar row, inside padding halved at the bottom --
  // added up from the tokens the CSS uses; get it wrong and the bottom
  // row is clipped with nothing to say so.
  toolbar->SetPreferredSize(gfx::Size(
      0, tokens::kLayoutChromePadding + tokens::kLayoutTabHeight +
             tokens::kLayoutChromeGap + tokens::kLayoutToolbarRowHeight +
             tokens::kLayoutChromePaddingBottom));
  views::WebView* toolbar_view = container->AddChildView(std::move(toolbar));

  views::WebView* page_view =
      container->AddChildView(std::make_unique<views::WebView>(browser_context_));
  // Flex 1 on the page only: the toolbar keeps its preferred height.
  layout->SetFlexForView(page_view, 1);

  contents_view_ = SetContentsView(std::move(container));
  toolbar_view_ = toolbar_view;
  page_view_ = page_view;

  RegisterWindowClosingCallback(
      base::BindOnce(&WegletWindow::OnWindowClosing, base::Unretained(this)));

  views::Widget::InitParams init_params(
      views::Widget::InitParams::CLIENT_OWNS_WIDGET,
      views::Widget::InitParams::TYPE_WINDOW);
  init_params.delegate = this;
  // No OS caption or border: the toolbar page draws its own tab strip and
  // minimize/maximize/close buttons. CreateFrameView below is what keeps
  // the window resizable and draggable without them.
  init_params.remove_standard_frame = true;
  // From tokens.json, like every other dimension in the window.
  init_params.bounds =
      gfx::Rect(0, 0, tokens::kLayoutWindowWidth, tokens::kLayoutWindowHeight);

  widget_ = std::make_unique<views::Widget>();
#if defined(USE_AURA)
  // Without this the widget is a plain NativeWidgetAura, which DCHECKs for
  // a parent it will never have; DesktopNativeWidgetAura owns a real
  // top-level OS window instead.
  init_params.native_widget = new views::DesktopNativeWidgetAura(widget_.get());
#endif
  widget_->Init(std::move(init_params));
  widget_->Show();

  // After the widget exists: a WebView's NativeViewHost needs a live widget
  // to parent into, or the page renders as a sliver.
  content::WebContents::CreateParams toolbar_params(browser_context_);
  toolbar_contents_ = content::WebContents::Create(toolbar_params);
  toolbar_contents_->SetDelegate(this);
  WindowHolder::CreateForWebContents(toolbar_contents_.get(), this);
  // Locked at 100%: isolated per-tab zoom (see EnsureContentsFor) still
  // reached the toolbar when the active tab happened to share its
  // chrome://weglet host, since the toolbar itself has no zoom mode of its
  // own to isolate it from that. Disabled mode ignores every change.
  zoom::ZoomController::CreateForWebContents(toolbar_contents_.get());
  zoom::ZoomController::FromWebContents(toolbar_contents_.get())
      ->SetZoomMode(zoom::ZoomController::ZOOM_MODE_DISABLED);
  toolbar_view_->SetWebContents(toolbar_contents_.get());

  content::NavigationController::LoadURLParams toolbar_url(
      PageUrl(contract::kToolbarPath));
  toolbar_url.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
  toolbar_contents_->GetController().LoadURLWithParams(toolbar_url);

  RegisterAccelerators();
  ShowActiveTab();
}

void WegletWindow::RegisterAccelerators() {
  views::FocusManager* focus = widget_->GetFocusManager();
  if (!focus) {
    return;
  }
  // kNormalPriority: ordinary browser shortcuts, which a page that has
  // taken the key for itself should keep.
  const ui::Accelerator accelerators[] = {
      ui::Accelerator(ui::VKEY_T, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_W, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_N, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_R, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_L, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_F, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_S, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_U, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_TAB, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_TAB, ui::EF_CONTROL_DOWN | ui::EF_SHIFT_DOWN),
      ui::Accelerator(ui::VKEY_NEXT, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_PRIOR, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_F5, ui::EF_NONE),
      ui::Accelerator(ui::VKEY_F12, ui::EF_NONE),
      ui::Accelerator(ui::VKEY_I, ui::EF_CONTROL_DOWN | ui::EF_SHIFT_DOWN),
      ui::Accelerator(ui::VKEY_LEFT, ui::EF_ALT_DOWN),
      ui::Accelerator(ui::VKEY_RIGHT, ui::EF_ALT_DOWN),
      ui::Accelerator(ui::VKEY_BROWSER_BACK, ui::EF_NONE),
      ui::Accelerator(ui::VKEY_BROWSER_FORWARD, ui::EF_NONE),
      ui::Accelerator(ui::VKEY_BROWSER_REFRESH, ui::EF_NONE),
      // Zoom. Two keys apiece: the OEM key most keyboards have, and its
      // numpad equivalent. OEM_PLUS gets a Shift variant too -- on most
      // layouts "+" is the shifted form of the "=" key.
      ui::Accelerator(ui::VKEY_OEM_PLUS, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_OEM_PLUS, ui::EF_CONTROL_DOWN | ui::EF_SHIFT_DOWN),
      ui::Accelerator(ui::VKEY_ADD, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_OEM_MINUS, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_SUBTRACT, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_0, ui::EF_CONTROL_DOWN),
      ui::Accelerator(ui::VKEY_NUMPAD0, ui::EF_CONTROL_DOWN),
  };
  for (const ui::Accelerator& accelerator : accelerators) {
    focus->RegisterAccelerator(accelerator,
                               ui::AcceleratorManager::kNormalPriority, this);
  }
  // Ctrl+1..9. Nine is the last tab, however many there are.
  for (int key = ui::VKEY_1; key <= ui::VKEY_9; ++key) {
    focus->RegisterAccelerator(
        ui::Accelerator(static_cast<ui::KeyboardCode>(key),
                        ui::EF_CONTROL_DOWN),
        ui::AcceleratorManager::kNormalPriority, this);
  }
}

bool WegletWindow::CanHandleAccelerators() const {
  // False once the window is going away: the tab model is being torn down.
  return widget_ != nullptr;
}

bool WegletWindow::AcceleratorPressed(const ui::Accelerator& accelerator) {
  const int modifiers = accelerator.modifiers();
  const bool shift = (modifiers & ui::EF_SHIFT_DOWN) != 0;
  const bool ctrl = (modifiers & ui::EF_CONTROL_DOWN) != 0;

  switch (accelerator.key_code()) {
    case ui::VKEY_T:
      OpenNewTab();
      return true;
    case ui::VKEY_W:
      CloseTab(bridge_->ActiveTabId(window_id_));
      return true;
    case ui::VKEY_N:
      // A second window, with its own tabs and its own active one.
      OpenNewWindow(browser_context_, bridge_);
      return true;
    case ui::VKEY_R:
    case ui::VKEY_F5:
    case ui::VKEY_BROWSER_REFRESH:
      Reload();
      return true;
    case ui::VKEY_L:
      FocusOmnibox();
      return true;
    case ui::VKEY_F:
      OpenFindBar();
      return true;
    case ui::VKEY_S:
      SavePage();
      return true;
    case ui::VKEY_U:
      ViewSource();
      return true;
    case ui::VKEY_F12:
      OpenDevTools();
      return true;
    case ui::VKEY_I:
      if (shift) {
        OpenDevTools();
        return true;
      }
      break;
    case ui::VKEY_TAB:
      // Shift reverses it, which is why both are registered.
      bridge_->CycleTab(window_id_, !shift);
      ShowActiveTab();
      return true;
    case ui::VKEY_NEXT:
      bridge_->CycleTab(window_id_, true);
      ShowActiveTab();
      return true;
    case ui::VKEY_PRIOR:
      bridge_->CycleTab(window_id_, false);
      ShowActiveTab();
      return true;
    case ui::VKEY_LEFT:
    case ui::VKEY_BROWSER_BACK:
      GoBack();
      return true;
    case ui::VKEY_RIGHT:
    case ui::VKEY_BROWSER_FORWARD:
      GoForward();
      return true;
    case ui::VKEY_OEM_PLUS:
    case ui::VKEY_ADD:
      ZoomActiveTab(content::PAGE_ZOOM_IN);
      return true;
    case ui::VKEY_OEM_MINUS:
    case ui::VKEY_SUBTRACT:
      ZoomActiveTab(content::PAGE_ZOOM_OUT);
      return true;
    case ui::VKEY_0:
    case ui::VKEY_NUMPAD0:
      ZoomActiveTab(content::PAGE_ZOOM_RESET);
      return true;
    default:
      break;
  }

  if (ctrl && accelerator.key_code() >= ui::VKEY_1 &&
      accelerator.key_code() <= ui::VKEY_9) {
    const size_t position =
        static_cast<size_t>(accelerator.key_code() - ui::VKEY_1) + 1;
    if (bridge_->ActivateTabAt(window_id_, position)) {
      ShowActiveTab();
    }
    return true;
  }
  // Not ours. False lets the focus manager pass the key to the page.
  return false;
}

void WegletWindow::OnWindowClosing() {
  // Before the widget goes: the focus manager holds this as a target.
  if (widget_) {
    if (views::FocusManager* focus = widget_->GetFocusManager()) {
      focus->UnregisterAccelerators(this);
    }
  }

  // The widget is still unwinding, so deleting inline pulls the ground out
  // from under it.
  base::SingleThreadTaskRunner::GetCurrentDefault()->DeleteSoon(FROM_HERE,
                                                               this);
}

views::Widget* WegletWindow::GetWidget() {
  return widget_.get();
}

const views::Widget* WegletWindow::GetWidget() const {
  return widget_.get();
}

std::unique_ptr<views::FrameView> WegletWindow::CreateFrameView(
    views::Widget* widget) {
  auto frame_view = std::make_unique<WegletFrameView>(widget);
  frame_view_ = frame_view.get();
  return frame_view;
}

// static
GURL WegletWindow::ResolveForEngine(const std::string& address) {
  // Weglet's own addresses are stored as the user sees them and mapped
  // here, from a table generated from contract.json -- the same file the
  // Rust side takes its copy from.
  for (const contract::InternalAddress& entry : contract::kInternalAddresses) {
    if (address == entry.address) {
      return PageUrl(entry.page);
    }
  }
  return GURL(address);
}

content::WebContents* WegletWindow::EnsureContentsFor(uint64_t id) {
  auto found = tabs_.find(id);
  if (found != tabs_.end()) {
    return found->second.contents.get();
  }

  content::WebContents* contents = AttachContents(
      id, content::WebContents::Create(
              content::WebContents::CreateParams(browser_context_)));

  // Not checked here: a restored URL may have been blocked since the
  // session was written, but the throttle sees it like any other navigation.
  const WegletBridge::BackForwardList history = bridge_->TabBackForwardList(id);
  if (history.entries.size() <= 1) {
    content::NavigationController::LoadURLParams load(
        ResolveForEngine(bridge_->TabUrl(id)));
    load.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
    contents->GetController().LoadURLWithParams(load);
    return contents;
  }

  // A restored tab gets its whole back/forward list, so Back works in it
  // the way it did before the restart.
  std::vector<std::unique_ptr<content::NavigationEntry>> entries;
  entries.reserve(history.entries.size());
  for (const std::string& address : history.entries) {
    entries.push_back(content::NavigationController::CreateNavigationEntry(
        ResolveForEngine(address), content::Referrer(),
        /*initiator_origin=*/std::nullopt, /*initiator_base_url=*/std::nullopt,
        ui::PAGE_TRANSITION_RELOAD, /*is_renderer_initiated=*/false,
        /*extra_headers=*/std::string(), browser_context_,
        /*blob_url_loader_factory=*/nullptr));
  }
  content::NavigationController& controller = contents->GetController();
  controller.Restore(static_cast<int>(history.cursor),
                     content::RestoreType::kRestored, &entries);
  controller.LoadIfNecessary();
  return contents;
}

content::WebContents* WegletWindow::AttachContents(
    uint64_t id,
    std::unique_ptr<content::WebContents> contents) {
  Tab tab;
  tab.contents = std::move(contents);
  tab.contents->SetDelegate(this);
  WindowHolder::CreateForWebContents(tab.contents.get(), this);
  // Isolated, not the default per-host mode: the default mode zoomed the
  // toolbar along with the tab (both are WebContents sharing one
  // HostZoomMap). Isolated keys the zoom level to this one render frame.
  zoom::ZoomController::CreateForWebContents(tab.contents.get());
  zoom::ZoomController::FromWebContents(tab.contents.get())
      ->SetZoomMode(zoom::ZoomController::ZOOM_MODE_ISOLATED);
  tab.observer =
      std::make_unique<WegletTabObserver>(this, id, tab.contents.get());

  content::WebContents* attached = tab.contents.get();
  tabs_.emplace(id, std::move(tab));
  return attached;
}

content::WebContents* WegletWindow::AddTab(
    const GURL& url,
    WindowOpenDisposition disposition,
    std::unique_ptr<content::WebContents> contents) {
  const uint64_t in_front = bridge_->ActiveTabId(window_id_);
  const uint64_t id = bridge_->OpenTab(window_id_, url.spec());
  if (id == 0) {
    // The model's own ceiling. Reported rather than silently ignored.
    LOG(WARNING) << "tab limit reached, dropping a new tab";
    return nullptr;
  }
  content::WebContents* attached = AttachContents(id, std::move(contents));
  if (disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB) {
    bridge_->ActivateTab(in_front);
  }
  ShowActiveTab();
  return attached;
}

void WegletWindow::ShowActiveTab() {
  content::WebContents* contents = EnsureContentsFor(bridge_->ActiveTabId(window_id_));
  if (page_view_) {
    page_view_->SetWebContents(contents);
    contents->Focus();
  }
  Notify();
}

void WegletWindow::NavigateActiveTabFromOmnibox(const std::string& input) {
  // Enter with a row highlighted accepts that suggestion instead of
  // re-resolving what is still sitting in the address bar's text.
  if (omnibox_highlighted_ >= 0 &&
      static_cast<size_t>(omnibox_highlighted_) < omnibox_suggestions_.size()) {
    AcceptOmniboxSuggestion(omnibox_highlighted_);
    return;
  }
  HideOmniboxSuggestions();

  // The model decides whether the text is an address or a search. A parser
  // here would be a second one next to the tested one.
  const std::string resolved = bridge_->ResolveOmnibox(input);
  if (resolved.empty()) {
    return;
  }
  bridge_->RecordHistory(input, resolved);
  NavigateActiveTabToResolvedUrl(resolved);
}

void WegletWindow::NavigateActiveTabToResolvedUrl(const std::string& resolved) {
  // Through ResolveForEngine like every other navigation path: the model
  // hands back "weglet://settings" verbatim, and the engine has never
  // heard of that scheme.
  const GURL url = ResolveForEngine(resolved);
  if (!url.is_valid()) {
    LOG(WARNING) << "omnibox produced an invalid URL, ignored";
    return;
  }

  const uint64_t id = bridge_->ActiveTabId(window_id_);
  content::WebContents* contents = EnsureContentsFor(id);
  content::NavigationController::LoadURLParams load(url);
  load.transition_type = ui::PAGE_TRANSITION_TYPED;
  contents->GetController().LoadURLWithParams(load);
}

void WegletWindow::QueryOmniboxSuggestions(const std::string& input,
                                           int anchor_left,
                                           int anchor_bottom) {
  omnibox_anchor_left_ = anchor_left;
  omnibox_anchor_bottom_ = anchor_bottom;
  omnibox_suggestions_ = bridge_->QueryOmniboxSuggestions(input);
  omnibox_highlighted_ = -1;
  if (omnibox_suggestions_.empty()) {
    HideOmniboxSuggestions();
    return;
  }
  ShowOmniboxSuggestionsPopup();
}

void WegletWindow::HideOmniboxSuggestions() {
  omnibox_suggestions_.clear();
  omnibox_highlighted_ = -1;
  if (omnibox_suggestions_popup_) {
    omnibox_suggestions_popup_->Hide();
  }
}

void WegletWindow::MoveOmniboxHighlight(bool forward) {
  if (omnibox_suggestions_.empty()) {
    return;
  }
  const int count = static_cast<int>(omnibox_suggestions_.size());
  if (omnibox_highlighted_ < 0) {
    omnibox_highlighted_ = forward ? 0 : count - 1;
  } else {
    omnibox_highlighted_ = (omnibox_highlighted_ + (forward ? 1 : -1) + count) % count;
  }
  ShowOmniboxSuggestionsPopup();
}

void WegletWindow::AcceptOmniboxSuggestion(int index) {
  if (index < 0 || static_cast<size_t>(index) >= omnibox_suggestions_.size()) {
    return;
  }
  const std::string url = omnibox_suggestions_[index].url;
  HideOmniboxSuggestions();
  NavigateActiveTabToResolvedUrl(url);
}

void WegletWindow::ShowOmniboxSuggestionsPopup() {
  if (!omnibox_suggestions_popup_) {
    omnibox_suggestions_popup_ =
        std::make_unique<WegletOmniboxSuggestionsPopup>(this, state_service_);
  }
  std::vector<WegletStateService::PendingOmniboxSuggestion> items;
  items.reserve(omnibox_suggestions_.size());
  for (const WegletBridge::OmniboxSuggestion& suggestion : omnibox_suggestions_) {
    items.push_back({suggestion.label, suggestion.url, suggestion.is_bookmark});
  }
  omnibox_suggestions_popup_->Show(std::move(items), omnibox_highlighted_,
                                   omnibox_anchor_left_, omnibox_anchor_bottom_);
}

// The engine's own Back and Forward: the page comes out of the back/forward
// cache with its scroll position and form state when it can, and a page
// that was a POST result is not silently fetched again as a GET.
void WegletWindow::GoBack() {
  content::WebContents* contents = ActiveTabContents();
  if (contents && contents->GetController().CanGoBack()) {
    contents->GetController().GoBack();
  }
}

void WegletWindow::GoForward() {
  content::WebContents* contents = ActiveTabContents();
  if (contents && contents->GetController().CanGoForward()) {
    contents->GetController().GoForward();
  }
}

void WegletWindow::Reload() {
  auto found = tabs_.find(bridge_->ActiveTabId(window_id_));
  if (found == tabs_.end()) {
    return;
  }
  found->second.contents->GetController().Reload(
      content::ReloadType::NORMAL, /*check_for_repost=*/true);
}

void WegletWindow::OpenNewTab() {
  if (bridge_->OpenTab(window_id_, "about:blank") == 0) {
    // The model's own ceiling. Reported rather than silently ignored.
    LOG(WARNING) << "tab limit reached";
    return;
  }
  ShowActiveTab();
}

void WegletWindow::CloseTab(uint64_t id) {
  if (!bridge_->CloseTab(id)) {
    return;
  }
  auto found = tabs_.find(id);
  if (found != tabs_.end()) {
    // WebView has no public accessor for its contents, so detach
    // unconditionally; ShowActiveTab sets it again.
    if (page_view_) {
      page_view_->SetWebContents(nullptr);
    }
    if (security_guard_) {
      security_guard_->Forget(found->second.contents.get());
    }
    // Order matters: the observer's destructor touches the contents.
    found->second.observer.reset();
    tabs_.erase(found);
  }
  // Closing the last tab leaves a fresh one in the model.
  ShowActiveTab();
}

void WegletWindow::ActivateTab(uint64_t id) {
  if (bridge_->ActivateTab(id)) {
    ShowActiveTab();
  }
}

void WegletWindow::MinimizeWindow() {
  widget_->Minimize();
}

void WegletWindow::ToggleMaximizeWindow() {
  if (widget_->IsMaximized()) {
    widget_->Restore();
  } else {
    widget_->Maximize();
  }
}

void WegletWindow::CloseWindow() {
  widget_->Close();
}

void WegletWindow::ShowSecurityNotice(content::WebContents* contents) {
  if (!contents) {
    return;
  }
  content::NavigationController::LoadURLParams load(
      PageUrl(contract::kSecurityNoticePath));
  load.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
  contents->GetController().LoadURLWithParams(load);
}

void WegletWindow::DismissSecurityNotice(content::WebContents* contents) {
  if (security_guard_) {
    security_guard_->ClearPendingNotice(contents);
  }
  if (TabIdFor(contents) == 0 || contents == nullptr) {
    return;
  }
  // The throttle cancelled the navigation rather than committing an error
  // page, so the entry before the notice is the page the user was on.
  content::NavigationController& controller = contents->GetController();
  if (controller.CanGoBack()) {
    controller.GoBack();
    return;
  }
  content::NavigationController::LoadURLParams load{GURL(url::kAboutBlankURL)};
  load.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
  controller.LoadURLWithParams(load);
}

void WegletWindow::OpenSaveShortcutPopup(int anchor_right, int anchor_bottom) {
  const uint64_t id = bridge_->ActiveTabId(window_id_);
  std::string suggested_name;
  for (const WegletBridge::TabInfo& tab : bridge_->Tabs(window_id_)) {
    if (tab.id == id) {
      suggested_name = tab.label;
      break;
    }
  }
  if (!shortcut_popup_) {
    shortcut_popup_ = std::make_unique<WegletShortcutPopup>(this, state_service_);
  }
  shortcut_popup_->Show(anchor_right, anchor_bottom, bridge_->TabUrl(id),
                       suggested_name);
}

void WegletWindow::HideSaveShortcutPopup() {
  if (shortcut_popup_) {
    shortcut_popup_->Hide();
  }
}

void WegletWindow::ZoomActiveTab(content::PageZoom zoom) {
  auto found = tabs_.find(bridge_->ActiveTabId(window_id_));
  if (found == tabs_.end()) {
    return;
  }
  zoom::PageZoom::Zoom(found->second.contents.get(), zoom);
}

void WegletWindow::OpenFindBar() {
  if (!find_bar_) {
    find_bar_ = std::make_unique<WegletFindBar>(this, state_service_);
  }
  find_bar_->Show();
}

void WegletWindow::HideFindBar() {
  if (find_bar_) {
    find_bar_->Hide();
  }
  content::WebContents* contents = ActiveTabContents();
  if (contents) {
    contents->StopFinding(content::STOP_FIND_ACTION_KEEP_SELECTION);
  }
  find_query_.clear();
}

void WegletWindow::FindInPage(const std::string& query) {
  content::WebContents* contents = ActiveTabContents();
  if (!contents) {
    return;
  }
  if (query.empty()) {
    contents->StopFinding(content::STOP_FIND_ACTION_CLEAR_SELECTION);
    find_query_.clear();
    if (find_bar_) {
      find_bar_->UpdateResult(query, 0, 0);
    }
    return;
  }
  find_query_ = base::UTF8ToUTF16(query);
  ++find_request_id_;
  auto options = blink::mojom::FindOptions::New();
  options->forward = true;
  options->new_session = true;
  options->find_match = true;
  contents->Find(find_request_id_, find_query_, std::move(options), /*skip_delay=*/false);
}

void WegletWindow::FindNext(bool forward) {
  content::WebContents* contents = ActiveTabContents();
  if (!contents || find_query_.empty()) {
    return;
  }
  // Every Find() call needs a strictly higher id than the last, a
  // follow-up ("find next") included -- FindRequestManager::Find DCHECKs it.
  ++find_request_id_;
  auto options = blink::mojom::FindOptions::New();
  options->forward = forward;
  options->new_session = false;
  options->find_match = true;
  contents->Find(find_request_id_, find_query_, std::move(options), /*skip_delay=*/false);
}

void WegletWindow::OpenDevTools(int inspect_x, int inspect_y) {
  content::WebContents* inspected = ActiveTabContents();
  if (!inspected) {
    return;
  }
  const uint64_t id = bridge_->OpenTab(window_id_, DevToolsFrontendUrl().spec());
  if (id == 0) {
    LOG(WARNING) << "tab limit reached, dropping devtools";
    return;
  }
  content::WebContents* devtools_contents = EnsureContentsFor(id);
  ShowActiveTab();
  // Self-deleting -- see WegletDevToolsBindings::WebContentsDestroyed.
  auto* bindings = new WegletDevToolsBindings(devtools_contents, inspected, this);
  if (inspect_x >= 0 && inspect_y >= 0) {
    bindings->InspectElementAt(inspect_x, inspect_y);
  }
}

void WegletWindow::CloseDevToolsTab(content::WebContents* devtools_contents) {
  const uint64_t id = TabIdFor(devtools_contents);
  if (id != 0) {
    CloseTab(id);
  }
}

void WegletWindow::SavePage() {
  content::WebContents* contents = ActiveTabContents();
  if (contents) {
    contents->OnSavePage();
  }
}

void WegletWindow::ViewSource() {
  content::WebContents* contents = ActiveTabContents();
  if (!contents) {
    return;
  }
  const GURL& current = contents->GetLastCommittedURL();
  // Already viewing it: nothing to toggle to, same as real Chrome greying
  // out Ctrl+U here.
  if (current.SchemeIs(content::kViewSourceScheme)) {
    return;
  }
  content::NavigationController::LoadURLParams load(
      GURL(content::kViewSourceScheme + std::string(":") + current.spec()));
  load.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
  contents->GetController().LoadURLWithParams(load);
}

void WegletWindow::ShowPermissionPrompt(const std::string& origin,
                                        std::vector<std::string> types,
                                        base::OnceCallback<void(bool)> callback) {
  if (!permission_prompt_) {
    permission_prompt_ = std::make_unique<WegletPermissionPrompt>(this, state_service_);
  }
  permission_prompt_->Show(origin, std::move(types), std::move(callback));
}

void WegletWindow::AnswerPermissionPrompt(bool allow) {
  if (permission_prompt_) {
    permission_prompt_->Answer(allow);
  }
}

void WegletWindow::ToggleSiteInfo(int anchor_right, int anchor_bottom) {
  if (!site_info_popup_) {
    site_info_popup_ = std::make_unique<WegletSiteInfoPopup>(this);
  }
  if (site_info_popup_->IsVisible()) {
    site_info_popup_->Hide();
  } else {
    site_info_popup_->Show(anchor_right, anchor_bottom);
  }
}

void WegletWindow::HideSiteInfo() {
  if (site_info_popup_) {
    site_info_popup_->Hide();
  }
}

void WegletWindow::ShowToast(const std::string& text_key) {
  constexpr size_t kMaxToasts = 2;
  toasts_.push_back({next_toast_id_++, text_key});
  if (toasts_.size() > kMaxToasts) {
    toasts_.erase(toasts_.begin());
  }
  if (!toast_popup_) {
    toast_popup_ = std::make_unique<WegletToastPopup>(this, state_service_);
  }
  toast_popup_->Show(toasts_);
}

void WegletWindow::DismissToast(int id) {
  std::erase_if(toasts_, [id](const WegletStateService::PendingToast& toast) {
    return toast.id == id;
  });
  if (!toast_popup_) {
    return;
  }
  if (toasts_.empty()) {
    toast_popup_->Hide();
  } else {
    toast_popup_->Show(toasts_);
  }
}

void WegletWindow::SetPermissionDecision(const std::string& id, bool allow) {
  content::WebContents* contents = ActiveTabContents();
  if (!contents || !state_service_ || !state_service_->permission_delegate()) {
    return;
  }
  state_service_->permission_delegate()->SetDecision(
      url::Origin::Create(contents->GetLastCommittedURL()), id, allow);
  if (site_info_popup_) {
    site_info_popup_->Refresh();
  }
}

void WegletWindow::ToggleMenu() {
  if (!menu_popup_) {
    menu_popup_ = std::make_unique<WegletMenuPopup>(this);
  }
  if (menu_popup_->IsVisible()) {
    menu_popup_->Hide();
  } else {
    menu_popup_->Show();
  }
}

void WegletWindow::HideMenu() {
  if (menu_popup_) {
    menu_popup_->Hide();
  }
}

void WegletWindow::HandleContextMenuAction(const std::string& id) {
  content::WebContents* target = pending_context_menu_contents_.get();
  const GURL link_url = pending_context_menu_link_;
  const gfx::Point point = pending_context_menu_point_;
  const std::vector<std::u16string> suggestions =
      std::move(pending_context_menu_suggestions_);
  HideContextMenu();

  constexpr std::string_view kSuggestionPrefix = "spellingSuggestion:";
  if (id.starts_with(kSuggestionPrefix)) {
    size_t index = 0;
    if (target &&
        base::StringToSizeT(id.substr(kSuggestionPrefix.size()), &index) &&
        index < suggestions.size()) {
      target->ReplaceMisspelling(suggestions[index]);
    }
    return;
  }

  if (id == "back") {
    GoBack();
  } else if (id == "forward") {
    GoForward();
  } else if (id == "reload") {
    Reload();
  } else if (id == "inspect") {
    OpenDevTools(point.x(), point.y());
  } else if (id == "viewSource") {
    ViewSource();
  } else if (id == "savePage") {
    SavePage();
  } else if (id == "openLinkNewTab") {
    if (!link_url.is_empty()) {
      if (bridge_->OpenTab(window_id_, link_url.spec()) == 0) {
        LOG(WARNING) << "tab limit reached, dropping open-link-in-new-tab";
      } else {
        ShowActiveTab();
      }
    }
  } else if (id == "copyLinkAddress") {
    if (!link_url.is_empty()) {
      ui::ScopedClipboardWriter writer(ui::ClipboardBuffer::kCopyPaste);
      writer.WriteText(base::UTF8ToUTF16(link_url.spec()));
    }
  } else if (!target) {
    return;
  } else if (id == "cut") {
    target->Cut();
  } else if (id == "copy") {
    target->Copy();
  } else if (id == "paste") {
    target->Paste();
  } else if (id == "selectAll") {
    target->SelectAll();
  }
}

void WegletWindow::HideContextMenu() {
  pending_context_menu_contents_.reset();
  pending_context_menu_link_ = GURL();
  pending_context_menu_point_ = gfx::Point();
  pending_context_menu_suggestions_.clear();
  if (context_menu_) {
    context_menu_->Hide();
  }
}

void WegletWindow::RegisterWebContents(content::WebContents* contents) {
  WindowHolder::CreateForWebContents(contents, this);
}

void WegletWindow::ProceedPastSecurityNotice(content::WebContents* contents) {
  if (!security_guard_ || !contents) {
    return;
  }
  const WegletSecurityGuard::Notice* notice =
      security_guard_->PendingNotice(contents);
  if (!notice) {
    LOG(WARNING) << "proceed requested with no notice pending";
    return;
  }
  // The page hides the button on a hard block, but it runs in a renderer
  // and a compromised one can send the message anyway. Decided here.
  if (notice->blocking) {
    LOG(WARNING) << "proceed refused: this notice is not dismissible";
    return;
  }

  const GURL target = notice->target;
  security_guard_->ClearPendingNotice(contents);
  // One navigation, this address; the guard consumes it on the next check.
  security_guard_->AllowOnce(contents, target);

  content::NavigationController::LoadURLParams load(target);
  load.transition_type = ui::PAGE_TRANSITION_TYPED;
  contents->GetController().LoadURLWithParams(load);
}

// Which tab a WebContents is, or 0 if it is not one of this window's.
uint64_t WegletWindow::TabIdFor(content::WebContents* contents) const {
  for (const auto& [id, tab] : tabs_) {
    if (tab.contents.get() == contents) {
      return id;
    }
  }
  return 0;
}

content::WebContents* WegletWindow::ActiveTabContents() const {
  auto found = tabs_.find(bridge_->ActiveTabId(window_id_));
  return found == tabs_.end() ? nullptr : found->second.contents.get();
}

// Ctrl+L. The widget focuses the toolbar's WebContents; which element gets
// the caret is the page's decision, carried on the next push.
void WegletWindow::FocusOmnibox() {
  if (toolbar_contents_) {
    toolbar_contents_->Focus();
  }
  if (state_service_) {
    state_service_->RequestOmniboxFocus(window_id_);
  }
}

// One place that says the tab model changed; the service decides which
// pages care and coalesces several changes into one push.
void WegletWindow::Notify() {
  if (state_service_) {
    state_service_->Notify(WegletStateService::kTabs);
  }
}

void WegletWindow::OnTabBackForwardListChanged(uint64_t id, bool new_document) {
  auto found = tabs_.find(id);
  if (found == tabs_.end()) {
    return;
  }
  content::NavigationController& controller =
      found->second.contents->GetController();
  WegletBridge::BackForwardList history;
  for (int index = 0; index < controller.GetEntryCount(); ++index) {
    content::NavigationEntry* entry = controller.GetEntryAtIndex(index);
    // The placeholder a WebContents holds before its first navigation
    // commits is not a page the user was on.
    if (entry->IsInitialEntry()) {
      continue;
    }
    if (index == controller.GetLastCommittedEntryIndex()) {
      history.cursor = history.entries.size();
    }
    history.entries.push_back(entry->GetURL().spec());
  }
  if (history.entries.empty()) {
    return;
  }
  bridge_->BackForwardListChanged(id, history, new_document);
  Notify();
}

void WegletWindow::OnTabTitleChanged(uint64_t id, const std::string& title) {
  bridge_->TitleChanged(id, title);
  Notify();
}

void WegletWindow::OnTabLoadingChanged(uint64_t id, bool loading) {
  bridge_->LoadingChanged(id, loading);
  Notify();
}

void WegletWindow::CloseContents(content::WebContents* source) {
  // window.close() from a page: close that tab, not the window.
  for (const auto& [id, tab] : tabs_) {
    if (tab.contents.get() == source) {
      CloseTab(id);
      return;
    }
  }
  if (widget_) {
    widget_->Close();
  }
}

content::WebContents* WegletWindow::OpenURLFromTab(
    content::WebContents* source,
    const content::OpenURLParams& params,
    base::OnceCallback<void(content::NavigationHandle&)>
        navigation_handle_callback) {
  // The toolbar is not a tab, and navigating it away would take the
  // window's controls with it.
  content::WebContents* target = nullptr;
  if (params.disposition == WindowOpenDisposition::CURRENT_TAB &&
      TabIdFor(source) != 0) {
    target = source;
  } else {
    // Every other disposition lands in a tab, not a window we do not
    // control.
    target = AddTab(params.url, params.disposition,
                    content::WebContents::Create(
                        content::WebContents::CreateParams(browser_context_)));
  }
  if (!target) {
    return nullptr;
  }

  // From `params`, not a bare URL: the initiator, referrer, POST body and
  // user gesture are what make a page's navigation stay the page's. A
  // navigation without an initiator counts as the user's own, and sends
  // SameSite=Strict cookies to the site the page linked to.
  base::WeakPtr<content::NavigationHandle> handle =
      target->GetController().LoadURLWithParams(
          content::NavigationController::LoadURLParams(params));
  if (handle && navigation_handle_callback) {
    std::move(navigation_handle_callback).Run(*handle);
  }
  return target;
}

content::WebContents* WegletWindow::AddNewContents(
    content::WebContents* source,
    std::unique_ptr<content::WebContents> new_contents,
    const GURL& target_url,
    WindowOpenDisposition disposition,
    const blink::mojom::WindowFeatures& window_features,
    bool user_gesture,
    bool* was_blocked) {
  // Popups and new windows become tabs too. Content created the contents,
  // so the opener can reach it: a sign-in popup posts its result back
  // through window.opener.
  return AddTab(target_url, disposition, std::move(new_contents));
}

bool WegletWindow::HandleContextMenu(content::RenderFrameHost& render_frame_host,
                                     const content::ContextMenuParams& params) {
  content::WebContents* source_contents =
      content::WebContents::FromRenderFrameHost(&render_frame_host);
  // The toolbar and our own popups have no business showing this -- every
  // other WebContents naming `this` as delegate is a real tab.
  if (!source_contents || source_contents == toolbar_contents_.get()) {
    return false;
  }

  std::vector<WegletContextMenu::Item> items;
  pending_context_menu_suggestions_.clear();
  if (!params.link_url.is_empty()) {
    items.push_back({"openLinkNewTab", true});
    items.push_back({"copyLinkAddress", true});
  } else if (params.is_editable) {
    // params.dictionary_suggestions is always empty here: Windows's native
    // spellchecker doesn't cache suggestions on the misspelling marker the
    // way Hunspell does, only the fact that the word is wrong (see the
    // comment in blink's ContextMenuController::CustomContextMenuAction and
    // WindowsSpellChecker::RequestTextCheckForAllLanguages). Real words are
    // fetched below, asynchronously, once the menu is already showing.
    items.push_back({"cut", true});
    items.push_back({"copy", true});
    items.push_back({"paste", true});
    items.push_back({"selectAll", true});
  } else if (!params.selection_text.empty()) {
    items.push_back({"copy", true});
  } else {
    bool can_go_back = false;
    bool can_go_forward = false;
    const uint64_t active_id = bridge_->ActiveTabId(window_id_);
    for (const WegletBridge::TabInfo& tab : bridge_->Tabs(window_id_)) {
      if (tab.id == active_id) {
        can_go_back = tab.can_go_back;
        can_go_forward = tab.can_go_forward;
        break;
      }
    }
    items.push_back({"back", can_go_back});
    items.push_back({"forward", can_go_forward});
    items.push_back({"reload", true});
    items.push_back({"savePage", true});
    items.push_back({"viewSource", true});
  }
  items.push_back({"inspect", true});

  // params.x/y are relative to the frame's own render widget, not the
  // window -- add that widget's screen origin to get a screen point.
  gfx::Point screen_point(params.x, params.y);
  content::RenderWidgetHostView* view =
      render_frame_host.GetRenderWidgetHost()->GetView();
  if (view) {
    screen_point += view->GetViewBounds().OffsetFromOrigin();
  }

  pending_context_menu_contents_ = source_contents->GetWeakPtr();
  pending_context_menu_link_ = params.link_url;
  pending_context_menu_point_ = gfx::Point(params.x, params.y);
  pending_context_menu_screen_point_ = screen_point;
  const int generation = ++context_menu_generation_;

  if (!context_menu_) {
    context_menu_ = std::make_unique<WegletContextMenu>(this, state_service_);
  }
  context_menu_->Show(screen_point, std::move(items));

  if (params.is_editable && !params.misspelled_word.empty() &&
      state_service_ && state_service_->spell_checker()) {
    state_service_->spell_checker()->GetPerLanguageSuggestions(
        params.misspelled_word,
        base::BindOnce(
            [](base::WeakPtr<WegletWindow> window, int generation,
               const spellcheck::PerLanguageSuggestions& per_language) {
              if (!window) {
                return;
              }
              std::vector<std::u16string> flat;
              spellcheck::FillSuggestions(per_language, &flat);
              window->OnSpellingSuggestions(generation, flat);
            },
            weak_factory_.GetWeakPtr(), generation));
  }
  return true;
}

void WegletWindow::OnSpellingSuggestions(
    int generation, const std::vector<std::u16string>& suggestions) {
  if (generation != context_menu_generation_ || !context_menu_ ||
      !context_menu_->IsVisible() || suggestions.empty()) {
    return;
  }

  constexpr size_t kMaxSuggestions = 5;
  const size_t count = std::min(suggestions.size(), kMaxSuggestions);
  std::vector<WegletContextMenu::Item> items;
  pending_context_menu_suggestions_.clear();
  for (size_t i = 0; i < count; ++i) {
    pending_context_menu_suggestions_.push_back(suggestions[i]);
    items.push_back({"spellingSuggestion:" + base::NumberToString(i), true,
                     base::UTF16ToUTF8(suggestions[i])});
  }
  items.push_back({"cut", true});
  items.push_back({"copy", true});
  items.push_back({"paste", true});
  items.push_back({"selectAll", true});
  items.push_back({"inspect", true});
  context_menu_->Show(pending_context_menu_screen_point_, std::move(items));
}

bool WegletWindow::HandleKeyboardEvent(content::WebContents* source,
                                       const input::NativeWebKeyboardEvent& event) {
  return unhandled_keyboard_event_handler_.HandleKeyboardEvent(
      event, widget_->GetFocusManager());
}

void WegletWindow::FindReply(content::WebContents* source,
                             int request_id,
                             int number_of_matches,
                             const gfx::Rect& selection_rect,
                             int active_match_ordinal,
                             bool final_update) {
  if (!find_bar_ || request_id != find_request_id_ || source != ActiveTabContents()) {
    return;
  }
  find_bar_->UpdateResult(base::UTF16ToUTF8(find_query_), number_of_matches,
                          active_match_ordinal);
}

void WegletWindow::RequestMediaAccessPermission(
    content::WebContents* web_contents,
    const content::MediaStreamRequest& request,
    content::MediaResponseCallback callback) {
  content::RenderFrameHost* rfh = content::RenderFrameHost::FromID(
      request.render_process_id, request.render_frame_id);
  if (!rfh || !state_service_ || !state_service_->permission_delegate()) {
    std::move(callback).Run(blink::mojom::StreamDevicesSet(),
                            blink::mojom::MediaStreamRequestResult::NOT_SUPPORTED,
                            nullptr);
    return;
  }

  const bool want_audio =
      request.audio_type == blink::mojom::MediaStreamType::DEVICE_AUDIO_CAPTURE;
  const bool want_video =
      request.video_type == blink::mojom::MediaStreamType::DEVICE_VIDEO_CAPTURE;

  state_service_->permission_delegate()->RequestMediaTypes(
      rfh, want_audio, want_video,
      base::BindOnce(
          [](content::MediaResponseCallback callback, bool audio_granted,
             bool video_granted) {
            auto devices = blink::mojom::StreamDevices::New();
            if (audio_granted) {
              const blink::MediaStreamDevices& available =
                  content::MediaCaptureDevices::GetInstance()->GetAudioCaptureDevices();
              if (!available.empty()) {
                devices->audio_device = available.front();
              }
            }
            if (video_granted) {
              const blink::MediaStreamDevices& available =
                  content::MediaCaptureDevices::GetInstance()->GetVideoCaptureDevices();
              if (!available.empty()) {
                devices->video_device = available.front();
              }
            }

            const bool has_any =
                devices->audio_device.has_value() || devices->video_device.has_value();
            blink::mojom::StreamDevicesSet set;
            if (has_any) {
              set.stream_devices.push_back(std::move(devices));
            }
            std::move(callback).Run(
                set,
                has_any ? blink::mojom::MediaStreamRequestResult::OK
                       : blink::mojom::MediaStreamRequestResult::PERMISSION_DENIED,
                nullptr);
          },
          std::move(callback)));
}

bool WegletWindow::CheckMediaAccessPermission(content::RenderFrameHost* render_frame_host,
                                              const url::Origin& security_origin,
                                              blink::mojom::MediaStreamType type) {
  if (!state_service_ || !state_service_->permission_delegate()) {
    return false;
  }
  const blink::PermissionType permission_type =
      type == blink::mojom::MediaStreamType::DEVICE_AUDIO_CAPTURE
          ? blink::PermissionType::AUDIO_CAPTURE
          : blink::PermissionType::VIDEO_CAPTURE;
  return state_service_->permission_delegate()->IsMediaGranted(security_origin, permission_type);
}

void WegletWindow::RunFileChooser(
    content::RenderFrameHost* render_frame_host,
    scoped_refptr<content::FileSelectListener> listener,
    const blink::mojom::FileChooserParams& params) {
  WegletFileSelectHelper::RunFileChooser(render_frame_host, std::move(listener),
                                         params);
}

void WegletWindow::DraggableRegionsChanged(
    const std::vector<blink::mojom::DraggableRegionPtr>& regions,
    content::WebContents* contents) {
  // Only the toolbar's own drag strip may move the window -- an arbitrary
  // tab reporting -webkit-app-region has no business doing that.
  if (contents != toolbar_contents_.get() || !frame_view_) {
    return;
  }
  frame_view_->SetDraggableRegions(regions);
}

}  // namespace weglet