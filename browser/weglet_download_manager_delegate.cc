// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_download_manager_delegate.h"

#include <shlobj.h>

#include <string_view>
#include <utility>

#include "base/functional/bind.h"
#include "base/win/scoped_co_mem.h"
#include "components/download/public/common/download_interrupt_reasons.h"
#include "components/download/public/common/download_item.h"
#include "components/download/public/common/download_target_info.h"
#include "content/public/browser/download_item_utils.h"
#include "content/public/browser/save_page_type.h"
#include "content/public/browser/web_contents.h"
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

}  // namespace

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

  if (bridge_ && bridge_->AskWhereToSaveDownloads()) {
    pending_dialog_ = PendingDialog::kDownloadTarget;
    download_target_callback_ = std::move(*callback);
    select_file_dialog_ = ui::SelectFileDialog::Create(this, nullptr);
    if (!select_file_dialog_) {
      FileSelectionCanceled();
      return true;
    }
    content::WebContents* web_contents =
        content::DownloadItemUtils::GetWebContents(item);
    select_file_dialog_->SelectFile(
        ui::SelectFileDialog::SELECT_SAVEAS_FILE, std::u16string(),
        requested_path, nullptr, /*file_type_index=*/0,
        base::FilePath::StringType(),
        web_contents ? web_contents->GetTopLevelNativeWindow() : nullptr,
        nullptr);
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
  can_save_as_complete_ = can_save_as_complete;
  pending_dialog_ = PendingDialog::kSavePackage;
  save_package_callback_ = std::move(callback);

  select_file_dialog_ = ui::SelectFileDialog::Create(this, nullptr);
  if (!select_file_dialog_) {
    FileSelectionCanceled();
    return;
  }

  ui::SelectFileDialog::FileTypeInfo file_types;
  file_types.extensions = {{default_extension}};

  select_file_dialog_->SelectFile(
      ui::SelectFileDialog::SELECT_SAVEAS_FILE, std::u16string(),
      suggested_path, &file_types, /*file_type_index=*/1, default_extension,
      web_contents->GetTopLevelNativeWindow(), nullptr);
}

void WegletDownloadManagerDelegate::FileSelected(const ui::SelectedFileInfo& file,
                                                  int index) {
  const PendingDialog dialog = pending_dialog_;
  pending_dialog_ = PendingDialog::kNone;

  if (dialog == PendingDialog::kDownloadTarget) {
    download::DownloadTargetInfo target_info;
    target_info.target_path = file.path();
    target_info.intermediate_path =
        file.path().AddExtension(FILE_PATH_LITERAL(".crdownload"));
    std::move(download_target_callback_).Run(std::move(target_info));
    return;
  }

  content::SavePackagePathPickedParams params;
  params.file_path = file.path();
  params.save_type = can_save_as_complete_ ? content::SAVE_PAGE_TYPE_AS_COMPLETE_HTML
                                           : content::SAVE_PAGE_TYPE_AS_ONLY_HTML;
  std::move(save_package_callback_)
      .Run(std::move(params), content::SavePackageDownloadCreatedCallback());
}

void WegletDownloadManagerDelegate::FileSelectionCanceled() {
  const PendingDialog dialog = pending_dialog_;
  pending_dialog_ = PendingDialog::kNone;

  if (dialog == PendingDialog::kDownloadTarget) {
    download::DownloadTargetInfo target_info;
    target_info.interrupt_reason = download::DOWNLOAD_INTERRUPT_REASON_USER_CANCELED;
    std::move(download_target_callback_).Run(std::move(target_info));
    return;
  }

  save_package_callback_.Reset();
}

void WegletDownloadManagerDelegate::GetSaveDir(
    content::BrowserContext* browser_context,
    base::FilePath* website_save_dir,
    base::FilePath* download_save_dir) {
  *download_save_dir = GetDownloadsFolder();
}

}  // namespace weglet
