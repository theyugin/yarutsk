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
- **Metadata lives on the node**: every piece of node-level formatting (style, comments, blank lines) is a property of the `YamlNode` itself and is reached via `parent.node(key).<field>`. There is no per-key/per-index accessor on the parent container — the parent exposes `node(key)` / `nodes()` and that's the only way in. `node(key)` returns a write-through handle (via `NodeParent` back-reference on `PyYamlScalar`; for container children it returns the live Python object already stored in the parent `dict`/`list`), so setter calls propagate to the parent's `inner` automatically.

### Style mutation API

`YamlMapping`, `YamlSequence`, and `YamlScalar` expose read/write properties for controlling YAML formatting. The same property set lives on every node type (with kind-appropriate values for `style`):

- **Common properties** (read/write on all three node types): `tag`, `anchor`, `style`, `blank_lines_before`, `comment_inline`, `comment_before`
  - `style` on `YamlScalar`: `"plain" | "single" | "double" | "literal" | "folded"`
  - `style` on `YamlMapping` / `YamlSequence`: `"block" | "flow"`
  - `blank_lines_before`: `int`, clamped 0–255 on write
  - `comment_inline` / `comment_before`: `str | None` — assign `None` to clear
- **Container-only properties**: `trailing_blank_lines`, `explicit_start`, `explicit_end`, `yaml_version`, `tag_directives` (top-level only)
- **Container navigation**:
  - `node(key)` — returns the live child node (write-through); used to reach any per-child metadata
  - `nodes()` — mapping: `[(key, node)]` pairs; sequence: `[node, ...]` — all with metadata preserved
- **Alias helpers on containers**: `get_alias(key)` / `set_alias(key, anchor_name)` — distinct from formatting metadata; stays on the parent because it replaces the child value with an alias reference.
- **In-place ops**: `sort_keys(...)` (mapping), `sort(...)` / `index(...)` (sequence) — preserve per-entry metadata
- **Copy**: `copy()` / `__copy__` / `__deepcopy__` — metadata-preserving
- **`to_python()`** — collapse to a plain `dict`/`list`/primitive tree (loses all metadata)
- **`format(*, styles=True, comments=True, blank_lines=True)`** — recursively resets cosmetic formatting to YAML defaults. `styles`: scalars → plain (multiline → literal), containers → block, `original` cleared. `comments`: clears `comment_before`/`comment_inline`. `blank_lines`: zeros `blank_lines_before` and `trailing_blank_lines`. Tags, anchors, and document-level markers are always preserved. Also available on `YamlScalar` with only `styles=True`.

Typical usage: `doc.node("key").style = "double"`, `doc.node("key").comment_inline = "hi"`, `doc.node(0).blank_lines_before = 2`. Sequence variants use integer indices instead of string keys; otherwise the surface is identical.

### Docs

When adding, changing, or removing public API methods, **update `docs/api.md`** (and `docs/integrations.md` if Schema behaviour changes) to match, alongside the `python/yarutsk/__init__.pyi` stub and the Rust source. The mkdocs site at <https://theyugin.github.io/yarutsk/> is the authoritative user-facing reference; `README.md` is a short landing page that points at the docs and should not duplicate API details.

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
- `python/yarutsk/__init__.pyi` — Python stub file for the extension module
