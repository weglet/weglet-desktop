# Weglet

A desktop browser built on Chromium's content layer, with no browser logic
in C++. Tabs, history, the omnibox, the profile, and the phishing
heuristics are Rust. C++ is embedding: a window, a `WebContentsDelegate`,
and the plumbing content asks every embedder for. The UI --
new tab, toolbar, settings, the security notice -- is TypeScript served
from `chrome://weglet/`.

This directory lives inside a Chromium checkout, at `src/weglet/`. It is
not a standalone build; see [Building](#building).

## Layout

```
weglet/
├── app/        ContentMainDelegate -- the process entry point
├── browser/    Window, tab bridge, WebUI plumbing, the navigation throttle
├── common/     Constants shared across processes
├── renderer/   Empty on purpose -- see docs/ARCHITECTURE.md
├── rust/       weglet-core, weglet-profile, weglet-security, weglet-url,
│               weglet-ffi -- everything that is a browser rather than
│               an embedding of one
├── ui/         TypeScript pages served at chrome://weglet/, and the
│               generators that turn contract.json / tokens.json into
│               TypeScript, C++, and Rust
└── docs/       Architecture and security model
```

## Why Rust for the logic

Chromium gives an embedder a `WebContentsDelegate` and a few dozen other
interfaces; what a browser does with tabs, history, and an address bar is
entirely up to it. Putting that in Rust means the tab model, the omnibox
parser, and the phishing heuristics are unit-tested without a browser
process, a display, or Chromium's own build graph -- `cargo test` from
`weglet/rust/` runs all of it in under a second.

C++ stays a thin adapter: it owns the window and forwards content's
callbacks into the Rust state machine through `weglet-ffi`'s C ABI.

## One contract, several languages

Two files, `ui/contract.json` and `ui/tokens.json`, are the source of
truth for the messages a page can send, the values it can be pushed, and
every design token (color, spacing, motion). `ui/generate_contract.py` and
`ui/generate_tokens.py` turn them into TypeScript, a C++ header, and a
Rust module. Nothing downstream is hand-written or checked in except the
two Rust files that need to build under a plain `cargo build` with no GN
action having run first (`weglet-core/src/generated_addresses.rs`,
`weglet-profile/src/generated_defaults.rs`).

Change a message's argument types in `contract.json`, and both the C++
validator and the TypeScript type change with it -- there is nothing left
to keep in step by hand.

## What ships with no network request

Fonts, icons, the ad/tracker blocklist, the brand-impersonation rules, and
the built-in search engine list are all data files compiled into the
binary, not fetched. See [`docs/security.md`](docs/security.md) for what
that promise covers and where it stops, and
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for what's licensed
from whom.

A profile can override any of those four lists locally -- see
`docs/security.md` -- without a rebuild.

## Building

Requires a Chromium checkout with this directory at `src/weglet/`,
`rustc`/`cargo` on `PATH`, and `gclient runhooks` run at least once (it
downloads the vendored node/TypeScript `ui/build_ui.py` compiles the
pages with).

```
gn gen out/Release --args="is_debug=false symbol_level=1 is_component_build=false enable_nacl=false blink_symbol_level=0"
autoninja -C out/Release weglet
```

`weglet/CHROMIUM_VERSION` records the revision this is built and tested
against; `check_chromium_version` warns (not fails) on a mismatch.

## Testing

```
cd weglet/rust && cargo test --workspace && cargo clippy --workspace --all-targets

cd weglet/ui
python3 generators_test.py   # the contract/token generators, standalone
python3 run_ui_tests.py      # protocol.ts validation, under node

autoninja -C out/Release weglet_unittests
out/Release/weglet_unittests    # the generated contract, from the C++ side
```

The first three run without a Chromium build at all. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for what each layer's tests actually
cover, and what not to hand-edit.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) -- the three layers, who
  owns what, and why the split is where it is
- [`docs/security.md`](docs/security.md) -- the privilege boundary, the
  risk heuristics, what's advisory versus enforced, and what's
  deliberately out of scope
- [`CONTRIBUTING.md`](CONTRIBUTING.md) -- running the tests, the
  generated-file boundary, commit conventions

## License

Apache License 2.0 -- see [`LICENSE`](LICENSE). Third-party fonts, icons,
and Rust dependencies are under their own licenses; see
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
