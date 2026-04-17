# Round-trip fidelity

yarutsk reproduces the source text exactly for everything it understands. A `loads` followed by `dumps` gives back the original string byte-for-byte in the common case:

```python
import yarutsk

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

## Specifically preserved

- **Scalar styles** — plain, `'single-quoted'`, `"double-quoted"`, literal block `|`, folded block `>`
- **Non-canonical scalars** — `yes`/`no`/`on`/`off`, `~`, `Null`, `True`/`False`, `0xFF`, `0o77` — reproduced as written, not re-canonicalised to `true`/`false`/`null`/`255`
- **YAML tags** — `!!str`, `!!python/tuple`, and any custom tag are emitted back verbatim
- **Anchors and aliases** — `&name` on the anchor node and `*name` for references are preserved; the Python layer returns the resolved value transparently
- **Blank lines** between mapping entries and sequence items
- **Explicit document markers** — `---` and `...`
- **Document-level directives** — `%YAML 1.1`, `%TAG`

## What's *not* round-tripped

The cases where output diverges from input are documented on the [Limitations](limitations.md) page — primarily around 64-bit integer range, underscore-separated numeric literals, and the 255-blank-line cap.

The test suite in [`tests/test_roundtrip.py`](https://github.com/theyugin/yarutsk/blob/main/tests/test_roundtrip.py) exercises the end-to-end load → dump path across all preserved features, including stream-based IO.
