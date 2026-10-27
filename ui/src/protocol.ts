// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Every message that crosses between a page and the browser, typed once.
//
// The channel is content's WebUI bindings, which exist only in a frame the
// WebUI factory claimed. There is no allow-list here to get wrong.

// Generated from contract.json, so the browser's registration and this
// type cannot go out of step.
export type { Outgoing, TabId } from "./contract.js";
export { pages, pageUrl, pushes } from "./contract.js";

import type { Outgoing, TabId } from "./contract.js";
import { pushNamespace, pushes, type PushName } from "./contract.js";
import { isLanguage, type Language } from "./i18n.js";
import { isAddressBarShape, type AddressBarShape } from "./theme.js";

// Every push carries the language (see WegletStateService::SendTo).
// Falls back to English rather than failing the whole push: a missing
// language for one frame is a much smaller problem than a missing tab
// list.
function parseLanguage(value: unknown): Language {
  if (typeof value !== "object" || value === null) {
    return "en";
  }
  const language = (value as { language?: unknown }).language;
  return isLanguage(language) ? language : "en";
}

// Same reasoning as parseLanguage; a malformed/missing flag must never
// read as "favicons are on".
function parseFaviconsEnabled(value: unknown): boolean {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const enabled = (value as { faviconsEnabled?: unknown }).faviconsEnabled;
  return enabled === true;
}

// Same reasoning as parseLanguage. Empty string is theme.ts's signal to
// fall back to its built-in default.
function parsePushedAccentColor(value: unknown): string {
  if (typeof value !== "object" || value === null) {
    return "";
  }
  const accentColor = (value as { accentColor?: unknown }).accentColor;
  return typeof accentColor === "string" ? accentColor : "";
}

function parsePushedAddressBarShape(value: unknown): AddressBarShape {
  if (typeof value !== "object" || value === null) {
    return "pill";
  }
  const shape = (value as { addressBarShape?: unknown }).addressBarShape;
  return isAddressBarShape(shape) ? shape : "pill";
}

export interface TabState {
  readonly id: TabId;
  readonly url: string;
  // The page title, its host until a title arrives, or a name for one of
  // Weglet's own pages. Decided by the model.
  readonly label: string;
  readonly canGoBack: boolean;
  readonly canGoForward: boolean;
  readonly loading: boolean;
}

export interface BrowserState {
  readonly tabs: readonly TabState[];
  readonly activeId: TabId;
  // Set for exactly one push, in answer to Ctrl+L. Focus lives in the DOM,
  // so this is how the browser asks for the caret.
  readonly focusOmnibox: boolean;
  // Whether the active tab's URL is saved. Decided by the browser: the
  // toolbar has no bookmark list of its own to check against.
  readonly bookmarked: boolean;
  readonly language: Language;
  readonly faviconsEnabled: boolean;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface SearchEngineChoice {
  readonly id: string;
  readonly label: string;
}

export interface SettingsState {
  // Sent by the browser: a list on this side would be a second copy.
  readonly engines: readonly SearchEngineChoice[];
  readonly searchEngine: string;
  readonly customSearchUrl: string;
  readonly restoreSession: boolean;
  readonly blockedHosts: readonly string[];
  readonly accentColor: string;
  readonly addressBarShape: "pill" | "rounded" | "square";
  readonly threatFeedEnabled: boolean;
  // Unix seconds; 0 means never successfully updated.
  readonly threatFeedUpdatedAt: number;
  readonly threatFeedLastUpdateFailed: boolean;
  readonly language: Language;
  readonly faviconsEnabled: boolean;
  readonly askWhereToSaveDownloads: boolean;
  // Every profile this machine has ever used, alphabetical.
  readonly profiles: readonly string[];
  // Parallel to `profiles`.
  readonly profileColors: readonly string[];
  readonly activeProfile: string;
}

export interface RiskNotice {
  readonly level: "warning" | "block";
  readonly title: string;
  readonly reason: string;
  readonly host: string;
  readonly target: string;
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface Shortcut {
  readonly title: string;
  readonly url: string;
}

export interface NewTabState {
  readonly shortcuts: readonly Shortcut[];
  // Which engine will answer. Composed by the browser, because the engine
  // is a profile setting and the page does not read the profile.
  readonly hint: string;
  readonly language: Language;
  readonly faviconsEnabled: boolean;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

// The "⋮" dropdown's own push: it has no state of its own beyond the
// language every push carries.
export interface MenuState {
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface ContextMenuItem {
  readonly id: string;
  readonly enabled: boolean;
  // Present only for items whose text isn't a fixed i18n string -- a
  // spelling suggestion's own word, for instance.
  readonly label?: string;
}

export interface ContextMenuState {
  readonly items: readonly ContextMenuItem[];
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface ToastItem {
  readonly id: number;
  // An i18n key ("toast.linkCopied"), not display text -- resolved by
  // toast.ts against its own language, like every other page's strings.
  readonly key: string;
}

export interface ToastState {
  readonly items: readonly ToastItem[];
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface OmniboxSuggestionItem {
  readonly label: string;
  readonly url: string;
  readonly isBookmark: boolean;
}

export interface OmniboxSuggestionsState {
  readonly items: readonly OmniboxSuggestionItem[];
  // -1 when nothing is highlighted.
  readonly highlightedIndex: number;
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface FindBarState {
  readonly query: string;
  readonly matchCount: number;
  readonly activeOrdinal: number;
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface PermissionPromptState {
  readonly origin: string;
  readonly types: readonly string[];
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface SitePermission {
  readonly id: string;
  readonly status: "granted" | "denied" | "ask";
}

export interface SiteInfoState {
  readonly isInternal: boolean;
  // Empty when isInternal is true.
  readonly origin: string;
  readonly permissions: readonly SitePermission[];
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface ShortcutPopupState {
  readonly url: string;
  // The active tab's title, offered as a starting point -- the field
  // stays editable, so a page with an unhelpful title is not stuck.
  readonly suggestedName: string;
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface BookmarkEntry {
  readonly title: string;
  readonly url: string;
}

export interface BookmarksState {
  readonly entries: readonly BookmarkEntry[];
  readonly language: Language;
  readonly faviconsEnabled: boolean;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

export interface SearchHistoryEntry {
  readonly query: string;
  readonly url: string;
  // Unix seconds. A number rather than a formatted string: the page
  // buckets these into days and shows them as "2h ago".
  readonly visitedAt: number;
}

export interface DownloadEntry {
  readonly filename: string;
  readonly status: "in-progress" | "complete" | "failed";
  // Empty unless the status is "failed".
  readonly errorMessage: string;
  // Already formatted: how bytes read best is a display decision, and the
  // page has no other reason to know the number. "1.2 MB of 4.0 MB" while
  // in progress, the final size once resolved.
  readonly sizeLabel: string;
  readonly startedAt: number;
}

export interface HistoryState {
  readonly searchEntries: readonly SearchHistoryEntry[];
  readonly downloads: readonly DownloadEntry[];
  readonly language: Language;
  readonly accentColor: string;
  readonly addressBarShape: AddressBarShape;
}

// What a page may ask the browser to do. Anything not registered is
// dropped with a log line.
interface ChromeSend {
  send(message: string, args?: unknown[]): void;
}

// Installed by the WebUI bindings before any page script runs. Absent when
// a page is opened outside Weglet, so this is a no-op.
function bridge(): ChromeSend | undefined {
  return (globalThis as { chrome?: ChromeSend }).chrome;
}

export function send(...message: Outgoing): void {
  const [name, ...args] = message;
  bridge()?.send(name, args);
}

// The browser pushes state by calling the function the contract names for
// this page.
export function onState(handler: (state: BrowserState) => void): void {
  install(pushes.state, (value: unknown) => {
    const parsed = parseState(value);
    if (parsed) {
      handler(parsed);
    }
  });
  // The browser only pushes on a change, so a page that loaded late asks
  // for the current state itself.
  send("requestState");
}

// WebUI calls a named function rather than posting a message. A page is an
// ES module, so nothing declared here is a global: the handler goes on an
// object named by the contract, and the browser calls
// `<pushNamespace>.<function>`. Merged rather than replaced -- a page may
// listen for more than one kind of push.
type Receiver = (value: unknown) => void;

function install(name: PushName, receiver: Receiver): void {
  const host = globalThis as Record<string, unknown>;
  const existing = (host[pushNamespace] ?? {}) as Record<string, Receiver>;
  host[pushNamespace] = { ...existing, [name]: receiver };
}

// The new tab page's own push, separate from the toolbar's.
export function onNewTab(handler: (state: NewTabState) => void): void {
  install(pushes.newTab, (value: unknown) => {
    const parsed = parseNewTab(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseNewTab(value: unknown): NewTabState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const candidate = value as { shortcuts?: unknown; hint?: unknown };
  if (!Array.isArray(candidate.shortcuts) || typeof candidate.hint !== "string") {
    console.warn("weglet: malformed new tab state", value);
    return null;
  }
  const shortcuts: Shortcut[] = [];
  for (const entry of candidate.shortcuts) {
    if (typeof entry !== "object" || entry === null) {
      return null;
    }
    const shortcut = entry as Record<string, unknown>;
    if (typeof shortcut.title !== "string" || typeof shortcut.url !== "string") {
      console.warn("weglet: malformed shortcut", entry);
      return null;
    }
    shortcuts.push({ title: shortcut.title, url: shortcut.url });
  }
  return {
    shortcuts,
    hint: candidate.hint,
    language: parseLanguage(value),
    faviconsEnabled: parseFaviconsEnabled(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The settings page's own push.
export function onSettings(handler: (state: SettingsState) => void): void {
  install(pushes.settings, (value: unknown) => {
    const parsed = parseSettings(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseSettings(value: unknown): SettingsState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const s = value as Record<string, unknown>;
  // The shape drives a CSS attribute, so an unrecognised one is dropped:
  // an attribute nobody styles leaves the address bar with no shape.
  if (
    s.addressBarShape !== "pill" &&
    s.addressBarShape !== "rounded" &&
    s.addressBarShape !== "square"
  ) {
    console.warn("weglet: malformed address bar shape", value);
    return null;
  }
  if (!Array.isArray(s.engines)) {
    console.warn("weglet: settings without an engine list", value);
    return null;
  }
  const engines: SearchEngineChoice[] = [];
  for (const entry of s.engines) {
    if (
      typeof entry !== "object" ||
      entry === null ||
      typeof (entry as SearchEngineChoice).id !== "string" ||
      typeof (entry as SearchEngineChoice).label !== "string"
    ) {
      console.warn("weglet: malformed engine", entry);
      return null;
    }
    engines.push({
      id: (entry as SearchEngineChoice).id,
      label: (entry as SearchEngineChoice).label,
    });
  }
  if (
    typeof s.searchEngine !== "string" ||
    typeof s.customSearchUrl !== "string" ||
    typeof s.restoreSession !== "boolean" ||
    typeof s.accentColor !== "string" ||
    !Array.isArray(s.blockedHosts) ||
    typeof s.threatFeedEnabled !== "boolean" ||
    typeof s.threatFeedUpdatedAt !== "number" ||
    typeof s.threatFeedLastUpdateFailed !== "boolean" ||
    typeof s.askWhereToSaveDownloads !== "boolean" ||
    !Array.isArray(s.profiles) ||
    !Array.isArray(s.profileColors) ||
    typeof s.activeProfile !== "string"
  ) {
    console.warn("weglet: malformed settings", value);
    return null;
  }
  return {
    engines,
    searchEngine: s.searchEngine,
    customSearchUrl: s.customSearchUrl,
    restoreSession: s.restoreSession,
    blockedHosts: s.blockedHosts.filter(
      (host): host is string => typeof host === "string",
    ),
    accentColor: s.accentColor,
    addressBarShape: s.addressBarShape,
    threatFeedEnabled: s.threatFeedEnabled,
    threatFeedUpdatedAt: s.threatFeedUpdatedAt,
    threatFeedLastUpdateFailed: s.threatFeedLastUpdateFailed,
    askWhereToSaveDownloads: s.askWhereToSaveDownloads,
    profiles: s.profiles.filter((name): name is string => typeof name === "string"),
    profileColors: s.profileColors.filter((color): color is string => typeof color === "string"),
    activeProfile: s.activeProfile,
    language: parseLanguage(value),
    faviconsEnabled: parseFaviconsEnabled(value),
  };
}

// The history page's own push: address-bar submissions and downloads.
// Both lists arrive empty until there is a store behind them.
export function onHistory(handler: (state: HistoryState) => void): void {
  install(pushes.history, (value: unknown) => {
    const parsed = parseHistory(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseHistory(value: unknown): HistoryState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { searchEntries?: unknown; downloads?: unknown };
  if (!Array.isArray(state.searchEntries) || !Array.isArray(state.downloads)) {
    console.warn("weglet: malformed history state", value);
    return null;
  }
  const searchEntries: SearchHistoryEntry[] = [];
  for (const item of state.searchEntries) {
    if (typeof item !== "object" || item === null) {
      return null;
    }
    const entry = item as Record<string, unknown>;
    if (
      typeof entry.query !== "string" ||
      typeof entry.url !== "string" ||
      typeof entry.visitedAt !== "number"
    ) {
      console.warn("weglet: malformed history entry", item);
      return null;
    }
    searchEntries.push({
      query: entry.query,
      url: entry.url,
      visitedAt: entry.visitedAt,
    });
  }
  const downloads: DownloadEntry[] = [];
  for (const item of state.downloads) {
    if (typeof item !== "object" || item === null) {
      return null;
    }
    const entry = item as Record<string, unknown>;
    // The status decides whether the row offers to open the file, so an
    // unrecognised one is dropped rather than treated as complete.
    if (entry.status !== "in-progress" && entry.status !== "complete" && entry.status !== "failed") {
      console.warn("weglet: malformed download status", item);
      return null;
    }
    if (
      typeof entry.filename !== "string" ||
      typeof entry.errorMessage !== "string" ||
      typeof entry.sizeLabel !== "string" ||
      typeof entry.startedAt !== "number"
    ) {
      console.warn("weglet: malformed download", item);
      return null;
    }
    downloads.push({
      filename: entry.filename,
      status: entry.status,
      errorMessage: entry.errorMessage,
      sizeLabel: entry.sizeLabel,
      startedAt: entry.startedAt,
    });
  }
  return {
    searchEntries,
    downloads,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The bookmarks page's own push. Empty until there is a store behind it.
export function onBookmarks(handler: (state: BookmarksState) => void): void {
  install(pushes.bookmarks, (value: unknown) => {
    const parsed = parseBookmarks(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseBookmarks(value: unknown): BookmarksState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { entries?: unknown };
  if (!Array.isArray(state.entries)) {
    console.warn("weglet: malformed bookmarks state", value);
    return null;
  }
  const entries: BookmarkEntry[] = [];
  for (const item of state.entries) {
    if (typeof item !== "object" || item === null) {
      return null;
    }
    const entry = item as Record<string, unknown>;
    if (typeof entry.title !== "string" || typeof entry.url !== "string") {
      console.warn("weglet: malformed bookmark", item);
      return null;
    }
    entries.push({ title: entry.title, url: entry.url });
  }
  return {
    entries,
    language: parseLanguage(value),
    faviconsEnabled: parseFaviconsEnabled(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The "⋮" dropdown's own push. Asked for once, on load -- there is
// nothing in it to change while the popup is open.
export function onMenu(handler: (state: MenuState) => void): void {
  install(pushes.menu, (value: unknown) => {
    handler({
      language: parseLanguage(value),
      accentColor: parsePushedAccentColor(value),
      addressBarShape: parsePushedAddressBarShape(value),
    });
  });
  send("requestState");
}

// The right-click menu's own push: a separate floating page, opened
// fresh (and reloaded) for every click, since the item list changes with
// what was clicked on.
export function onContextMenu(handler: (state: ContextMenuState) => void): void {
  install(pushes.contextMenu, (value: unknown) => {
    const parsed = parseContextMenu(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseContextMenu(value: unknown): ContextMenuState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { items?: unknown };
  if (!Array.isArray(state.items)) {
    console.warn("weglet: malformed context menu state", value);
    return null;
  }
  const items: ContextMenuItem[] = [];
  for (const entry of state.items) {
    if (typeof entry !== "object" || entry === null) {
      return null;
    }
    const item = entry as Record<string, unknown>;
    if (typeof item.id !== "string" || typeof item.enabled !== "boolean") {
      console.warn("weglet: malformed context menu item", entry);
      return null;
    }
    if (item.label !== undefined && typeof item.label !== "string") {
      console.warn("weglet: malformed context menu item", entry);
      return null;
    }
    items.push({
      id: item.id,
      enabled: item.enabled,
      ...(item.label !== undefined ? { label: item.label } : {}),
    });
  }
  return {
    items,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

export function onToast(handler: (state: ToastState) => void): void {
  install(pushes.toast, (value: unknown) => {
    const parsed = parseToast(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseToast(value: unknown): ToastState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { items?: unknown };
  if (!Array.isArray(state.items)) {
    console.warn("weglet: malformed toast state", value);
    return null;
  }
  const items: ToastItem[] = [];
  for (const entry of state.items) {
    if (typeof entry !== "object" || entry === null) {
      return null;
    }
    const item = entry as Record<string, unknown>;
    if (typeof item.id !== "number" || typeof item.key !== "string") {
      console.warn("weglet: malformed toast item", entry);
      return null;
    }
    items.push({ id: item.id, key: item.key });
  }
  return {
    items,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The address bar's dropdown: bookmarks and history matching what is
// typed, reloaded fresh on every keystroke -- see toast.ts's own comment
// on why a popup like this reloads rather than diffing.
export function onOmniboxSuggestions(
  handler: (state: OmniboxSuggestionsState) => void,
): void {
  install(pushes.omniboxSuggestions, (value: unknown) => {
    const parsed = parseOmniboxSuggestions(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseOmniboxSuggestions(value: unknown): OmniboxSuggestionsState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { items?: unknown; highlightedIndex?: unknown };
  if (!Array.isArray(state.items) || typeof state.highlightedIndex !== "number") {
    console.warn("weglet: malformed omnibox suggestions state", value);
    return null;
  }
  const items: OmniboxSuggestionItem[] = [];
  for (const entry of state.items) {
    if (typeof entry !== "object" || entry === null) {
      return null;
    }
    const item = entry as Record<string, unknown>;
    if (
      typeof item.label !== "string" ||
      typeof item.url !== "string" ||
      typeof item.isBookmark !== "boolean"
    ) {
      console.warn("weglet: malformed omnibox suggestion", entry);
      return null;
    }
    items.push({ label: item.label, url: item.url, isBookmark: item.isBookmark });
  }
  return {
    items,
    highlightedIndex: state.highlightedIndex,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The find bar's own push: sent on every reply from the renderer while a
// find session is active for the tab the bar is open on.
export function onFindBar(handler: (state: FindBarState) => void): void {
  install(pushes.findBar, (value: unknown) => {
    const parsed = parseFindBar(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseFindBar(value: unknown): FindBarState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as Record<string, unknown>;
  if (
    typeof state.query !== "string" ||
    typeof state.matchCount !== "number" ||
    typeof state.activeOrdinal !== "number"
  ) {
    console.warn("weglet: malformed find bar state", value);
    return null;
  }
  return {
    query: state.query,
    matchCount: state.matchCount,
    activeOrdinal: state.activeOrdinal,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The permission prompt's own push: a fresh popup for every request.
export function onPermissionPrompt(handler: (state: PermissionPromptState) => void): void {
  install(pushes.permissionPrompt, (value: unknown) => {
    const parsed = parsePermissionPrompt(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parsePermissionPrompt(value: unknown): PermissionPromptState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { origin?: unknown; types?: unknown };
  if (typeof state.origin !== "string" || !Array.isArray(state.types)) {
    console.warn("weglet: malformed permission prompt state", value);
    return null;
  }
  const types = state.types.filter((type): type is string => typeof type === "string");
  return {
    origin: state.origin,
    types,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The site-info popup's own push: a fresh popup for every open, and
// reloaded (so a fresh one again) when a permission is changed from it.
export function onSiteInfo(handler: (state: SiteInfoState) => void): void {
  install(pushes.siteInfo, (value: unknown) => {
    const parsed = parseSiteInfo(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseSiteInfo(value: unknown): SiteInfoState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as { isInternal?: unknown; origin?: unknown; permissions?: unknown };
  if (typeof state.isInternal !== "boolean") {
    console.warn("weglet: malformed site info state", value);
    return null;
  }
  if (state.isInternal) {
    return {
      isInternal: true,
      origin: "",
      permissions: [],
      language: parseLanguage(value),
      accentColor: parsePushedAccentColor(value),
      addressBarShape: parsePushedAddressBarShape(value),
    };
  }
  if (typeof state.origin !== "string" || !Array.isArray(state.permissions)) {
    console.warn("weglet: malformed site info state", value);
    return null;
  }
  const permissions: SitePermission[] = [];
  for (const entry of state.permissions) {
    if (typeof entry !== "object" || entry === null) {
      return null;
    }
    const permission = entry as Record<string, unknown>;
    if (
      typeof permission.id !== "string" ||
      (permission.status !== "granted" &&
        permission.status !== "denied" &&
        permission.status !== "ask")
    ) {
      console.warn("weglet: malformed site permission", entry);
      return null;
    }
    permissions.push({ id: permission.id, status: permission.status });
  }
  return {
    isInternal: false,
    origin: state.origin,
    permissions,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The save-shortcut popup's own push: a separate floating page, opened
// fresh for each click of the toolbar's "keep" button.
export function onShortcutPopup(handler: (state: ShortcutPopupState) => void): void {
  install(pushes.shortcutPopup, (value: unknown) => {
    const parsed = parseShortcutPopup(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseShortcutPopup(value: unknown): ShortcutPopupState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const state = value as Record<string, unknown>;
  if (typeof state.url !== "string" || typeof state.suggestedName !== "string") {
    console.warn("weglet: malformed shortcut popup state", value);
    return null;
  }
  return {
    url: state.url,
    suggestedName: state.suggestedName,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// The security notice's own push: it is a separate page shown over the
// one being blocked and has no use for a tab list.
export function onRisk(handler: (notice: RiskNotice) => void): void {
  install(pushes.risk, (value: unknown) => {
    const parsed = parseRisk(value);
    if (parsed) {
      handler(parsed);
    }
  });
  send("requestState");
}

function parseRisk(value: unknown): RiskNotice | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const notice = value as Record<string, unknown>;
  // The level decides whether a bypass is offered, so an unrecognised one
  // is dropped rather than guessed: guessing "warning" would invent a way
  // through a block.
  if (notice.level !== "warning" && notice.level !== "block") {
    console.warn("weglet: malformed risk level", value);
    return null;
  }
  if (
    typeof notice.title !== "string" ||
    typeof notice.reason !== "string" ||
    typeof notice.host !== "string" ||
    typeof notice.target !== "string"
  ) {
    console.warn("weglet: malformed risk notice", value);
    return null;
  }
  return {
    level: notice.level,
    title: notice.title,
    reason: notice.reason,
    host: notice.host,
    target: notice.target,
    language: parseLanguage(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

// Validates rather than casts. A bug on either side should surface as a
// dropped update with a console line, not as an exception halfway through
// rendering the tab strip.
function parseState(value: unknown): BrowserState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const candidate = value as {
    tabs?: unknown;
    activeId?: unknown;
    focusOmnibox?: unknown;
    bookmarked?: unknown;
  };
  if (
    !Array.isArray(candidate.tabs) ||
    typeof candidate.activeId !== "string" ||
    typeof candidate.focusOmnibox !== "boolean" ||
    typeof candidate.bookmarked !== "boolean"
  ) {
    console.warn("weglet: malformed state", value);
    return null;
  }
  const tabs: TabState[] = [];
  for (const entry of candidate.tabs) {
    const tab = parseTab(entry);
    if (!tab) {
      return null;
    }
    tabs.push(tab);
  }
  return {
    tabs,
    activeId: candidate.activeId,
    focusOmnibox: candidate.focusOmnibox,
    bookmarked: candidate.bookmarked,
    language: parseLanguage(value),
    faviconsEnabled: parseFaviconsEnabled(value),
    accentColor: parsePushedAccentColor(value),
    addressBarShape: parsePushedAddressBarShape(value),
  };
}

function parseTab(value: unknown): TabState | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const tab = value as Record<string, unknown>;
  if (
    typeof tab.id !== "string" ||
    typeof tab.url !== "string" ||
    typeof tab.label !== "string" ||
    typeof tab.canGoBack !== "boolean" ||
    typeof tab.canGoForward !== "boolean" ||
    typeof tab.loading !== "boolean"
  ) {
    console.warn("weglet: malformed tab", value);
    return null;
  }
  return {
    id: tab.id,
    url: tab.url,
    label: tab.label,
    canGoBack: tab.canGoBack,
    canGoForward: tab.canGoForward,
    loading: tab.loading,
  };
}
