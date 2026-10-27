// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// ContentBrowserClient: policy answers and the navigation throttle.

#include "weglet/browser/weglet_content_browser_client.h"

#include <utility>

#include "base/functional/bind.h"
#include "components/embedder_support/user_agent_utils.h"
#include "components/spellcheck/common/spellcheck.mojom.h"
#include "content/public/browser/render_frame_host.h"
#include "mojo/public/cpp/bindings/binder_map.h"
#include "weglet/browser/weglet_browser_main_parts.h"
#include "weglet/browser/weglet_navigation_throttle.h"
#include "weglet/browser/weglet_spell_check_host.h"

namespace weglet {

WegletContentBrowserClient::WegletContentBrowserClient() = default;
WegletContentBrowserClient::~WegletContentBrowserClient() = default;

std::unique_ptr<content::BrowserMainParts>
WegletContentBrowserClient::CreateBrowserMainParts(bool is_integration_test) {
  auto parts = std::make_unique<WegletBrowserMainParts>();
  browser_main_parts_ = parts->GetWeakPtr();
  return parts;
}

std::string WegletContentBrowserClient::GetProduct() {
  // Chrome's own product string: a distinct one would make every Weglet
  // user uniquely identifiable to every site they visit.
  return embedder_support::GetProductAndVersion();
}

std::string WegletContentBrowserClient::GetUserAgent() {
  return embedder_support::GetUserAgent();
}

blink::UserAgentMetadata WegletContentBrowserClient::GetUserAgentMetadata() {
  return embedder_support::GetUserAgentMetadata();
}

void WegletContentBrowserClient::CreateThrottlesForNavigation(
    content::NavigationThrottleRegistry& registry) {
  WegletNavigationThrottle::MaybeCreateAndAdd(registry);
}

void WegletContentBrowserClient::RegisterBrowserInterfaceBindersForFrame(
    content::RenderFrameHost* render_frame_host,
    mojo::BinderMapWithContext<content::RenderFrameHost*>* map) {
  map->Add<spellcheck::mojom::SpellCheckHost>(base::BindRepeating(
      &WegletContentBrowserClient::BindSpellCheckHost,
      base::Unretained(this)));
}

void WegletContentBrowserClient::BindSpellCheckHost(
    content::RenderFrameHost* frame_host,
    mojo::PendingReceiver<spellcheck::mojom::SpellCheckHost> receiver) {
  if (!browser_main_parts_) {
    return;
  }
  WegletSpellCheckHost::Create(browser_main_parts_->spell_checker(),
                               std::move(receiver));
}

}  // namespace weglet
