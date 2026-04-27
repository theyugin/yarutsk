# Contributing to yarutsk

Thanks for taking the time to contribute. This document covers the gotchas
that aren't obvious from the code. For an architectural orientation (call
lifecycle and the `NodeParent` write-through invariant), see [INTERNALS.md](INTERNALS.md).

## Setup

```bash
git submodule update --init --recursive   # yaml-test-suite + yaml-rust2
uv sync --group dev
.venv/bin/maturin develop
```

Python 3.12+ is required.

## Build after Rust changes

After any change to `src/**/*.rs`, rebuild with **maturin directly**:

```bash
.venv/bin/maturin develop
.venv/bin/python -c "import yarutsk; ..."
```

Do **not** use `uv run <cmd>` after Rust edits — `uv run` will re-sync the
environment and clobber the freshly built extension. Always invoke the
`.venv/bin/*` binaries directly.

## Tests

```bash
# core tests
.venv/bin/pytest tests/ --ignore=tests/test_yaml_suite.py -q

# yaml-test-suite compliance (requires the submodule)
.venv/bin/pytest tests/test_yaml_suite.py -q

# rust unit tests
cargo test
```

## Lint / format

Run these before sending a PR:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
.venv/bin/ruff check .
.venv/bin/ruff format --check .
.venv/bin/mypy
```

The `-D warnings` flag is enforced — every clippy diagnostic must be fixed,
not silenced. If a particular lint genuinely does not apply, add a targeted
`#[allow(...)]` on the specific item with a one-line comment explaining why.

## Dependency / licence audit

```bash
cargo install cargo-deny     # one-time
cargo deny check
```

The `deny.toml` at the repo root pins the accepted license set and denies any
crate with an active security advisory.

## Fuzzing (optional)

A `cargo-fuzz` scaffold lives in `fuzz/`. It is not run in CI (too slow for
every PR) but is useful when touching the scanner, parser, or emitter.

```bash
cargo install cargo-fuzz     # one-time
./fuzz/seed_corpus.sh        # populate fuzz/corpus/* from yaml-test-suite
cargo +nightly fuzz run scanner -- -max_total_time=30
cargo +nightly fuzz run parser  -- -max_total_time=30
cargo +nightly fuzz run roundtrip -- -max_total_time=30
```

Fuzzing requires a nightly toolchain (libFuzzer integration).

## Vendored scanner / parser

The four files `src/core/{scanner,parser,char_traits,debug}.rs` are derived
from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2). The upstream
source is a git submodule pinned in `vendor/yaml-rust2`; our diff against
it is `vendor/yarutsk.patch`. Day-to-day you edit the files in `src/core/`
directly — the build never reads the submodule or the patch.

Refresh from upstream: `make vendor-refresh` (re-applies the current patch)
or `tools/refresh-vendor.sh <commit>` (move to a new pin). After
intentionally changing any of the four files, run `make vendor-regen-patch`
and commit the patch update alongside the code change. Full procedure:
[vendor/VENDORING.md](vendor/VENDORING.md).

If a fix in our copy is also a bug in upstream, send the upstream PR first;
when it lands in a release, refresh and the patch will shrink.

## Syncing docs

The mkdocs site under `docs/` is the authoritative user-facing reference
(published to <https://theyugin.github.io/yarutsk/>). When adding, changing,
or removing any public method on `YamlMapping`, `YamlSequence`, `YamlScalar`,
or `Schema`, update `docs/api.md` (and `docs/integrations.md` if Schema
behaviour changes) alongside the `python/yarutsk/__init__.pyi` stub and the Rust source.

`README.md` is a short landing page that points at the docs site and should
not duplicate API details.
