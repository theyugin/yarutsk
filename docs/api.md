# API reference

The complete public surface of yarutsk on a single page. Authoritative signatures live in [`yarutsk.pyi`](https://github.com/theyugin/yarutsk/blob/main/yarutsk.pyi).

## Loading and dumping

| Function | Purpose |
|---|---|
| `load(stream, *, schema=None)` | Load the first document from a file-like object |
| `loads(text, *, schema=None)` | Load the first document from a string |
| `load_all(stream, *, schema=None)` | Load every document from a multi-doc stream |
| `loads_all(text, *, schema=None)` | Load every document from a multi-doc string |
| `iter_load_all(stream, *, schema=None)` | Iterator over documents in a stream |
| `iter_loads_all(text, *, schema=None)` | Iterator over documents in a string |
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

## YamlScalar

Top-level scalar documents are wrapped in a `YamlScalar` node:

```python
doc = yarutsk.loads("42")
doc.value                              # 42 (Python int)
doc.to_python()                        # same as .value

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

## YamlMapping

`YamlMapping` is a subclass of `dict` with insertion-ordered keys. Constructor:

```python
# YamlMapping(mapping=None, *, style="block", tag=None)
m = yarutsk.YamlMapping({"a": 1, "b": 2}, style="flow")
yarutsk.dumps(m)                       # '{a: 1, b: 2}\n'
```

The full method surface, grouped by concern:

### Read / write

Every standard `dict` method works unchanged: `doc[k]`, `doc[k] = v`, `del doc[k]`, `in`, `len`, `get`, `pop`, `setdefault`, `update`, `keys` / `values` / `items`, iteration, equality, `json.dumps(doc)`. Setting an existing key preserves its position.

Also:

- `doc.to_python()` — deep conversion to a plain Python `dict` / `list` / primitive tree (loses all style metadata)
- `doc.node(key)` — returns the underlying `YamlScalar` / `YamlMapping` / `YamlSequence` preserving style/tag/anchor; `KeyError` if absent
- `doc.nodes()` — `[(key, node)]` pairs with metadata preserved

### Style

Per-child style setters/getters form `get_/set_` pairs and raise `TypeError` when applied to the wrong node kind:

- `doc.style` / `doc.style = "block" | "flow"` — container style of this mapping
- `doc.get_scalar_style(key)` / `doc.set_scalar_style(key, "plain" | "single" | "double" | "literal" | "folded")` — scalar-only; `TypeError` on containers
- `doc.get_container_style(key)` / `doc.set_container_style(key, "block" | "flow")` — container-only; `TypeError` on scalars
- `doc.tag` / `doc.tag = "!!map"` — YAML tag
- `doc.anchor` / `doc.anchor = "myanchor"` — emits `&myanchor` before the mapping
- For top-level nodes: `explicit_start`, `explicit_end`, `yaml_version`, `tag_directives`

```python
doc["nested"] = yarutsk.YamlMapping(style="flow")
doc["nested"]["x"] = 1
doc["nested"].set_scalar_style("x", "double")
```

### Comments

`get_/set_` pairs; pass `None` to `set_*` to clear:

```python
doc.get_comment_inline("key")
doc.set_comment_inline("key", "hi")
doc.set_comment_inline("key", None)       # clear

doc.get_comment_before("key")
doc.set_comment_before("key", "block\ncomment")
```

### Blank lines

```python
doc.get_blank_lines_before("key")      # int
doc.set_blank_lines_before("key", 2)   # set to 2 (clamped 0–255)
doc.trailing_blank_lines = 1           # blank lines after all entries
```

### Aliases

```python
doc = yarutsk.loads("base: &val 1\nref: *val\n")
doc.get_alias("ref")                   # 'val'
doc.get_alias("base")                  # None (has anchor, not alias)
doc["ref"]                             # 1  (resolved value always accessible)

doc.set_alias("other", "anchor")       # mark value as emitting *anchor
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

`YamlSequence` is a subclass of `list`. Everything on `YamlMapping` applies, keyed by **integer index** instead of string key. Constructor:

```python
# YamlSequence(iterable=None, *, style="block", tag=None)
s = yarutsk.YamlSequence([1, 2, 3], style="flow")
yarutsk.dumps(s)                       # '[1, 2, 3]\n'
```

All standard `list` operations work: indexing (negative supported), slicing, `append`, `insert`, `pop`, `remove`, `extend`, `index`, `count`, `reverse`, `in`, `len`, iteration, equality, `json.dumps`.

Index-keyed variants of the same methods. `IndexError` on out-of-range indices; `TypeError` when a per-kind accessor is applied to the wrong node kind.

```python
# Underlying node access
doc.node(0)                              # YamlScalar / YamlMapping / YamlSequence
doc.nodes()                              # [node, node, ...] preserving metadata

# Style / tags / anchors
doc.get_scalar_style(0)
doc.set_scalar_style(0, "double")        # scalar-only; TypeError on containers
doc.get_container_style(0)
doc.set_container_style(0, "flow")       # container-only; TypeError on scalars
doc[0] = yarutsk.YamlScalar("item", style="single")

# Comments — index-keyed
doc.get_comment_inline(0)
doc.set_comment_inline(0, "first item")
doc.get_comment_before(2)
doc.set_comment_before(2, "group B")

# Blank lines
doc.get_blank_lines_before(0)
doc.set_blank_lines_before(0, 1)

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
