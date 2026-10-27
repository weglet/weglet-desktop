// Copyright 2026 Weglet - Licensed under Apache 2.0

#include "weglet/browser/weglet_profile_manager.h"

#include "base/logging.h"
#include "weglet/browser/weglet_bridge.h"
#include "weglet/browser/weglet_browser_context.h"
#include "weglet/browser/weglet_download_observer.h"
#include "weglet/browser/weglet_security_guard.h"
#include "weglet/browser/weglet_state_service.h"
#include "weglet/browser/weglet_web_ui_data_source.h"

namespace weglet {

WegletProfileManager::Instance::Instance() = default;
WegletProfileManager::Instance::~Instance() = default;

WegletProfileManager::WegletProfileManager(WindowsSpellChecker* spell_checker)
    : spell_checker_(spell_checker) {}

WegletProfileManager::~WegletProfileManager() {
  for (auto& [name, instance] : instances_) {
    if (instance->bridge && !instance->bridge->SaveSession()) {
      LOG(ERROR) << "could not save the session for " << name << " -- open tabs will be lost";
    }
    if (instance->bridge && instance->bridge->SettingsDirty() &&
        !instance->bridge->FlushSettings()) {
      LOG(ERROR) << "could not write settings for " << name << " -- the last changes are lost";
    }
    // Before the bridge: the state service is owned by the context and
    // holds a pointer to it.
    instance->browser_context.reset();
    instance->bridge.reset();
  }
  instances_.clear();
}

WegletProfileManager::Instance& WegletProfileManager::GetOrCreate(
    const std::string& profile_name) {
  auto found = instances_.find(profile_name);
  if (found != instances_.end()) {
    return *found->second;
  }

  auto instance = std::make_unique<Instance>();
  instance->browser_context =
      std::make_unique<WegletBrowserContext>(/*off_the_record=*/false, profile_name);
  AddWegletWebUIDataSource(instance->browser_context.get());

  instance->bridge = std::make_unique<WegletBridge>(profile_name);
  instance->browser_context->download_manager_delegate()->SetBridge(instance->bridge.get());

  WegletSecurityGuard::CreateForBrowserContext(instance->browser_context.get(),
                                               instance->bridge.get());
  WegletStateService::CreateForBrowserContext(
      instance->browser_context.get(), instance->bridge.get(),
      WegletSecurityGuard::FromBrowserContext(instance->browser_context.get()),
      instance->browser_context->permission_delegate(), spell_checker_);

  instance->download_observer = std::make_unique<WegletDownloadObserver>(
      instance->browser_context->GetDownloadManager(), instance->bridge.get(),
      WegletStateService::FromBrowserContext(instance->browser_context.get()));

  Instance& ref = *instance;
  instances_.emplace(profile_name, std::move(instance));
  return ref;
}

void WegletProfileManager::FlushDirtySettings() {
  for (auto& [name, instance] : instances_) {
    if (instance->bridge && instance->bridge->SettingsDirty() &&
        !instance->bridge->FlushSettings()) {
      LOG(ERROR) << "could not write settings for " << name
                 << " -- changes are not being saved";
    }
  }
}

void WegletProfileManager::SaveAllSessions() {
  for (auto& [name, instance] : instances_) {
    if (instance->bridge && !instance->bridge->SaveSession()) {
      LOG(ERROR) << "could not save the session for " << name;
    }
  }
}

}  // namespace weglet
