# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

`yarutsk` is a Python YAML library that round-trips documents preserving comments, scalar styles, tags, anchors/aliases, blank lines, and explicit document markers. It is a PyO3 extension module written in Rust, built with Maturin.

The scanner and parser are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) with one modification: the comment-skipping loop in `scanner.rs` emits `Comment` tokens instead of discarding them.

## Build and development commands

**After any Rust change**, build with maturin directly — do NOT use `uv run`:
```bash
.venv/bin/maturin develop          # debug build
.venv/bin/python -c "import yarutsk; ..."  # then test
```

Using `uv run` after Rust changes will clobber the maturin build.

First-time setup:
```bash
uv sync --group dev
.venv/bin/maturin develop
```

**Run tests:**
```bash
.venv/bin/pytest tests/ --ignore=tests/test_yaml_suite.py -v   # core tests
.venv/bin/pytest tests/test_yaml_suite.py -q                    # yaml-test-suite compliance
.venv/bin/pytest tests/test_roundtrip.py -v                     # round-trip tests only
```

**Lint / format:**
```bash
cargo fmt
cargo clippy
.venv/bin/ruff check .
```

**Type check (Python stubs):**
```bash
.venv/bin/mypy
```

## Architecture

The pipeline is: scanner → parser → builder → Rust data model → PyO3 Python types → emitter.

### Rust source files

- **`scanner.rs`** — tokeniser vendored from yaml-rust2. Modified to emit `Comment` tokens (inline and block) rather than discard them.
- **`parser.rs`** — event-based parser vendored from yaml-rust2. Converts token stream to `Event` enum (MappingStart, ScalarToken, etc.).
- **`types.rs`** — core Rust data model: `YamlNode` (Mapping/Sequence/Scalar/Null/Alias), `YamlMapping` (IndexMap-backed), `YamlSequence`, `YamlScalar`, `ScalarStyle`, `ContainerStyle`, `ScalarValue`.
- **`builder.rs`** — walks the parser event stream and constructs `YamlNode` trees. Tracks a frame stack for nested containers. Associates `Comment` tokens with mapping entries (inline vs. before-key). Resolves anchor/alias references.
- **`emitter.rs`** — hand-written block-style serialiser. Takes a `YamlNode` tree and writes YAML text, reproducing original styles, comments, blank lines, tags, and anchors.
- **`lib.rs`** — PyO3 glue. Defines `PyYamlMapping`, `PyYamlSequence`, `PyYamlScalar` as Python-visible classes extending `dict`, `list`, and a plain pyclass respectively. Each PyO3 type holds a Rust `inner` field with the full data model; the parent `dict`/`list` is kept in sync on every mutation. Exposes `load`, `load_all`, `loads`, `loads_all`, `dump`, `dump_all`, `dumps`, `dumps_all` to Python.

### Key design constraints

- `PyYamlMapping` extends Python `dict`; `PyYamlSequence` extends Python `list`. This requires Python 3.12+ (PyO3's `extends = PyList` support).
- Aliases are stored as `YamlNode::Alias { name, resolved }` — the `resolved` box holds the expanded value for Python access while `name` is preserved for round-trip emission as `*name`.
- `ScalarValue::from_str` in `types.rs` implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`) which is preserved as-written via `original: Option<String>` on `YamlScalar`.

### Test files

- `tests/test_roundtrip.py` — end-to-end load→dump fidelity
- `tests/test_comments.py` — comment preservation and mutation
- `tests/test_api.py` — Python API surface
- `tests/test_yaml_suite.py` — [yaml-test-suite](https://github.com/yaml/yaml-test-suite) compliance (requires `yaml-test-suite` submodule)
- `tests/typing_check.py` — mypy strict type-checking of the public API
- `yarutsk.pyi` — Python stub file for the extension module
