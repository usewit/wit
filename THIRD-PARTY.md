# Third-party software and fonts

Wit is built on other people's open work. This file credits it and names
each licence. Every licence below allows Wit to use and redistribute the
work, including in the installers.

## Fonts bundled in `fonts/`

All are under the SIL Open Font License 1.1. Each family's licence text is in
`fonts/licenses/`.

| Font | Copyright |
|---|---|
| EB Garamond | 2017 The EB Garamond Project Authors |
| Crimson Pro | 2018 The Crimson Pro Project Authors |
| Libre Baskerville | 2012 The Libre Baskerville Project Authors (Reserved Font Name "Libre Baskerville") |
| Literata | 2017 The Literata Project Authors |
| Lora | 2011 The Lora Project Authors (Reserved Font Name "Lora") |
| Alegreya | 2011 The Alegreya Project Authors |
| Source Serif 4 | 2014 The Source Serif 4 Project Authors |
| Andika | 2004–2022 SIL International |
| Atkinson Hyperlegible | 2020 Braille Institute of America, Inc. |
| Courier Prime | 2015 The Courier Prime Project Authors |

Typst also embeds its own default fonts (New Computer Modern, Libertinus and
DejaVu Sans Mono), under the GUST Font License, the SIL Open Font License and
the Bitstream Vera licence.

## Web interface (`web/`)

| Package | Licence |
|---|---|
| CodeMirror 6 (`codemirror`, `@codemirror/*`, `@lezer/highlight`) | MIT |
| `diff` (jsdiff) | BSD-3-Clause |
| Fonts from Fontsource: Atkinson Hyperlegible, Courier Prime, IBM Plex Mono, Inter, JetBrains Mono, Literata, Merriweather, Newsreader, Source Sans 3 | SIL Open Font License 1.1 |
| Build tools: Vite, TypeScript (not shipped) | MIT, Apache-2.0 |

Their own dependencies are under MIT, ISC, BSD-3-Clause, Apache-2.0 or
MPL-2.0.

## Server and desktop app (Rust)

| Crate | Licence |
|---|---|
| Typst (`typst`, `typst-kit`, `typst-layout`, `typst-pdf`, `typst-render`), which lays out the pages | Apache-2.0 |
| Tauri (`tauri`, `tauri-plugin-dialog`, `tauri-plugin-opener`), the desktop app | Apache-2.0 or MIT |
| `axum`, `tokio`, `tower-http`, `tracing`, `tracing-subscriber` | MIT |
| `pulldown-cmark` (Markdown) | MIT |
| `imagesize` | MIT |
| `serde`, `serde_json`, `serde_yaml`, `anyhow`, `comemo`, `url` | MIT or Apache-2.0 |

Their own dependencies are almost all under MIT, Apache-2.0, BSD, ISC, Zlib,
Unicode-3.0 or the Unlicense. A few are under MPL-2.0 (`cssparser`,
`cssparser-macros`, `selectors`, `dtoa-short`, `option-ext`). Wit uses them
unmodified, and their source is on crates.io, which is what MPL-2.0 asks of
software that ships them.

The full list, with versions, is in `Cargo.lock` and `web/package-lock.json`.
