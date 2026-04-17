# Error handling

yarutsk defines a small exception hierarchy rooted at `YarutskError`:

```
Exception
└── YarutskError         # base for all library errors
    ├── ParseError       # malformed YAML input
    ├── LoaderError      # schema loader callable raised
    └── DumperError      # schema dumper raised or returned wrong type
```

## ParseError

Raised on malformed YAML input. The message includes the error description and source position (line, column).

```python
import yarutsk

try:
    yarutsk.loads("key: [unclosed")
except yarutsk.ParseError as e:
    print(e)   # while parsing a flow sequence, expected ',' or ']' at byte ...

# Catch any library error with the base class
try:
    yarutsk.loads("key: [unclosed")
except yarutsk.YarutskError as e:
    print(e)
```

## LoaderError

A Schema loader callable raised. The message includes the tag name so you know which loader misbehaved.

```python
schema = yarutsk.Schema()
schema.add_loader("!color", lambda s: s.split(","))  # expects str

try:
    yarutsk.loads("bg: !color\n  r: 255\n  g: 0\n  b: 128\n", schema=schema)
except yarutsk.LoaderError as e:
    print(e)   # Schema loader for tag '!color' raised: AttributeError: ...
```

## DumperError

A Schema dumper raised or returned the wrong shape. The message includes the Python type name.

```python
schema = yarutsk.Schema()
schema.add_dumper(MyType, lambda x: "not-a-tuple")  # must return (tag, data)

try:
    yarutsk.dumps(doc, schema=schema)
except yarutsk.DumperError as e:
    print(e)   # Schema dumper for MyType must return (tag, data) tuple: ...
```

## Standard Python errors

```python
# Unsupported Python type (no schema dumper registered) → RuntimeError
yarutsk.dumps(object())                # RuntimeError: Cannot convert ...

# Missing key → KeyError (standard dict behaviour)
doc = yarutsk.loads("a: 1")
doc["missing"]                         # KeyError: 'missing'
doc.comment_inline("missing")          # KeyError: 'missing'

# Out-of-range index → IndexError (standard list behaviour)
seq = yarutsk.loads("- 1\n- 2")
seq[99]                                # IndexError
```
