// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The privilege boundary: which URLs get WebUI bindings.

#include "weglet/browser/weglet_web_ui_controller_factory.h"

#include "base/memory/raw_ptr.h"
#include "base/no_destructor.h"
#include "content/public/browser/web_contents.h"
#include "content/public/browser/web_ui_controller.h"
#include "content/public/common/url_constants.h"
#include "url/gurl.h"
#include "weglet/browser/weglet_devtools_ui.h"
#include "weglet/browser/weglet_message_handler.h"
#include "weglet/browser/weglet_state_service.h"
#include "weglet/browser/weglet_window.h"
#include "weglet/common/weglet_host.h"
#include "weglet/ui/generated_contract.h"

namespace weglet {
namespace {

// True only for chrome://weglet/... -- exact scheme, exact host. A prefix
// test would accept "chrome://weglet.evil.example/" and hand it a channel
// into the browser.
bool IsWegletPage(const GURL& url) {
  return url.SchemeIs(content::kChromeUIScheme) &&
         url.host() == kHost;
}

// Which of our pages this URL is. Worked out once and carried by the
// controller from then on.
contract::PageKind KindOf(const GURL& url) {
  const std::string_view path = url.path();
  // path() carries the leading slash; the generated paths do not. An
  // empty path is the page the data source serves by default.
  const std::string_view relative =
      path.size() > 1 ? path.substr(1) : std::string_view(contract::kNewtabPath);
  return contract::KindForPath(relative);
}

// Gives a Weglet page its message channel, and registers the page with the
// state service for as long as the controller lives.
class WegletWebUIController : public content::WebUIController {
 public:
  WegletWebUIController(content::WebUI* web_ui,
                        WegletWindow* window,
                        WegletStateService* state,
                        contract::PageKind kind)
      : content::WebUIController(web_ui), state_(state), web_ui_(web_ui) {
    // No service means no profile behind this page. The handler is added
    // either way -- an unregistered chrome.send() crashes the browser --
    // and drops what it cannot do.
    if (state_) {
      // Which window's tabs the toolbar shows is decided here, once.
      state_->AddPage(web_ui, kind, window ? window->window_id() : 0);
    }
    web_ui->AddMessageHandler(std::make_unique<WegletMessageHandler>(
        window, state_ ? state_->bridge() : nullptr, state_));
  }

  WegletWebUIController(const WegletWebUIController&) = delete;
  WegletWebUIController& operator=(const WegletWebUIController&) = delete;

  ~WegletWebUIController() override {
    if (state_) {
      state_->RemovePage(web_ui_);
    }
  }

 private:
  const raw_ptr<WegletStateService> state_;
  const raw_ptr<content::WebUI> web_ui_;
};

}  // namespace

// static
void WegletWebUIControllerFactory::Register() {
  // NoDestructor: content holds this pointer for the life of the process.
  // It constructs in place, so it needs the default constructor and
  // friendship to reach it.
  static base::NoDestructor<WegletWebUIControllerFactory> factory;
  static bool registered = false;
  if (!registered) {
    content::WebUIControllerFactory::RegisterFactory(factory.get());
    registered = true;
  }
}

std::unique_ptr<content::WebUIController>
WegletWebUIControllerFactory::CreateWebUIControllerForURL(
    content::WebUI* web_ui,
    const GURL& url) {
  if (IsDevToolsFrontend(url)) {
    return std::make_unique<WegletDevToolsUI>(web_ui);
  }
  if (!IsWegletPage(url)) {
    return nullptr;
  }
  // Looked up from the WebContents, not held: a factory outlives every
  // window. Null for a page open outside one.
  content::WebContents* contents = web_ui->GetWebContents();
  WegletWindow* window = WegletWindow::FromWebContents(contents);
  WegletStateService* state =
      contents ? WegletStateService::FromBrowserContext(
                     contents->GetBrowserContext())
               : nullptr;
  return std::make_unique<WegletWebUIController>(web_ui, window, state,
                                                 KindOf(url));
}

content::WebUI::TypeID WegletWebUIControllerFactory::GetWebUIType(
    content::BrowserContext* browser_context,
    const GURL& url) {
  // Any non-null value distinguishes a WebUI of this type from not a
  // WebUI; content compares these for identity. All Weglet pages share
  // one type, one origin and one set of bindings. The DevTools front-end
  // is a type of its own, with no bindings. Different values: the build
  // merges identical constants into one address.
  static const int kWegletWebUIType = 0;
  static const int kDevToolsWebUIType = 1;
  if (IsWegletPage(url)) {
    return &kWegletWebUIType;
  }
  if (IsDevToolsFrontend(url)) {
    return &kDevToolsWebUIType;
  }
  return content::WebUI::kNoWebUI;
}

bool WegletWebUIControllerFactory::UseWebUIForURL(
    content::BrowserContext* browser_context,
    const GURL& url) {
  return IsWegletPage(url) || IsDevToolsFrontend(url);
}

}  // namespace weglet