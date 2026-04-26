# CLAUDE.md

## What this project is

`yarutsk` is a Python YAML library (PyO3 + Maturin) that round-trips comments, scalar styles, tags, anchors/aliases, blank lines, and explicit doc markers. Scanner/parser are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2); the only modification is that `src/core/scanner.rs` emits `Comment` tokens instead of discarding them.

## Build & test

**After any Rust change, use maturin directly — `uv run` clobbers the maturin build.**

```bash
uv sync --group dev                              # first-time setup
.venv/bin/maturin develop                        # debug rebuild after Rust changes
.venv/bin/pytest tests/ --ignore=tests/test_yaml_suite.py -v
.venv/bin/pytest tests/test_yaml_suite.py -q     # yaml-test-suite compliance
.venv/bin/mypy
cargo fmt && cargo clippy -- -D warnings         # clippy must be clean before done
.venv/bin/ruff check .
```

## Architecture

Pipeline: scanner → parser → builder → Rust data model → PyO3 Python types → emitter.

Rust source lives under `src/core/` (parse/emit + data model) and `src/py/` (PyO3 layer):

| File | Role |
|---|---|
| `src/lib.rs` | PyO3 module entry; exception hierarchy; `load*`/`dump*` wrappers; `doc_field!` macro |
| `src/core/scanner.rs` | Vendored tokeniser, modified to emit `Comment` tokens |
| `src/core/parser.rs` | Vendored event-based parser |
| `src/core/builder.rs` | Builds `YamlNode` trees; associates comments with entries; resolves aliases; holds `TagPolicy` |
| `src/core/types.rs` | Data model: `YamlNode`, `YamlMapping` (IndexMap), `YamlSequence`, `YamlScalar`, `ScalarStyle`, `ContainerStyle`, `ScalarValue` |
| `src/core/emitter.rs` | Hand-written block-style serialiser; preserves styles/comments/blank-lines/tags/anchors |
| `src/core/char_traits.rs`, `src/core/debug.rs` | Vendored helpers |
| `src/py/py_mapping.rs` | `PyYamlMapping` |
| `src/py/py_sequence.rs` | `PyYamlSequence` |
| `src/py/py_scalar.rs` | `PyYamlScalar` (plain pyclass) |
| `src/py/py_iter.rs` | `PyYamlIter` for `iter_load_all*` |
| `src/py/convert.rs` | Python ↔ `YamlNode` conversion; anchor state; `scalar_to_py_with_tag` |
| `src/py/schema.rs` | `Schema`: per-call loader/dumper registry; built-ins for `!!binary`, `!!timestamp` |
| `src/py/streaming.rs` | `PyStreamWriter` and char-source adapters for streaming I/O |

Python-visible class names are `YamlMapping` / `YamlSequence` / `YamlScalar`; the `PyYaml…` prefix is Rust-internal.

## Exception hierarchy

`YarutskError` is the base. `ParseError`, `LoaderError`, `DumperError` extend it. All four are exported.

## Key design constraints

- `PyYamlMapping` extends `dict`, `PyYamlSequence` extends `list` — requires Python 3.12+ (PyO3 `extends = PyList`).
- Aliases are stored as `YamlNode::Alias { name, resolved }`: `resolved` is the expanded value for Python access, `name` is preserved for round-trip emission as `*name`.
- `ScalarValue::from_str` in `src/core/types.rs` implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`); the original spelling is preserved via `original: Option<String>` on `YamlScalar`.
- **Dual mutation sync**: setting style on a nested container must update both the Rust `inner` and the Python-side parent `dict`/`list` — both must stay in sync on every mutation.
- **Metadata lives on the node**: style, comments, blank lines are properties of the `YamlNode`. Reach them via `parent.node(key)` / `parent.node(index)` — there is no per-key/per-index accessor on the parent. `node(key)` returns a write-through handle (via `NodeParent` back-ref on `PyYamlScalar`; for container children it returns the live Python child already stored in the parent collection).

## Public Python API

The full surface (read/write properties, `node`/`nodes`/`get_alias`/`set_alias`, `sort_keys`/`sort`/`index`, `copy`/`__copy__`/`__deepcopy__`, `to_python`, `format(...)`, `Schema`) is documented in **`docs/api.md`** — that file is the source of truth. Mirror any public-API change across:

1. The Rust source.
2. `python/yarutsk/__init__.pyi` (mypy stub).
3. `docs/api.md` (and `docs/integrations.md` for Schema changes).

`README.md` is a short landing page — do not duplicate API details there.

## Schema (custom type handling)

```python
schema = yarutsk.Schema()
schema.add_loader("!mytag", lambda val: MyType(val))
schema.add_dumper(MyType, lambda obj: ("!mytag", str(obj)))
doc = yarutsk.load(text, schema=schema)
```

Loaders receive the default-coerced value; dumpers return `(tag, data)`. Built-ins (always active): `!!binary` ↔ `bytes`, `!!timestamp` ↔ `datetime`/`date`. Tags registered in the schema bypass `ScalarValue` coercion via `TagPolicy` (`src/core/builder.rs`) so loaders see the raw YAML string.

## Tests

`tests/` covers round-trip, comments, API surface, constructors, schema, loading, type coercion, serialization, sort, threading, invalid input, and yaml-test-suite compliance (`tests/test_yaml_suite.py`, requires submodule). `tests/typing_check.py` runs strict mypy against the public stub.
