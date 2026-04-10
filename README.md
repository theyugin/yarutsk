# yarutsk

A Python YAML library that round-trips documents while preserving **comments** and **insertion order**.

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
doc.dump(out)
print(out.getvalue())
# # database config
# host: localhost  # primary
# port: 5433
```

## Installation

Built with [Maturin](https://github.com/PyO3/maturin). From the repo root:

```bash
pip install maturin
maturin develop
```

## API

```python
# Load
doc  = yarutsk.load(stream)            # first document
docs = yarutsk.load_all(stream)        # all documents

# Dump
yarutsk.dump(doc, stream)
yarutsk.dump_all(docs, stream)

# Mapping access
doc["key"]                             # get (returns native Python type or PyYamlDocument)
doc["key"] = value                     # set
"key" in doc                           # contains
doc.keys()                             # insertion-ordered list
doc.to_dict()                          # deep conversion to plain Python dict/list

# Sequence access
doc[0]                                 # get by index (negative indices supported)
doc[0] = value                         # set by index
len(doc)                               # item count

# Comments
doc.get_comment_inline("key")          # -> str | None
doc.get_comment_before("key")          # -> str | None
doc.set_comment_inline("key", text)
doc.set_comment_before("key", text)

# Sorting — mappings
doc.sort_keys()                        # alphabetical, in-place
doc.sort_keys(reverse=True)            # reverse alphabetical
doc.sort_keys(key=lambda k: len(k))    # custom key function on key strings
doc.sort_keys(recursive=True)          # also sort all nested mappings

# Sorting — sequences
doc.sort()                             # natural order, in-place
doc.sort(reverse=True)
doc.sort(key=lambda v: len(v))         # custom key function on item values
```

Sorting preserves all comments — each entry or item carries its inline and before-key comments with it when reordered.

## Running tests

The repo contains three test suites. You need Rust (nightly) and Python 3.9+ with [uv](https://github.com/astral-sh/uv).

```bash
# 1. Clone with the yaml-test-suite submodule
git clone --recurse-submodules https://github.com/anomalyco/yarutsk
cd yarutsk

# 2. Create a virtual environment and install dev dependencies
#    (includes pytest, pyyaml, ruamel-yaml, pytest-benchmark)
uv sync --group dev

# 3. Build the extension in dev (debug) mode
uv run maturin develop

# 4. Run the suites
uv run pytest tests/test_yarutsk.py -v          # core library tests
uv run pytest tests/test_sort.py -v             # key-ordering tests
uv run pytest tests/test_yaml_suite.py -q       # yaml-test-suite compliance
```

`test_yaml_suite.py` requires the `yaml-test-suite` submodule and PyYAML (both available after the steps above). Round-trip tests that fail due to YAML normalisation (flow→block, anchors, folded scalars) are marked `xfail` and do not count as failures.

## Benchmarks

Benchmarks compare yarutsk against PyYAML and ruamel.yaml using [pytest-benchmark](https://pytest-benchmark.readthedocs.io/):

```bash
uv run pytest benchmarks/ -v --benchmark-min-rounds=10
```

ruamel.yaml is the closest analogue to yarutsk (it also preserves comments), so it is the primary point of comparison.

## Internals

The scanner and parser are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) (MIT licensed) with one targeted modification: the comment-skipping loop in the scanner now emits `Comment` tokens instead of discarding them. Everything else — block/flow parsing, scalar type coercion, multi-document support — comes from yaml-rust2 unchanged. The builder layer wires those tokens to the data model, and a hand-written block-style emitter serialises it back out.

## Disclaimer

This library was created with [Claude Code](https://claude.ai/code) (Anthropic). The design, implementation, tests, and this README were written by Claude under human direction.
