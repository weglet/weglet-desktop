// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_devtools_bindings.h"

#include <string_view>
#include <utility>
#include <vector>

#include "base/base64.h"
#include "base/functional/bind.h"
#include "base/strings/string_number_conversions.h"
#include "base/strings/string_util.h"
#include "base/strings/utf_string_conversions.h"
#include "content/public/browser/navigation_handle.h"
#include "content/public/browser/render_frame_host.h"
#include "content/public/browser/storage_partition.h"
#include "content/public/browser/web_contents.h"
#include "ipc/constants.mojom.h"
#include "net/http/http_response_headers.h"
#include "net/traffic_annotation/network_traffic_annotation.h"
#include "services/network/public/cpp/resource_request.h"
#include "services/network/public/cpp/simple_url_loader.h"
#include "services/network/public/cpp/simple_url_loader_stream_consumer.h"
#include "services/network/public/mojom/url_response_head.mojom.h"
#include "weglet/browser/weglet_devtools_ui.h"
#include "weglet/browser/weglet_window.h"

namespace weglet {

namespace {

base::DictValue BuildObjectForResponse(const net::HttpResponseHeaders* rh,
                                       bool success,
                                       int net_error) {
  base::DictValue response;
  int response_code = 200;
  if (rh) {
    response_code = rh->response_code();
  } else if (!success) {
    response_code = 404;
  }
  response.Set("statusCode", response_code);
  response.Set("netError", net_error);
  response.Set("netErrorName", net::ErrorToString(net_error));

  base::DictValue headers;
  size_t iterator = 0;
  std::string name;
  std::string value;
  while (rh && rh->EnumerateHeaderLines(&iterator, &name, &value)) {
    headers.Set(name, value);
  }
  response.Set("headers", std::move(headers));
  return response;
}

// Same reasoning as content_shell's own copy: kept in sync with
// kMaxMessageChunkSize in chrome/browser/devtools/devtools_ui_bindings.cc.
constexpr size_t kMaxMessageChunkSize =
    IPC::mojom::kChannelMaximumMessageSize / 4;

}  // namespace

class WegletDevToolsBindings::NetworkResourceLoader
    : public network::SimpleURLLoaderStreamConsumer {
 public:
  NetworkResourceLoader(int stream_id,
                        int request_id,
                        WegletDevToolsBindings* bindings,
                        std::unique_ptr<network::SimpleURLLoader> loader,
                        network::mojom::URLLoaderFactory* url_loader_factory)
      : stream_id_(stream_id),
        request_id_(request_id),
        bindings_(bindings),
        loader_(std::move(loader)) {
    loader_->SetOnResponseStartedCallback(base::BindOnce(
        &NetworkResourceLoader::OnResponseStarted, base::Unretained(this)));
    loader_->DownloadAsStream(url_loader_factory, this);
  }

  NetworkResourceLoader(const NetworkResourceLoader&) = delete;
  NetworkResourceLoader& operator=(const NetworkResourceLoader&) = delete;

 private:
  void OnResponseStarted(const GURL& final_url,
                         const network::mojom::URLResponseHead& response_head) {
    response_headers_ = response_head.headers;
  }

  void OnDataReceived(std::string_view chunk, base::OnceClosure resume) override {
    bool encoded = !base::IsStringUTF8(chunk);
    base::Value chunk_value =
        encoded ? base::Value(base::Base64Encode(chunk)) : base::Value(chunk);
    bindings_->CallClientFunction("DevToolsAPI", "streamWrite",
                                  base::Value(stream_id_),
                                  std::move(chunk_value), base::Value(encoded));
    std::move(resume).Run();
  }

  void OnComplete(bool success) override {
    base::DictValue response =
        BuildObjectForResponse(response_headers_.get(), success, loader_->NetError());
    bindings_->SendMessageAck(request_id_, std::move(response));
    bindings_->loaders_.erase(bindings_->loaders_.find(this));
  }

  void OnRetry(base::OnceClosure start_retry) override { NOTREACHED(); }

  const int stream_id_;
  const int request_id_;
  const raw_ptr<WegletDevToolsBindings> bindings_;
  std::unique_ptr<network::SimpleURLLoader> loader_;
  scoped_refptr<net::HttpResponseHeaders> response_headers_;
};

WegletDevToolsBindings::WegletDevToolsBindings(
    content::WebContents* devtools_contents,
    content::WebContents* inspected_contents,
    WegletWindow* owner)
    : content::WebContentsObserver(devtools_contents),
      inspected_contents_(inspected_contents),
      owner_(owner) {}

WegletDevToolsBindings::~WegletDevToolsBindings() {
  if (agent_host_) {
    agent_host_->DetachClient(this);
  }
}

void WegletDevToolsBindings::ReadyToCommitNavigation(
    content::NavigationHandle* navigation_handle) {
  content::RenderFrameHost* frame = navigation_handle->GetRenderFrameHost();
  if (navigation_handle->IsInPrimaryMainFrame()) {
    // The front-end is an ordinary tab, and its address bar can take it
    // anywhere. Only the front-end's own origin gets the channel: on any
    // other page it would be a protocol connection to the inspected tab.
    if (!IsDevToolsFrontend(navigation_handle->GetURL())) {
      frontend_host_.reset();
      if (agent_host_) {
        agent_host_->DetachClient(this);
        agent_host_ = nullptr;
      }
      return;
    }
    frontend_host_ = content::DevToolsFrontendHost::Create(
        frame,
        base::BindRepeating(&WegletDevToolsBindings::HandleMessageFromDevToolsFrontend,
                            base::Unretained(this)));
    return;
  }
  std::string origin = navigation_handle->GetURL().DeprecatedGetOriginAsURL().spec();
  auto it = extensions_api_.find(origin);
  if (it == extensions_api_.end()) {
    return;
  }
  content::DevToolsFrontendHost::SetupExtensionsAPI(frame, it->second);
}

void WegletDevToolsBindings::InspectElementAt(int x, int y) {
  if (agent_host_) {
    agent_host_->InspectElement(inspected_contents_->GetFocusedFrame(), x, y);
  } else {
    inspect_element_at_x_ = x;
    inspect_element_at_y_ = y;
  }
}

void WegletDevToolsBindings::AttachInternal() {
  if (agent_host_) {
    agent_host_->DetachClient(this);
  }
  agent_host_ = content::DevToolsAgentHost::GetOrCreateForTab(inspected_contents_);
  agent_host_->AttachClient(this);
  if (inspect_element_at_x_ != -1) {
    agent_host_->InspectElement(inspected_contents_->GetFocusedFrame(),
                                inspect_element_at_x_, inspect_element_at_y_);
    inspect_element_at_x_ = -1;
    inspect_element_at_y_ = -1;
  }
}

void WegletDevToolsBindings::PrimaryMainDocumentElementAvailable() {
  if (frontend_host_) {
    AttachInternal();
  }
}

void WegletDevToolsBindings::WebContentsDestroyed() {
  if (agent_host_) {
    agent_host_->DetachClient(this);
    agent_host_ = nullptr;
  }
  delete this;
}

void WegletDevToolsBindings::HandleMessageFromDevToolsFrontend(base::DictValue message) {
  const std::string* method = message.FindString("method");
  if (!method) {
    return;
  }

  int request_id = message.FindInt("id").value_or(0);
  base::ListValue* params_value = message.FindList("params");
  base::ListValue params;
  if (params_value) {
    params = std::move(*params_value);
  }

  if (*method == "dispatchProtocolMessage" && params.size() == 1) {
    const std::string* protocol_message = params[0].GetIfString();
    if (!agent_host_ || !protocol_message) {
      return;
    }
    agent_host_->DispatchProtocolMessage(this, base::as_byte_span(*protocol_message));
  } else if (*method == "loadCompleted") {
    CallClientFunction("DevToolsAPI", "setUseSoftMenu", base::Value(true));
  } else if (*method == "loadNetworkResource" && params.size() == 3) {
    const std::string* url = params[0].GetIfString();
    const std::string* headers = params[1].GetIfString();
    std::optional<const int> stream_id = params[2].GetIfInt();
    if (!url || !headers || !stream_id.has_value()) {
      return;
    }

    GURL gurl(*url);
    if (!gurl.is_valid()) {
      base::DictValue response;
      response.Set("statusCode", 404);
      response.Set("urlValid", false);
      SendMessageAck(request_id, std::move(response));
      return;
    }

    net::NetworkTrafficAnnotationTag traffic_annotation =
        net::DefineNetworkTrafficAnnotation("devtools_handle_front_end_messages", R"(
            semantics {
              sender: "Developer Tools"
              description:
                "When user opens Developer Tools, the browser may fetch "
                "additional resources from the network to enrich the debugging "
                "experience (e.g. source map resources)."
              trigger: "User opens Developer Tools to debug a web page."
              data: "Any resources requested by Developer Tools."
              destination: OTHER
            }
            policy {
              cookies_allowed: YES
              cookies_store: "user"
              setting:
                "It's not possible to disable this feature from settings."
              policy_exception_justification:
                "Not implemented."
            })");

    auto resource_request = std::make_unique<network::ResourceRequest>();
    resource_request->url = gurl;
    resource_request->site_for_cookies = net::SiteForCookies::FromUrl(gurl);
    resource_request->headers.AddHeadersFromString(*headers);

    auto* partition = inspected_contents_->GetPrimaryMainFrame()->GetStoragePartition();
    auto factory = partition->GetURLLoaderFactoryForBrowserProcess();

    auto simple_url_loader =
        network::SimpleURLLoader::Create(std::move(resource_request), traffic_annotation);
    auto resource_loader = std::make_unique<NetworkResourceLoader>(
        *stream_id, request_id, this, std::move(simple_url_loader), factory.get());
    loaders_.insert(std::move(resource_loader));
    return;
  } else if (*method == "getPreferences") {
    SendMessageAck(request_id, preferences_.Clone());
    return;
  } else if (*method == "getHostConfig") {
    SendMessageAck(request_id, {});
    return;
  } else if (*method == "setPreference") {
    if (params.size() < 2) {
      return;
    }
    const std::string* name = params[0].GetIfString();
    if (!name || !params[1].is_string()) {
      return;
    }
    preferences_.Set(*name, std::move(params[1]));
  } else if (*method == "removePreference") {
    const std::string* name = params[0].GetIfString();
    if (!name) {
      return;
    }
    preferences_.Remove(*name);
  } else if (*method == "requestFileSystems") {
    CallClientFunction("DevToolsAPI", "fileSystemsLoaded",
                       base::Value(base::Value::Type::LIST));
  } else if (*method == "reattach") {
    if (!agent_host_) {
      return;
    }
    agent_host_->DetachClient(this);
    agent_host_->AttachClient(this);
  } else if (*method == "registerExtensionsAPI") {
    if (params.size() < 2) {
      return;
    }
    const std::string* origin = params[0].GetIfString();
    const std::string* script = params[1].GetIfString();
    if (!origin || !script) {
      return;
    }
    extensions_api_[*origin + "/"] = *script;
  } else {
    return;
  }

  if (request_id) {
    SendMessageAck(request_id, {});
  }
}

void WegletDevToolsBindings::DispatchProtocolMessage(
    content::DevToolsAgentHost* agent_host,
    base::span<const uint8_t> message) {
  std::string_view str_message(reinterpret_cast<const char*>(message.data()),
                               message.size());
  if (str_message.length() < kMaxMessageChunkSize) {
    CallClientFunction("DevToolsAPI", "dispatchMessage",
                       base::Value(std::string(str_message)));
    return;
  }
  size_t total_size = str_message.length();
  for (size_t pos = 0; pos < str_message.length(); pos += kMaxMessageChunkSize) {
    std::string_view chunk = str_message.substr(pos, kMaxMessageChunkSize);
    CallClientFunction("DevToolsAPI", "dispatchMessageChunk",
                       base::Value(std::string(chunk)),
                       base::Value(base::NumberToString(pos ? 0 : total_size)));
  }
}

void WegletDevToolsBindings::CallClientFunction(
    const std::string& object_name,
    const std::string& method_name,
    base::Value arg1,
    base::Value arg2,
    base::Value arg3,
    base::OnceCallback<void(base::Value)> cb) {
  web_contents()->GetPrimaryMainFrame()->AllowInjectingJavaScript();

  base::ListValue arguments;
  if (!arg1.is_none()) {
    arguments.Append(std::move(arg1));
    if (!arg2.is_none()) {
      arguments.Append(std::move(arg2));
      if (!arg3.is_none()) {
        arguments.Append(std::move(arg3));
      }
    }
  }
  web_contents()->GetPrimaryMainFrame()->ExecuteJavaScriptMethod(
      base::ASCIIToUTF16(object_name), base::ASCIIToUTF16(method_name),
      std::move(arguments), std::move(cb));
}

void WegletDevToolsBindings::SendMessageAck(int request_id, base::DictValue arg) {
  CallClientFunction("DevToolsAPI", "embedderMessageAck", base::Value(request_id),
                     base::Value(std::move(arg)));
}

void WegletDevToolsBindings::AgentHostClosed(content::DevToolsAgentHost* agent_host) {
  agent_host_ = nullptr;
  if (owner_) {
    owner_->CloseDevToolsTab(web_contents());
  }
}

bool WegletDevToolsBindings::MayAccessAllCookies() {
  return true;
}

}  // namespace weglet
