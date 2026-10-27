// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_download_manager_delegate.h"

#include <shlobj.h>
#include <shlwapi.h>

#include <string_view>
#include <utility>

#include "base/functional/bind.h"
#include "base/no_destructor.h"
#include "base/task/single_thread_task_runner.h"
#include "base/task/thread_pool.h"
#include "base/win/scoped_co_mem.h"
#include "components/download/public/common/download_interrupt_reasons.h"
#include "components/download/public/common/download_item.h"
#include "components/download/public/common/download_target_info.h"
#include "components/services/quarantine/quarantine_impl.h"
#include "content/public/browser/download_item_utils.h"
#include "content/public/browser/save_page_type.h"
#include "content/public/browser/web_contents.h"
#include "mojo/public/cpp/bindings/self_owned_receiver.h"
#include "net/base/filename_util.h"
#include "ui/shell_dialogs/select_file_policy.h"
#include "ui/shell_dialogs/selected_file_info.h"
#include "weglet/browser/weglet_bridge.h"

namespace weglet {

namespace {

base::FilePath GetDownloadsFolder() {
  base::win::ScopedCoMem<wchar_t> path_buf;
  if (SUCCEEDED(::SHGetKnownFolderPath(FOLDERID_Downloads, 0, nullptr, &path_buf))) {
    return base::FilePath(std::wstring_view(path_buf.get()));
  }
  return base::FilePath();
}

// In the browser process, on a COM thread of its own: the Windows
// attachment API it calls needs COM and may run a virus scanner, which
// blocks. Rejected: a utility process as Chrome uses, which would keep a
// crashing scanner out of the browser process but needs a
// ContentUtilityClient that registers the service.
void BindQuarantine(
    mojo::PendingReceiver<quarantine::mojom::Quarantine> receiver) {
  static base::NoDestructor<scoped_refptr<base::SingleThreadTaskRunner>>
      task_runner(base::ThreadPool::CreateCOMSTATaskRunner(
          {base::MayBlock(), base::TaskPriority::USER_VISIBLE}));
  (*task_runner)
      ->PostTask(FROM_HERE,
                 base::BindOnce(
                     [](mojo::PendingReceiver<quarantine::mojom::Quarantine>
                            receiver) {
                       mojo::MakeSelfOwnedReceiver(
                           std::make_unique<quarantine::QuarantineImpl>(),
                           std::move(receiver));
                     },
                     std::move(receiver)));
}

// A program, a script, a shortcut: anything Windows itself treats as able
// to run code when opened. Asked of the OS rather than a list kept here,
// so it stays in step with what the OS will actually execute. By extension:
// AssocIsDangerous takes a file type, and answers false for a full path to
// an .exe.
bool CanRunCode(const base::FilePath& path) {
  const base::FilePath::StringType extension = path.Extension();
  return !extension.empty() && ::AssocIsDangerous(extension.c_str());
}

}  // namespace

WegletDownloadManagerDelegate::DialogRequest::DialogRequest() = default;
WegletDownloadManagerDelegate::DialogRequest::DialogRequest(DialogRequest&&) =
    default;
WegletDownloadManagerDelegate::DialogRequest&
WegletDownloadManagerDelegate::DialogRequest::operator=(DialogRequest&&) =
    default;
WegletDownloadManagerDelegate::DialogRequest::~DialogRequest() = default;

WegletDownloadManagerDelegate::WegletDownloadManagerDelegate() = default;

WegletDownloadManagerDelegate::~WegletDownloadManagerDelegate() {
  if (select_file_dialog_) {
    select_file_dialog_->ListenerDestroyed();
  }
}

void WegletDownloadManagerDelegate::GetNextId(content::DownloadIdCallback callback) {
  std::move(callback).Run(next_download_id_++);
}

bool WegletDownloadManagerDelegate::DetermineDownloadTarget(
    download::DownloadItem* item,
    download::DownloadTargetCallback* callback) {
  if (!item->GetForcedFilePath().empty()) {
    download::DownloadTargetInfo target_info;
    target_info.target_path = item->GetForcedFilePath();
    target_info.intermediate_path = item->GetForcedFilePath();
    std::move(*callback).Run(std::move(target_info));
    return true;
  }

  const base::FilePath downloads_dir = GetDownloadsFolder();
  const base::FilePath requested_path = downloads_dir.Append(net::GenerateFileName(
      item->GetURL(), item->GetContentDisposition(), /*referrer_charset=*/std::string(),
      item->GetSuggestedFilename(), item->GetMimeType(), "download"));

  // A file that can run code is never saved without a dialog, whatever the
  // setting: otherwise any page can put a program in Downloads without a
  // click.
  if (CanRunCode(requested_path) ||
      (bridge_ && bridge_->AskWhereToSaveDownloads())) {
    DialogRequest request;
    request.download_target = std::move(*callback);
    request.suggested_path = requested_path;
    if (content::WebContents* web_contents =
            content::DownloadItemUtils::GetWebContents(item)) {
      request.web_contents = web_contents->GetWeakPtr();
    }
    RequestDialog(std::move(request));
    return true;
  }

  download::DownloadPathReservationTracker::GetReservedPath(
      item, requested_path, downloads_dir, downloads_dir,
      /*create_directory=*/true,
      download::DownloadPathReservationTracker::UNIQUIFY,
      base::BindOnce(&WegletDownloadManagerDelegate::OnTargetPathReserved,
                     weak_factory_.GetWeakPtr(), std::move(*callback)));
  return true;
}

void WegletDownloadManagerDelegate::OnTargetPathReserved(
    download::DownloadTargetCallback callback,
    download::PathValidationResult result,
    const base::FilePath& target_path) {
  download::DownloadTargetInfo target_info;
  target_info.target_path = target_path;
  target_info.intermediate_path =
      target_path.empty() ? target_path
                          : target_path.AddExtension(FILE_PATH_LITERAL(".crdownload"));
  if (target_path.empty()) {
    target_info.interrupt_reason = download::DOWNLOAD_INTERRUPT_REASON_FILE_FAILED;
  }
  std::move(callback).Run(std::move(target_info));
}

void WegletDownloadManagerDelegate::ChooseSavePath(
    content::WebContents* web_contents,
    const base::FilePath& suggested_path,
    const base::FilePath::StringType& default_extension,
    bool can_save_as_complete,
    content::SavePackagePathPickedCallback callback) {
  DialogRequest request;
  request.save_package = std::move(callback);
  request.suggested_path = suggested_path;
  request.default_extension = default_extension;
  request.can_save_as_complete = can_save_as_complete;
  request.web_contents = web_contents->GetWeakPtr();
  RequestDialog(std::move(request));
}

void WegletDownloadManagerDelegate::RequestDialog(DialogRequest request) {
  dialog_queue_.push_back(std::move(request));
  if (dialog_queue_.size() == 1) {
    ShowNextDialog();
  }
}

void WegletDownloadManagerDelegate::ShowNextDialog() {
  // One open at a time: a callback run by FinishFrontDialog can queue a
  // request, and RequestDialog may already have opened it.
  if (is_dialog_open_) {
    return;
  }
  while (!dialog_queue_.empty()) {
    select_file_dialog_ = ui::SelectFileDialog::Create(this, nullptr);
    if (!select_file_dialog_) {
      FinishFrontDialog(std::nullopt);
      continue;
    }

    const DialogRequest& request = dialog_queue_.front();
    const bool is_save_package = !request.save_package.is_null();
    ui::SelectFileDialog::FileTypeInfo file_types;
    if (is_save_package) {
      file_types.extensions = {{request.default_extension}};
    }
    select_file_dialog_->SelectFile(
        ui::SelectFileDialog::SELECT_SAVEAS_FILE, std::u16string(),
        request.suggested_path, is_save_package ? &file_types : nullptr,
        /*file_type_index=*/is_save_package ? 1 : 0, request.default_extension,
        request.web_contents ? request.web_contents->GetTopLevelNativeWindow()
                             : gfx::NativeWindow(),
        nullptr);
    is_dialog_open_ = true;
    return;
  }
}

void WegletDownloadManagerDelegate::FinishFrontDialog(
    std::optional<base::FilePath> path) {
  if (dialog_queue_.empty()) {
    return;
  }
  DialogRequest request = std::move(dialog_queue_.front());
  dialog_queue_.pop_front();

  if (!request.download_target.is_null()) {
    download::DownloadTargetInfo target_info;
    if (path) {
      target_info.target_path = *path;
      target_info.intermediate_path =
          path->AddExtension(FILE_PATH_LITERAL(".crdownload"));
    } else {
      target_info.interrupt_reason =
          download::DOWNLOAD_INTERRUPT_REASON_USER_CANCELED;
    }
    std::move(request.download_target).Run(std::move(target_info));
    return;
  }

  // A cancelled Ctrl+S has nothing to report: dropping the callback is how
  // SavePackage learns of it.
  if (!path) {
    return;
  }
  content::SavePackagePathPickedParams params;
  params.file_path = *path;
  params.save_type = request.can_save_as_complete
                         ? content::SAVE_PAGE_TYPE_AS_COMPLETE_HTML
                         : content::SAVE_PAGE_TYPE_AS_ONLY_HTML;
  std::move(request.save_package)
      .Run(std::move(params), content::SavePackageDownloadCreatedCallback());
}

void WegletDownloadManagerDelegate::FileSelected(const ui::SelectedFileInfo& file,
                                                  int index) {
  is_dialog_open_ = false;
  FinishFrontDialog(file.path());
  ShowNextDialog();
}

void WegletDownloadManagerDelegate::FileSelectionCanceled() {
  is_dialog_open_ = false;
  FinishFrontDialog(std::nullopt);
  ShowNextDialog();
}

void WegletDownloadManagerDelegate::GetSaveDir(
    content::BrowserContext* browser_context,
    base::FilePath* website_save_dir,
    base::FilePath* download_save_dir) {
  *download_save_dir = GetDownloadsFolder();
}

download::QuarantineConnectionCallback
WegletDownloadManagerDelegate::GetQuarantineConnectionCallback() {
  return base::BindRepeating(&BindQuarantine);
}

}  // namespace weglet
