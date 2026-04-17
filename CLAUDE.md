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

The Rust source is split into `src/lib.rs` plus two submodules: `src/core/` (parse/emit machinery and the Rust data model) and `src/py/` (the PyO3-facing layer).

- **`src/lib.rs`** — PyO3 module entry. Defines the exception hierarchy (`YarutskError`, `ParseError`, `LoaderError`, `DumperError`), registers the Python classes, and wraps each module-level function (`load`, `loads`, `load_all`, `loads_all`, `iter_load_all`, `iter_loads_all`, `dump`, `dumps`, `dump_all`, `dumps_all`). Holds the `doc_field!` macro which extracts doc-level metadata from any of the three Python node types, plus `emit_doc_to_string` / `emit_doc_to_stream` helpers.
- **`src/core/char_traits.rs`** — vendored character classification helpers from yaml-rust2.
- **`src/core/debug.rs`** — vendored debugging helpers. Output controlled by `YAMLRUST2_DEBUG` env var (debug builds only).
- **`src/core/scanner.rs`** — tokeniser vendored from yaml-rust2. Modified to emit `Comment` tokens (inline and block) rather than discard them.
- **`src/core/parser.rs`** — event-based parser vendored from yaml-rust2. Converts token stream to `Event` enum (MappingStart, ScalarToken, etc.).
- **`src/core/types.rs`** — core Rust data model: `YamlNode` (Mapping/Sequence/Scalar/Null/Alias), `YamlMapping` (IndexMap-backed), `YamlSequence`, `YamlScalar`, `ScalarStyle`, `ContainerStyle`, `ScalarValue`.
- **`src/core/builder.rs`** — walks the parser event stream and constructs `YamlNode` trees. Tracks a frame stack for nested containers, associates `Comment` tokens with mapping entries (inline vs. before-key), and resolves anchor/alias references. Holds `TagPolicy { raw_tags: HashSet<String> }` which controls which YAML tags bypass `ScalarValue` coercion — used by the Schema system to pass raw strings to custom loaders.
- **`src/core/emitter.rs`** — hand-written block-style serialiser. Reproduces original styles, comments, blank lines, tags, and anchors. Exposes both `emit_docs` (→ `String`) and `emit_docs_to` (streaming to a writer).
- **`src/py/py_mapping.rs`** — `PyYamlMapping` (extends Python `dict`). Read/write properties, style mutators, comment accessors, `sort_keys`, `copy` / `__copy__` / `__deepcopy__`.
- **`src/py/py_sequence.rs`** — `PyYamlSequence` (extends Python `list`). Same surface as `PyYamlMapping`, keyed by integer index (`sort`, `index`).
- **`src/py/py_scalar.rs`** — `PyYamlScalar` (plain pyclass). Holds a scalar value plus style/tag/anchor/doc-level metadata.
- **`src/py/py_iter.rs`** — `PyYamlIter`, the streaming iterator returned by `iter_load_all` / `iter_loads_all`.
- **`src/py/convert.rs`** — conversion between Python objects and `YamlNode`. Anchor-state management (`init_anchor_state` / `clear_anchor_state`), `extract_yaml_node`, `node_to_doc`, `parse_stream`, `parse_text`, and `DocMeta`. Also contains `scalar_to_py_with_tag()` which applies schema/built-in loaders before returning Python values.
- **`src/py/schema.rs`** — `Schema` pyclass: a per-call registry of custom loaders/dumpers. Built-in handlers (always active) for `!!binary` (base64↔`bytes`) and `!!timestamp` (ISO 8601↔`datetime`/`date`).
- **`src/py/streaming.rs`** — `PyStreamWriter` (adapts a Python IO object to a Rust writer), plus `CharsSource`, `StringCharsIter`, and `PyIoCharsIter` used by the streaming parse/emit paths.

Each PyO3 container type holds a Rust `inner` field with the full data model; the parent `dict`/`list` is kept in sync on every mutation. The Python-visible class names are `YamlMapping` / `YamlSequence` / `YamlScalar` — the `PyYaml…` prefix is Rust-internal only.

### Exception hierarchy

`YarutskError` is the base exception; `ParseError`, `LoaderError`, and `DumperError` all extend it. All four are exported from the module.

### Key design constraints

- `PyYamlMapping` extends Python `dict`; `PyYamlSequence` extends Python `list`. This requires Python 3.12+ (PyO3's `extends = PyList` support).
- Aliases are stored as `YamlNode::Alias { name, resolved }` — the `resolved` box holds the expanded value for Python access while `name` is preserved for round-trip emission as `*name`.
- `ScalarValue::from_str` in `src/core/types.rs` implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`) which is preserved as-written via `original: Option<String>` on `YamlScalar`.
- **Dual mutation sync**: setting style on a nested container requires updating both the Rust `inner` and the Python-side parent dict — both must stay in sync on every mutation.
- **Overloaded methods via `*args`**: methods like `blank_lines_before(key)` / `blank_lines_before(key, n)` use a single PyO3 `*args` signature — 0 extra args = get, 1 extra arg = set. This is the workaround for PyO3's lack of true overloads.

### Style mutation API

`YamlMapping` and `YamlSequence` expose read/write properties and methods for controlling YAML formatting:

- **Properties** (read/write): `tag`, `anchor`, `style`, `trailing_blank_lines`, `explicit_start`, `explicit_end`, `yaml_version`, `tag_directives`
- **`node(key)`** — returns the underlying node preserving all style metadata (mapping only; sequence indexing goes through `__getitem__`)
- **`nodes()`** (mapping only) — returns `[(key, node)]` pairs with style metadata preserved
- **`scalar_style(key, style)`** — sets scalar quoting style: `"plain"`, `"single"`, `"double"`, `"literal"`, `"folded"`
- **`container_style(key, style)`** — sets `"block"` or `"flow"` on a nested mapping/sequence value; syncs both Rust `inner` and the Python parent container
- **`blank_lines_before(key)`** / **`blank_lines_before(key, n)`** — gets or sets blank lines before a key/index (0–255, clamped)
- **`comment_inline(key)`** / **`comment_inline(key, comment)`** — overloaded getter/setter for the trailing inline comment on a key/index
- **`comment_before(key)`** / **`comment_before(key, comment)`** — overloaded getter/setter for the comment block on the lines preceding a key/index
- **`get_comment_inline`** / **`set_comment_inline`** / **`get_comment_before`** / **`set_comment_before`** — non-overloaded explicit variants, useful when the overloaded form is ambiguous
- **`alias_name(key)`** — returns the anchor name if the child is an alias, else `None`
- **`set_alias(key, anchor_name)`** — replaces the child value with an alias reference to the given anchor
- **`sort_keys(...)`** (mapping) / **`sort(...)`** / **`index(...)`** (sequence) — in-place operations that preserve per-entry metadata
- **`copy()`** / **`__copy__`** / **`__deepcopy__`** — metadata-preserving copies
- **`to_dict()`** — collapse to a plain Python `dict`/`list`/primitive tree (loses all style metadata)
- **`format(*, styles=True, comments=True, blank_lines=True)`** — recursively resets cosmetic formatting to YAML defaults. `styles`: scalars → plain (multiline → literal), containers → block, `original` cleared. `comments`: clears `comment_before`/`comment_inline`. `blank_lines`: zeros `blank_lines_before` and `trailing_blank_lines`. Tags, anchors, and document-level markers are always preserved. Also available on `YamlScalar` (styles-only; `comments` and `blank_lines` are no-ops there).

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
- `TagPolicy` in `src/core/builder.rs` bypasses `ScalarValue` coercion for tags registered in the schema, so custom loaders receive the raw YAML string.

### Test files

- `tests/test_roundtrip.py` — end-to-end load→dump fidelity
- `tests/test_comments.py` — comment preservation and mutation
- `tests/test_api.py` — Python API surface
- `tests/test_constructors.py` — direct construction of `YamlMapping` / `YamlSequence` / `YamlScalar`
- `tests/test_schema.py` — Schema and custom type handling
- `tests/test_loading.py` — loading behaviour
- `tests/test_types.py` — type coercion
- `tests/test_serialization.py` — serialization edge cases
- `tests/test_sort.py` — mapping sort behaviour
- `tests/test_threading.py` — concurrent use from multiple threads
- `tests/test_invalid_input.py` — error handling and validation
- `tests/test_yaml_suite.py` — [yaml-test-suite](https://github.com/yaml/yaml-test-suite) compliance (requires `yaml-test-suite` submodule)
- `tests/typing_check.py` — mypy strict type-checking of the public API
- `yarutsk.pyi` — Python stub file for the extension module
