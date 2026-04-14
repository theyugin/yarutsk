# yarutsk

[![PyPI](https://img.shields.io/pypi/v/yarutsk)](https://pypi.org/project/yarutsk/)
[![Python 3.12+](https://img.shields.io/pypi/pyversions/yarutsk)](https://pypi.org/project/yarutsk/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Python YAML library that round-trips documents while preserving **comments**, **insertion order**, **scalar styles**, **tags**, **anchors and aliases**, **blank lines**, and **explicit document markers**.

## What it does

Most YAML libraries silently drop comments on load. yarutsk keeps them attached to their keys — both inline (`key: value  # like this`) and block-level (`# above a key`) — so a load → modify → dump cycle leaves the rest of the file intact.

```python
import io
import yarutsk

doc = yarutsk.load(io.StringIO("""
# database config
host: localhost  # primary
port: 5432
"""))

doc["port"] = 5433

out = io.StringIO()
yarutsk.dump(doc, out)
print(out.getvalue())
# # database config
# host: localhost  # primary
# port: 5433
```

`YamlMapping` is a subclass of `dict` and `YamlSequence` is a subclass of `list`, so they work everywhere a dict or list is expected:

```python
import json

doc = yarutsk.loads("name: Alice\nscores: [10, 20, 30]")

isinstance(doc, dict)           # True
isinstance(doc["scores"], list) # True
json.dumps(doc)                 # '{"name": "Alice", "scores": [10, 20, 30]}'
```

## Round-trip fidelity

yarutsk reproduces the source text exactly for everything it understands. A `loads` followed by `dumps` gives back the original string byte-for-byte in the common case:

```python
src = """\
defaults: &base
  timeout: 30
  retries: 3

service:
  name: api
  config: *base
"""
assert yarutsk.dumps(yarutsk.loads(src)) == src
```

Specifically preserved:

- **Scalar styles** — plain, `'single-quoted'`, `"double-quoted"`, literal block `|`, folded block `>`
- **Non-canonical scalars** — `yes`/`no`/`on`/`off`, `~`, `Null`, `True`/`False`, `0xFF`, `0o77` — reproduced as written, not re-canonicalised to `true`/`false`/`null`/`255`
- **YAML tags** — `!!str`, `!!python/tuple`, and any custom tag are emitted back verbatim
- **Anchors and aliases** — `&name` on the anchor node and `*name` for references are preserved; the Python layer returns the resolved value transparently
- **Blank lines** between mapping entries and sequence items
- **Explicit document markers** — `---` and `...`

## Installation

```bash
pip install yarutsk
```

To build from source (requires Rust 1.85+):

```bash
pip install maturin
git clone --recurse-submodules https://github.com/theyugin/yarutsk
cd yarutsk
maturin develop
```

## API

### Loading and dumping

```python
# Load from stream (StringIO / BytesIO)
doc  = yarutsk.load(stream)            # first document
docs = yarutsk.load_all(stream)        # all documents as a list

# Load from string
doc  = yarutsk.loads(text)
docs = yarutsk.loads_all(text)

# Dump to stream
yarutsk.dump(doc, stream)
yarutsk.dump_all(docs, stream)

# Dump to string
text = yarutsk.dumps(doc)
text = yarutsk.dumps_all(docs)

# Custom indentation (default is 2 spaces)
text = yarutsk.dumps(doc, indent=4)
```

`load` / `loads` return a `YamlMapping`, `YamlSequence`, or `YamlScalar` (for a top-level scalar document), or `None` for empty input. Nested container nodes are `YamlMapping` or `YamlSequence`; scalar leaves inside mappings and sequences are returned as native Python primitives (`int`, `float`, `bool`, `str`, `bytes`, `datetime.datetime`, `datetime.date`, or `None`).

### Type conversions

#### Implicit coercion

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

#### Explicit tags

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

Tags are preserved through the round-trip: load → dump reproduces the original tag and source text exactly.

```python
import datetime

# !!binary
doc = yarutsk.loads("data: !!binary aGVsbG8=\n")
doc["data"]                            # b'hello'
yarutsk.dumps(doc)                     # 'data: !!binary aGVsbG8=\n'

# !!timestamp
doc = yarutsk.loads("ts: !!timestamp 2024-01-15T10:30:00\n")
doc["ts"]                              # datetime.datetime(2024, 1, 15, 10, 30)

doc = yarutsk.loads("ts: !!timestamp 2024-01-15\n")
doc["ts"]                              # datetime.date(2024, 1, 15)

# !!float promotes integers
doc = yarutsk.loads("x: !!float 1\n")
doc["x"]                               # 1.0  (float, not int)

# !!str forces a string
doc = yarutsk.loads("x: !!str 42\n")
doc["x"]                               # '42'

# Dumping Python bytes / datetime automatically produces the right tag
mapping = yarutsk.loads("x: placeholder\n")
mapping["x"] = b"hello"
yarutsk.dumps(mapping)                 # 'x: !!binary aGVsbG8=\n'

mapping["x"] = datetime.datetime(2024, 1, 15, 10, 30)
yarutsk.dumps(mapping)                 # 'x: !!timestamp 2024-01-15T10:30:00\n'
```

### Schema — custom types

`Schema` lets you register loaders (tag → Python object, fired on load) and dumpers (Python type → tag + data, fired on dump). Pass it as a keyword argument to any load or dump function.

#### Mapping types

The loader receives a `YamlMapping` (dict-like); the dumper returns a `(tag, dict)` tuple:

```python
import yarutsk

class Point:
    def __init__(self, x, y): self.x, self.y = x, y

schema = yarutsk.Schema()
schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

doc = yarutsk.loads("origin: !point\n  x: 0\n  y: 0\n", schema=schema)
doc["origin"]                          # Point(0, 0)

doc["pos"] = Point(3, 4)               # assigning custom objects works too
yarutsk.dumps(doc, schema=schema)
# origin: !point
#   x: 0
#   y: 0
# pos: !point
#   x: 3
#   y: 4
```

#### Scalar types

The loader receives the raw scalar string; the dumper returns a `(tag, str)` tuple:

```python
class Color:
    def __init__(self, r, g, b): self.r, self.g, self.b = r, g, b

schema = yarutsk.Schema()
schema.add_loader("!color", lambda s: Color(*[int(x) for x in s.split(",")]))
schema.add_dumper(Color, lambda c: ("!color", f"{c.r},{c.g},{c.b}"))

doc = yarutsk.loads("bg: !color 255,0,128\n", schema=schema)
doc["bg"]                              # Color(255, 0, 128)
yarutsk.dumps(doc, schema=schema)      # 'bg: !color 255,0,128\n'
```

#### Overriding built-in tags

Registering a loader for `!!int`, `!!float`, `!!bool`, `!!null`, or `!!str` bypasses the built-in coercion. The callable receives the **raw YAML string** rather than the already-converted Python value:

```python
schema = yarutsk.Schema()
schema.add_loader("!!int", lambda raw: int(raw, 0))  # parses 0xFF, 0o77, etc.

doc = yarutsk.loads("x: !!int 0xFF\n", schema=schema)
doc["x"]                               # 255
```

Multiple dumpers for the same type are checked in registration order; the first `isinstance` match wins.

### YamlScalar

Top-level scalar documents are wrapped in a `YamlScalar` node:

```python
doc = yarutsk.loads("42")
doc.value                              # 42 (Python int)
doc.to_dict()                          # same as .value

# Scalar style
doc = yarutsk.loads("---\n'hello'\n")
doc.style                              # 'single'
doc.style = "double"                   # 'plain'|'single'|'double'|'literal'|'folded'

# YAML tag
doc = yarutsk.loads("!!str 42")
doc.tag                                # '!!str'
doc.tag = None                         # clear tag

# Explicit document markers
doc = yarutsk.loads("---\n42\n...")
doc.explicit_start                     # True
doc.explicit_end                       # True
doc.explicit_start = False
doc.explicit_end   = False
```

### YamlMapping

`YamlMapping` is a subclass of `dict` with insertion-ordered keys. All standard dict operations work directly:

```python
# Standard dict interface (inherited)
doc["key"]                             # get (KeyError if missing)
doc["key"] = value                     # set (preserves position if key exists)
del doc["key"]                         # delete
"key" in doc                           # membership test
len(doc)                               # number of entries
for key in doc: ...                    # iterate over keys in order
doc.keys()                             # KeysView in insertion order
doc.values()                           # ValuesView in insertion order
doc.items()                            # ItemsView of (key, value) pairs
doc.get("key")                         # returns None if missing
doc.get("key", default)                # returns default if missing
doc.pop("key")                         # remove & return (KeyError if missing)
doc.pop("key", default)                # remove & return, or default
doc.setdefault("key", default)         # get or insert default
doc.update(other)                      # merge from dict or YamlMapping
doc == {"a": 1}                        # equality comparison

# Works with any dict-expecting library
isinstance(doc, dict)                  # True
json.dumps(doc)                        # works

# Conversion
doc.to_dict()                          # deep conversion to plain Python dict

# Comments (1-arg = get, 2-arg = set; pass None to clear)
doc.comment_inline("key")             # -> str | None
doc.comment_before("key")             # -> str | None
doc.comment_inline("key", text)
doc.comment_before("key", text)

# YAML tag
doc.tag                                # -> str | None  (e.g. '!!python/object:Foo')
doc.tag = "!!map"

# Explicit document markers
doc.explicit_start                     # bool
doc.explicit_end                       # bool
doc.explicit_start = True
doc.explicit_end   = True

# Node access — returns YamlScalar/YamlMapping/YamlSequence preserving style/tag/anchor
node = doc.node("key")                # KeyError if absent

# Scalar style shortcut (equivalent to: doc.node("key").style = "single")
doc.scalar_style("key", "single")     # 'plain'|'single'|'double'|'literal'|'folded'

# Container style (read from source; also settable to switch block ↔ flow)
doc.style                              # -> 'block' | 'flow'
doc.style = "flow"                     # emit as {key: value, ...}

# Blank lines before a key (1-arg = get, 2-arg = set)
doc.blank_lines_before("key")         # -> int
doc.blank_lines_before("key", 2)      # emit 2 blank lines before this key
doc.trailing_blank_lines              # blank lines after all entries
doc.trailing_blank_lines = 1

# Sorting
doc.sort_keys()                        # alphabetical, in-place
doc.sort_keys(reverse=True)            # reverse alphabetical
doc.sort_keys(key=lambda k: len(k))    # custom key function on key strings
doc.sort_keys(recursive=True)          # also sort all nested mappings
```

### YamlSequence

`YamlSequence` is a subclass of `list`. All standard list operations work directly:

```python
# Standard list interface (inherited)
doc[0]                                 # get by index (negative indices supported)
doc[0] = value                         # set by index
del doc[0]                             # delete by index
value in doc                           # membership test
len(doc)                               # number of items
for item in doc: ...                   # iterate over items
doc.append(value)                      # add to end
doc.insert(idx, value)                 # insert before index
doc.pop()                              # remove & return last item
doc.pop(idx)                           # remove & return item at index
doc.remove(value)                      # remove first occurrence (ValueError if missing)
doc.extend(iterable)                   # append items from list or YamlSequence
doc.index(value)                       # index of first occurrence
doc.count(value)                       # number of occurrences
doc.reverse()                          # reverse in-place
doc == [1, 2, 3]                       # equality comparison

# Works with any list-expecting library
isinstance(doc, list)                  # True
json.dumps(doc)                        # works

# Conversion
doc.to_dict()                          # deep conversion to plain Python list

# Comments (1-arg = get, 2-arg = set; pass None to clear)
doc.comment_inline(idx)               # -> str | None
doc.comment_before(idx)               # -> str | None
doc.comment_inline(idx, text)
doc.comment_before(idx, text)

# YAML tag
doc.tag                                # -> str | None  (e.g. '!!python/tuple')
doc.tag = None

# Explicit document markers
doc.explicit_start                     # bool
doc.explicit_end                       # bool
doc.explicit_start = True
doc.explicit_end   = True

# Scalar style shortcut (equivalent to: doc.node(idx).style = "single")
doc.scalar_style(0, "double")         # 'plain'|'single'|'double'|'literal'|'folded'

# Container style
doc.style                              # -> 'block' | 'flow'
doc.style = "flow"                     # emit as [item, ...]

# Blank lines before an item (1-arg = get, 2-arg = set)
doc.blank_lines_before(0)             # -> int
doc.blank_lines_before(0, 1)          # emit 1 blank line before item 0
doc.trailing_blank_lines              # blank lines after all items
doc.trailing_blank_lines = 0

# Sorting (preserves comment metadata)
doc.sort()                             # natural order, in-place
doc.sort(reverse=True)
doc.sort(key=lambda v: len(v))         # custom key function on item values
```

Sorting preserves all comments — each entry or item carries its inline and before-key comments with it when reordered.

## Comparison

| Feature | yarutsk | ruamel.yaml | PyYAML |
|---|---|---|---|
| Comments preserved | Yes | Yes | No |
| Scalar styles preserved | Yes | Partial | No |
| Insertion order preserved | Yes | Yes | No |
| Blank lines preserved | Yes | Partial | No |
| Tags preserved | Yes | Yes | No |
| Anchors/aliases preserved | Yes | Yes | No |
| `dict` / `list` subclasses | Yes | No | No |
| Rust speed | Yes | No | No |
| Python 3.12+ required | Yes | No | No |

yarutsk focuses on **round-trip fidelity**: if you need to edit a config file and emit it back without touching the formatting, it keeps every comment, blank line, and scalar quote style exactly as written. ruamel.yaml offers similar round-trip support in pure Python. PyYAML is faster for load-only workloads where output formatting doesn't matter.

## Error handling

```python
import yarutsk

# Malformed YAML → ValueError
try:
    yarutsk.loads("key: [unclosed")
except ValueError as e:
    print(e)   # scan error: ...

# Unsupported Python type → TypeError
try:
    yarutsk.dumps({"key": {1, 2}})  # sets are not supported
except TypeError as e:
    print(e)

# Missing key → KeyError  (standard dict behaviour)
doc = yarutsk.loads("a: 1")
doc["missing"]               # KeyError: 'missing'
doc.comment_inline("missing")  # KeyError: 'missing'

# Bad index → IndexError  (standard list behaviour)
seq = yarutsk.loads("- 1\n- 2")
seq[99]                      # IndexError
```

## Limitations

- **Integer range**: integers are stored as 64-bit signed (`i64`). Values outside `[-9223372036854775808, 9223372036854775807]` are loaded as strings.
- **Underscore separators**: `1_000` is not parsed as an integer — it is loaded as the string `"1_000"` (and round-tripped faithfully as such).
- **Blank line cap**: at most 255 blank lines before any entry are tracked; runs longer than that are clamped to 255 on load.
- **Block only by default**: the emitter writes block-style YAML. Flow containers (`{...}` / `[...]`) from the source are preserved if they were already flow-style, but there is no option to force everything to flow on dump.
- **Streaming**: the entire document must fit in memory; incremental/streaming parse is not supported.
- **YAML version**: the scanner implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`). Most YAML 1.2-only documents work, but a small number of spec edge cases differ — see `tests/test_yaml_suite.py` for the `xfail` markers.

## Benchmarks

Compare load, dump, and round-trip performance against PyYAML and ruamel.yaml across small, medium, and large inputs:

```bash
uv sync --group benchmark
uv run maturin develop --release
uv run pytest benchmarks/ -v --benchmark-sort=name
```

## Running tests

You need Rust 1.85+ and Python 3.12+ with [uv](https://github.com/astral-sh/uv). Python 3.12 is the minimum — `YamlSequence` subclasses `list`, which requires PyO3's `extends = PyList` support introduced in Python 3.12.

```bash
# 1. Clone with the yaml-test-suite submodule
git clone --recurse-submodules https://github.com/theyugin/yarutsk
cd yarutsk

# 2. Create a virtual environment and install dev dependencies
uv sync --group dev

# 3. Build the extension in dev (debug) mode
uv run maturin develop

# 4. Run the suites
uv run pytest tests/ --ignore=tests/test_yaml_suite.py -v  # core library tests
uv run pytest tests/test_yaml_suite.py -q                   # yaml-test-suite compliance
```

`test_yaml_suite.py` requires the `yaml-test-suite` submodule. Tests that fail due to known YAML normalisation differences are marked `xfail` and do not count as failures.

## Internals

The scanner and parser are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) (MIT licensed) with one targeted modification: the comment-skipping loop in the scanner now emits `Comment` tokens instead of discarding them. Everything else — block/flow parsing, scalar type coercion, multi-document support — comes from yaml-rust2 unchanged. The builder layer wires those tokens to the data model, and a hand-written block-style emitter serialises it back out.

`YamlMapping` and `YamlSequence` are PyO3 pyclasses that extend Python's built-in `dict` and `list` types. A Rust `inner` field stores the full YAML data model (including comments); the parent dict/list is kept in sync on every mutation so that all standard Python operations work transparently.

## Disclaimer

This library was created with [Claude Code](https://claude.ai/code) (Anthropic). The design, implementation, tests, and this README were written by Claude under human direction.
