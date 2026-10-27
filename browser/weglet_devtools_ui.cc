// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The bundled-resource part of
// chrome/browser/ui/webui/devtools/devtools_ui_data_source.cc. The remote
// and custom front-ends it can also serve are left out: each one is a
// request to a server.

#include "weglet/browser/weglet_devtools_ui.h"

#include <memory>
#include <string>
#include <string_view>
#include <utility>

#include "base/memory/ref_counted_memory.h"
#include "base/strings/string_util.h"
#include "content/public/browser/browser_context.h"
#include "content/public/browser/devtools_frontend_host.h"
#include "content/public/browser/url_data_source.h"
#include "content/public/browser/web_contents.h"
#include "content/public/browser/web_ui.h"
#include "content/public/common/bindings_policy.h"
#include "content/public/common/url_constants.h"
#include "services/network/public/mojom/content_security_policy.mojom.h"

namespace weglet {
namespace {

constexpr char kHost[] = "devtools";
// Everything the front-end loads sits under this path.
constexpr std::string_view kBundledPrefix = "/bundled/";

std::string MimeTypeFor(std::string_view path) {
  struct Extension {
    std::string_view suffix;
    std::string_view mime_type;
  };
  static constexpr Extension kExtensions[] = {
      {".html", "text/html"},
      {".css", "text/css"},
      {".js", "application/javascript"},
      {".mjs", "application/javascript"},
      {".json", "application/json"},
      {".map", "application/json"},
      {".png", "image/png"},
      {".gif", "image/gif"},
      {".svg", "image/svg+xml"},
      {".avif", "image/avif"},
      {".woff2", "font/woff2"},
  };
  for (const Extension& extension : kExtensions) {
    if (base::EndsWith(path, extension.suffix,
                       base::CompareCase::INSENSITIVE_ASCII)) {
      return std::string(extension.mime_type);
    }
  }
  return "text/plain";
}

class DevToolsDataSource : public content::URLDataSource {
 public:
  DevToolsDataSource() = default;
  DevToolsDataSource(const DevToolsDataSource&) = delete;
  DevToolsDataSource& operator=(const DevToolsDataSource&) = delete;
  ~DevToolsDataSource() override = default;

  // content::URLDataSource:
  std::string GetSource() override { return kHost; }

  void StartDataRequest(const GURL& url,
                        const content::WebContents::Getter& wc_getter,
                        GotDataCallback callback) override {
    const std::string_view path = url.path();
    if (!path.starts_with(kBundledPrefix)) {
      // Null is the data source's "not found".
      std::move(callback).Run(nullptr);
      return;
    }
    std::move(callback).Run(content::DevToolsFrontendHost::GetFrontendResourceBytes(
        path.substr(kBundledPrefix.size())));
  }

  std::string GetMimeType(const GURL& url) override {
    return MimeTypeFor(url.path());
  }

  // Chrome's own policy for the bundled front-end, without the remote
  // script origin it allows for a front-end fetched from a server.
  std::string GetContentSecurityPolicy(
      network::mojom::CSPDirectiveName directive) override {
    switch (directive) {
      case network::mojom::CSPDirectiveName::ObjectSrc:
        return "object-src 'none';";
      case network::mojom::CSPDirectiveName::ScriptSrc:
        return "script-src 'self';";
      default:
        return std::string();
    }
  }

  bool ShouldServeMimeTypeAsContentTypeHeader() override { return true; }
};

}  // namespace

GURL DevToolsFrontendUrl() {
  // targetType=tab: the bindings attach the front-end to the tab's own
  // target, the one DevToolsAgentHost::GetOrCreateForTab returns.
  return GURL(std::string(content::kChromeDevToolsScheme) + "://" + kHost +
              std::string(kBundledPrefix) + "devtools_app.html?targetType=tab");
}

bool IsDevToolsFrontend(const GURL& url) {
  return url.SchemeIs(content::kChromeDevToolsScheme) && url.host() == kHost;
}

WegletDevToolsUI::WegletDevToolsUI(content::WebUI* web_ui)
    : content::WebUIController(web_ui) {
  web_ui->SetBindings(content::BindingsPolicySet());
  content::URLDataSource::Add(web_ui->GetWebContents()->GetBrowserContext(),
                              std::make_unique<DevToolsDataSource>());
}

WegletDevToolsUI::~WegletDevToolsUI() = default;

}  // namespace weglet
