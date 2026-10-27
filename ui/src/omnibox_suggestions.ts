// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The address bar's dropdown: bookmarks and history matching what is
// typed. Its own floating popup, not DOM inside the toolbar's own page --
// see WegletOmniboxSuggestionsPopup, reloaded on every keystroke like the
// context menu, so there is no diffing to get wrong.

import { byId, el, runPage } from "./dom.js";
import { setIcon } from "./icons.js";
import {
  onOmniboxSuggestions,
  send,
  type OmniboxSuggestionItem,
  type OmniboxSuggestionsState,
} from "./protocol.js";
import { applyAccent, applyAddressBarShape } from "./theme.js";

function row(item: OmniboxSuggestionItem, index: number, highlighted: boolean): HTMLElement {
  const icon = el("div", { className: "suggestion-icon" });
  setIcon(icon, item.isBookmark ? "bookmark" : "history", 16);

  return el("div", {
    className: highlighted ? "suggestion-row highlighted" : "suggestion-row",
    children: [
      icon,
      el("div", {
        className: "suggestion-text",
        children: [
          el("div", { className: "suggestion-label", text: item.label }),
          el("div", { className: "suggestion-url", text: item.url }),
        ],
      }),
    ],
    // el()'s onClick, not a bare listener, so the row is keyboard-reachable.
    onClick: () => send("acceptOmniboxSuggestion", index),
  });
}

function main(): void {
  const list = byId("suggestion-list");

  onOmniboxSuggestions((state: OmniboxSuggestionsState) => {
    applyAccent(state.accentColor);
    applyAddressBarShape(state.addressBarShape);
    list.replaceChildren(
      ...state.items.map((item, index) => row(item, index, index === state.highlightedIndex)),
    );
  });
}

runPage(main);
