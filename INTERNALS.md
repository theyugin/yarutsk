# Internals

Orientation for contributors. The user-facing reference is the [mkdocs site](https://theyugin.github.io/yarutsk/) and [docs/api.md](docs/api.md); this file is for people working on the Rust source.

For the file-by-file role of each Rust module, see [CLAUDE.md](CLAUDE.md) and the `//!` module docstrings at the top of each `src/**/*.rs` file.

## Lifecycle of a call

**Load** (`yarutsk.load(text)` → Python object):

```
str/bytes/IO
  → core::scanner   (vendored; emits Comment tokens — yarutsk modification)
  → core::parser    (vendored; event stream)
  → core::builder   (events → YamlNode tree; attaches comments/blanks; resolves aliases; applies TagPolicy)
  → core::types::YamlNode
  → py::convert::node_to_doc / materialise_node   (wraps in PyYamlMapping / PyYamlSequence / PyYamlScalar; runs schema loaders)
  → Python object
```

**Dump** (`yarutsk.dump(obj)` → text):

```
Python object
  → py::convert::extract_yaml_node   (walks Py objects; runs schema dumpers; tracks anchor identity)
  → core::types::YamlNode
  → core::emitter::emit_docs(_to)    (block-style serialiser; preserves styles/comments/blanks/tags/anchors)
  → str / Python IO
```

`load_all*` / `iter_load_all*` use `py::streaming::PyIoCharsIter` instead of an in-memory string so input streams in 8 KB chunks.

## Key invariants

- **Two layered tree types.** The pure-Rust core uses `YamlNode` (no `Py`); that's what scanner→parser→builder produces and the emitter consumes. The pyclasses store a parallel **live** tree: `YamlMapping<LiveNode>` / `YamlSequence<LiveNode>`. `LiveNode` ([src/py/live.rs](src/py/live.rs)) has three variants — `Scalar(YamlScalar)` inline, `Alias { name, resolved, materialised, meta }`, and `LivePy(Py<PyAny>)`. The boundary lives in [src/py/convert.rs](src/py/convert.rs): `materialise_*` for load and `extract_*` for dump.
- **Single source of truth.** Each `PyYamlMapping`/`PyYamlSequence` holds one `inner: YamlMapping<LiveNode>` / `YamlSequence<LiveNode>`. There is no parallel Python-side `dict`/`list` (these classes do not extend `dict`/`list`); the manually-implemented dunder methods read from and write to `inner` directly. No "dual mutation sync" — single store, single write.
- **Container children are live `Py` handles.** Inside the live tree, child mappings and sequences are stored as `LiveNode::LivePy(Py<…>)`. Because `Py` is reference-counted, `m['child']` returns the same Python object every time and mutations propagate by identity, no copy-back step required. Alias identity-sharing works the same way: `materialised` holds the `Py` produced for the anchor.
- **Scalar promotion, not `NodeParent`.** Scalars live inline as `LiveNode::Scalar(YamlScalar)`. When `mapping.node(k)` / `sequence.node(i)` first reaches one, the slot is **atomically promoted** to `LiveNode::LivePy(Py<PyYamlScalar>)` (see `is_scalar_child` in [src/py/live.rs](src/py/live.rs) and `map_child_node` / `seq_child_node` in [src/py/convert.rs](src/py/convert.rs)). After promotion, mutations on the borrowed `PyYamlScalar` land directly on the same `Py` the parent's `inner` now holds — no separate write-back step.
- **`LivePy` covers typed *and* opaque values.** It wraps either a typed yarutsk pyclass (`PyYamlMapping`/`PyYamlSequence`/`PyYamlScalar`) or an opaque user-supplied object (schema-loader output, user-assigned custom class, anything `py_to_node` couldn't natively convert). The distinction is decided by **downcast at access sites**: `for_each_live_child` ([src/py/convert.rs](src/py/convert.rs)) attempts `cast::<PyYamlMapping/Sequence/Scalar>` and skips opaque values; `extract_yaml_node` runs on whatever is there and falls through to the active schema dumper for opaque values. Earlier revisions had a separate `Container`/`OpaquePy` split in `YamlNode`; that was unified in commit 47e5637 because the access-site downcast was already authoritative.
- **Aliases preserve their spelling.** `LiveNode::Alias { name, resolved, materialised, meta }` — `resolved` (an `Arc<YamlNode>`) is the expanded value Python sees; `name` is what the emitter writes back as `*name`; `materialised` caches the resolved `Py` so repeat reads share identity with the anchor.
- **`TagPolicy` bypasses scalar coercion.** Tags registered on a `Schema` are added to `TagPolicy::raw_tags`, so the builder hands the raw YAML string to the loader instead of pre-coercing via `ScalarValue::from_str`.

## Vendored code

`src/core/scanner.rs` and `src/core/parser.rs` are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2). The only modification is that `scanner.rs` emits `Comment` tokens instead of discarding them. Avoid editing these files for non-fix reasons; doing so makes future upstream syncs painful.
