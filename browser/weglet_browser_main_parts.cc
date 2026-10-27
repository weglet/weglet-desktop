// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Browser-process startup and shutdown.

#include "weglet/browser/weglet_browser_main_parts.h"

#include <utility>

#include "base/command_line.h"
#include "base/functional/bind.h"
#include "base/functional/callback_helpers.h"
#include "base/logging.h"
#include "base/process/launch.h"
#include "base/run_loop.h"
#include "base/task/single_thread_task_runner.h"
#include "base/task/thread_pool.h"
#include "base/time/time.h"
#include "components/spellcheck/browser/windows_spell_checker.h"
#include "content/public/common/result_codes.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_browser_context.h"
#include "weglet/browser/weglet_devtools_manager_delegate.h"
#include "weglet/browser/weglet_profile_manager.h"
#include "weglet/browser/weglet_web_ui_controller_factory.h"
#include "weglet/browser/weglet_window.h"

#if defined(USE_AURA)
#include "ui/display/screen.h"
#include "ui/views/views_delegate.h"
#include "ui/views/widget/desktop_aura/desktop_screen.h"
#include "ui/wm/core/wm_state.h"
#endif

namespace weglet {
namespace {

#if defined(USE_AURA)
// Every method has a usable default; this exists because something has to
// be installed before the first Widget.
class WegletViewsDelegate : public views::ViewsDelegate {
 public:
  WegletViewsDelegate() = default;
  WegletViewsDelegate(const WegletViewsDelegate&) = delete;
  WegletViewsDelegate& operator=(const WegletViewsDelegate&) = delete;
  ~WegletViewsDelegate() override = default;
};
#endif

constexpr char kProfileSwitch[] = "profile";

// What --profile resolved to at launch, or "Default" if it was absent
// or WegletBridge::IsValidProfileName rejects it -- a launch is not the
// place to fail over a stray command-line value.
std::string ResolveProfileName() {
  const std::string requested =
      base::CommandLine::ForCurrentProcess()->GetSwitchValueUTF8(kProfileSwitch);
  if (!requested.empty() && WegletBridge::IsValidProfileName(requested)) {
    return requested;
  }
  const std::string last = WegletBridge::LastProfile();
  return last.empty() ? "Default" : last;
}

}  // namespace

WegletBrowserMainParts::WegletBrowserMainParts() = default;
WegletBrowserMainParts::~WegletBrowserMainParts() = default;


int WegletBrowserMainParts::PreMainMessageLoopRun() {
#if defined(USE_AURA)
  wm_state_ = std::make_unique<wm::WMState>();
  views_delegate_ = std::make_unique<WegletViewsDelegate>();
  screen_ = views::CreateDesktopScreen();
#endif

  // Native Windows spellchecker -- see WegletSpellCheckHost. Created
  // unconditionally like real Chrome's own InitializePlatformSpellchecker:
  // cheap until a language checker is actually requested below.
  spell_checker_ = std::make_unique<WindowsSpellChecker>(
      base::ThreadPool::CreateCOMSTATaskRunner({base::MayBlock()}));
  spell_checker_->RetrieveSpellcheckLanguages(
      base::BindOnce(&WegletBrowserMainParts::OnSpellcheckLanguagesRetrieved,
                     weak_factory_.GetWeakPtr()));

  profile_manager_ = std::make_unique<WegletProfileManager>(spell_checker_.get());
  const std::string profile_name = ResolveProfileName();
  WegletProfileManager::Instance& initial = profile_manager_->GetOrCreate(profile_name);

  // Serves the DevTools front-end tab loads -- see WegletWindow::OpenDevTools.
  WegletDevToolsManagerDelegate::StartHttpHandler(initial.browser_context.get());

  // Registered before that navigation is asked whether it is a WebUI:
  // that answer decides whether the page gets a channel to the browser.
  WegletWebUIControllerFactory::Register();

  WegletWindow::SetProfileSwitchHandler(base::BindRepeating(
      &WegletBrowserMainParts::SwitchToProfile, weak_factory_.GetWeakPtr()));
  WegletWindow::SetOpenProfileWindowHandler(base::BindRepeating(
      &WegletBrowserMainParts::OpenProfileWindow, weak_factory_.GetWeakPtr()));

  settings_flush_timer_.Start(
      FROM_HERE, initial.bridge->SettingsFlushInterval(),
      base::BindRepeating(&WegletBrowserMainParts::FlushSettings,
                          base::Unretained(this)));
  session_save_timer_.Start(
      FROM_HERE, initial.bridge->SessionSaveInterval(),
      base::BindRepeating(&WegletBrowserMainParts::SaveSession,
                          base::Unretained(this)));

  // One WegletWindow per window the model restored.
  for (uint64_t window : initial.bridge->Windows()) {
    WegletWindow::CreateAndShow(initial.browser_context.get(), initial.bridge.get(), window);
  }

  // Anything other than RESULT_CODE_NORMAL_EXIT aborts startup.
  return content::RESULT_CODE_NORMAL_EXIT;
}

void WegletBrowserMainParts::WillRunMainMessageLoop(
    std::unique_ptr<base::RunLoop>& run_loop) {
  // Handing the quit closure to the window layer is what makes closing the
  // last window end the process.
  WegletWindow::SetQuitClosure(run_loop->QuitClosure());
}

void WegletBrowserMainParts::OnSpellcheckLanguagesRetrieved(
    const std::vector<std::string>& lang_tags) {
  // One native checker per language pack the OS actually has installed.
  // Which one("s") is irrelevant to WegletSpellCheckHost::RequestTextCheck,
  // which checks against every created checker regardless of what language
  // the renderer thinks it asked for -- see InitializeDictionaries.
  for (const std::string& lang_tag : lang_tags) {
    spell_checker_->CreateSpellChecker(lang_tag, base::DoNothing());
  }
}

void WegletBrowserMainParts::FlushSettings() {
  if (profile_manager_) {
    profile_manager_->FlushDirtySettings();
  }
}

void WegletBrowserMainParts::SaveSession() {
  if (profile_manager_) {
    profile_manager_->SaveAllSessions();
  }
}

void WegletBrowserMainParts::SwitchToProfile(const std::string& profile_name) {
  if (!WegletBridge::IsValidProfileName(profile_name)) {
    return;
  }

  WegletBridge::SetLastProfile(profile_name);

  base::CommandLine relaunch_command_line(*base::CommandLine::ForCurrentProcess());
  relaunch_command_line.RemoveSwitch(kProfileSwitch);
  relaunch_command_line.AppendSwitchUTF8(kProfileSwitch, profile_name);
  base::LaunchProcess(relaunch_command_line, base::LaunchOptions());

  // Closing every window drops the count to zero, which runs the quit
  // closure WillRunMainMessageLoop installed -- see
  // WegletWindow::QuitIfLastWindow. PostMainMessageLoopRun flushes
  // settings and the session on the way out, same as any other quit.
  WegletWindow::CloseAllWindows();
}

void WegletBrowserMainParts::OpenProfileWindow(const std::string& profile_name) {
  if (!profile_manager_ || !WegletBridge::IsValidProfileName(profile_name)) {
    return;
  }
  WegletProfileManager::Instance& instance = profile_manager_->GetOrCreate(profile_name);
  WegletWindow::OpenNewWindow(instance.browser_context.get(), instance.bridge.get());
}

void WegletBrowserMainParts::PostMainMessageLoopRun() {
  WegletDevToolsManagerDelegate::StopHttpHandler();

  // Stopped first: neither should fire while the model is going away.
  settings_flush_timer_.Stop();
  session_save_timer_.Stop();

  // Every profile's own save-then-flush, then torn down in the order
  // each instance needs -- see WegletProfileManager's destructor.
  profile_manager_.reset();
  spell_checker_.reset();

#if defined(USE_AURA)
  screen_.reset();
  views_delegate_.reset();
  wm_state_.reset();
#endif
}

}  // namespace weglet
