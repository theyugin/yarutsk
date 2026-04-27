# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Internal
- Vendored yaml-rust2 sources now tracked as a `vendor/yaml-rust2` git submodule (pinned to v0.11.0) plus `vendor/yarutsk.patch`. `make vendor-refresh` / `make vendor-regen-patch` drive the refresh workflow. Build is unchanged.

## [0.8.2] - 2026-04-28

### Internal
- Collapsed `LiveNode::Container` and `LiveNode::OpaquePy` into a single `LivePy` variant; the typed-vs-opaque distinction now happens at access sites by downcast.
- Flattened `ScalarRepr` enum into `value: ScalarValue, source: Option<String>` fields on `YamlScalar`; the demotion-on-mutation rule is visible in `set_value` instead of hidden in an enum transition.

## [0.8.1] - 2026-04-26

### Fixed
- README comparison table no longer claims `dict` / `list` subclassing as a yarutsk feature (stale post-0.8.0).

## [0.8.0] - 2026-04-26

### Changed
- **Breaking**: `YamlMapping` and `YamlSequence` no longer subclass `dict` / `list`. They still implement the full dict/list protocol (subscript, iteration, `len`, `in`, `keys`/`values`/`items`, `append`/`extend`, etc.), but `isinstance(m, dict)` is now `False`. Call `m.to_python()` (recursive) to get a plain `dict`/`list` — required for `json.dumps`, pydantic `model_validate`, msgspec, cattrs, and any library that strictly checks input type.

### Typing
- Tightened public stub: `YamlMapping.to_python()` returns `dict[str, Any]`, `YamlSequence.to_python()` returns `list[Any]`, `YamlIter` is `Iterator[YamlNode]`, and `get`/`pop`/`setdefault` plus `YamlSequence.__getitem__` use `@overload` for stdlib-style narrowing.

### Internal
- Deduplicated mapping/sequence pyclasses via a shared `container_metadata_pymethods!` macro; extracted `ChildContainer` + `for_each_opaque_child` recursion helper.
- Collapsed redundant `convert::DocMeta` into `builder::DocMetadata`; introduced `DocMetaSource` trait for uniform doc-level metadata extraction.
- Unified `needs_quoting_for_key` into `needs_quoting`; the for-key path was a behavioral duplicate.
- Added `synthetic_alias` helper for auto-anchor stub construction.

## [0.7.8] - 2026-04-26

### Changed
- **Breaking**: aliases now share Python identity with their anchored container — `*foo` and the `&foo`-anchored mapping/sequence surface as the same Python object, so mutations through any reference are visible through the others (matches plain Python dict/list reference semantics).

### Internal
- Refactor of builder/converter/type system: `NodeMeta` sidecar, typed `MapKey` for complex keys, `ScalarRepr` enum, explicit `EmitCtx`/`LoadCtx` (no more thread-locals), consolidated load/dump entry points, `Arc`-shared alias storage.

## [0.7.7] - 2026-04-26

### Changed
- Internal refactor only; no public API changes.

## [0.7.6] - 2026-04-24

### Fixed
- `YamlMapping.sort_keys(recursive=True)` now also sorts mappings nested inside sequences. Sequence item order is left unchanged.

## [0.7.5] - 2026-04-24

### Fixed
- Literal/folded block scalars no longer silently drop `\r` and other C0 controls — the emitter now falls back to double-quoted when the value can't be preserved as a block scalar.
- Newline-only scalars (`"\n"`, `"\n\n"`, …) now emit with `|+` keep chomping so they round-trip; previously the clip indicator lost every trailing newline.

## [0.7.4] - 2026-04-24

### Fixed
- Block-scalar chomping indicators (`|-`, `|+`, `>-`, `>+`) are preserved on round-trip.

## [0.7.3] - 2026-04-24

### Fixed
- Complex mapping keys (`? key` / `: value`) round-trip correctly.
- Verbatim tags (`!<…>`) no longer percent-encode flow indicators; shorthand tags still do.

## [0.7.2] - 2026-04-24

### Fixed
- Plain/single-quoted scalars and keys containing C0 controls or DEL are upgraded to double-quoted on emit.
- Plain keys inside flow mappings are quoted when they contain flow indicators.
- `comment_before` / `blank_lines_before` emit correctly when set on the first node of a root mapping or sequence.

## [0.7.1] - 2026-04-22

### Changed
- Shortened the `__init__.pyi` module docstring.

## [0.7.0] - 2026-04-22

### Added
- `comment_inline`, `comment_before`, `blank_lines_before` properties on `YamlScalar`.
- `loads` / `loads_all` / `iter_loads_all` accept UTF-8 `bytes` / `bytearray`.
- `YamlScalar.value` and `.to_python()` apply `!!binary` and `!!timestamp` tag handling.

### Changed
- **Breaking**: per-key / per-index metadata accessors removed from `YamlMapping` and `YamlSequence`; use `parent.node(key).<field>` instead.
- **Breaking**: mixed Python / Rust layout — extension is now `yarutsk._yarutsk`, stubs moved to `python/yarutsk/__init__.pyi`.

## [0.6.3] - 2026-04-20

### Changed
- `pyo3/extension-module` is now gated behind a cargo feature for `cargo check` / IDE ergonomics.
- Consolidated the scattered `Emitter` impl blocks in `src/core/emitter.rs` into a single block.

### Fixed
- `Documentation` URL in `pyproject.toml` now points to the mkdocs site.

## [0.6.2] - 2026-04-19

### Fixed
- Plain scalars containing `,` `[` `]` `{` `}` inside flow containers are now quoted on emit.

## [0.6.1] - 2026-04-19

### Fixed
- Plain scalars with leading or trailing whitespace are now quoted on emit so
  they survive round-trip; previously the parser stripped the surrounding
  whitespace and tab-only strings came back as `null`.

## [0.6.0] - 2026-04-18

Breaking API refresh: every per-key/per-index accessor is now a `get_/set_` pair.

### Added
- `YamlSequence.node(index)` and `YamlSequence.nodes()`.
- `get_scalar_style` / `set_scalar_style`, `get_container_style` / `set_container_style` on `YamlMapping` and `YamlSequence`.
- `get_blank_lines_before` / `set_blank_lines_before` on `YamlMapping` and `YamlSequence`.
- `get_alias` on `YamlMapping` and `YamlSequence` (replaces `alias_name`).

### Changed
- `set_container_style` raises `TypeError` on scalar children (previously silent no-op).

### Removed
- Setter-only `scalar_style(key, style)` and `container_style(key, style)` — use `set_scalar_style` / `set_container_style`.
- Overloaded `blank_lines_before`, `comment_inline`, `comment_before` — use the explicit `get_/set_` pairs.
- `alias_name` — renamed to `get_alias`.
- No-op `comments=` and `blank_lines=` keyword arguments on `YamlScalar.format` (scalars have no comments or blank lines to reset).

## [0.5.4] - 2026-04-18

### Changed
- Cache `datetime.datetime` / `datetime.date` imports for faster `!!timestamp` round-trip.
- Streaming parser buffer switched to `String` + byte cursor (less memory for ASCII).
- Inline scalar emit skips the per-call `String` allocation.
- Sort comparators (`sort_keys`, `sort`) issue one Python rich-compare per step instead of two.

### Fixed
- UTF-8 codepoints straddling an 8 KB stream-chunk boundary no longer raise a decode error.

## [0.5.3] - 2026-04-18

### Added
- `idempotent_emit` fuzz target and `hypothesis` property tests for `Schema`.

### Fixed
- Phantom blank lines before empty plain scalars (null sequence items, empty
  mapping keys) on re-parse.
- Percent-decoded tag characters now re-encoded on emit so whitespace, control
  chars, and non-ASCII in tags round-trip correctly.
- Inline comments after quoted scalars in sequences were silently dropped.
- Inline comments on null sequence items under a mapping value were
  misclassified as before-comments on re-parse.

## [0.5.2] - 2026-04-18

### Changed
- Reduce `.clone()` calls across builder, mapping, and sequence hot paths.
- Enable `clippy::pedantic` (warn) and fix its diagnostics.

## [0.5.1] - 2026-04-18

### Fixed
- Quote plain scalars with value `---` / `...` so they don't re-parse as
  document markers.
- Indent root-level block scalar content so it doesn't collide with
  comment / document-marker syntax.

## [0.5.0] - 2026-04-18

### Added
- `AnchorGuard` RAII helper that clears thread-local anchor state on drop,
  protecting against leaks if emit paths panic or early-return.
- `cargo-fuzz` scaffold in `fuzz/` with `scanner`, `parser`, and `roundtrip`
  targets, plus a `seed_corpus.sh` helper that populates corpora from
  `yaml-test-suite`. Not wired into CI — run locally with `cargo +nightly
  fuzz run <target>`.
- Strict Ruff lint selection (`E`, `W`, `F`, `I`, `UP`, `B`, `SIM`, `RUF`) and
  a `[lints.clippy] all = "deny"` entry in `Cargo.toml`.
- `CHANGELOG.md`, `CONTRIBUTING.md`, issue/PR templates, `.editorconfig`,
  and a `deny.toml` for `cargo-deny` (licenses, advisories, bans).
- Dependabot configuration for weekly Cargo, pip, and GitHub Actions updates.
- MSRV declaration (`rust-version = "1.85"`) in `Cargo.toml`.
- Sdist build runs on every pull request, not just on tag push.
- MkDocs documentation site under `docs/`, published to
  <https://theyugin.github.io/yarutsk/> via a new `Docs` GitHub Actions
  workflow that builds on pull requests and deploys on version tags.
- `docs/integrations.md` page covering pydantic / msgspec / cattrs
  integration patterns (tag-based and whole-document).

### Changed
- CI toolchain pinned to stable Rust (was `nightly`) across all platform jobs
  and the `maturin-action` wheel builds.
- Per-document metadata in the `Builder` consolidated into a single
  `Vec<DocMetadata>` (previously four parallel `Vec`s).
- Emit helpers in `src/lib.rs` extracted into a shared `extract_doc_and_meta`
  path used by both string and stream emit, and by `dump_all` / `dumps_all`.
- `README.md` minimised to a landing page — the authoritative reference is
  now the mkdocs site. `CLAUDE.md` and `CONTRIBUTING.md` sync-target
  guidance updated accordingly (edit `docs/api.md` + `yarutsk.pyi`, not the
  README, when changing the public API).
- Prominent AI-authored notice moved to the top of the README and the docs
  landing page (previously a trailing `## Disclaimer` section).

## [0.4.2] - 2026-04-18

### Changed
- Replaced `to_dict` with `to_python`, which collapses any `YamlMapping`,
  `YamlSequence`, or `YamlScalar` to plain Python `dict`/`list`/primitive
  trees (dropping all style metadata).

## [0.4.1] - 2026-04-18

### Changed
- Internal refactors to reduce line count; no behavioural changes.

## [0.4.0] - 2026-04-17

### Added
- Revised direct-construction API for `YamlMapping`, `YamlSequence`, and
  `YamlScalar`, allowing style metadata to be specified at construction time.

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
- Streaming load/dump via Python IO objects (`load(stream)`, `dump(doc, stream)`,
  `iter_load_all(stream)`).

## [0.3.3] - 2026-04-17

### Fixed
- Various compatibility fixes.

## [0.3.2] - 2026-04-17

### Fixed
- Self-referential structures no longer cause infinite recursion during
  serialisation.

## [0.3.1] - 2026-04-17

### Added
- API additions for style manipulation.

## [0.3.0] - 2026-04-16

### Changed
- Significant internal refactor of the Rust data model and PyO3 bindings.

[Unreleased]: https://github.com/theyugin/yarutsk/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/theyugin/yarutsk/compare/v0.7.8...v0.8.0
[0.7.8]: https://github.com/theyugin/yarutsk/compare/v0.7.7...v0.7.8
[0.7.7]: https://github.com/theyugin/yarutsk/compare/v0.7.6...v0.7.7
[0.7.6]: https://github.com/theyugin/yarutsk/compare/v0.7.5...v0.7.6
[0.7.5]: https://github.com/theyugin/yarutsk/compare/v0.7.4...v0.7.5
[0.7.4]: https://github.com/theyugin/yarutsk/compare/v0.7.3...v0.7.4
[0.7.3]: https://github.com/theyugin/yarutsk/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/theyugin/yarutsk/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/theyugin/yarutsk/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/theyugin/yarutsk/compare/v0.6.3...v0.7.0
[0.6.3]: https://github.com/theyugin/yarutsk/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/theyugin/yarutsk/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/theyugin/yarutsk/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/theyugin/yarutsk/compare/v0.5.4...v0.6.0
[0.5.4]: https://github.com/theyugin/yarutsk/compare/v0.5.3...v0.5.4
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
