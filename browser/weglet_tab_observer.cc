// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Turns one tab's engine events into calls on its window.

#include "weglet/browser/weglet_tab_observer.h"

#include <string>

#include "base/strings/utf_string_conversions.h"
#include "content/public/browser/navigation_entry.h"
#include "content/public/browser/navigation_handle.h"
#include "url/gurl.h"
#include "weglet/browser/weglet_window.h"

namespace weglet {

WegletTabObserver::WegletTabObserver(WegletWindow* window,
                                    uint64_t tab_id,
                                    content::WebContents* contents)
    : content::WebContentsObserver(contents), window_(window), tab_id_(tab_id) {}

WegletTabObserver::~WegletTabObserver() = default;

void WegletTabObserver::DidFinishNavigation(
    content::NavigationHandle* navigation_handle) {
  // Subframes and uncommitted navigations are not where the tab is. An
  // error page is: hiding it would leave the address bar on the previous
  // page.
  if (!navigation_handle->IsInPrimaryMainFrame() ||
      !navigation_handle->HasCommitted()) {
    return;
  }

  window_->OnTabBackForwardListChanged(tab_id_,
                               /*new_document=*/!navigation_handle->IsSameDocument());
}

void WegletTabObserver::NavigationListPruned(
    const content::PrunedDetails& pruned_details) {
  window_->OnTabBackForwardListChanged(tab_id_, /*new_document=*/false);
}

void WegletTabObserver::NavigationEntriesDeleted() {
  window_->OnTabBackForwardListChanged(tab_id_, /*new_document=*/false);
}

void WegletTabObserver::TitleWasSet(content::NavigationEntry* entry) {
  const std::u16string title = entry ? entry->GetTitle() : std::u16string();
  window_->OnTabTitleChanged(tab_id_, base::UTF16ToUTF8(title));
}

void WegletTabObserver::DidStartLoading() {
  window_->OnTabLoadingChanged(tab_id_, true);
}

void WegletTabObserver::DidStopLoading() {
  window_->OnTabLoadingChanged(tab_id_, false);
}

}  // namespace weglet
