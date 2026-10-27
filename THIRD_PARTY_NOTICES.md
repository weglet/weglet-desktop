# Third-party notices

Weglet is licensed under the Apache License, Version 2.0 (see `LICENSE`).
It embeds Chromium's content layer, which carries its own license and is
not part of this repository. This file covers only what lives inside
`weglet/`: fonts, icon data, and Rust crates compiled into `weglet-ffi`.

Nothing listed here is fetched at build time or at runtime. Every font and
every icon path is a file in this repository, compiled into the binary --
see `docs/security.md` for why that matters.

## Fonts (`ui/fonts/`)

Three typefaces, each under the SIL Open Font License 1.1. The full text
of each project's license is checked in beside its font files
(`*-OFL.txt`); reproduced here for one place to look.

- **Instrument Sans** -- Copyright 2022 The Instrument Sans Project
  Authors (https://github.com/Instrument/instrument-sans)
- **Outfit** -- Copyright 2021 The Outfit Project Authors
  (https://github.com/Outfitio/Outfit-Fonts)
- **JetBrains Mono** -- Copyright 2020 The JetBrains Mono Project Authors
  (https://github.com/JetBrains/JetBrainsMono)

All three are distributed here as `.woff2`, converted from the upstream
`.ttf` release with `fonttools`. The OFL permits this: modifying a font's
format is not the reserved-name restriction the license is about, and no
font here is redistributed under a name the OFL reserves.

### SIL Open Font License 1.1 (full text)

```
Copyright (c) <dates>, <Copyright Holder> (<URL|email>),
with Reserved Font Name <Reserved Font Name>.

This Font Software is licensed under the SIL Open Font License, Version
1.1. This license is copied below, and is also available with a FAQ at:
https://openfontlicense.org

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to any
document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may include
source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical writer or
other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining a
copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or in
the appropriate machine-readable metadata fields within text or binary
files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name
as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any Modified
Version, except to acknowledge the contribution(s) of the Copyright
Holder(s) and the Author(s) or with their explicit written permission.

5) The Font Software, modified or unmodified, in part or in whole, must be
distributed entirely under this license, and must not be distributed
under any other license. The requirement for fonts to remain under this
license does not apply to any document created using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are not
met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER
DEALINGS IN THE FONT SOFTWARE.
```

## Icons (`ui/src/icons.ts`)

Icon path data taken from **Tabler Icons** (https://tabler.io/icons),
Copyright (c) 2020-2026 Paweł Kuna, MIT License.

Only the SVG path coordinates are used, copied by hand into a plain data
table -- see the comment at the top of `icons.ts` for why: no `innerHTML`,
anywhere, means no parsed-SVG code path for a future icon to slip through
on.

```
MIT License

Copyright (c) 2020-2026 Paweł Kuna

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rust crates (`rust/`)

Direct dependencies of the workspace; all MIT OR Apache-2.0. Pulled in via
`cargo`, not vendored. The transitive tree each of these brings in is
larger and not fully MIT/Apache: `idna` pulls in ICU tables licensed
Unicode-3.0, for instance. Run `cargo license` from `weglet/rust/` for the
complete tree with exact versions and licenses.

| Crate | Used for | License |
|---|---|---|
| [`serde`](https://serde.rs) | Reading and writing the profile's files and the per-profile override files (`brands.json`, `engines.json`) | MIT OR Apache-2.0 |
| [`serde_json_lenient`](https://docs.rs/serde_json_lenient) | The JSON those files are written in, with comments allowed in hand-edited ones | MIT OR Apache-2.0 |
| [`toml`](https://docs.rs/toml) | Converting the TOML files an older Weglet wrote | MIT OR Apache-2.0 |
| [`thiserror`](https://docs.rs/thiserror) | Error types in `weglet-profile` | MIT OR Apache-2.0 |
| [`url`](https://docs.rs/url) | URL parsing shared by `weglet-url` and `weglet-security` | MIT OR Apache-2.0 |
| [`idna`](https://docs.rs/idna) | Punycode and IDNA normalisation -- what lets the risk heuristics compare an internationalised domain to its Latin skeleton | MIT OR Apache-2.0 |
| [`psl`](https://docs.rs/psl) | The Public Suffix List, compiled in, so `blog.google` and `google.co.nz` can be told apart from an impostor | MIT OR Apache-2.0 |

## Chromium

Everything under `content/public` and the rest of the Chromium tree this
repository is a subdirectory of is licensed under Chromium's own BSD-style
license, not Apache-2.0. See `//LICENSE` at the root of the Chromium
checkout.
