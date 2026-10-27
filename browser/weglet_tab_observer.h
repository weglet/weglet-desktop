// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Turns one tab's engine events into calls on its window.

#ifndef WEGLET_BROWSER_WEGLET_TAB_OBSERVER_H_
#define WEGLET_BROWSER_WEGLET_TAB_OBSERVER_H_

#include <cstdint>

#include "base/memory/raw_ptr.h"
#include "content/public/browser/web_contents_observer.h"

namespace weglet {

class WegletWindow;

// Separate from WegletWindow: a window has many tabs, and
// WebContentsObserver watches one.
class WegletTabObserver : public content::WebContentsObserver {
 public:
  // `window` owns this observer, so it outlives it.
  WegletTabObserver(WegletWindow* window,
                    uint64_t tab_id,
                    content::WebContents* contents);
  WegletTabObserver(const WegletTabObserver&) = delete;
  WegletTabObserver& operator=(const WegletTabObserver&) = delete;
  ~WegletTabObserver() override;

  // content::WebContentsObserver:
  void DidFinishNavigation(
      content::NavigationHandle* navigation_handle) override;
  // The engine dropped entries from the back/forward list on its own.
  void NavigationListPruned(
      const content::PrunedDetails& pruned_details) override;
  void NavigationEntriesDeleted() override;
  void TitleWasSet(content::NavigationEntry* entry) override;
  void DidStartLoading() override;
  void DidStopLoading() override;

 private:
  const raw_ptr<WegletWindow> window_;
  const uint64_t tab_id_;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_TAB_OBSERVER_H_
