// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The settings page: search, general, security, appearance.

import { spawnRipple } from "./anim.js";
import {byId, el, runPage} from "./dom.js";
import { applyI18n, t, type Language } from "./i18n.js";
import { createModal } from "./modal.js";
import {
  onSettings,
  send,
  type SearchEngineChoice,
  type SettingsState,
} from "./protocol.js";
import { applyAccent, applyAddressBarShape } from "./theme.js";
import { accentPresets } from "./tokens.js";

// The default is the first preset, so "reset" and the first swatch cannot
// disagree.
const DEFAULT_ACCENT = accentPresets[0];

// The panels, in the order the switcher shows them. One list, so the
// index arithmetic and the panels cannot end up in different orders.
const PANELS = ["search", "general", "security", "appearance", "profiles"] as const;
type PanelName = (typeof PANELS)[number];

// Set by a row's "Rename" button; read by the new-profile form's submit,
// which sends renameProfile instead of switchProfile while this is set.
let renameTarget: string | null = null;

function setProfileFormMode(target: string | null): void {
  renameTarget = target;
  const input = byId<HTMLInputElement>("new-profile-input");
  input.value = target ?? "";
  input.focus();
  byId("new-profile-mode-hint").textContent = target
    ? t("settings.profiles.renamingHint", currentLanguage).replace("{name}", target)
    : "";
  byId<HTMLButtonElement>("new-profile-btn").textContent = target
    ? t("settings.profiles.renameSubmit", currentLanguage)
    : t("settings.profiles.create", currentLanguage);
}


const SHAPES = ["pill", "rounded", "square"] as const;
type Shape = (typeof SHAPES)[number];

// ---------------------------------------------------------------------
// Switcher
// ---------------------------------------------------------------------

function showPanel(name: PanelName): void {
  const index = PANELS.indexOf(name);
  if (index < 0) {
    return;
  }

  // Custom properties rather than a pixel offset: the pill's width is a
  // fraction of the switcher, so a resize moves it with nothing listening.
  byId("switcher").style.setProperty("--count", String(PANELS.length));
  byId("switcher-pill").style.setProperty("--index", String(index));

  for (const panel of PANELS) {
    const tab = byId(`tab-${panel}`);
    const active = panel === name;
    tab.classList.toggle("active", active);
    tab.setAttribute("aria-selected", String(active));
  }
  for (const node of document.querySelectorAll<HTMLElement>("[role='tabpanel']")) {
    node.hidden = node.dataset.panel !== name;
  }
}

function wireSwitcher(): void {
  PANELS.forEach((panel, index) => {
    const tab = byId(`tab-${panel}`);
    tab.addEventListener("click", () => showPanel(panel));
    tab.addEventListener("keydown", (event: Event) => {
      const key = (event as KeyboardEvent).key;
      if (key === "Enter" || key === " ") {
        event.preventDefault();
        showPanel(panel);
        return;
      }
      // Arrow keys move between tabs, which is the only way to reach a
      // panel without a pointer once focus is inside the strip.
      const delta = key === "ArrowRight" ? 1 : key === "ArrowLeft" ? -1 : 0;
      if (delta === 0) {
        return;
      }
      event.preventDefault();
      const next = PANELS[(index + delta + PANELS.length) % PANELS.length];
      if (next) {
        showPanel(next);
        byId(`tab-${next}`).focus();
      }
    });
  });
  showPanel("search");
}

// ---------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------

function renderEngines(
  engines: readonly SearchEngineChoice[],
  current: string,
): void {
  const row = byId("engine-choice");
  row.replaceChildren(
    ...engines.map((engine) => {
      const pill = el("button", {
        className: engine.id === current ? "engine-pill active" : "engine-pill",
        text: engine.label,
        onClick: () => send("setSearchEngine", engine.id),
      });
      pill.setAttribute("aria-pressed", String(engine.id === current));
      return pill;
    }),
  );
}

let deleteProfileTarget: string | null = null;

function openDeleteProfileConfirm(profile: string): void {
  deleteProfileTarget = profile;
  byId("delete-profile-body").textContent = t(
    "settings.profiles.deleteConfirmBody",
    currentLanguage,
  ).replace("{name}", profile);
  deleteProfileModal.open();
}

const deleteProfileModal = createModal({
  backdropId: "delete-profile-backdrop",
  focusId: "delete-profile-cancel",
  cancelId: "delete-profile-cancel",
  onConfirm: () => {
    if (deleteProfileTarget) {
      send("deleteProfile", deleteProfileTarget);
    }
    deleteProfileModal.close();
  },
});

function deleteProfileButton(profile: string): HTMLElement {
  return el("button", {
    className: "blocked-host-remove",
    text: t("settings.profiles.delete", currentLanguage),
    onClick: () => openDeleteProfileConfirm(profile),
  });
}

function renderProfiles(
  profiles: readonly string[],
  colors: readonly string[],
  active: string,
): void {
  const list = byId("profile-list");
  list.replaceChildren(
    ...profiles.map((profile, index) => {
      const swatch = el("div", { className: "profile-swatch" });
      swatch.style.background = colors[index] ?? "#666";

      const nameBtn = el("button", {
        className: profile === active ? "profile-name-btn active" : "profile-name-btn",
        text: profile,
        onClick: () => {
          if (profile !== active) {
            send("switchProfile", profile);
          }
        },
      });

      const rename = el("button", {
        className: "blocked-host-remove",
        text: t("settings.profiles.rename", currentLanguage),
        onClick: () => setProfileFormMode(profile),
      });

      const openWindow = el("button", {
        className: "blocked-host-remove",
        text: t("settings.profiles.openWindow", currentLanguage),
        onClick: () => send("openProfileWindow", profile),
      });

      const shortcut = el("button", {
        className: "blocked-host-remove",
        text: t("settings.profiles.shortcut", currentLanguage),
        onClick: () => send("createProfileShortcut", profile),
      });

      const children: HTMLElement[] = [swatch, nameBtn, openWindow, rename, shortcut];
      if (profile !== active) {
        children.push(deleteProfileButton(profile));
      }
      return el("div", { className: "profile-row", children });
    }),
  );
}

function toggle(id: string, on: boolean, onChange: (next: boolean) => void): void {
  const node = byId(id);
  node.classList.toggle("on", on);
  node.setAttribute("aria-checked", String(on));
  // Replaced rather than added: render runs on every push, and adding
  // would stack a listener per update.
  const clone = node.cloneNode(true) as HTMLElement;
  node.replaceWith(clone);
  clone.addEventListener("click", () => onChange(!on));
  clone.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (key === "Enter" || key === " ") {
      event.preventDefault();
      onChange(!on);
    }
  });
}

function renderBlockedHosts(hosts: readonly string[]): void {
  const list = byId("blocked-hosts-list");
  if (hosts.length === 0) {
    list.replaceChildren(
      el("div", { className: "blocked-hosts-empty", text: "No sites blocked." }),
    );
    return;
  }
  list.replaceChildren(
    ...hosts.map((host, index) => {
      const remove = el("button", {
        className: "blocked-host-remove",
        text: "Remove",
        // By index, which is how the profile stores the list. The browser
        // bounds-checks it, so a stale index is a dropped message.
        onClick: () => send("unblockHost", index),
      });
      remove.setAttribute("aria-label", `Unblock ${host}`);
      return el("div", {
        className: "blocked-host-row",
        children: [el("span", { text: host }), remove],
      });
    }),
  );
}

function isValidHex(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

function renderAccent(accent: string): void {
  const row = byId("color-preset-row");
  row.replaceChildren(
    ...accentPresets.map((preset) => {
      const swatch = el("button", {
        className:
          preset.toLowerCase() === accent.toLowerCase()
            ? "color-preset active"
            : "color-preset",
        onClick: () => send("setAccentColor", preset),
      });
      swatch.style.background = preset;
      swatch.setAttribute("aria-label", `Accent ${preset}`);
      return swatch;
    }),
  );

  byId("hex-swatch").style.background = accent;
  const hex = byId<HTMLInputElement>("hex-input");
  // Not while the user is mid-word: overwriting the field they are typing
  // in is the same mistake as repainting the address bar.
  if (document.activeElement !== hex) {
    hex.value = accent;
  }
}

function renderShape(shape: Shape): void {
  for (const node of document.querySelectorAll<HTMLElement>("[data-shape]")) {
    const active = node.dataset.shape === shape;
    node.classList.toggle("active", active);
    node.setAttribute("aria-pressed", String(active));
  }
  // Applied to this page too, so the preview is the real thing.
  applyAddressBarShape(shape);
}

function renderLanguage(language: Language): void {
  for (const node of document.querySelectorAll<HTMLElement>("[data-lang]")) {
    const active = node.dataset.lang === language;
    node.classList.toggle("active", active);
    node.setAttribute("aria-pressed", String(active));
  }
}

function renderThreatFeedStatus(state: SettingsState): void {
  const status = byId("threat-feed-status");
  const base = t("settings.security.phishingHint", state.language);
  if (state.threatFeedUpdatedAt === 0) {
    status.textContent = `${base} ${t("settings.security.phishingHintNotUpdated", state.language)}`;
    return;
  }
  const when = new Date(state.threatFeedUpdatedAt * 1000).toLocaleString();
  const suffix = state.threatFeedLastUpdateFailed
    ? t("settings.security.phishingHintFailed", state.language)
    : t("settings.security.phishingHintUpdated", state.language);
  status.textContent = `${base} ${suffix.replace("{when}", when)}`;
}

// ---------------------------------------------------------------------

// Set while a refresh is in flight, so the next settings push (success
// or failure both end in one) can put the button back.
let refreshPending = false;

// Read by click handlers, which have no push of their own to read the
// language from.
let currentLanguage: Language = "en";

// Shared by the confirm button's click and the modal's Enter-key handling.
function confirmFaviconsWarning(): void {
  send("setFaviconsEnabled", true);
  faviconsWarningModal.close();
}

const faviconsWarningModal = createModal({
  backdropId: "favicons-warning-backdrop",
  focusId: "favicons-warning-cancel",
  cancelId: "favicons-warning-cancel",
  onConfirm: confirmFaviconsWarning,
});

function render(state: SettingsState): void {
  currentLanguage = state.language;
  renderEngines(state.engines, state.searchEngine);
  byId("custom-engine-row").hidden = state.searchEngine !== "custom";
  const custom = byId<HTMLInputElement>("custom-engine-input");
  if (document.activeElement !== custom) {
    custom.value = state.customSearchUrl;
  }

  toggle("restore-session-toggle", state.restoreSession, (next) =>
    send("setRestoreSession", next),
  );
  toggle("threat-feed-toggle", state.threatFeedEnabled, (next) =>
    send("setThreatFeedEnabled", next),
  );
  toggle("ask-where-to-save-toggle", state.askWhereToSaveDownloads, (next) =>
    send("setAskWhereToSaveDownloads", next),
  );
  renderThreatFeedStatus(state);
  // Turning it off needs no confirmation; turning it on does -- see
  // faviconsWarningModal below.
  toggle("favicons-toggle", state.faviconsEnabled, (next) => {
    if (next) {
      faviconsWarningModal.open();
    } else {
      send("setFaviconsEnabled", false);
    }
  });

  if (refreshPending) {
    refreshPending = false;
    const refresh = byId<HTMLButtonElement>("refresh-threat-feed-btn");
    refresh.disabled = false;
    refresh.textContent = t("settings.security.refresh", state.language);
  }

  renderBlockedHosts(state.blockedHosts);
  renderAccent(state.accentColor);
  applyAccent(state.accentColor);
  renderShape(state.addressBarShape);
  renderLanguage(state.language);
  renderProfiles(state.profiles, state.profileColors, state.activeProfile);
  applyI18n(state.language);
}

function confirmingButton(id: string, action: () => void): void {
  const node = byId<HTMLButtonElement>(id);
  let armed = false;
  let original = "";
  let timer = 0;

  node.addEventListener("click", (event) => {
    spawnRipple(node, event.clientX, event.clientY);
    if (!armed) {
      // Read fresh rather than cached at wiring time: applyI18n may have
      // repainted this label since, and a cached copy would revert the
      // button to whatever language was active when the page loaded.
      original = node.textContent ?? "";
      armed = true;
      node.textContent = t("settings.general.confirmAgain", currentLanguage);
      timer = window.setTimeout(() => {
        armed = false;
        node.textContent = original;
      }, 3000);
      return;
    }
    window.clearTimeout(timer);
    armed = false;
    node.textContent = original;
    action();
  });
}

function main(): void {
  wireSwitcher();

  const done = byId<HTMLButtonElement>("done");
  done.addEventListener("click", (event) => {
    send("goBack");
    spawnRipple(done, event.clientX, event.clientY);
  });

  byId("favicons-warning-cancel").addEventListener("click", () => faviconsWarningModal.close());
  byId("delete-profile-cancel").addEventListener("click", () => deleteProfileModal.close());
  byId("delete-profile-confirm").addEventListener("click", () => {
    if (deleteProfileTarget) {
      send("deleteProfile", deleteProfileTarget);
    }
    deleteProfileModal.close();
  });
  const faviconsConfirm = byId<HTMLButtonElement>("favicons-warning-confirm");
  faviconsConfirm.addEventListener("click", (event) => {
    spawnRipple(faviconsConfirm, event.clientX, event.clientY);
    confirmFaviconsWarning();
  });

  byId("custom-engine-input").addEventListener("change", () => {
    send("setCustomSearchUrl", byId<HTMLInputElement>("custom-engine-input").value.trim());
  });

  const blockInput = byId<HTMLInputElement>("block-host-input");
  const blockHost = (): void => {
    const host = blockInput.value.trim();
    if (host) {
      send("blockHost", host);
      blockInput.value = "";
    }
  };
  byId("block-host-btn").addEventListener("click", blockHost);
  blockInput.addEventListener("keydown", (event: Event) => {
    if ((event as KeyboardEvent).key === "Enter") {
      blockHost();
    }
  });

  const hex = byId<HTMLInputElement>("hex-input");
  hex.addEventListener("change", () => {
    const value = hex.value.trim();
    if (isValidHex(value)) {
      send("setAccentColor", value);
    } else {
      // Reverted rather than left as typed: an invalid value that stays
      // in the field looks accepted.
      send("requestState");
    }
  });

  byId("reset-color-btn").addEventListener("click", (event) => {
    send("setAccentColor", DEFAULT_ACCENT);
    spawnRipple(byId("reset-color-btn"), event.clientX, event.clientY);
  });

  for (const node of document.querySelectorAll<HTMLElement>("[data-shape]")) {
    node.addEventListener("click", (event) => {
      const shape = node.dataset.shape;
      if (shape) {
        send("setAddressBarShape", shape);
      }
      spawnRipple(node, (event as MouseEvent).clientX, (event as MouseEvent).clientY);
    });
  }

  for (const node of document.querySelectorAll<HTMLElement>("[data-lang]")) {
    node.addEventListener("click", (event) => {
      const lang = node.dataset.lang;
      if (lang) {
        send("setLanguage", lang);
      }
      spawnRipple(node, (event as MouseEvent).clientX, (event as MouseEvent).clientY);
    });
  }

  const refresh = byId<HTMLButtonElement>("refresh-threat-feed-btn");
  refresh.addEventListener("click", (event) => {
    send("refreshThreatFeed");
    refreshPending = true;
    refresh.disabled = true;
    refresh.textContent = t("settings.security.refreshing", currentLanguage);
    spawnRipple(refresh, event.clientX, event.clientY);
  });

  confirmingButton("clear-data-btn", () => send("clearBrowsingData"));

  const newProfileInput = byId<HTMLInputElement>("new-profile-input");
  const submitProfileForm = (): void => {
    const name = newProfileInput.value.trim();
    if (!name) {
      return;
    }
    if (renameTarget) {
      if (name !== renameTarget) {
        send("renameProfile", renameTarget, name);
      }
      setProfileFormMode(null);
    } else {
      send("switchProfile", name);
    }
    newProfileInput.value = "";
  };
  byId("new-profile-btn").addEventListener("click", submitProfileForm);
  newProfileInput.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (key === "Enter") {
      submitProfileForm();
    } else if (key === "Escape" && renameTarget) {
      setProfileFormMode(null);
      newProfileInput.value = "";
    }
  });

  onSettings(render);
}

runPage(main);
