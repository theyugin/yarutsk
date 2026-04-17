# Library integrations

The [`Schema`](api.md#schema-custom-types) API is enough to plug yarutsk into any of the common Python serialization libraries — each integration is a two-line registration. Two patterns are useful:

- **Tag-based** — preserves the surrounding YAML formatting and comments. Individual tagged values are swapped for typed objects; the document around them stays a yarutsk node.
- **Whole-document** — the entire document is the typed object. Convenient but loses comments because the model layer has no place to carry them.

The examples below all use this `Endpoint` shape:

```yaml
# tag-based input — endpoints are typed inline
endpoints:
  - !endpoint
    host: 127.0.0.1
    port: 8080
  - !endpoint
    host: 0.0.0.0
    port: 8443
```

## Tag-based registration

The non-obvious bit per library: **pydantic accepts `YamlMapping` directly** (it's a `dict` subclass), but **msgspec and cattrs both need `.to_python()`** to flatten the yarutsk wrappers into a plain `dict`.

### pydantic v2

```python
import yarutsk
from pydantic import BaseModel

class Endpoint(BaseModel):
    host: str
    port: int

schema = yarutsk.Schema()
schema.add_loader("!endpoint", Endpoint.model_validate)
schema.add_dumper(Endpoint, lambda e: ("!endpoint", e.model_dump()))
```

### msgspec

```python
import msgspec
import yarutsk

class Endpoint(msgspec.Struct):
    host: str
    port: int

schema = yarutsk.Schema()
schema.add_loader("!endpoint", lambda d: msgspec.convert(d.to_python(), Endpoint))
schema.add_dumper(Endpoint, lambda e: ("!endpoint", msgspec.to_builtins(e)))
```

### attrs + cattrs

```python
import attrs
import cattrs
import yarutsk

@attrs.define
class Endpoint:
    host: str
    port: int

schema = yarutsk.Schema()
schema.add_loader("!endpoint", lambda d: cattrs.structure(d.to_python(), Endpoint))
schema.add_dumper(Endpoint, lambda e: ("!endpoint", cattrs.unstructure(e)))
```

With any of the three, `yarutsk.loads(text, schema=schema)` returns a document where `doc["endpoints"][0]` is an `Endpoint` instance, and `yarutsk.dumps(doc, schema=schema)` emits the original tags back out. Comments and formatting around the tagged values are preserved because the surrounding mapping/sequence is still a yarutsk node.

## Whole-document validation

If the entire document is the typed object, validate the loaded yarutsk node directly. This is convenient but **discards comments and styles** — once data crosses into a model layer it loses its yarutsk metadata, and dumping the model back produces clean default formatting.

```python
# pydantic — for mapping-rooted documents
config = Config.model_validate(yarutsk.loads(text))
print(yarutsk.dumps(config.model_dump()))

# pydantic — for list-rooted documents, BaseModel doesn't apply.
# Use TypeAdapter:
from pydantic import TypeAdapter
adapter = TypeAdapter(list[Endpoint])
endpoints = adapter.validate_python(yarutsk.loads(text))
print(yarutsk.dumps(adapter.dump_python(endpoints)))
```

```python
# msgspec — generic types work directly for both root shapes
config    = msgspec.convert(yarutsk.loads(text).to_python(), Config)
endpoints = msgspec.convert(yarutsk.loads(text).to_python(), list[Endpoint])
```

```python
# cattrs — same; generic types work directly for both root shapes
config    = cattrs.structure(yarutsk.loads(text).to_python(), Config)
endpoints = cattrs.structure(yarutsk.loads(text).to_python(), list[Endpoint])
```

When comment and formatting fidelity matter, prefer the tag-based pattern: it keeps the surrounding document as a yarutsk node and only swaps individual tagged values for typed objects.

For the underlying mechanism, see [Schema — custom types](api.md#schema-custom-types).
