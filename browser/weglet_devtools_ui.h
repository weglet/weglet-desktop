// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// devtools://devtools/: the DevTools front-end, served from the resources
// bundled into the binary. It talks to the page it inspects through
// WegletDevToolsBindings, in process, so no port is ever opened for it.

#ifndef WEGLET_BROWSER_WEGLET_DEVTOOLS_UI_H_
#define WEGLET_BROWSER_WEGLET_DEVTOOLS_UI_H_

#include "content/public/browser/web_ui_controller.h"
#include "url/gurl.h"

namespace content {
class WebUI;
}

namespace weglet {

// The front-end's own page, the one WegletWindow::OpenDevTools opens.
GURL DevToolsFrontendUrl();

// True for devtools://devtools/ -- exact scheme, exact host, as for
// Weglet's own pages.
bool IsDevToolsFrontend(const GURL& url);

// Serves the front-end. Grants no bindings: the front-end gets its one
// channel into the browser from WegletDevToolsBindings, which attaches it
// only to this origin.
class WegletDevToolsUI : public content::WebUIController {
 public:
  explicit WegletDevToolsUI(content::WebUI* web_ui);
  WegletDevToolsUI(const WegletDevToolsUI&) = delete;
  WegletDevToolsUI& operator=(const WegletDevToolsUI&) = delete;
  ~WegletDevToolsUI() override;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_DEVTOOLS_UI_H_
