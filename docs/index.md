---
title: yarutsk
---

# yarutsk

!!! warning "AI-authored"
    This library — design, implementation, tests, and documentation — was written by [Claude Code](https://claude.ai/code) (Anthropic) under human direction.

A Python YAML library that round-trips documents while preserving **comments**, **insertion order**, **scalar styles**, **tags**, **anchors and aliases**, **blank lines**, and **explicit document markers**.

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

`YamlMapping` and `YamlSequence` implement the dict/list protocols (subscript, iteration, `len`, `in`, etc.) but are not `dict`/`list` subclasses — `isinstance(m, dict)` is `False`. Call `m.to_python()` (recursive) to get a plain `dict`/`list` for libraries that need it (including `json.dumps`).

## What's preserved

- **Scalar styles** — plain, `'single-quoted'`, `"double-quoted"`, literal block `|`, folded block `>`
- **Non-canonical scalars** — `yes`/`no`/`on`/`off`, `~`, `Null`, `True`/`False`, `0xFF`, `0o77` — reproduced as written, not re-canonicalised
- **YAML tags** — `!!str`, `!!python/tuple`, and any custom tag are emitted back verbatim
- **Anchors and aliases** — `&name` on the anchor node and `*name` for references; the Python layer returns the resolved value transparently
- **Blank lines** between mapping entries and sequence items
- **Explicit document markers** — `---` and `...`

## Where to go next

- [Installation](installation.md) — `pip install yarutsk` and from-source builds
- [API reference](api.md) — the full public surface on a single page
- [Library integrations](integrations.md) — pydantic, msgspec, cattrs
- [Round-trip fidelity](roundtrip.md) — exactly what reproduces byte-for-byte
- [Error handling](errors.md) — the exception hierarchy
- [Thread safety](threading.md) — what's safe to share across threads

Source: <https://github.com/theyugin/yarutsk>
