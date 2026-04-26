"""
Type-checking tests for yarutsk — run with mypy, not pytest.

    uv run mypy

Each function exercises a part of the public API with explicit annotations.
mypy will error if the inferred types contradict the declared ones.
"""

from __future__ import annotations

import io
from collections.abc import Callable
from typing import Any

import yarutsk
from yarutsk import Schema, YamlIter, YamlMapping, YamlScalar, YamlSequence


def check_schema_construction() -> None:
    schema: Schema = yarutsk.Schema()
    _ = schema


def check_schema_add_loader() -> None:
    schema = yarutsk.Schema()
    schema.add_loader("!point", lambda d: d)
    schema.add_loader("!!int", lambda raw: int(str(raw), 0))


def check_schema_add_dumper() -> None:
    schema = yarutsk.Schema()
    schema.add_dumper(int, lambda n: ("!!int", str(n)))
    schema.add_dumper(list, lambda lst: ("!!seq", lst))


def check_load_from_stream() -> None:
    doc: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.load(io.StringIO("key: val"))
    doc2: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.load(io.BytesIO(b"key: val"))
    _ = doc, doc2


def check_loads_from_string() -> None:
    doc: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.loads("key: val")
    empty: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.loads("")
    _ = doc, empty


def check_load_all() -> None:
    docs: list[YamlMapping | YamlSequence | YamlScalar] = yarutsk.load_all(
        io.StringIO("---\na: 1\n---\nb: 2")
    )
    _ = docs


def check_loads_all() -> None:
    docs: list[YamlMapping | YamlSequence | YamlScalar] = yarutsk.loads_all("---\na: 1\n---\nb: 2")
    _ = docs


def check_load_with_schema() -> None:
    schema = yarutsk.Schema()
    doc: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.load(
        io.StringIO("key: val"), schema=schema
    )
    doc2: YamlMapping | YamlSequence | YamlScalar | None = yarutsk.loads("key: val", schema=schema)
    _ = doc, doc2


def check_load_all_with_schema() -> None:
    schema = yarutsk.Schema()
    docs: list[YamlMapping | YamlSequence | YamlScalar] = yarutsk.load_all(
        io.StringIO("a: 1"), schema=schema
    )
    docs2: list[YamlMapping | YamlSequence | YamlScalar] = yarutsk.loads_all("a: 1", schema=schema)
    _ = docs, docs2


def check_dump_to_stream(doc: YamlMapping | YamlSequence | YamlScalar) -> None:
    yarutsk.dump(doc, io.StringIO())


def check_dumps_to_string(doc: YamlMapping | YamlSequence | YamlScalar) -> None:
    text: str = yarutsk.dumps(doc)
    _ = text


def check_dump_all_to_stream(
    docs: list[YamlMapping | YamlSequence | YamlScalar],
) -> None:
    yarutsk.dump_all(docs, io.StringIO())


def check_dumps_all_to_string(
    docs: list[YamlMapping | YamlSequence | YamlScalar],
) -> None:
    text: str = yarutsk.dumps_all(docs)
    _ = text


def check_dump_with_schema(doc: YamlMapping | YamlSequence | YamlScalar) -> None:
    schema = yarutsk.Schema()
    yarutsk.dump(doc, io.StringIO(), schema=schema)
    text: str = yarutsk.dumps(doc, schema=schema)
    _ = text


def check_dump_all_with_schema(
    docs: list[YamlMapping | YamlSequence | YamlScalar],
) -> None:
    schema = yarutsk.Schema()
    yarutsk.dump_all(docs, io.StringIO(), schema=schema)
    text: str = yarutsk.dumps_all(docs, schema=schema)
    _ = text


def check_none_narrowing() -> str:
    doc = yarutsk.loads("key: val")
    if doc is None:
        return ""
    # After the None check mypy knows doc: YamlMapping | YamlSequence | YamlScalar
    return yarutsk.dumps(doc)


def check_scalar_value(s: YamlScalar) -> None:
    import datetime

    v: int | float | bool | str | bytes | datetime.datetime | datetime.date | None = s.value
    d: int | float | bool | str | bytes | datetime.datetime | datetime.date | None = s.to_python()
    eq: bool = s == 42
    _ = v, d, eq


def check_scalar_style(s: YamlScalar) -> None:
    style: str = s.style
    s.style = "single"
    s.style = "double"
    _ = style


def check_scalar_document_markers(s: YamlScalar) -> None:
    start: bool = s.explicit_start
    end: bool = s.explicit_end
    s.explicit_start = True
    s.explicit_end = False
    _ = start, end


def check_scalar_tag(s: YamlScalar) -> None:
    tag: str | None = s.tag
    s.tag = "!!str"
    s.tag = None
    _ = tag


def check_scalar_yaml_version(s: YamlScalar) -> None:
    ver: str | None = s.yaml_version
    s.yaml_version = "1.2"
    s.yaml_version = None
    _ = ver


def check_scalar_tag_directives(s: YamlScalar) -> None:
    dirs: list[tuple[str, str]] = s.tag_directives
    s.tag_directives = [("!", "!foo!"), ("!bar!", "tag:example.com,2024:")]
    _ = dirs


def check_mapping_access(m: YamlMapping) -> None:
    val = m["key"]  # YamlMapping | YamlSequence | int | float | bool | str | None
    m["key"] = "new"
    m["key"] = 42
    contained: bool = "key" in m
    length: int = len(m)
    keys: list[str] = m.keys()
    _ = val, contained, length, keys


def check_mapping_to_python(m: YamlMapping) -> None:
    d: dict[str, Any] = m.to_python()
    _ = d


def check_mapping_dict_compat(m: YamlMapping) -> None:
    del m["key"]
    vals = m.values()
    pairs = m.items()
    got = m.get("key")
    got2 = m.get("key", "fallback")
    popped = m.pop("key")
    popped2 = m.pop("key", 0)
    m.update({"a": 1})
    sd = m.setdefault("key", "default")
    eq: bool = m == {"a": 1}
    for k in m:
        _k: str = k
    _ = vals, pairs, got, got2, popped, popped2, sd, eq


def check_mapping_comments(m: YamlMapping) -> None:
    inline: str | None = m.node("key").comment_inline
    before: str | None = m.node("key").comment_before
    m.node("key").comment_inline = "note"
    m.node("key").comment_before = "header"
    _ = inline, before


def check_mapping_sort(m: YamlMapping) -> None:
    m.sort_keys()
    m.sort_keys(reverse=True)
    key_fn: Callable[[str], int] = len
    m.sort_keys(key=key_fn, reverse=True, recursive=True)


def check_mapping_document_markers(m: YamlMapping) -> None:
    start: bool = m.explicit_start
    end: bool = m.explicit_end
    m.explicit_start = True
    m.explicit_end = False
    _ = start, end


def check_mapping_tag(m: YamlMapping) -> None:
    tag: str | None = m.tag
    m.tag = "!!map"
    m.tag = None
    _ = tag


def check_mapping_yaml_version(m: YamlMapping) -> None:
    ver: str | None = m.yaml_version
    m.yaml_version = "1.2"
    m.yaml_version = None
    _ = ver


def check_mapping_tag_directives(m: YamlMapping) -> None:
    dirs: list[tuple[str, str]] = m.tag_directives
    m.tag_directives = [("!", "!foo!")]
    _ = dirs


def check_mapping_node(m: YamlMapping) -> None:
    node: YamlMapping | YamlSequence | YamlScalar = m.node("key")
    _ = node


def check_mapping_scalar_style(m: YamlMapping) -> None:
    m.node("key").style = "single"
    m.node("key").style = "double"
    m.node("key").style = "plain"


def check_sequence_access(s: YamlSequence) -> None:
    item = s[0]
    s[0] = "replaced"
    s[-1] = 99
    length: int = len(s)
    _ = item, length


def check_sequence_to_python(s: YamlSequence) -> None:
    d: list[Any] = s.to_python()
    _ = d


def check_sequence_list_compat(s: YamlSequence) -> None:
    del s[0]
    contained: bool = "x" in s
    eq: bool = s == [1, 2, 3]
    s.append("x")
    s.insert(0, "y")
    popped = s.pop()
    popped2 = s.pop(0)
    s.remove("x")
    s.extend(["a", "b"])
    idx: int = s.index("x")
    idx2: int = s.index("x", 1, 5)
    n: int = s.count("x")
    s.reverse()
    for item in s:
        _ = item
    _ = contained, eq, popped, popped2, idx, idx2, n


def check_sequence_comments(s: YamlSequence) -> None:
    inline: str | None = s.node(0).comment_inline
    before: str | None = s.node(-1).comment_before
    s.node(0).comment_inline = "note"
    s.node(0).comment_before = "header"
    _ = inline, before


def check_sequence_sort(s: YamlSequence) -> None:
    s.sort()
    s.sort(reverse=True)

    def key_fn(v: Any) -> int:
        return len(str(v))

    s.sort(key=key_fn, reverse=True)


def check_sequence_document_markers(s: YamlSequence) -> None:
    start: bool = s.explicit_start
    end: bool = s.explicit_end
    s.explicit_start = True
    s.explicit_end = False
    _ = start, end


def check_sequence_tag(s: YamlSequence) -> None:
    tag: str | None = s.tag
    s.tag = "!!seq"
    s.tag = None
    _ = tag


def check_sequence_yaml_version(s: YamlSequence) -> None:
    ver: str | None = s.yaml_version
    s.yaml_version = "1.2"
    s.yaml_version = None
    _ = ver


def check_sequence_tag_directives(s: YamlSequence) -> None:
    dirs: list[tuple[str, str]] = s.tag_directives
    s.tag_directives = [("!", "!foo!")]
    _ = dirs


def check_public_type_aliases() -> None:
    style: yarutsk.ScalarStyle = "double"
    cstyle: yarutsk.ContainerStyle = "flow"
    _node: yarutsk.YamlNode
    _ = style, cstyle


def check_mapping_constructor() -> None:
    m: YamlMapping = YamlMapping({"a": 1, "b": {"c": 2}})
    m2: YamlMapping = YamlMapping({"x": [1, 2, 3]})
    _ = m, m2


def check_mapping_nodes(m: YamlMapping) -> None:
    pairs: list[tuple[str, YamlMapping | YamlSequence | YamlScalar]] = m.nodes()
    _ = pairs


def check_sequence_constructor() -> None:
    s: YamlSequence = YamlSequence([1, 2, {"x": 3}])
    s2: YamlSequence = YamlSequence(["a", "b"])
    _ = s, s2


def check_mapping_copy(m: YamlMapping) -> None:
    import copy

    shallow: YamlMapping = m.__copy__()
    deep: YamlMapping = m.__deepcopy__({})
    copy_shallow: YamlMapping = copy.copy(m)
    copy_deep: YamlMapping = copy.deepcopy(m)
    _ = shallow, deep, copy_shallow, copy_deep


def check_sequence_copy(s: YamlSequence) -> None:
    import copy

    shallow: YamlSequence = s.__copy__()
    deep: YamlSequence = s.__deepcopy__({})
    copy_shallow: YamlSequence = copy.copy(s)
    copy_deep: YamlSequence = copy.deepcopy(s)
    _ = shallow, deep, copy_shallow, copy_deep


def check_iter_loads_all() -> None:
    it: YamlIter = yarutsk.iter_loads_all("a: 1\n---\nb: 2\n")
    _ = it


def check_iter_load_all() -> None:
    stream = io.StringIO("a: 1\n---\nb: 2\n")
    it: YamlIter = yarutsk.iter_load_all(stream)
    _ = it


def check_yaml_iter_protocol(it: YamlIter) -> None:
    same: YamlIter = iter(it)
    doc: YamlMapping | YamlSequence | YamlScalar = next(it)
    _ = same, doc


def check_yaml_iter_is_iterator() -> None:
    import itertools
    from collections.abc import Iterator

    it = yarutsk.iter_loads_all("a: 1\n---\nb: 2\n")
    as_iter: Iterator[YamlMapping | YamlSequence | YamlScalar] = it
    head: list[YamlMapping | YamlSequence | YamlScalar] = list(itertools.islice(as_iter, 1))
    _ = head


def check_sequence_slice(s: YamlSequence) -> None:
    item: Any = s[0]
    sub: list[Any] = s[1:3]
    _ = item, sub


def check_mapping_get_pop_default_overloads(m: YamlMapping) -> None:
    a: Any = m.get("k")
    b: Any = m.get("k", "fallback")
    p: Any = m.pop("k")
    p2: Any = m.pop("k", 0)
    sd: Any = m.setdefault("k")
    sd2: Any = m.setdefault("k", "init")
    _ = a, b, p, p2, sd, sd2


def check_mapping_constructor_with_schema() -> None:
    schema = yarutsk.Schema()
    m: YamlMapping = YamlMapping({"a": 1}, schema=schema)
    _ = m


def check_sequence_constructor_with_schema() -> None:
    schema = yarutsk.Schema()
    s: YamlSequence = YamlSequence([1, 2], schema=schema)
    _ = s


def check_dumps_plain_dict() -> None:
    text: str = yarutsk.dumps({"a": 1, "b": [2, 3]})
    _ = text


def check_dumps_plain_list() -> None:
    text: str = yarutsk.dumps([1, 2, 3])
    _ = text


def check_dumps_plain_scalar() -> None:
    text: str = yarutsk.dumps("hello")
    _ = text


def check_dump_all_mixed() -> None:
    text: str = yarutsk.dumps_all([{"a": 1}, [2, 3]])
    _ = text
