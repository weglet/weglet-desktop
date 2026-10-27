// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// protocol.ts validates every push before a page acts on it: this decides
// whether a malformed message is a dropped update or an exception halfway
// through rendering the tab strip.
//
// Run under Chromium's vendored node -- see run_ui_tests.py. No test
// framework, because the project has no npm.

import {
  onNewTab,
  onRisk,
  onSettings,
  onState,
  pushes,
  type BrowserState,
  type NewTabState,
  type RiskNotice,
  type SettingsState,
} from "../src/protocol.js";

type Receiver = (value: unknown) => void;

interface TestGlobal {
  chrome?: { send(message: string, args?: unknown[]): void };
  weglet?: Record<string, Receiver>;
}

const host = globalThis as TestGlobal;

// The runner exits non-zero on failure. Declared rather than pulled from
// @types/node: one field is not worth a dependency.
declare const process: { exit(code: number): never };

let failures = 0;
let checks = 0;

function check(condition: boolean, what: string): void {
  checks += 1;
  if (!condition) {
    failures += 1;
    console.error(`  FAIL ${what}`);
  }
}

function equal(actual: unknown, expected: unknown, what: string): void {
  check(
    JSON.stringify(actual) === JSON.stringify(expected),
    `${what}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
  );
}

// Each test starts from nothing installed and nothing sent.
const sent: string[] = [];

function reset(): void {
  host.weglet = {};
  sent.length = 0;
  host.chrome = {
    send(message: string): void {
      sent.push(message);
    },
  };
}

// Calls whatever the page installed for this push, as the browser would.
function push(name: string, value: unknown): void {
  const receiver = host.weglet?.[name];
  if (!receiver) {
    throw new Error(`nothing installed for ${name}`);
  }
  receiver(value);
}

function test(name: string, body: () => void): void {
  reset();
  console.log(name);
  body();
}

const tab = {
  id: "1",
  url: "https://example.com",
  label: "Example",
  canGoBack: false,
  canGoForward: true,
  loading: false,
};

// --- registration -----------------------------------------------------

test("registering installs the generated name and asks for state", () => {
  onState(() => {});
  check(
    typeof host.weglet?.[pushes.state] === "function",
    `installs weglet.${pushes.state}`,
  );
  // The browser only pushes on a change, so a page that loaded late asks.
  equal(sent, ["requestState"], "asks for state on registration");
});

test("several pushes coexist on one global", () => {
  onState(() => {});
  onSettings(() => {});
  check(
    typeof host.weglet?.[pushes.state] === "function" &&
      typeof host.weglet?.[pushes.settings] === "function",
    "installing one does not replace the other",
  );
});

// --- browser state ----------------------------------------------------

test("a well-formed state arrives intact", () => {
  const got: BrowserState[] = [];
  onState((state) => got.push(state));
  push(pushes.state, { tabs: [tab], activeId: "1", focusOmnibox: false, bookmarked: false });
  equal(got[0]?.tabs.length, 1, "one tab");
  equal(got[0]?.tabs[0]?.loading, false, "carries loading");
  equal(got[0]?.activeId, "1", "active id");
  equal(got[0]?.focusOmnibox, false, "carries the focus request");
});

test("a malformed state is dropped, not passed on", () => {
  let calls = 0;
  onState(() => {
    calls += 1;
  });
  for (const bad of [
    null,
    "not an object",
    {},
    { tabs: [], activeId: 1, focusOmnibox: false, bookmarked: false }, // an id as a number
    { tabs: "no", activeId: "1", focusOmnibox: false, bookmarked: false },
    { tabs: [{ ...tab, id: 1 }], activeId: "1", focusOmnibox: false, bookmarked: false },
    { tabs: [{ ...tab, loading: "yes" }], activeId: "1", focusOmnibox: false, bookmarked: false },
    { tabs: [{ ...tab, canGoBack: undefined }], activeId: "1", focusOmnibox: false, bookmarked: false },
    { tabs: [], activeId: "1", bookmarked: false },
    { tabs: [], activeId: "1", focusOmnibox: "yes", bookmarked: false },
    { tabs: [], activeId: "1", focusOmnibox: false }, // missing bookmarked
    { tabs: [], activeId: "1", focusOmnibox: false, bookmarked: "yes" },
  ]) {
    push(pushes.state, bad);
  }
  equal(calls, 0, "handler never ran");
});

// One bad tab invalidates the update rather than being skipped: a strip
// missing a tab is worse than a strip that did not repaint.
test("a state with one bad tab is rejected whole", () => {
  let calls = 0;
  onState(() => {
    calls += 1;
  });
  push(pushes.state, {
    tabs: [tab, { ...tab, id: 2 }],
    activeId: "1",
    focusOmnibox: false,
    bookmarked: false,
  });
  equal(calls, 0, "rejected");
});

// --- new tab ----------------------------------------------------------

test("new tab state arrives and is validated", () => {
  const got: NewTabState[] = [];
  onNewTab((state) => got.push(state));
  push(pushes.newTab, {
    shortcuts: [{ title: "Example", url: "https://example.com" }],
    hint: "Searches go to DuckDuckGo",
  });
  equal(got[0]?.shortcuts.length, 1, "one shortcut");
  equal(got[0]?.hint, "Searches go to DuckDuckGo", "hint");

  let calls = 0;
  onNewTab(() => {
    calls += 1;
  });
  push(pushes.newTab, { shortcuts: [{ title: 1, url: "x" }], hint: "" });
  push(pushes.newTab, { shortcuts: "no", hint: "" });
  push(pushes.newTab, { shortcuts: [], hint: 7 });
  equal(calls, 0, "malformed new tab state dropped");
});

// --- settings ---------------------------------------------------------

test("settings arrive and an unknown shape is refused", () => {
  const got: SettingsState[] = [];
  onSettings((state) => got.push(state));
  const good = {
    engines: [{ id: "duckduckgo", label: "DuckDuckGo" }],
    searchEngine: "duckduckgo",
    customSearchUrl: "",
    restoreSession: true,
    blockedHosts: ["ads.example"],
    accentColor: "#A855F7",
    addressBarShape: "pill",
    threatFeedEnabled: true,
    threatFeedUpdatedAt: 0,
    threatFeedLastUpdateFailed: false,
    askWhereToSaveDownloads: false,
    profiles: ["Default"],
    profileColors: ["#A855F7"],
    activeProfile: "Default",
  };
  push(pushes.settings, good);
  equal(got[0]?.addressBarShape, "pill", "shape");
  equal(got[0]?.engines.length, 1, "engine list");

  let calls = 0;
  onSettings(() => {
    calls += 1;
  });
  // The shape drives a CSS attribute. An unrecognised one would leave the
  // address bar with no shape at all.
  push(pushes.settings, { ...good, addressBarShape: "octagon" });
  push(pushes.settings, { ...good, engines: "no" });
  push(pushes.settings, { ...good, restoreSession: "yes" });
  equal(calls, 0, "malformed settings dropped");
});

// --- the security notice ---------------------------------------------

test("a risk notice arrives with its level intact", () => {
  const got: RiskNotice[] = [];
  onRisk((notice) => got.push(notice));
  push(pushes.risk, {
    level: "block",
    title: "Possible impersonation of Google",
    reason: "why",
    host: "gooogle.com",
    target: "https://gooogle.com/",
  });
  equal(got[0]?.level, "block", "level");
  equal(got[0]?.host, "gooogle.com", "host");
});

// The level decides whether a way through is offered. Guessing "warning"
// would invent a way past a block.
test("an unrecognised risk level is refused rather than guessed", () => {
  let calls = 0;
  onRisk(() => {
    calls += 1;
  });
  for (const level of ["danger", "", null, 2, undefined]) {
    push(pushes.risk, {
      level,
      title: "t",
      reason: "r",
      host: "h",
      target: "https://x.example/",
    });
  }
  equal(calls, 0, "no notice rendered");
});

// --- result -----------------------------------------------------------

if (failures > 0) {
  console.error(`\n${failures} of ${checks} checks failed`);
  process.exit(1);
}
console.log(`\n${checks} checks passed`);
