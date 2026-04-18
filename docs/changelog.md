# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.3] - 2026-04-18

### Added
- `idempotent_emit` fuzz target and `hypothesis` property tests for `Schema`.

### Fixed
- Phantom blank lines before empty plain scalars (null sequence items, empty mapping keys) on re-parse.
- Percent-decoded tag characters now re-encoded on emit so whitespace, control chars, and non-ASCII in tags round-trip correctly.
- Inline comments after quoted scalars in sequences were silently dropped.
- Inline comments on null sequence items under a mapping value were misclassified as before-comments on re-parse.

## [0.5.2] - 2026-04-18

### Changed
- Reduce `.clone()` calls across builder, mapping, and sequence hot paths.
- Enable `clippy::pedantic` (warn) and fix its diagnostics.

## [0.5.1] - 2026-04-18

### Fixed
- Quote plain scalars with value `---` / `...` so they don't re-parse as document markers.
- Indent root-level block scalar content so it doesn't collide with comment / document-marker syntax.

## [0.5.0] - 2026-04-18

### Added
- `AnchorGuard` RAII helper that clears thread-local anchor state on drop, protecting against leaks if emit paths panic or early-return.
- `cargo-fuzz` scaffold in `fuzz/` with `scanner`, `parser`, and `roundtrip` targets, plus a `seed_corpus.sh` helper that populates corpora from `yaml-test-suite`. Not wired into CI — run locally with `cargo +nightly fuzz run <target>`.
- Strict Ruff lint selection (`E`, `W`, `F`, `I`, `UP`, `B`, `SIM`, `RUF`) and a `[lints.clippy] all = "deny"` entry in `Cargo.toml`.
- `CHANGELOG.md`, `CONTRIBUTING.md`, issue/PR templates, `.editorconfig`, and a `deny.toml` for `cargo-deny` (licenses, advisories, bans).
- Dependabot configuration for weekly Cargo, pip, and GitHub Actions updates.
- MSRV declaration (`rust-version = "1.85"`) in `Cargo.toml`.
- Sdist build runs on every pull request, not just on tag push.
- MkDocs documentation site under `docs/`, published to <https://theyugin.github.io/yarutsk/> via a new `Docs` GitHub Actions workflow that builds on pull requests and deploys on version tags.
- `docs/integrations.md` page covering pydantic / msgspec / cattrs integration patterns (tag-based and whole-document).

### Changed
- CI toolchain pinned to stable Rust (was `nightly`) across all platform jobs and the `maturin-action` wheel builds.
- Per-document metadata in the `Builder` consolidated into a single `Vec<DocMetadata>` (previously four parallel `Vec`s).
- Emit helpers in `src/lib.rs` extracted into a shared `extract_doc_and_meta` path used by both string and stream emit, and by `dump_all` / `dumps_all`.
- `README.md` minimised to a landing page — the authoritative reference is now the mkdocs site. `CLAUDE.md` and `CONTRIBUTING.md` sync-target guidance updated accordingly (edit `docs/api.md` + `yarutsk.pyi`, not the README, when changing the public API).
- Prominent AI-authored notice moved to the top of the README and the docs landing page (previously a trailing `## Disclaimer` section).

## [0.4.2] - 2026-04-18

### Changed
- Replaced `to_dict` with `to_python`, which collapses any `YamlMapping`, `YamlSequence`, or `YamlScalar` to plain Python `dict`/`list`/primitive trees (dropping all style metadata).

## [0.4.1] - 2026-04-18

### Changed
- Internal refactors to reduce line count; no behavioural changes.

## [0.4.0] - 2026-04-17

### Added
- Revised direct-construction API for `YamlMapping`, `YamlSequence`, and `YamlScalar`, allowing style metadata to be specified at construction time.

## [0.3.7] - 2026-04-17

### Changed
- Broader accepted-input set for `dump` / `dumps` / `dump_all` / `dumps_all`.

## [0.3.6] - 2026-04-17

### Fixed
- Python stub file (`yarutsk.pyi`) corrections.

## [0.3.5] - 2026-04-17

### Changed
- Internal refactors; no behavioural changes.

## [0.3.4] - 2026-04-17

### Added
- Streaming load/dump via Python IO objects (`load(stream)`, `dump(doc, stream)`, `iter_load_all(stream)`).

## [0.3.3] - 2026-04-17

### Fixed
- Various compatibility fixes.

## [0.3.2] - 2026-04-17

### Fixed
- Self-referential structures no longer cause infinite recursion during serialisation.

## [0.3.1] - 2026-04-17

### Added
- API additions for style manipulation.

## [0.3.0] - 2026-04-16

### Changed
- Significant internal refactor of the Rust data model and PyO3 bindings.

[Unreleased]: https://github.com/theyugin/yarutsk/compare/v0.5.3...HEAD
[0.5.3]: https://github.com/theyugin/yarutsk/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/theyugin/yarutsk/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/theyugin/yarutsk/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/theyugin/yarutsk/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/theyugin/yarutsk/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/theyugin/yarutsk/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/theyugin/yarutsk/compare/v0.3.7...v0.4.0
[0.3.7]: https://github.com/theyugin/yarutsk/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/theyugin/yarutsk/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/theyugin/yarutsk/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/theyugin/yarutsk/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/theyugin/yarutsk/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/theyugin/yarutsk/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/theyugin/yarutsk/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/theyugin/yarutsk/releases/tag/v0.3.0
