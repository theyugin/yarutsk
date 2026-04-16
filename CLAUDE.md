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
cargo clippy -- -D warnings   # treat all warnings as errors
.venv/bin/ruff check .
```

Always run `cargo clippy -- -D warnings` after Rust changes and fix every diagnostic before considering the work done.

**Type check (Python stubs):**
```bash
.venv/bin/mypy
```

## Architecture

The pipeline is: scanner → parser → builder → Rust data model → PyO3 Python types → emitter.

### Rust source files

- **`char_traits.rs`** — vendored character classification helpers from yaml-rust2.
- **`debug.rs`** — vendored debugging helpers. Debug output controlled by `YAMLRUST2_DEBUG` env var (debug builds only).
- **`scanner.rs`** — tokeniser vendored from yaml-rust2. Modified to emit `Comment` tokens (inline and block) rather than discard them.
- **`parser.rs`** — event-based parser vendored from yaml-rust2. Converts token stream to `Event` enum (MappingStart, ScalarToken, etc.).
- **`types.rs`** — core Rust data model: `YamlNode` (Mapping/Sequence/Scalar/Null/Alias), `YamlMapping` (IndexMap-backed), `YamlSequence`, `YamlScalar`, `ScalarStyle`, `ContainerStyle`, `ScalarValue`.
- **`builder.rs`** — walks the parser event stream and constructs `YamlNode` trees. Tracks a frame stack for nested containers. Associates `Comment` tokens with mapping entries (inline vs. before-key). Resolves anchor/alias references. Also contains `TagPolicy { raw_tags: HashSet<String> }` which controls which YAML tags bypass `ScalarValue` coercion — used by the Schema system to pass raw strings to custom loaders.
- **`emitter.rs`** — hand-written block-style serialiser. Takes a `YamlNode` tree and writes YAML text, reproducing original styles, comments, blank lines, tags, and anchors.
- **`lib.rs`** — PyO3 glue. Defines `PyYamlMapping`, `PyYamlSequence`, `PyYamlScalar` as Python-visible classes extending `dict`, `list`, and a plain pyclass respectively. Each PyO3 type holds a Rust `inner` field with the full data model; the parent `dict`/`list` is kept in sync on every mutation. Exposes `load`, `load_all`, `loads`, `loads_all`, `dump`, `dump_all`, `dumps`, `dumps_all` to Python. Also defines `Schema` (a per-call registry of custom loaders/dumpers), built-in handlers for `!!binary` (base64↔bytes) and `!!timestamp` (ISO 8601↔`datetime`/`date`), and a `scalar_to_py_with_tag()` helper that applies these before returning Python values.

### Key design constraints

- `PyYamlMapping` extends Python `dict`; `PyYamlSequence` extends Python `list`. This requires Python 3.12+ (PyO3's `extends = PyList` support).
- Aliases are stored as `YamlNode::Alias { name, resolved }` — the `resolved` box holds the expanded value for Python access while `name` is preserved for round-trip emission as `*name`.
- `ScalarValue::from_str` in `types.rs` implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`) which is preserved as-written via `original: Option<String>` on `YamlScalar`.
- **Dual mutation sync**: setting style on a nested container requires updating both the Rust `inner` and the Python-side parent dict — both must stay in sync on every mutation.
- **Overloaded methods via `*args`**: methods like `blank_lines_before(key)` / `blank_lines_before(key, n)` use a single PyO3 `*args` signature — 0 extra args = get, 1 extra arg = set. This is the workaround for PyO3's lack of true overloads.

### Style mutation API

`PyYamlMapping` and `PyYamlSequence` expose read/write properties and methods for controlling YAML formatting:

- **Properties** (read/write): `tag`, `style`, `explicit_start`, `explicit_end`, `yaml_version`, `tag_directives`
- **`node(key)`** / **`node(idx)`** — returns the underlying `YamlNode` preserving all style metadata
- **`scalar_style(key, style)`** — sets scalar quoting style: `"plain"`, `"single"`, `"double"`, `"literal"`, `"folded"`
- **`container_style(key, style)`** — sets `"block"` or `"flow"` on a nested mapping/sequence value; syncs both Rust `inner` and Python parent dict
- **`blank_lines_before(key)`** / **`blank_lines_before(key, n)`** — gets or sets blank lines before a key/index (0–255, clamped)
- **`format(*, styles=True, comments=True, blank_lines=True)`** — recursively resets cosmetic formatting to YAML defaults. `styles`: scalars → plain (multiline → literal), containers → block, `original` cleared. `comments`: clears `comment_before`/`comment_inline`. `blank_lines`: zeros `blank_lines_before` and `trailing_blank_lines`. Tags, anchors, and document-level markers are always preserved. Also available on `PyYamlScalar` (styles-only; comments/blank_lines are no-ops).

Sequence variants use integer indices instead of string keys.

### README

When adding, changing, or removing public API methods, **update `README.md`** to match. The README is the primary user-facing reference; it must stay in sync with the implementation.

### Schema / custom type handling

```python
schema = yarutsk.Schema()
schema.add_loader("!mytag", lambda val: MyType(val))
schema.add_dumper(MyType, lambda obj: ("!mytag", str(obj)))
doc = yarutsk.load(text, schema=schema)
```

- Loaders receive the default-coerced Python value for the tagged scalar.
- Dumpers return `(tag: str, data)` tuples.
- Built-in handlers (always active): `!!binary` ↔ `bytes` (base64), `!!timestamp` ↔ `datetime.datetime` / `datetime.date`.
- `TagPolicy` in `builder.rs` bypasses `ScalarValue` coercion for tags registered in the schema, so custom loaders receive the raw YAML string.

### Test files

- `tests/test_roundtrip.py` — end-to-end load→dump fidelity
- `tests/test_comments.py` — comment preservation and mutation
- `tests/test_api.py` — Python API surface
- `tests/test_schema.py` — Schema and custom type handling
- `tests/test_loading.py` — loading behaviour
- `tests/test_types.py` — type coercion
- `tests/test_serialization.py` — serialization edge cases
- `tests/test_sort.py` — mapping sort behaviour
- `tests/test_invalid_input.py` — error handling and validation
- `tests/test_yaml_suite.py` — [yaml-test-suite](https://github.com/yaml/yaml-test-suite) compliance (requires `yaml-test-suite` submodule)
- `tests/typing_check.py` — mypy strict type-checking of the public API
- `yarutsk.pyi` — Python stub file for the extension module
