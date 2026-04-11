"""
Type-checking tests for yarutsk — run with mypy, not pytest.

    uv run mypy

Each function exercises a part of the public API with explicit annotations.
mypy will error if the inferred types contradict the declared ones.
"""

from __future__ import annotations

import io
from typing import Any, Callable
from collections.abc import KeysView

import yarutsk
from yarutsk import YamlMapping, YamlScalar, YamlSequence


# ── load / loads ──────────────────────────────────────────────────────────────

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
    docs: list[YamlMapping | YamlSequence | YamlScalar] = yarutsk.loads_all(
        "---\na: 1\n---\nb: 2"
    )
    _ = docs


# ── dump / dumps ──────────────────────────────────────────────────────────────

def check_dump_to_stream(doc: YamlMapping | YamlSequence | YamlScalar) -> None:
    yarutsk.dump(doc, io.StringIO())


def check_dumps_to_string(doc: YamlMapping | YamlSequence | YamlScalar) -> None:
    text: str = yarutsk.dumps(doc)
    _ = text


def check_dump_all_to_stream(docs: list[YamlMapping | YamlSequence | YamlScalar]) -> None:
    yarutsk.dump_all(docs, io.StringIO())


def check_dumps_all_to_string(docs: list[YamlMapping | YamlSequence | YamlScalar]) -> None:
    text: str = yarutsk.dumps_all(docs)
    _ = text


# ── None-guard required before using the document ────────────────────────────

def check_none_narrowing() -> str:
    doc = yarutsk.loads("key: val")
    if doc is None:
        return ""
    # After the None check mypy knows doc: YamlMapping | YamlSequence | YamlScalar
    return yarutsk.dumps(doc)


def check_scalar(s: YamlScalar) -> None:
    v: int | float | bool | str | None = s.value
    d: int | float | bool | str | None = s.to_dict()
    eq: bool = s == 42
    _ = v, d, eq


# ── YamlMapping interface ─────────────────────────────────────────────────────

def check_mapping_access(m: YamlMapping) -> None:
    val = m["key"]             # YamlMapping | YamlSequence | int | float | bool | str | None
    m["key"] = "new"
    m["key"] = 42
    contained: bool = "key" in m
    length: int = len(m)
    keys: KeysView[str] = m.keys()
    _ = val, contained, length, keys


def check_mapping_to_dict(m: YamlMapping) -> None:
    d: Any = m.to_dict()
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
    inline: str | None = m.get_comment_inline("key")
    before: str | None = m.get_comment_before("key")
    m.set_comment_inline("key", "note")
    m.set_comment_before("key", "header")
    _ = inline, before


def check_mapping_sort(m: YamlMapping) -> None:
    m.sort_keys()
    m.sort_keys(reverse=True)
    key_fn: Callable[[str], int] = len
    m.sort_keys(key=key_fn, reverse=True, recursive=True)


# ── YamlSequence interface ────────────────────────────────────────────────────

def check_sequence_access(s: YamlSequence) -> None:
    item = s[0]
    s[0] = "replaced"
    s[-1] = 99
    length: int = len(s)
    _ = item, length


def check_sequence_to_dict(s: YamlSequence) -> None:
    d: Any = s.to_dict()
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
    inline: str | None = s.get_comment_inline(0)
    before: str | None = s.get_comment_before(-1)
    s.set_comment_inline(0, "note")
    s.set_comment_before(0, "header")
    _ = inline, before


def check_sequence_sort(s: YamlSequence) -> None:
    s.sort()
    s.sort(reverse=True)
    key_fn: Callable[[Any], int] = lambda v: len(str(v))
    s.sort(key=key_fn, reverse=True)


# ── Type errors that mypy should catch (kept as comments to document intent) ──
#
#   yarutsk.dumps(yarutsk.loads("x: 1"))   # loads returns YamlMapping | YamlSequence | None
#                                           # dumps requires YamlMapping | YamlSequence
#
#   keys: list[int] = list(m.keys())        # keys() returns KeysView[str], not list[int]
#
#   text: int = yarutsk.dumps(m)           # dumps returns str
