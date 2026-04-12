# yarutsk

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

Built with [Maturin](https://github.com/PyO3/maturin). From the repo root:

```bash
pip install maturin
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
```

`load` / `loads` return a `YamlMapping`, `YamlSequence`, `YamlScalar`, or `None` (for empty input). Nested nodes are also `YamlMapping` or `YamlSequence`; scalar leaves are returned as native Python primitives (`int`, `float`, `bool`, `str`, or `None`).

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
doc.get_tag()                          # '!!str'
doc.set_tag(None)                      # clear tag

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

# Comments
doc.get_comment_inline("key")          # -> str | None
doc.get_comment_before("key")          # -> str | None
doc.set_comment_inline("key", text)
doc.set_comment_before("key", text)

# YAML tag
doc.get_tag()                          # -> str | None  (e.g. '!!python/object:Foo')
doc.set_tag("!!map")

# Explicit document markers
doc.explicit_start                     # bool
doc.explicit_end                       # bool
doc.explicit_start = True
doc.explicit_end   = True

# Node access — returns YamlScalar/YamlMapping/YamlSequence preserving style/tag/anchor
node = doc.get_node("key")            # KeyError if absent

# Scalar style shortcut (equivalent to: doc.get_node("key").style = "single")
doc.set_scalar_style("key", "single") # 'plain'|'single'|'double'|'literal'|'folded'

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

# Comments (addressed by integer index)
doc.get_comment_inline(idx)            # -> str | None
doc.get_comment_before(idx)            # -> str | None
doc.set_comment_inline(idx, text)
doc.set_comment_before(idx, text)

# YAML tag
doc.get_tag()                          # -> str | None  (e.g. '!!python/tuple')
doc.set_tag(None)

# Explicit document markers
doc.explicit_start                     # bool
doc.explicit_end                       # bool
doc.explicit_start = True
doc.explicit_end   = True

# Sorting (preserves comment metadata)
doc.sort()                             # natural order, in-place
doc.sort(reverse=True)
doc.sort(key=lambda v: len(v))         # custom key function on item values
```

Sorting preserves all comments — each entry or item carries its inline and before-key comments with it when reordered.

## Running tests

You need Rust (nightly) and Python 3.12+ with [uv](https://github.com/astral-sh/uv). Python 3.12 is the minimum — `YamlSequence` subclasses `list`, which requires PyO3's `extends = PyList` support introduced in Python 3.12.

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
