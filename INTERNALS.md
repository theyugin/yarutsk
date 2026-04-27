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

- **Single source of truth.** Each `PyYamlMapping`/`PyYamlSequence` holds one `inner: YamlMapping`/`YamlSequence`. There is no parallel Python-side `dict`/`list` (these classes do not extend `dict`/`list`); the manually-implemented dunder methods read from and write to `inner` directly. No "dual mutation sync" — single store, single write.
- **Container children are live `Py` handles.** Inside the Rust tree, child mappings and sequences are stored as `YamlNode::Container(Py<PyYamlMapping|PyYamlSequence>)`. Because `Py` is reference-counted, `m['child']` returns the same Python object every time and mutations propagate by identity, no copy-back step required. This is also how alias identity-sharing works: the resolved `Arc<YamlNode>` ultimately points at the same `Container(Py<…>)`.
- **`NodeParent` write-through for scalars only** ([src/py/convert.rs](src/py/convert.rs)). Scalars are *not* stored as `Container`/`OpaquePy`; they live inline as `YamlNode::Scalar(YamlScalar)`. So when `mapping.node(k)` / `sequence.node(i)` returns a scalar, that `PyYamlScalar` carries a `NodeParent` back-reference so setters reach the parent's `inner`. Without it, mutations would land on a clone and disappear at emit time.
- **`Container` and `OpaquePy` are split, not overloaded.** `Container(Py<PyAny>)` is guaranteed to wrap a typed `PyYamlMapping`/`PyYamlSequence`; that invariant is held by `wrap_materialised` at construction time. `OpaquePy(Py<PyAny>)` wraps anything else — schema-loader output, user-assigned custom classes, values that `py_to_node` couldn't natively convert. The dump path runs `extract_yaml_node` on both variants (re-traversing `OpaquePy` with the active schema); recursive container walks (sort, format, deep-copy) follow only `Container`. Code that pattern-matches on `YamlNode` should handle each explicitly; conflating them re-introduces the bug the split was made to avoid.
- **Aliases preserve their spelling.** `YamlNode::Alias { name, resolved, materialised, meta }` — `resolved` (an `Arc<YamlNode>`) is the expanded value Python sees; `name` is what the emitter writes back as `*name`; `materialised` caches the resolved `Py` so repeat reads share identity with the anchor.
- **`TagPolicy` bypasses scalar coercion.** Tags registered on a `Schema` are added to `TagPolicy::raw_tags`, so the builder hands the raw YAML string to the loader instead of pre-coercing via `ScalarValue::from_str`.

## Vendored code

`src/core/scanner.rs` and `src/core/parser.rs` are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2). The only modification is that `scanner.rs` emits `Comment` tokens instead of discarding them. Avoid editing these files for non-fix reasons; doing so makes future upstream syncs painful.
