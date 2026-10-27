// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_download_observer.h"

#include "components/download/public/common/download_interrupt_reasons.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_state_service.h"

namespace weglet {

WegletDownloadObserver::WegletDownloadObserver(
    content::DownloadManager* manager,
    WegletBridge* bridge,
    WegletStateService* state)
    : manager_(manager), bridge_(bridge), state_(state) {
  manager_->AddObserver(this);
}

WegletDownloadObserver::~WegletDownloadObserver() {
  if (manager_) {
    manager_->RemoveObserver(this);
  }
}

void WegletDownloadObserver::OnDownloadCreated(
    content::DownloadManager* manager,
    download::DownloadItem* item) {
  // Target path may still be empty here (see download_manager.h); every
  // update below carries it, so the record gets it once it is set.
  bridge_->DownloadStarted(item->GetGuid(), item->GetURL().spec(),
                           item->GetTargetFilePath().AsUTF8Unsafe());
  item->AddObserver(this);
  state_->Notify(WegletStateService::kHistory);
}

void WegletDownloadObserver::ManagerGoingDown(
    content::DownloadManager* manager) {
  manager_ = nullptr;
}

void WegletDownloadObserver::OnDownloadUpdated(download::DownloadItem* item) {
  const std::string& guid = item->GetGuid();
  const std::string path = item->GetTargetFilePath().AsUTF8Unsafe();
  switch (item->GetState()) {
    case download::DownloadItem::IN_PROGRESS: {
      const int64_t total = item->GetTotalBytes();
      bridge_->DownloadProgress(guid, path, item->GetReceivedBytes(),
                                total > 0 ? total : -1);
      break;
    }
    case download::DownloadItem::COMPLETE:
      bridge_->DownloadCompleted(guid, path, item->GetReceivedBytes());
      break;
    case download::DownloadItem::CANCELLED:
    case download::DownloadItem::INTERRUPTED:
      bridge_->DownloadFailed(
          guid, download::DownloadInterruptReasonToString(item->GetLastReason()));
      break;
    case download::DownloadItem::MAX_DOWNLOAD_STATE:
      break;
  }
  state_->Notify(WegletStateService::kHistory);
}

void WegletDownloadObserver::OnDownloadDestroyed(download::DownloadItem* item) {
  item->RemoveObserver(this);
}

}  // namespace weglet
