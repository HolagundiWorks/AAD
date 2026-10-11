# Contributing to AAD

AAD (Accelerated Architecture Drafting) is open source under MIT OR Apache-2.0. Contributions are
welcome under the same terms.

1. Read [CLAUDE.md](CLAUDE.md): scope (2D only), the never-crash rules, clean-room and asset rules.
2. Build and test with `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layering, wasm).
3. New behaviour is a command with tests; update [docs/status.md](docs/status.md) when functionality lands.
4. Do not add Autodesk material (fonts, patterns, icons, templates, help text) or GPL/LGPL/AGPL code.

Report bugs and security issues through GitHub issues; for a security issue, open a private advisory.
