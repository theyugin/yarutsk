# yarutsk

[![PyPI](https://img.shields.io/pypi/v/yarutsk)](https://pypi.org/project/yarutsk/)
[![Python 3.12+](https://img.shields.io/pypi/pyversions/yarutsk)](https://pypi.org/project/yarutsk/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> **⚠ AI-authored.** This library — design, implementation, tests, and documentation — was written by [Claude Code](https://claude.ai/code) (Anthropic) under human direction.

A Python YAML library that round-trips documents while preserving **comments**, **insertion order**, **scalar styles**, **tags**, **anchors and aliases**, **blank lines**, and **explicit document markers**.

Full documentation: **<https://theyugin.github.io/yarutsk/>**

## Quick start

```bash
pip install yarutsk
```

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

`YamlMapping` and `YamlSequence` implement the dict/list protocols (subscript, iteration, `len`, `in`, etc.) but are not `dict`/`list` subclasses. Call `doc.to_python()` (recursive) for a plain `dict`/`list` — needed for `json.dumps`, pydantic, msgspec, cattrs, and other libraries that type-check input strictly.

Python 3.12+ required. Pre-built wheels for Linux / macOS / Windows on x86_64 and aarch64.

## What's preserved

- **Scalar styles** — plain, `'single'`, `"double"`, literal `|`, folded `>`
- **Non-canonical scalars** — `yes`/`no`/`on`/`off`, `~`, `0xFF`, `0o77` reproduced as written
- **YAML tags** — `!!str`, `!!binary`, `!!timestamp`, and any custom tag
- **Anchors and aliases** — `&name` / `*name` round-trip intact
- **Blank lines** between entries and **explicit document markers** (`---`, `...`)

## Comparison

| Feature | yarutsk | ruamel.yaml | PyYAML |
|---|---|---|---|
| Comments preserved | Yes | Yes | No |
| Scalar styles preserved | Yes | Partial | No |
| Insertion order preserved | Yes | Yes | No |
| Blank lines preserved | Yes | Partial | No |
| Tags preserved | Yes | Yes | No |
| Anchors/aliases preserved | Yes | Yes | No |
| Rust speed | Yes | No | No |

yarutsk focuses on **round-trip fidelity**: edit a config file and emit it back without touching the formatting. ruamel.yaml offers similar fidelity in pure Python. PyYAML is faster for load-only workloads where output formatting doesn't matter.

## Documentation

Everything — the full API, type conversions, Schema and library integrations (pydantic / msgspec / cattrs), error handling, thread safety, and limitations — lives at **<https://theyugin.github.io/yarutsk/>**.

Direct links:

- [Installation](https://theyugin.github.io/yarutsk/installation/)
- [API reference](https://theyugin.github.io/yarutsk/api/)
- [Library integrations](https://theyugin.github.io/yarutsk/integrations/)
- [Round-trip fidelity](https://theyugin.github.io/yarutsk/roundtrip/)
- [Error handling](https://theyugin.github.io/yarutsk/errors/)
- [Thread safety](https://theyugin.github.io/yarutsk/threading/)
- [Limitations](https://theyugin.github.io/yarutsk/limitations/)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)

## Benchmarks

Compare load, dump, and round-trip performance against PyYAML and ruamel.yaml:

```bash
make bench
```

## License

MIT. The scanner and parser are vendored from [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) (also MIT) with one targeted modification: the comment-skipping loop now emits `Comment` tokens instead of discarding them.
