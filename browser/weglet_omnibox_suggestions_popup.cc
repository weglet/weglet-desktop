// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_omnibox_suggestions_popup.h"

#include <algorithm>
#include <utility>

#include "content/public/browser/navigation_controller.h"
#include "content/public/browser/web_contents.h"
#include "ui/base/page_transition_types.h"
#include "ui/views/controls/webview/webview.h"
#include "ui/views/widget/widget.h"
#include "weglet/browser/weglet_window.h"
#include "weglet/common/weglet_host.h"
#include "weglet/ui/generated_contract.h"

namespace weglet {

namespace {

constexpr int kWidth = 360;
constexpr int kItemHeight = 40;
constexpr int kChrome = 8;
// Same margin WegletShortcutPopup uses below its own anchor.
constexpr int kMargin = 4;

}  // namespace

WegletOmniboxSuggestionsPopup::WegletOmniboxSuggestionsPopup(WegletWindow* owner,
                                                             WegletStateService* state)
    : owner_(owner), state_(state) {}

WegletOmniboxSuggestionsPopup::~WegletOmniboxSuggestionsPopup() {
  if (state_ && contents_) {
    state_->ClearPendingOmniboxSuggestions(contents_.get());
  }
  if (web_view_) {
    web_view_->SetWebContents(nullptr);
    web_view_ = nullptr;
  }
  widget_.reset();
}

gfx::Rect WegletOmniboxSuggestionsPopup::BoundsFor(int item_count, int anchor_left,
                                                   int anchor_bottom) const {
  const gfx::Rect client = owner_->GetWidget()->GetClientAreaBoundsInScreen();
  const int width = std::min(kWidth, client.width());
  const int height =
      std::min(kItemHeight * std::max(item_count, 1) + kChrome, client.height());
  const int max_x = client.x() + client.width() - width;
  const int x = std::clamp(client.x() + anchor_left, client.x(), std::max(client.x(), max_x));
  const int max_y = client.y() + client.height() - height;
  const int y = std::clamp(client.y() + anchor_bottom + kMargin, client.y(),
                           std::max(client.y(), max_y));
  return gfx::Rect(x, y, width, height);
}

void WegletOmniboxSuggestionsPopup::EnsureWidget() {
  if (widget_) {
    return;
  }

  content::WebContents::CreateParams params(owner_->browser_context());
  contents_ = content::WebContents::Create(params);
  contents_->SetDelegate(this);
  owner_->RegisterWebContents(contents_.get());

  auto web_view = std::make_unique<views::WebView>(owner_->browser_context());
  views::WebView* web_view_ptr = web_view.get();

  views::Widget::InitParams init_params(
      views::Widget::InitParams::CLIENT_OWNS_WIDGET,
      views::Widget::InitParams::TYPE_POPUP);
  init_params.delegate = this;
  init_params.parent = owner_->GetWidget()->GetNativeView();
  init_params.bounds = BoundsFor(1, 0, 0);
  // Never wants keyboard focus -- the address bar keeps it while this
  // shows. It still accepts clicks on its own rows.
  init_params.activatable = views::Widget::InitParams::Activatable::kNo;

  widget_ = std::make_unique<views::Widget>();
  widget_->Init(std::move(init_params));
  widget_->SetContentsView(std::move(web_view));
  web_view_ = web_view_ptr;
  web_view_->SetWebContents(contents_.get());
}

void WegletOmniboxSuggestionsPopup::Show(std::vector<Suggestion> items,
                                         int highlighted_index, int anchor_left,
                                         int anchor_bottom) {
  EnsureWidget();

  const int item_count = static_cast<int>(items.size());
  state_->SetPendingOmniboxSuggestions(
      contents_.get(), {std::move(items), highlighted_index});

  // Reloaded so the page's own script re-runs against the fresh list --
  // same reasoning as WegletToastPopup and WegletContextMenu.
  content::NavigationController::LoadURLParams load(
      PageUrl(contract::kOmniboxSuggestionsPath));
  load.transition_type = ui::PAGE_TRANSITION_AUTO_TOPLEVEL;
  contents_->GetController().LoadURLWithParams(load);

  widget_->SetBounds(BoundsFor(item_count, anchor_left, anchor_bottom));
  widget_->Show();
}

void WegletOmniboxSuggestionsPopup::Hide() {
  if (!widget_) {
    return;
  }
  if (contents_) {
    state_->ClearPendingOmniboxSuggestions(contents_.get());
  }
  widget_->Hide();
}

bool WegletOmniboxSuggestionsPopup::IsVisible() const {
  return widget_ && widget_->IsVisible();
}

views::Widget* WegletOmniboxSuggestionsPopup::GetWidget() {
  return widget_.get();
}

const views::Widget* WegletOmniboxSuggestionsPopup::GetWidget() const {
  return widget_.get();
}

void WegletOmniboxSuggestionsPopup::CloseContents(content::WebContents* source) {
  Hide();
}

}  // namespace weglet
