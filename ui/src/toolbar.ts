// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The toolbar: tab strip, address bar, window controls.

import {
  attachTooltip,
  mixChannel,
  mixRgb,
  register,
  scheduleFrame,
  spawnRipple,
  Strip,
} from "./anim.js";
import {byId, el, runPage} from "./dom.js";
import { faviconUrl } from "./favicon.js";
import { applyI18n, t, type Language } from "./i18n.js";
import { setIcon, type IconName } from "./icons.js";
import { onState, pageUrl, pages, send, type BrowserState, type TabState } from "./protocol.js";
import { applyAccent, applyAddressBarShape, liveChannels } from "./theme.js";
import { channels, tokens } from "./tokens.js";

// surfaceHover comes from theme.ts's liveChannels, not destructured
// here like the rest, since applyAccent retints it at runtime.
const {
  textDim: TEXT_DIM,
  text: TEXT,
  borderStrong: BORDER_STRONG,
  danger: DANGER,
  background: BACKGROUND,
} = channels;

function textAt(heat: number): string {
  const r = mixChannel(TEXT_DIM[0], TEXT[0], heat);
  const g = mixChannel(TEXT_DIM[1], TEXT[1], heat);
  const b = mixChannel(TEXT_DIM[2], TEXT[2], heat);
  return `rgb(${r},${g},${b})`;
}

function rgba(colour: readonly [number, number, number], alpha: number): string {
  return `rgba(${colour[0]},${colour[1]},${colour[2]},${alpha})`;
}

// ---------------------------------------------------------------------
// Toolbar buttons
// ---------------------------------------------------------------------

interface Button {
  readonly el: HTMLElement;
  // Disabled buttons take no heat: a control that lights up and does
  // nothing is worse than one that stays dim.
  disabled: boolean;
}

const buttons: Button[] = [];
const buttonStrip = new Strip(1.08, false);

// Read by tooltip getters, which fire on pointerenter -- long after the
// buttons were wired at module load, when the first push's language
// was not yet known.
let currentLanguage: Language = "en";

function navButton(
  id: string,
  iconName: IconName,
  tooltipKey: string,
  onActivate: (event: MouseEvent) => void,
  // Where the icon lands. Defaults to the button itself; downloads-button
  // passes its icon slot instead, so setIcon's replaceChildren does not
  // wipe out the spinner markup sitting beside that slot.
  iconHost?: Element,
  // Off for bookmark-toggle: its own state change (outline -> filled) is
  // feedback enough, and a ripple under that read as two things happening.
  ripple = true,
): Button {
  const node = byId(id);
  setIcon(iconHost ?? node, iconName);
  attachTooltip(node, () => t(tooltipKey, currentLanguage));

  const button: Button = { el: node, disabled: false };
  const index = buttons.length;
  buttons.push(button);

  node.addEventListener("pointerenter", () => {
    if (!button.disabled) {
      buttonStrip.hover(index);
      scheduleFrame();
    }
  });
  node.addEventListener("pointerleave", () => {
    buttonStrip.hover(null);
    scheduleFrame();
  });
  node.addEventListener("click", (event) => {
    if (button.disabled) {
      return;
    }
    onActivate(event);
    if (ripple) {
      spawnRipple(node, event.clientX, event.clientY);
    }
  });
  // A div standing in for a button has to answer the keyboard like one.
  node.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (!button.disabled && (key === "Enter" || key === " ")) {
      event.preventDefault();
      node.click();
    }
  });
  return button;
}

function setDisabled(button: Button, disabled: boolean): void {
  button.disabled = disabled;
  button.el.classList.toggle("disabled", disabled);
  if (disabled) {
    // Cleared, or the button keeps the inline colours it had when it went
    // dim and .disabled loses to them.
    button.el.style.backgroundColor = "";
    button.el.style.borderColor = "";
    button.el.style.color = "";
    button.el.style.transform = "";
  }
}

function renderButtons(now: number): void {
  buttons.forEach((button, index) => {
    if (button.disabled) {
      return;
    }
    const presentation = buttonStrip.presentation(index, now);
    button.el.style.backgroundColor = rgba(liveChannels.surfaceHover, presentation.heat);
    button.el.style.borderColor = rgba(BORDER_STRONG, presentation.heat);
    button.el.style.color = textAt(presentation.heat);
    button.el.style.transform = `scale(${presentation.scale})`;
  });
}

// ---------------------------------------------------------------------
// Window controls
// ---------------------------------------------------------------------

const windowButtons: { el: HTMLElement; destructive: boolean }[] = [];
const windowStrip = new Strip(1, false);

function windowButton(
  id: string,
  iconName: IconName,
  destructive: boolean,
  onActivate: () => void,
): void {
  const node = byId(id);
  setIcon(node, iconName, 14);

  const index = windowButtons.length;
  windowButtons.push({ el: node, destructive });

  node.addEventListener("pointerenter", () => {
    windowStrip.hover(index);
    scheduleFrame();
  });
  node.addEventListener("pointerleave", () => {
    windowStrip.hover(null);
    scheduleFrame();
  });
  node.addEventListener("click", (event) => {
    onActivate();
    spawnRipple(node, event.clientX, event.clientY);
  });
  // A div standing in for a button has to answer the keyboard like one --
  // same reasoning as navButton.
  node.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (key === "Enter" || key === " ") {
      event.preventDefault();
      node.click();
    }
  });
}

function renderWindowButtons(now: number): void {
  windowButtons.forEach((button, index) => {
    const presentation = windowStrip.presentation(index, now);
    // Close tints towards a muted red: it is the one control in the window
    // whose result cannot be undone.
    const lit = button.destructive ? mixRgb(DANGER, BACKGROUND, 0.45) : liveChannels.surfaceHover;
    button.el.style.backgroundColor = rgba(lit, presentation.heat);
    button.el.style.color = textAt(presentation.heat);
  });
}

// ---------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------

let tabElements: HTMLElement[] = [];

// ---------------------------------------------------------------------
// Tab drag-to-reorder
// ---------------------------------------------------------------------

interface DragState {
  readonly tabId: string;
  readonly pointerId: number;
  readonly startClientX: number;
  readonly startIndex: number;
  readonly pillWidth: number;
  target: number;
}

let drag: DragState | null = null;
// Set for exactly the click that follows a drag, so letting go of a
// dragged tab does not also activate whatever slot it landed on.
let suppressNextClick = false;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

// Slides every pill between the dragged tab's old slot and its target
// out of the way by one pill's width, leaving a gap for it to drop into.
function shiftSiblings(target: number): void {
  if (!drag) {
    return;
  }
  tabElements.forEach((pill, index) => {
    if (index === drag!.startIndex) {
      return;
    }
    pill.classList.add("drag-shifting");
    let shift = 0;
    if (drag!.startIndex < target && index > drag!.startIndex && index <= target) {
      shift = -1;
    } else if (drag!.startIndex > target && index < drag!.startIndex && index >= target) {
      shift = 1;
    }
    pill.style.transform = shift ? `translateX(${shift * drag!.pillWidth}px)` : "";
  });
}

function clearDragStyles(): void {
  for (const pill of tabElements) {
    pill.classList.remove("dragging", "drag-shifting");
    pill.style.transform = "";
  }
}

function moveDrag(event: PointerEvent): void {
  if (!drag || event.pointerId !== drag.pointerId) {
    return;
  }
  const pill = tabElements[drag.startIndex];
  if (!pill) {
    return;
  }
  const deltaX = event.clientX - drag.startClientX;
  pill.style.transform = `translateX(${deltaX}px)`;

  const target = clamp(
    drag.startIndex + Math.round(deltaX / drag.pillWidth),
    0,
    tabElements.length - 1,
  );
  if (target !== drag.target) {
    drag.target = target;
    shiftSiblings(target);
  }
}

function endDrag(event: PointerEvent): void {
  if (!drag || event.pointerId !== drag.pointerId) {
    return;
  }
  const { tabId, startIndex, target } = drag;
  clearDragStyles();
  drag = null;
  suppressNextClick = true;
  if (target !== startIndex) {
    send("reorderTab", tabId, target);
  }
}

// Keyed on the URL the address resolves to, from the contract, so a page
// that moves does not silently stop being marked.
const MARKED_PAGES: readonly (readonly [string, IconName])[] = [
  [pageUrl(pages.settings), "settings"],
  [pageUrl(pages.history), "history"],
  [pageUrl(pages.bookmarks), "bookmark"],
];

function faviconFor(tab: TabState, faviconsEnabled: boolean): HTMLElement {
  // .loading drives the spinner in toolbar.css.
  const slot = el("span", {
    className: tab.loading ? "tab-favicon loading" : "tab-favicon",
  });
  // Weglet's own pages always get their own mark, favicons or not: an
  // internal page has no real site to ask.
  const marked = MARKED_PAGES.find(([url]) => url === tab.url);
  if (marked) {
    const mark = el("span", { className: "tab-favicon-mark" });
    setIcon(mark, marked[1], 14);
    slot.appendChild(mark);
    return slot;
  }
  // Everything else gets nothing by default: a favicon has to come from
  // somewhere, and asking tells them what the user has open. See
  // docs/security.md and Settings' "Show site icons" toggle.
  const src = faviconsEnabled ? faviconUrl(tab.url) : null;
  if (src) {
    const img = el("img", { className: "tab-favicon-mark" });
    img.src = src;
    img.alt = "";
    img.referrerPolicy = "no-referrer";
    img.addEventListener("error", () => img.remove(), { once: true });
    slot.appendChild(img);
  }
  return slot;
}

function tabPill(
  tab: TabState,
  index: number,
  active: boolean,
  faviconsEnabled: boolean,
): HTMLElement {
  const close = el("span", { className: "tab-close", text: "\u00d7" });
  close.setAttribute("aria-label", `Close ${tab.label}`);
  // pointerdown as well as click: pressing close must not also start
  // whatever the pill does with a press.
  close.addEventListener("pointerdown", (event) => event.stopPropagation());
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    send("closeTab", tab.id);
  });

  const pill = el("div", {
    className: active ? "tab-pill active" : "tab-pill",
    children: [
      faviconFor(tab, faviconsEnabled),
      el("span", { className: "tab-title", text: tab.label }),
      close,
    ],
  });
  pill.setAttribute("role", "tab");
  pill.setAttribute("aria-selected", String(active));
  pill.tabIndex = 0;
  pill.style.height = `${tokens.layoutTabHeight}px`;

  pill.addEventListener("click", () => {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    send("activateTab", tab.id);
  });
  pill.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (key === "Enter" || key === " ") {
      event.preventDefault();
      send("activateTab", tab.id);
    }
  });
  // A press only becomes a drag once it has moved a few pixels; short of
  // that it's a click. setPointerCapture below routes pointermove/up here
  // regardless of what's visually underneath.
  pill.addEventListener("pointerdown", (event: PointerEvent) => {
    if (event.button !== 0) {
      return;
    }
    const pointerId = event.pointerId;
    const startX = event.clientX;
    let moved = false;

    const onMove = (moveEvent: PointerEvent): void => {
      if (moveEvent.pointerId !== pointerId) {
        return;
      }
      if (!moved && Math.abs(moveEvent.clientX - startX) > 4) {
        moved = true;
        pill.setPointerCapture(pointerId);
        pill.classList.add("dragging");
        drag = {
          tabId: tab.id,
          pointerId,
          startClientX: startX,
          startIndex: index,
          pillWidth: pill.getBoundingClientRect().width,
          target: index,
        };
      }
      if (moved) {
        moveDrag(moveEvent);
      }
    };
    const onUp = (upEvent: PointerEvent): void => {
      if (upEvent.pointerId !== pointerId) {
        return;
      }
      pill.removeEventListener("pointermove", onMove);
      pill.removeEventListener("pointerup", onUp);
      pill.removeEventListener("pointercancel", onUp);
      if (moved) {
        endDrag(upEvent);
      }
    };
    pill.addEventListener("pointermove", onMove);
    pill.addEventListener("pointerup", onUp);
    pill.addEventListener("pointercancel", onUp);
  });
  return pill;
}

// ---------------------------------------------------------------------

function isEditingAddress(): boolean {
  return document.activeElement === byId("address-bar");
}

function render(state: BrowserState): void {
  currentLanguage = state.language;
  applyI18n(state.language);
  applyAccent(state.accentColor);
  applyAddressBarShape(state.addressBarShape);

  const strip = byId("tabstrip");
  const newTab = byId("new-tab");

  // Rebuilt rather than diffed -- at most a hundred pills. Skipped
  // mid-drag so a push doesn't yank the DOM out from under the pointer;
  // the strip catches up once the drag ends.
  if (!drag) {
    for (const old of strip.querySelectorAll(".tab-pill")) {
      old.remove();
    }
    tabElements = [];

    state.tabs.forEach((tab, index) => {
      const pill = tabPill(tab, index, tab.id === state.activeId, state.faviconsEnabled);
      strip.insertBefore(pill, newTab);
      tabElements.push(pill);
    });
  }

  // Independent of the drag guard above: back/forward and the address bar
  // track the active tab live regardless of whether the strip repainted.
  const active = state.tabs.find((tab) => tab.id === state.activeId);

  const address = byId<HTMLInputElement>("address-bar");
  if (!isEditingAddress()) {
    // Blank for the new tab page: its internal URL tells the user nothing
    // and invites editing.
    const url = active?.url ?? "";
    address.value = url.startsWith("chrome://weglet/") ? "" : url;
  }

  // Ctrl+L. The browser focuses this page and sets this for exactly one
  // push; putting the caret here is the part it cannot do itself. After
  // the value above, so selecting selects what is shown.
  if (state.focusOmnibox) {
    address.focus();
    address.select();
  }

  setDisabled(back, !active?.canGoBack);
  setDisabled(forward, !active?.canGoForward);
  const bookmarkToggle = byId("bookmark-toggle");
  bookmarkToggle.classList.toggle("bookmarked", state.bookmarked);
  setIcon(bookmarkToggle, "bookmark", 18, state.bookmarked);

  const siteInfo = byId("site-info");
  const isInternal = (active?.url ?? "").startsWith("chrome://weglet/");
  siteInfo.classList.toggle("protected", isInternal);
  setIcon(siteInfo, isInternal ? "shield-check" : "settings", 18, isInternal);

  scheduleFrame();
}

const back = navButton("go-back", "arrow-left", "toolbar.back", () => send("goBack"));
const forward = navButton("go-forward", "arrow-right", "toolbar.forward", () =>
  send("goForward"),
);
navButton("reload", "refresh", "toolbar.reload", () => send("reload"));
navButton(
  "bookmark-toggle",
  "bookmark",
  "toolbar.bookmark",
  () => {
    const wasBookmarked = byId("bookmark-toggle").classList.contains("bookmarked");
    send("toggleBookmark");
    send("showToast", wasBookmarked ? "toast.bookmarkRemoved" : "toast.bookmarkAdded");
  },
  undefined,
  false,
);
navButton("site-info", "settings", "toolbar.siteInfo", () => {
  // Sent as the button's own on-screen rect: the popup is a separate
  // floating widget -- see WegletSiteInfoPopup.
  const rect = byId("site-info").getBoundingClientRect();
  send("toggleSiteInfo", Math.round(rect.right), Math.round(rect.bottom));
});
navButton("copy-url", "link", "toolbar.copyUrl", () => {
  const url = byId<HTMLInputElement>("address-bar").value;
  if (url) {
    void navigator.clipboard.writeText(url);
    send("showToast", "toast.linkCopied");
  }
});
navButton("keep", "plus", "toolbar.keep", () => {
  // Sent as the button's own on-screen rect: the popup is a separate
  // floating widget, and the browser has no other way to know where
  // this page's DOM put it.
  const rect = byId("keep").getBoundingClientRect();
  send("openSaveShortcutPopup", Math.round(rect.right), Math.round(rect.bottom));
});
navButton(
  "downloads-button",
  "download",
  "toolbar.downloads",
  () => {},
  byId("downloads-icon-slot"),
);
navButton("open-settings", "dots-vertical", "toolbar.settingsHistory", () => send("toggleMenu"));

function main(): void {
  const newTab = byId("new-tab");
  setIcon(newTab, "plus");
  attachTooltip(newTab, () => t("toolbar.newTab", currentLanguage));
  newTab.addEventListener("click", () => {
    send("newTab");
  });

  windowButton("win-minimize", "minus", false, () => send("minimizeWindow"));
  windowButton("win-maximize", "square", false, () => send("toggleMaximizeWindow"));
  windowButton("win-close", "x", true, () => send("closeWindow"));

  const address = byId<HTMLInputElement>("address-bar");
  address.addEventListener("keydown", (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (key === "Enter") {
      const text = address.value.trim();
      if (text) {
        // The browser accepts whichever row is highlighted (if any)
        // instead of re-resolving this text -- see
        // WegletWindow::NavigateActiveTabFromOmnibox.
        send("navigate", text);
        // Focus moves to the page. Leaving it here sends the next
        // keystroke to the wrong place and stops render updating the
        // field.
        address.blur();
      }
    } else if (key === "Escape") {
      address.blur();
      send("hideOmniboxSuggestions");
      send("requestState");
    } else if (key === "ArrowDown") {
      event.preventDefault();
      send("moveOmniboxHighlightNext");
    } else if (key === "ArrowUp") {
      event.preventDefault();
      send("moveOmniboxHighlightPrevious");
    }
  });
  address.addEventListener("input", () => {
    const text = address.value.trim();
    if (!text) {
      send("hideOmniboxSuggestions");
      return;
    }
    const rect = address.getBoundingClientRect();
    send(
      "queryOmniboxSuggestions",
      text,
      Math.round(rect.left),
      Math.round(rect.bottom),
    );
  });
  address.addEventListener("blur", () => {
    send("hideOmniboxSuggestions");
  });

  register(buttonStrip, () => buttons.length, renderButtons);
  register(windowStrip, () => windowButtons.length, renderWindowButtons);

  onState(render);
}

runPage(main);
