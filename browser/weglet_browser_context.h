// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The browsing profile and the subsystems it does not enable.

#ifndef WEGLET_BROWSER_WEGLET_BROWSER_CONTEXT_H_
#define WEGLET_BROWSER_WEGLET_BROWSER_CONTEXT_H_

#include <memory>
#include <string>

#include "base/files/file_path.h"
#include "components/client_hints/browser/in_memory_client_hints_controller_delegate.h"
#include "content/public/browser/browser_context.h"
#include "weglet/browser/weglet_download_manager_delegate.h"
#include "weglet/browser/weglet_permission_delegate.h"

namespace weglet {

// One browsing profile: cookies, cache, storage. Everything on disk hangs
// off the path this returns.
//
// Most getters below return nullptr on purpose. Each is an optional
// subsystem, and each one turned on is another thing that can reach the
// network by itself.
class WegletBrowserContext : public content::BrowserContext {
 public:
  // `profile` names the subdirectory under Weglet's data root -- see
  // ProfilePath.
  WegletBrowserContext(bool off_the_record, const std::string& profile);
  WegletBrowserContext(const WegletBrowserContext&) = delete;
  WegletBrowserContext& operator=(const WegletBrowserContext&) = delete;
  ~WegletBrowserContext() override;

  // content::BrowserContext:
  base::FilePath GetPath() const override;
  bool IsOffTheRecord() override;
  std::unique_ptr<content::ZoomLevelDelegate> CreateZoomLevelDelegate(
      const base::FilePath& partition_path) override;
  content::DownloadManagerDelegate* GetDownloadManagerDelegate() override;
  content::BrowserPluginGuestManager* GetGuestManager() override;
  storage::SpecialStoragePolicy* GetSpecialStoragePolicy() override;
  content::PlatformNotificationService* GetPlatformNotificationService()
      override;
  content::PushMessagingService* GetPushMessagingService() override;
  content::StorageNotificationService* GetStorageNotificationService() override;
  content::SSLHostStateDelegate* GetSSLHostStateDelegate() override;
  content::PermissionControllerDelegate* GetPermissionControllerDelegate()
      override;
  content::ClientHintsControllerDelegate* GetClientHintsControllerDelegate()
      override;
  content::BackgroundFetchDelegate* GetBackgroundFetchDelegate() override;
  content::BackgroundSyncController* GetBackgroundSyncController() override;
  content::BrowsingDataRemoverDelegate* GetBrowsingDataRemoverDelegate()
      override;
  content::ReduceAcceptLanguageControllerDelegate*
  GetReduceAcceptLanguageControllerDelegate() override;

  // For the toolbar's site-info popup, which needs the concrete type --
  // GetPermissionControllerDelegate() above only hands out the interface.
  WegletPermissionDelegate* permission_delegate() { return &permission_delegate_; }

  // For WegletBrowserMainParts to hand the delegate a WegletBridge once
  // one exists -- it does not at construction time. Concrete type, like
  // permission_delegate() above.
  WegletDownloadManagerDelegate* download_manager_delegate() {
    return &download_manager_delegate_;
  }

 private:
  // %LOCALAPPDATA%\Weglet\<profile> on Windows, the XDG data dir
  // elsewhere. Chosen once in the constructor.
  static base::FilePath ProfilePath(const std::string& profile);

  const bool off_the_record_;
  base::FilePath path_;

  // In-memory only: remembers which origins asked for which Client
  // Hints (Accept-CH/Critical-CH). Without this a site's Critical-CH
  // retry never sees the hint it asked for and never converges.
  client_hints::InMemoryClientHintsControllerDelegate client_hints_delegate_;
  WegletPermissionDelegate permission_delegate_;
  // Not nullptr like the rest of this file: Ctrl+S needs somewhere to ask
  // for a save path. It reaches no network itself -- see
  // WegletDownloadManagerDelegate's own comment.
  WegletDownloadManagerDelegate download_manager_delegate_;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_BROWSER_CONTEXT_H_
