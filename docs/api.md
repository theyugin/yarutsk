# API reference

The complete public surface of yarutsk on a single page. Authoritative signatures live in [`python/yarutsk/__init__.pyi`](https://github.com/theyugin/yarutsk/blob/main/python/yarutsk/__init__.pyi).

## Loading and dumping

| Function | Purpose |
|---|---|
| `load(stream, *, schema=None)` | Load the first document from a file-like object |
| `loads(text, *, schema=None)` | Load the first document from a string or UTF-8 bytes |
| `load_all(stream, *, schema=None)` | Load every document from a multi-doc stream |
| `loads_all(text, *, schema=None)` | Load every document from a multi-doc string or UTF-8 bytes |
| `iter_load_all(stream, *, schema=None)` | Iterator over documents in a stream |
| `iter_loads_all(text, *, schema=None)` | Iterator over documents in a string or UTF-8 bytes |
| `dump(doc, stream, *, schema=None, indent=2)` | Emit a single document to a file-like object |
| `dumps(doc, *, schema=None, indent=2)` | Emit a single document to a string |
| `dump_all(docs, stream, *, schema=None, indent=2)` | Emit multiple documents to a stream |
| `dumps_all(docs, *, schema=None, indent=2)` | Emit multiple documents to a string |

`load` / `loads` return a `YamlMapping`, `YamlSequence`, or `YamlScalar` (for a top-level scalar document), or `None` for empty input. Nested container nodes are `YamlMapping` or `YamlSequence`; scalar leaves inside mappings and sequences are returned as native Python primitives (`int`, `float`, `bool`, `str`, `bytes`, `datetime.datetime`, `datetime.date`, or `None`).

`dump` / `dumps` accept `YamlMapping`, `YamlSequence`, and `YamlScalar` objects (preserving comments, styles, and tags), but also plain Python types: `dict`, `list`, `tuple`, `set`, `frozenset`, `bytes`, `bytearray`, scalar primitives, and any `collections.abc.Mapping` or iterable. Plain types are auto-converted with default formatting.

`iter_load_all` / `iter_loads_all` return a `YamlIter` object that drives the parser on demand and yields documents one at a time — never accumulating all documents in memory:

```python
import io
import yarutsk

stream = io.StringIO("---\na: 1\n---\nb: 2\n---\nc: 3\n")
for doc in yarutsk.iter_load_all(stream):
    print(doc)   # {'a': 1}, then {'b': 2}, then {'c': 3}
```

`load` / `load_all` also stream from IO in 8 KB chunks rather than reading the entire input first, but they still build and return the full document tree.

`loads` / `loads_all` / `iter_loads_all` accept either `str` or UTF-8 `bytes`/`bytearray` — useful for feeding raw process output directly:

```python
out = subprocess.run([...], capture_output=True, check=True).stdout
doc = yarutsk.loads(out)
```

Non-UTF-8 bytes raise `UnicodeDecodeError`; any other type raises `TypeError`.

## Type conversions

### Implicit coercion

Plain YAML values (no tag) are converted to Python types automatically:

| Value pattern | Python type | Examples |
|---|---|---|
| Decimal integer | `int` | `42`, `-7` |
| Hex / octal integer | `int` | `0xFF` → `255`, `0o17` → `15` |
| Float | `float` | `3.14`, `1.5e2`, `.inf`, `-.inf`, `.nan` |
| `true` / `false` (any case) | `bool` | `True`, `FALSE` |
| `yes` / `no` / `on` / `off` (any case) | `bool` | YAML 1.1 booleans |
| `null`, `Null`, `NULL`, `~`, empty value | `None` | — |
| Anything else | `str` | `hello`, `"quoted"` |

Non-canonical forms are **reproduced as written** on dump — `yes` stays `yes`, `0xFF` stays `0xFF`, `~` stays `~`.

### Explicit tags

A `!!tag` overrides implicit coercion and controls which Python type is returned:

| Tag | Python type | Notes |
|---|---|---|
| `!!str` | `str` | Forces string even if the value looks like an int, bool, or null |
| `!!int` | `int` | Parses decimal, hex (`0xFF`), and octal (`0o17`) |
| `!!float` | `float` | Promotes integer literals (`!!float 1` → `1.0`) |
| `!!bool` | `bool` | — |
| `!!null` | `None` | Forces null regardless of content (`!!null ""` → `None`) |
| `!!binary` | `bytes` | Base64-decoded on load; base64-encoded on dump |
| `!!timestamp` | `datetime.datetime` or `datetime.date` | Date-only values return `date`; datetime values return `datetime` |

```python
import datetime, yarutsk

# !!binary
doc = yarutsk.loads("data: !!binary aGVsbG8=\n")
doc["data"]                            # b'hello'

# !!timestamp
doc = yarutsk.loads("ts: !!timestamp 2024-01-15T10:30:00\n")
doc["ts"]                              # datetime.datetime(2024, 1, 15, 10, 30)

# !!float promotes integers
doc = yarutsk.loads("x: !!float 1\n")
doc["x"]                               # 1.0  (float, not int)
```

Dumping Python `bytes` / `datetime` auto-applies the appropriate tag.

## Schema — custom types

`Schema` lets you register loaders (tag → Python object, fired on load) and dumpers (Python type → tag + data, fired on dump). Pass it as a keyword argument to any load or dump function.

Schemas can be populated either via the constructor kwargs:

```python
schema = yarutsk.Schema(
    loaders={"!point": lambda d: Point(d["x"], d["y"])},
    dumpers=[(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))],
)
```

…or imperatively via `add_loader` / `add_dumper`. Once a schema is bound to a load/dump call, it is **frozen**: subsequent `add_loader`/`add_dumper` calls raise `RuntimeError`. Construct a fresh schema (or pass everything through the constructor) for new registrations.

### Mapping types

Loader receives a `YamlMapping`; dumper returns a `(tag, dict)` tuple:

```python
import yarutsk

class Point:
    def __init__(self, x, y): self.x, self.y = x, y

schema = yarutsk.Schema()
schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

doc = yarutsk.loads("origin: !point\n  x: 0\n  y: 0\n", schema=schema)
doc["origin"]                          # Point(0, 0)
```

### Scalar types

Loader receives the raw scalar string; dumper returns a `(tag, str)` tuple:

```python
class Color:
    def __init__(self, r, g, b): self.r, self.g, self.b = r, g, b

schema = yarutsk.Schema()
schema.add_loader("!color", lambda s: Color(*[int(x) for x in s.split(",")]))
schema.add_dumper(Color, lambda c: ("!color", f"{c.r},{c.g},{c.b}"))
```

A dumper can return a `YamlScalar`, `YamlMapping`, or `YamlSequence` as the second tuple element to control the emitted style — the tag from the first element is stamped on top. Returning a `YamlMapping(style="flow")` or `YamlSequence(style="flow")` emits the container in flow style.

### Overriding built-in tags

Registering a loader for `!!int`, `!!float`, `!!bool`, `!!null`, or `!!str` bypasses the built-in coercion. The callable receives the **raw YAML string** rather than the already-converted Python value:

```python
schema = yarutsk.Schema()
schema.add_loader("!!int", lambda raw: int(raw, 0))  # parses 0xFF, 0o77, etc.
doc = yarutsk.loads("x: !!int 0xFF\n", schema=schema)
doc["x"]                               # 255
```

Multiple dumpers for the same type are checked in registration order; the first `isinstance` match wins.

Worked examples for plugging yarutsk into pydantic / msgspec / cattrs live on the [Library integrations](integrations.md) page.

## YamlNode — abstract base

`YamlNode` is the abstract base class shared by `YamlMapping`, `YamlSequence`, and `YamlScalar`. `isinstance(x, yarutsk.YamlNode)` is `True` for any document node returned by `load`/`loads` or constructed directly. Constructing `YamlNode` itself raises `TypeError`; instantiate one of the concrete subclasses.

The base owns the metadata fields common to every node — document-level (`explicit_start`, `explicit_end`, `yaml_version`, `tag_directives`) and per-node (`tag`, `anchor`, `blank_lines_before`, `comment_inline`, `comment_before`). Subclass-specific surface (`style`, `format()`, `to_python()`, `node(...)`, container/scalar value accessors) lives on the concrete classes below.

## YamlScalar

Top-level scalar documents are wrapped in a `YamlScalar` node:

```python
doc = yarutsk.loads("42")
doc.value                              # 42 (Python int)
doc.to_python()                        # same as .value

# .value applies built-in tag handling
doc = yarutsk.loads("!!binary aGVsbG8=")
doc.value                              # b'hello'
doc = yarutsk.loads("!!timestamp 2024-01-01")
doc.value                              # datetime.date(2024, 1, 1)

# Style
doc = yarutsk.loads("---\n'hello'\n")
doc.style                              # 'single'
doc.style = "double"                   # 'plain'|'single'|'double'|'literal'|'folded'

# Tag
doc = yarutsk.loads("!!str 42")
doc.tag                                # '!!str'

# Anchor (demonstrated on a scalar root)
doc = yarutsk.loads("&root 42\n")
doc.anchor                             # 'root'
```

`YamlScalar` can be constructed directly to control emission when assigning into a mapping or sequence:

```python
# Constructor: YamlScalar(value, *, style="plain", tag=None)
doc["x"] = yarutsk.YamlScalar("hello", style="double")    # 'x: "hello"\n'
doc["x"] = yarutsk.YamlScalar("42", tag="!!str")          # 'x: !!str 42\n'
doc["x"] = yarutsk.YamlScalar(b"hello")                   # 'x: !!binary aGVsbG8=\n'
doc["x"] = yarutsk.YamlScalar(datetime.date(2024, 1, 15)) # 'x: !!timestamp 2024-01-15\n'
```

- `value` — `bool`, `int`, `float`, `str`, `None`, `bytes`, `bytearray`, `datetime.datetime`, or `datetime.date`
- `style` — `"plain"` (default), `"single"`, `"double"`, `"literal"`, `"folded"`
- `tag` — YAML tag string, or `None`. For `bytes` defaults to `"!!binary"`, for datetime defaults to `"!!timestamp"`

### Comments and blank lines

Comments and blank-lines-before live directly on each node. Reach the child via `parent.node(key)` (or `parent.node(index)` for a sequence) and read/write the attribute directly:

```python
doc = yarutsk.loads("port: 5432  # db port\n")
doc.node("port").comment_inline          # 'db port'

doc.node("port").comment_inline = "updated"
yarutsk.dumps(doc)                       # 'port: 5432  # updated\n'

doc.node("port").blank_lines_before = 2  # int property, clamped 0–255
```

For bare-scalar documents, `comment_before` and `comment_inline` are both preserved on the scalar:

```python
doc = yarutsk.loads("# hello\n42  # answer\n")
doc.comment_before                       # 'hello'
doc.comment_inline                       # 'answer'
```

## YamlMapping

`YamlMapping` is a standalone class implementing the `dict` protocol with
insertion-ordered keys. **`isinstance(m, dict)` is False** — call
`m.to_python()` to get a plain `dict` (recursively) when interop with
`dict`-typed APIs is needed (e.g. `json.dumps(m.to_python())`).

```python
# YamlMapping(mapping=None, *, style="block", tag=None)
m = yarutsk.YamlMapping({"a": 1, "b": 2}, style="flow")
yarutsk.dumps(m)                       # '{a: 1, b: 2}\n'
```

The full method surface, grouped by concern:

### Read / write

The dict protocol works as expected: `doc[k]`, `doc[k] = v`, `del doc[k]`, `in`, `len`, `get`, `pop`, `popitem`, `setdefault`, `update`, `clear`, `keys` / `values` / `items` (return `list`s), iteration, equality, `__or__` / `__ior__` (PEP 584), `copy`, pickle round-trip via `__reduce__`. Setting an existing key preserves its position.

Also:

- `doc.to_python()` — deep conversion to a plain Python `dict` / `list` / primitive tree (loses all style metadata). Applies built-in tag handling (`!!binary`→`bytes`, `!!timestamp`→`datetime`/`date`)
- `doc.node(key)` — returns the underlying `YamlScalar` / `YamlMapping` / `YamlSequence` preserving style/tag/anchor; `KeyError` if absent
- `doc.nodes()` — `[(key, node)]` pairs with metadata preserved

### Per-child metadata — use `node(key)`

Style, comments, and blank-lines-before live on each child node. Reach the child with `doc.node(key)` and read/write the attribute directly:

```python
doc["nested"] = yarutsk.YamlMapping(style="flow")
doc["nested"]["x"] = 1
doc["nested"].node("x").style = "double"           # scalar style

doc.node("key").comment_inline = "hi"              # comment on a child
doc.node("key").comment_inline = None              # clear
doc.node("key").comment_before = "block\ncomment"
doc.node("key").blank_lines_before = 2             # int, clamped 0–255
```

`node(key)` returns a live handle: setter calls propagate to the parent, so the change is visible on the next `dumps(doc)`.

### Whole-mapping properties

- `doc.style` / `doc.style = "block" | "flow"` — container style of this mapping itself
- `doc.tag` / `doc.tag = "!!map"` — YAML tag
- `doc.anchor` / `doc.anchor = "myanchor"` — emits `&myanchor` before the mapping
- `doc.blank_lines_before` — `int`, clamped 0–255
- `doc.trailing_blank_lines = 1` — blank lines after all entries
- Top-level-only: `explicit_start`, `explicit_end`, `yaml_version`, `tag_directives`

### Aliases

```python
doc = yarutsk.loads("base: &val 1\nref: *val\n")
doc.get_alias("ref")                   # 'val'
doc.get_alias("base")                  # None (has anchor, not alias)
doc["ref"]                             # 1  (resolved value always accessible)

doc.set_alias("other", "anchor")       # mark value as emitting *anchor
```

Aliases share Python identity with the anchored container, so mutations
through any reference are visible through the others — same reference
semantics as plain Python dicts and lists:

```python
doc = yarutsk.loads("a: &x {port: 8080}\nb: *x\n")
assert doc["a"] is doc["b"]
doc["b"]["port"] = 9090
assert doc["a"]["port"] == 9090        # shared via the anchor
```

### Sorting

```python
doc.sort_keys()                        # alphabetical, in-place
doc.sort_keys(reverse=True)
doc.sort_keys(key=lambda k: len(k))    # custom key
doc.sort_keys(recursive=True)          # also sort nested mappings
```

Sorting preserves per-entry comments — each entry carries its inline and before-key comments with it.

### Copy

- `doc.copy()` — metadata-preserving shallow copy
- `copy.copy(doc)` / `copy.deepcopy(doc)` — same

### Format

See [Normalizing formatting](#normalizing-formatting).

## YamlSequence

`YamlSequence` is a standalone class implementing the `list` protocol.
**`isinstance(s, list)` is False** — call `s.to_python()` for a plain `list`
(recursively) when interop with `list`-typed APIs is needed. Everything on
`YamlMapping` applies, keyed by **integer index** instead of string key.
Constructor:

```python
# YamlSequence(iterable=None, *, style="block", tag=None)
s = yarutsk.YamlSequence([1, 2, 3], style="flow")
yarutsk.dumps(s)                       # '[1, 2, 3]\n'
```

The list protocol works as expected: indexing (negative supported), slicing, `append`, `insert`, `pop`, `remove`, `extend`, `index`, `count`, `reverse`, `sort`, `clear`, `copy`, `in`, `len`, iteration, equality and ordering comparisons, `+` / `+=` / `*` / `*=`, pickle round-trip.

Per-item metadata is reached the same way as mappings — via `seq.node(i)`. `IndexError` on out-of-range indices.

```python
# Underlying node access
doc.node(0)                              # YamlScalar / YamlMapping / YamlSequence
doc.nodes()                              # [node, node, ...] preserving metadata

# Style
doc.node(0).style = "double"             # scalar: plain|single|double|literal|folded
doc.node(1).style = "flow"               # container: block|flow
doc[0] = yarutsk.YamlScalar("item", style="single")

# Comments
doc.node(0).comment_inline = "first item"
doc.node(2).comment_before = "group B"

# Blank lines
doc.node(0).blank_lines_before = 1

# Aliases
doc.get_alias(idx)                       # anchor name if alias, else None
doc.set_alias(idx, "anchor")

# Sorting (preserves comment metadata)
doc.sort()
doc.sort(reverse=True)
doc.sort(key=lambda v: len(v))
doc.sort(recursive=True)
```

## Normalizing formatting

`format()` strips all cosmetic metadata and resets the document to clean YAML defaults. Available on `YamlMapping`, `YamlSequence`, and `YamlScalar`; recurses into nested containers.

```python
src = """\
# Config
server:
  host: 'localhost'  # primary
  port: 8080

  debug: yes
"""
doc = yarutsk.loads(src)
doc.format()
print(yarutsk.dumps(doc))
# server:
#   host: localhost
#   port: 8080
#   debug: yes
```

Three keyword flags (all `True` by default) control what resets:

| Flag | Effect |
|---|---|
| `styles=True` | Scalar quoting → plain (multiline strings → literal block `\|`); container style → block; non-canonical originals (`0xFF`, `1.5e10`) cleared |
| `comments=True` | `comment_before` and `comment_inline` cleared on every entry/item |
| `blank_lines=True` | `blank_lines_before` and `trailing_blank_lines` zeroed |

Tags, anchors, and document-level markers (`explicit_start`, `yaml_version`, etc.) are **always preserved** — they are semantic, not cosmetic.

## Exceptions

| Class | Raised when |
|---|---|
| `YarutskError` | Base class for all library errors |
| `ParseError` | YAML input is malformed |
| `LoaderError` | Schema loader callable raised |
| `DumperError` | Schema dumper raised or returned the wrong type |

Standard Python errors also surface naturally: `RuntimeError` for unsupported Python types without a registered dumper, `KeyError` for missing mapping keys, `IndexError` for out-of-range sequence indices.

See [Error handling](errors.md) for worked examples.
