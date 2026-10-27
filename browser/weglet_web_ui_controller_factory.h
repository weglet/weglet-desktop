// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The privilege boundary: which URLs get WebUI bindings.

#ifndef WEGLET_BROWSER_WEGLET_WEB_UI_CONTROLLER_FACTORY_H_
#define WEGLET_BROWSER_WEGLET_WEB_UI_CONTROLLER_FACTORY_H_

#include <memory>

#include "base/no_destructor.h"
#include "content/public/browser/web_ui.h"
#include "content/public/browser/web_ui_controller_factory.h"

namespace weglet {

// Decides which URLs are Weglet pages, which decides whether a frame gets
// the bindings that make chrome.send() exist. The test is an exact host
// match on content's own scheme and nothing else.
//
// Also serves the DevTools front-end at devtools://devtools/, with no
// bindings -- see WegletDevToolsUI.
//
// Registered once at startup and never destroyed.
class WegletWebUIControllerFactory : public content::WebUIControllerFactory {
 public:
  // Registers the single instance with content. Idempotent.
  static void Register();

  WegletWebUIControllerFactory(const WegletWebUIControllerFactory&) = delete;
  WegletWebUIControllerFactory& operator=(const WegletWebUIControllerFactory&) =
      delete;

  // content::WebUIControllerFactory:
  std::unique_ptr<content::WebUIController> CreateWebUIControllerForURL(
      content::WebUI* web_ui,
      const GURL& url) override;
  content::WebUI::TypeID GetWebUIType(content::BrowserContext* browser_context,
                                      const GURL& url) override;
  bool UseWebUIForURL(content::BrowserContext* browser_context,
                      const GURL& url) override;

 private:
  // Held by a NoDestructor in Register(). Private so nobody makes a
  // second one: content compares factories by identity.
  friend class base::NoDestructor<WegletWebUIControllerFactory>;

  WegletWebUIControllerFactory() = default;
  ~WegletWebUIControllerFactory() override = default;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_WEB_UI_CONTROLLER_FACTORY_H_
