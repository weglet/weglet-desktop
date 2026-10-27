// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Every profile this process currently has a window open in. Most runs
// have exactly one (the profile it launched into); "Open in new window"
// on the Settings page's Profiles tab is what creates a second.

#ifndef WEGLET_BROWSER_WEGLET_PROFILE_MANAGER_H_
#define WEGLET_BROWSER_WEGLET_PROFILE_MANAGER_H_

#include <map>
#include <memory>
#include <string>

#include "base/memory/raw_ptr.h"

class WindowsSpellChecker;

namespace weglet {

class WegletBridge;
class WegletBrowserContext;
class WegletDownloadObserver;

class WegletProfileManager {
 public:
  struct Instance {
    Instance();
    Instance(const Instance&) = delete;
    Instance& operator=(const Instance&) = delete;
    ~Instance();

    std::unique_ptr<WegletBrowserContext> browser_context;
    std::unique_ptr<WegletBridge> bridge;
    std::unique_ptr<WegletDownloadObserver> download_observer;
  };

  // `spell_checker` outlives this: it is shared across every profile,
  // not profile data itself.
  explicit WegletProfileManager(WindowsSpellChecker* spell_checker);
  WegletProfileManager(const WegletProfileManager&) = delete;
  WegletProfileManager& operator=(const WegletProfileManager&) = delete;
  ~WegletProfileManager();

  // Builds `profile_name`'s context, bridge and the services that hang
  // off them the first time it is asked for; every later call for the
  // same name returns that same instance. Once created, an instance is
  // never torn down before the whole process exits -- closing every
  // window into a second profile leaves it idle rather than freeing it,
  // trading a little memory for not having to track per-profile window
  // counts on top of the existing global one.
  Instance& GetOrCreate(const std::string& profile_name);

  void FlushDirtySettings();
  void SaveAllSessions();

 private:
  const raw_ptr<WindowsSpellChecker> spell_checker_;
  std::map<std::string, std::unique_ptr<Instance>> instances_;
};

}  // namespace weglet

#endif  // WEGLET_BROWSER_WEGLET_PROFILE_MANAGER_H_
