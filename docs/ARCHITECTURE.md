# Architecture

Weglet is built around one idea: Chromium is an engine, not a browser.
Everything that makes a browser a browser -- tabs, history, the omnibox,
the profile, phishing detection -- lives in Rust, outside Chromium's build
graph and outside the browser process's own message loop. C++ is a thin
adapter that gives content the callbacks it asks every embedder for and
forwards them into Rust.

## The three layers

### C++ (`app/`, `browser/`, `common/`, `renderer/`)

Embeds the content layer: `ContentMainDelegate`, a `BrowserContext` whose
delegate methods mostly return `nullptr` on purpose (push messaging,
notifications, background sync -- subsystems Weglet does not want opening
connections on their own), and a window on views/Aura.

The toolbar is not native views chrome -- it is a second `WebContents`
loading `chrome://weglet/toolbar.html`, so the same TypeScript/CSS toolkit
that builds the new-tab page and settings builds the toolbar too, and
there is one rendering path for all of Weglet's UI instead of two.

Privilege is granted by `WegletWebUIControllerFactory::UseWebUIForURL`,
which checks an exact scheme and an exact host. A prefix test would let
`chrome://weglet.evil.example/` claim the same bindings; the exact match
is what makes a page's origin the only thing that decides whether it gets
a channel into the browser process.

`renderer/` is close to empty: `WegletContentRendererClient` overrides
nothing content doesn't already handle. There is no custom scripting
environment for Weglet's own pages -- they are ordinary WebUI content.

### Rust (`rust/`)

Five crates, layered by what each one is allowed to know:

- **`weglet-url`** -- host parsing with no IO. Knows that a backslash ends
  a URL's authority the same as a slash, and that the host is what
  follows the *last* `@`. Everything downstream that needs a host from a
  URL goes through this, so there is one place that can get it wrong
  instead of several.
- **`weglet-core`** -- tabs, windows, history, the omnibox parser. No IO,
  no platform, no knowledge that a browser process or a renderer exists.
  This is what makes `cargo test` from `weglet/rust/` a complete test of
  the tab and window model without a display or a browser running.
  A tab's back/forward list is the one exception to "Rust decides": the
  engine's own navigation controller owns it and does the navigating, so
  Back gets the page cache, the scroll position and the form state, and
  a page's `history.back()` and the Back button walk the same list. The
  tab keeps a copy, updated after every commit, for the tab strip and the
  session, and a restored tab hands its copy back to the engine.
- **`weglet-security`** -- the phishing/impersonation heuristics and the
  ad/tracker blocklist. Advisory: nothing else depends on it being right,
  and its false-positive/false-negative rates are measured against a
  corpus (`weglet-security/tests/corpus.rs`) rather than trusted on
  faith.
- **`weglet-profile`** -- settings and session persistence. Atomic writes
  (temp file + rename), because a partial write on an update that failed
  to finish must never be read back as valid.
- **`weglet-ffi`** -- the C ABI the other four are compiled behind. An
  opaque `WegletState` handle, ~60 `extern "C"` functions, every string
  crossing the boundary owned and freed by whichever side allocated it.
  `weglet_ffi.h` is the contract; `rust/weglet-ffi/tests/boundary.rs`
  binds every function by its C signature and calls it the way C++ does,
  so a signature drift between the header and the Rust source is a link
  error in `cargo test`, not undefined behaviour in the browser process.

### TypeScript (`ui/`)

Four pages -- new tab, toolbar, settings, the security notice -- served
from `chrome://weglet/`. No framework, no build-time dependency beyond the
TypeScript compiler Chromium already vendors.

`ui/src/dom.ts` has no way to set `innerHTML`. Every string that reaches
the DOM goes through `textContent`; icons are built from path-data arrays
(`icons.ts`) via `createElementNS`, not parsed from SVG markup. That is a
structural choice, not a lint rule: there is no code path in this codebase
that parses untrusted HTML, so there's nothing for a future contributor to
extend by accident into one.

## The generated contract

`ui/contract.json` and `ui/tokens.json` are the only place a message name,
a message's argument types, a page's route, or a design token is written.
`ui/generate_contract.py` and `ui/generate_tokens.py` turn them into:

- a TypeScript module (`ui/src/contract.ts`, `ui/src/tokens.ts`)
- a C++ header (`gen/weglet/ui/generated_contract.h`,
  `generated_tokens.h`) with a `MessageSpec` table the browser process
  validates every incoming message against
- two checked-in Rust files (`weglet-core/src/generated_addresses.rs`,
  `weglet-profile/src/generated_defaults.rs`) for Weglet's internal
  addresses and the default accent color, so those two crates build under
  a plain `cargo build` with no GN action having run first

This exists because the alternative -- a message name typed by hand in
three languages -- drifts. `generators_test.py` and
`browser/weglet_contract_unittest.cc` both exercise the generator and its
output directly, independent of the rest of the build.

## State flow

The browser process is the only writer of Weglet's state; every page is a
reader that renders whatever it's pushed and sends messages back through
`chrome.send`.

- **`WegletBridge`** (`browser/weglet_bridge.h`) is the sync boundary
  between C++ and the Rust `WegletState`: every call blocks, because the
  Rust side has no IO and nothing to block on.
- **`WegletStateService`** (a `BrowserContext`-scoped
  `base::SupportsUserData::Data`) tracks which pages exist, which window
  each toolbar belongs to, and pushes state to the pages a given change
  actually affects -- a settings change doesn't repaint the toolbar, and a
  tab-model change in one window doesn't repaint another window's
  toolbar.
- **`WegletMessageHandler`** validates every incoming message against the
  generated `MessageSpec` table before it reaches a handler. A page is
  Weglet's own code, but it runs in a renderer process; a compromised
  renderer is exactly the case this validation is for.

## Navigation and security

`WegletSecurityGuard` (also `BrowserContext`-scoped) is the one place that
decides whether a navigation may proceed: the user's block list, then the
built-in one, then the heuristics in `weglet-security`.

It's reached two ways:

- **`WegletNavigationThrottle`**, registered from
  `WegletContentBrowserClient::CreateThrottlesForNavigation`, which is
  content's own extension point for this. It runs on every top-level
  navigation the network stack starts, including redirects -- the case a
  shortened link resolving to a lookalike domain arrives as.
- **`WegletWindow`**, for the URLs it hands the engine directly (a
  restored tab, a programmatic navigation).

A stopped navigation records a `Notice` on the guard and opens
`chrome://weglet/security_notice.html`, which pulls the notice's wording
from `WegletStateService` -- the same push mechanism every other page
uses, not a separate channel.

## Multi-window

`WindowId` is a first-class value in `weglet-core`: `AppState` owns
`Vec<Window>`, each with its own tabs and its own active tab, rather than
one flat tab list. `WegletWindow::window_id_` is what every tab-model call
from C++ is scoped to, so two windows can never observe or mutate each
other's tabs.

`Session` (in `weglet-profile`) persists per-window; a session file
written before windows existed is read as a single window on load, not
discarded.

## What's deliberately out of scope

- **Extensions.** No extension system exists.
- **Sync.** No account system, no cross-device anything.
- **Encryption at rest.** `settings.json` and `session.json` are plain
  files; disk encryption is the platform's job, not Weglet's.
- **Per-resource ad blocking.** The built-in blocklist currently applies
  at navigation time, through the same throttle as the phishing
  heuristics -- not per subresource. `CreateURLLoaderThrottles` is the
  extension point for that; nothing currently uses it for resource-level
  blocking.

See [`security.md`](security.md) for the security model in
detail, including what each of the four data-driven lists (blocklist,
brand rules, sensitive words, search engines) can and can't be overridden
by a profile.
