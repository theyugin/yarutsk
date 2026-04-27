# CLAUDE.md

## What this project is

`yarutsk` is a Python YAML library (PyO3 + Maturin) that round-trips comments, scalar styles, tags, anchors/aliases, blank lines, and explicit doc markers. `src/core/{scanner,parser,char_traits,debug}.rs` are derived from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) — upstream lives as a git submodule at `vendor/yaml-rust2` (pinned to v0.11.0) and our diff is `vendor/yarutsk.patch`. The build reads the in-tree files directly; the patch + submodule exist for refresh workflow only. See [vendor/VENDORING.md](vendor/VENDORING.md).

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
| `src/lib.rs` | PyO3 module entry; exception hierarchy; `load*`/`dump*` wrappers |
| `src/core/scanner.rs` | Vendored tokeniser; modified to emit `Comment` tokens |
| `src/core/parser.rs` | Vendored event-based parser; `Event` variants extended with anchor names, end-line, chomping, flow-style flags; collects comment tokens into `pending_comments` |
| `src/core/builder.rs` | Builds `YamlNode` trees; associates comments with entries; resolves aliases; holds `TagPolicy` |
| `src/core/types.rs` | Data model: `YamlNode`, `YamlMapping` (IndexMap), `YamlSequence`, `YamlScalar`, `ScalarStyle`, `ContainerStyle`, `ScalarValue` |
| `src/core/emitter.rs` | Hand-written block-style serialiser; preserves styles/comments/blank-lines/tags/anchors |
| `src/core/char_traits.rs`, `src/core/debug.rs` | Vendored helpers |
| `src/py/py_node.rs` | `PyYamlNode` — abstract base class extended by the three concrete pyclasses; owns the document-level metadata fields |
| `src/py/py_mapping.rs` | `PyYamlMapping` |
| `src/py/py_sequence.rs` | `PyYamlSequence` |
| `src/py/py_scalar.rs` | `PyYamlScalar` |
| `src/py/py_iter.rs` | `PyYamlIter` for `iter_load_all*` |
| `src/py/convert.rs` | Python ↔ `YamlNode` conversion; anchor state; `scalar_to_py_with_tag` |
| `src/py/schema.rs` | `Schema`: per-call loader/dumper registry; built-ins for `!!binary`, `!!timestamp`; freeze-on-first-use guard |
| `src/py/streaming.rs` | `PyStreamWriter` and char-source adapters for streaming I/O |
| `src/py/sort.rs`, `src/py/style_parse.rs` | Small focused helpers extracted from `convert.rs` |

Python-visible class names are `YamlMapping` / `YamlSequence` / `YamlScalar`; the `PyYaml…` prefix is Rust-internal.

## Exception hierarchy

`YarutskError` is the base. `ParseError`, `LoaderError`, `DumperError` extend it. All four are exported.

## Key design constraints

- `PyYamlMapping`, `PyYamlSequence`, and `PyYamlScalar` all extend the abstract `PyYamlNode` base class (Python name `YamlNode`). `isinstance(x, yarutsk.YamlNode)` is `True` for any document node; constructing `YamlNode` directly raises `TypeError`. The base owns the document-level metadata fields (`explicit_start`/`explicit_end`/`yaml_version`/`tag_directives`); the concrete subclasses access them via `slf.as_super()`.
- The three concrete pyclasses are standalone wrappers — they do **not** extend `dict`/`list`. The dict/list protocol on the first two is implemented manually via dunder methods. `isinstance(m, dict)` is `False`; use `m.to_python()` for a plain `dict`/`list`.
- **Two layered tree types.** The pure-Rust core uses `YamlNode` (no `Py`), which is what scanner→parser→builder produces and the emitter consumes. The pyclasses store a parallel **live** form: `YamlMapping<LiveNode>` / `YamlSequence<LiveNode>`. `LiveNode` (in `src/py/live.rs`) has three variants — `Scalar(YamlScalar)` inline, `Alias { name, resolved, materialised, meta }`, and `LivePy(Py<PyAny>)`. Conversion happens in `src/py/convert.rs`: `materialise_*` for load (`YamlNode` → `LiveNode`) and `extract_*` for dump (`LiveNode` → `YamlNode`).
- **Single source of truth**: each container's `inner: YamlMapping<LiveNode>` / `YamlSequence<LiveNode>` is the authoritative store. Child mappings/sequences are held as `LiveNode::LivePy(Py<…>)` so `m['child']` returns the same `Py` every time and mutations propagate by reference identity. There is no parallel Python-side `dict`/`list` to keep in sync.
- **`LiveNode::LivePy` covers both typed and opaque.** It holds either a typed yarutsk pyclass (`PyYamlMapping`/`PyYamlSequence`/`PyYamlScalar`, identity-shared via `Py` refcount) or an arbitrary user-supplied object (loader output, custom type, anything `py_to_node` couldn't natively convert). The distinction is made by **downcast at access sites** — recursive container walks (sort, format, deep-copy) successfully cast to a typed pyclass and recurse; the dump path runs `extract_yaml_node` on whatever is there and lets the active schema dumper handle opaque values.
- Aliases are stored as `LiveNode::Alias { name, resolved, materialised, meta }`: `resolved` (an `Arc<YamlNode>`) is the expanded value Python sees, `name` is preserved for round-trip emission as `*name`, and `materialised` caches the resolved Python object so repeat reads share identity with the anchor.
- `ScalarValue::from_str` in `src/core/types.rs` implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`); the original spelling is preserved via `ScalarRepr::Preserved { value, source }` on `YamlScalar`.
- **Metadata lives on the node**, not on the parent. Reach style/comments/blank-lines via `parent.node(key)` / `parent.node(index)`. For container children, `node(...)` returns the live `Py` already stored in `LivePy`. For scalar leaves (still inline as `LiveNode::Scalar`), the slot is **promoted on first access** to `LivePy(Py<PyYamlScalar>)` so subsequent reads share identity and setters land on the borrow held by the parent — otherwise the mutation would land on a clone and disappear at emit time.

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

`Schema` may also be populated via constructor kwargs (`Schema(loaders={...}, dumpers=[...])`) and is **frozen** the first time it is bound to a load/dump call — subsequent `add_loader`/`add_dumper` calls raise `RuntimeError`. The freeze flag is an `AtomicBool` so concurrent loads sharing a schema don't contend on a pyclass mut-borrow.

## Tests

`tests/` covers round-trip, comments, API surface, constructors, schema, loading, type coercion, serialization, sort, threading, invalid input, and yaml-test-suite compliance (`tests/test_yaml_suite.py`, requires submodule). `tests/typing_check.py` runs strict mypy against the public stub.
