# Contributing to AAD

AAD (Accelerated Architecture Drafting) is open source under MIT OR Apache-2.0. Contributions are
welcome under the same terms.

1. Read [CLAUDE.md](CLAUDE.md): scope (2D only), the never-crash rules, clean-room and asset rules.
2. Build and test with `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layering, wasm).
3. New behaviour is a command with tests; update [docs/status.md](docs/status.md) when functionality lands.
4. Do not add Autodesk material (fonts, patterns, icons, templates, help text) or GPL/LGPL/AGPL code.

Report bugs and security issues through GitHub issues; for a security issue, open a private advisory.

## Keeping the repository clean

- Generated output stays out of git: `target/`, `dist/`, `log/`, `temp/`, `plan/` are ignored. Do
  not commit build output, installers, screenshots, benchmark or profiler output, or logs. Put
  screenshots and scratch files outside the working tree (the asset check fails on stray images).
- Release files go in `dist/` (ignored, kept). Keep the latest validated installer until its
  replacement has been built and validated.
- Do not run `cargo clean` as part of normal work. It can remove all of `target/`, not only what
  you name; to free space, delete `target/debug` by hand ([docs/BUILD_AND_DEVELOPMENT.md](docs/BUILD_AND_DEVELOPMENT.md)).
- A new dependency needs a use in the code of the crate that declares it. Remove a dependency in its
  own commit, separate from version upgrades, after `cargo check --workspace --all-targets`.
- Every new file asset needs a row in `ATTRIBUTION.md`; every new document is linked from
  `README.md`; a document describes what exists, not what is planned (planned work goes in
  `docs/status.md`).
- Before deleting a file, find its references (`grep`, `#[path]`, scripts, workflows) and record why
  in the commit message.
