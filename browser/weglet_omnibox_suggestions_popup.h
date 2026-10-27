// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The address bar's dropdown: bookmarks and history matching what is
// typed, plus a highlighted row for arrow-key/Enter selection.

#ifndef WEGLET_BROWSER_WEGLET_OMNIBOX_SUGGESTIONS_POPUP_H_
#define WEGLET_BROWSER_WEGLET_OMNIBOX_SUGGESTIONS_POPUP_H_

#include <memory>
#include <vector>

#include "base/memory/raw_ptr.h"
#include "content/public/browser/web_contents_delegate.h"
#include "ui/gfx/geometry/rect.h"
#include "ui/views/widget/widget_delegate.h"
#include "weglet/browser/weglet_state_service.h"

namespace content {
class WebContents;
}

namespace views {
class WebView;
class Widget;
}  // namespace views

namespace weglet {

class WegletWindow;

// A real second top-level widget, same reasoning as WegletToastPopup --
// the toolbar's own WebContents is clipped to the chrome strip and
// cannot draw a list that overlaps tab content below it. Non-activatable
// like Toast: the address bar keeps keyboard focus while this shows.
class WegletOmniboxSuggestionsPopup : public views::WidgetDelegate,
                                      public content::WebContentsDelegate {
 public:
  using Suggestion = WegletStateService::PendingOmniboxSuggestion;

  // `owner` and `state` must outlive this popup.
  WegletOmniboxSuggestionsPopup(WegletWindow* owner, WegletStateService* state);
  WegletOmniboxSuggestionsPopup(const WegletOmniboxSuggestionsPopup&) = delete;
  WegletOmniboxSuggestionsPopup& operator=(const WegletOmniboxSuggestionsPopup&) = delete;
  ~WegletOmniboxSuggestionsPopup() override;

  // Creates the widget on first use; otherwise repositions, reloads and
  // re-shows it. `anchor_left`/`anchor_bottom` are the address bar's own
  // on-screen rect, in the toolbar page's own coordinates.
  void Show(std::vector<Suggestion> items, int highlighted_index,
           int anchor_left, int anchor_bottom);
  void Hide();
  bool IsVisible() const;

  // views::WidgetDelegate:
  views::Widget* GetWidget() override;
  const views::Widget* GetWidget() const override;

  // content::WebContentsDelegate:
  void CloseContents(content::WebContents* source) override;

 private:
  void EnsureWidget();
  gfx::Rect BoundsFor(int item_count, int anchor_left, int anchor_bottom) const;

  const raw_ptr<WegletWindow> owner_;
  const raw_ptr<WegletStateService> state_;

  // CLIENT_OWNS_WIDGET, matching WegletWindow's own widget.
  std::unique_ptr<views::Widget> widget_;
  // Owned by the widget's view tree.
  raw_ptr<views::WebView> web_view_ = nullptr;
  std::unique_ptr<content::WebContents> contents_;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_OMNIBOX_SUGGESTIONS_POPUP_H_
