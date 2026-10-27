// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// ContentBrowserClient: policy answers and the navigation throttle.

#ifndef WEGLET_BROWSER_WEGLET_CONTENT_BROWSER_CLIENT_H_
#define WEGLET_BROWSER_WEGLET_CONTENT_BROWSER_CLIENT_H_

#include <memory>
#include <string>

#include "base/memory/raw_ptr.h"
#include "components/spellcheck/common/spellcheck.mojom-forward.h"
#include "content/public/browser/content_browser_client.h"
#include "mojo/public/cpp/bindings/pending_receiver.h"

namespace weglet {

class WegletBrowserMainParts;

// Where the browser process answers content's questions about policy.
class WegletContentBrowserClient : public content::ContentBrowserClient {
 public:
  WegletContentBrowserClient();
  WegletContentBrowserClient(const WegletContentBrowserClient&) = delete;
  WegletContentBrowserClient& operator=(const WegletContentBrowserClient&) =
      delete;
  ~WegletContentBrowserClient() override;

  WegletBrowserMainParts* browser_main_parts() { return browser_main_parts_; }

  // content::ContentBrowserClient:
  std::unique_ptr<content::BrowserMainParts> CreateBrowserMainParts(
      bool is_integration_test) override;
  std::string GetUserAgent() override;
  blink::UserAgentMetadata GetUserAgentMetadata() override;
  std::string GetProduct() override;

  // Where the block list and the risk heuristics take effect. The one
  // point that sees a redirect, which is how a shortened link resolving
  // to a lookalike arrives.
  void CreateThrottlesForNavigation(
      content::NavigationThrottleRegistry& registry) override;

  // Binds spellcheck::mojom::SpellCheckHost -- see WegletSpellCheckHost.
  void RegisterBrowserInterfaceBindersForFrame(
      content::RenderFrameHost* render_frame_host,
      mojo::BinderMapWithContext<content::RenderFrameHost*>* map) override;

  // No scheme of our own to register a loader factory for: our pages
  // are chrome://weglet/, served by a WebUIDataSource.

 private:
  void BindSpellCheckHost(
      content::RenderFrameHost* frame_host,
      mojo::PendingReceiver<spellcheck::mojom::SpellCheckHost> receiver);

  // Owned by the content layer, which outlives this client.
  raw_ptr<WegletBrowserMainParts> browser_main_parts_ = nullptr;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_CONTENT_BROWSER_CLIENT_H_
