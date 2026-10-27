// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// ChooseSavePath is for Ctrl+S. DetermineDownloadTarget and GetNextId are
// for everything else content::DownloadManager hands a download -- an
// ordinary <a download> click, a server Content-Disposition, and so on.
// Without them, content's own defaults leave every regular download with
// no id and no target path (see download_manager_delegate.cc), so nothing
// downloads at all.

#ifndef WEGLET_BROWSER_WEGLET_DOWNLOAD_MANAGER_DELEGATE_H_
#define WEGLET_BROWSER_WEGLET_DOWNLOAD_MANAGER_DELEGATE_H_

#include <cstdint>
#include <deque>
#include <optional>

#include "base/files/file_path.h"
#include "base/memory/raw_ptr.h"
#include "base/memory/scoped_refptr.h"
#include "base/memory/weak_ptr.h"
#include "components/download/public/common/download_path_reservation_tracker.h"
#include "content/public/browser/download_manager_delegate.h"
#include "ui/shell_dialogs/select_file_dialog.h"

namespace content {
class WebContents;
}

namespace weglet {

class WegletBridge;

class WegletDownloadManagerDelegate : public content::DownloadManagerDelegate,
                                      public ui::SelectFileDialog::Listener {
 public:
  WegletDownloadManagerDelegate();
  WegletDownloadManagerDelegate(const WegletDownloadManagerDelegate&) = delete;
  WegletDownloadManagerDelegate& operator=(const WegletDownloadManagerDelegate&) = delete;
  ~WegletDownloadManagerDelegate() override;

  // Not available at construction time -- see
  // WegletBrowserMainParts::PreMainMessageLoopRun. Null until set means
  // "ask where to save" reads as off, its default.
  void SetBridge(WegletBridge* bridge) { bridge_ = bridge; }

  // content::DownloadManagerDelegate:
  void GetNextId(content::DownloadIdCallback callback) override;
  bool DetermineDownloadTarget(download::DownloadItem* item,
                              download::DownloadTargetCallback* callback) override;
  void ChooseSavePath(content::WebContents* web_contents,
                      const base::FilePath& suggested_path,
                      const base::FilePath::StringType& default_extension,
                      bool can_save_as_complete,
                      content::SavePackagePathPickedCallback callback) override;
  // The default is empty paths, and SavePackage DCHECK-crashes trying to
  // create a directory at an empty one.
  void GetSaveDir(content::BrowserContext* browser_context,
                  base::FilePath* website_save_dir,
                  base::FilePath* download_save_dir) override;
  // Marks every finished download as coming from the internet (the
  // Mark-of-the-Web), which is what makes SmartScreen and Office's
  // Protected View check the file when it is opened. Without it a
  // downloaded program runs as if it had been on the machine all along.
  download::QuarantineConnectionCallback GetQuarantineConnectionCallback()
      override;

 private:
  // One Save As dialog: a download's target or Ctrl+S. Exactly one of the
  // two callbacks is set.
  struct DialogRequest {
    DialogRequest();
    DialogRequest(DialogRequest&&);
    DialogRequest& operator=(DialogRequest&&);
    ~DialogRequest();

    download::DownloadTargetCallback download_target;
    content::SavePackagePathPickedCallback save_package;
    base::FilePath suggested_path;
    // Ctrl+S only: the extension the page suggests, and whether it can be
    // saved with its resources.
    base::FilePath::StringType default_extension;
    bool can_save_as_complete = false;
    // The dialog's parent window. The tab may close while the request
    // waits for an earlier dialog.
    base::WeakPtr<content::WebContents> web_contents;
  };

  // ui::SelectFileDialog::Listener:
  void FileSelected(const ui::SelectedFileInfo& file, int index) override;
  void FileSelectionCanceled() override;

  // Queues a dialog and shows it once every earlier one has been answered.
  // A second download can arrive while the first one's dialog is open.
  void RequestDialog(DialogRequest request);
  // Opens the dialog for the front of the queue. Answers it as cancelled
  // when no dialog can be created, and moves on to the next.
  void ShowNextDialog();
  // Runs the front request's callback with `path`, or as cancelled when it
  // is empty, and removes it from the queue.
  void FinishFrontDialog(std::optional<base::FilePath> path);

  void OnTargetPathReserved(download::DownloadTargetCallback callback,
                            download::PathValidationResult result,
                            const base::FilePath& target_path);

  uint32_t next_download_id_ = 1;

  // Not owned; outlives this (see SetBridge). Null before it is set and
  // for the lifetime of tests that never call it.
  raw_ptr<WegletBridge> bridge_ = nullptr;

  // The front request is the one whose dialog is open, while
  // is_dialog_open_ says one is.
  std::deque<DialogRequest> dialog_queue_;
  bool is_dialog_open_ = false;
  scoped_refptr<ui::SelectFileDialog> select_file_dialog_;

  base::WeakPtrFactory<WegletDownloadManagerDelegate> weak_factory_{this};
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_DOWNLOAD_MANAGER_DELEGATE_H_
