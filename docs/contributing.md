# Contributing to yarutsk

Thanks for taking the time to contribute. This document covers the gotchas that aren't obvious from the code.

## Setup

```bash
git submodule update --init --recursive   # yaml-test-suite
uv sync --frozen --group dev
.venv/bin/maturin develop --locked
```

Python 3.12+ is required.

## Build after Rust changes

After any change to `src/**/*.rs`, rebuild with **maturin directly**:

```bash
.venv/bin/maturin develop --locked
.venv/bin/python -c "import yarutsk; ..."
```

Do **not** use `uv run <cmd>` after Rust edits — `uv run` will re-sync the environment and clobber the freshly built extension. Always invoke the `.venv/bin/*` binaries directly.

## Tests

```bash
# core tests
.venv/bin/pytest tests/ --ignore=tests/test_yaml_suite.py -q

# yaml-test-suite compliance (requires the submodule)
.venv/bin/pytest tests/test_yaml_suite.py -q

# rust unit tests
cargo test --locked
```

## Lint / format

Run these before sending a PR:

```bash
cargo fmt
cargo clippy --all-targets --locked -- -D warnings
.venv/bin/ruff check .
.venv/bin/ruff format --check .
.venv/bin/mypy
```

The `-D warnings` flag is enforced — every clippy diagnostic must be fixed, not silenced. If a particular lint genuinely does not apply, add a targeted `#[allow(...)]` on the specific item with a one-line comment explaining why.

## Dependency / licence audit

```bash
cargo install cargo-deny     # one-time
cargo deny check
```

The `deny.toml` at the repo root pins the accepted license set and denies any crate with an active security advisory.

## Fuzzing (optional)

A `cargo-fuzz` scaffold lives in `fuzz/`. All targets run for a bounded period
in scheduled weekly CI rather than on every PR. They are also useful locally
when touching the scanner, parser, or emitter.

```bash
cargo install cargo-fuzz     # one-time
./fuzz/seed_corpus.sh        # populate fuzz/corpus/* from yaml-test-suite
cargo +nightly fuzz run scanner -- -max_total_time=30
cargo +nightly fuzz run parser  -- -max_total_time=30
cargo +nightly fuzz run roundtrip -- -max_total_time=30
cargo +nightly fuzz run idempotent_emit -- -max_total_time=30
```

Fuzzing requires a nightly toolchain (libFuzzer integration).

## Syncing docs

The mkdocs site under `docs/` is the authoritative user-facing reference (published to <https://theyugin.github.io/yarutsk/>). When adding, changing, or removing any public method on `YamlMapping`, `YamlSequence`, `YamlScalar`, or `Schema`, update `docs/api.md` (and `docs/integrations.md` if Schema behaviour changes) alongside the `yarutsk.pyi` stub and the Rust source.

`README.md` is a short landing page that points at the docs site and should not duplicate API details.
