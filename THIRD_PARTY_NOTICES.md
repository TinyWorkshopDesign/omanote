# Third-party notices

Omanote is released under the [MIT License](LICENSE). It bundles or derives
material from the projects below, each under its own license.

| Material | Where | Source | License |
| --- | --- | --- | --- |
| Icon glyphs (subset of Symbols Nerd Font v3.5.1) | `app/static/fonts/omanote-symbols.woff2`, app and tray icons | [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) "Symbols Only"; every glyph used is from [Material Design Icons](https://github.com/Templarian/MaterialDesign) | Symbols Only font: MIT; Material Design Icons glyphs: Apache License 2.0 (per the Nerd Fonts [license audit](https://github.com/ryanoasis/nerd-fonts/blob/master/license-audit.md)) |
| Theme palettes | `app/src/themes.css`, `app/src/lib/themes.json` | [Omarchy](https://github.com/basecamp/omarchy) themes | MIT (Omarchy); the palettes derive from the original theme projects |
| JetBrains Mono | bundled in the app through `@fontsource-variable/jetbrains-mono` | [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) | SIL Open Font License 1.1 |
| E2EE test vectors | `crates/omanote-core/tests/joplin_vectors.json` | generated with the crypto code of [Joplin](https://github.com/laurent22/joplin) | test data produced by running Joplin; no Joplin code is included |

Rust crates and npm packages used at build time keep their own licenses, listed in
`Cargo.lock` and `app/package-lock.json`.
