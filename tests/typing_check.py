"""
Type-checking tests for yarutsk — run with mypy, not pytest.

    uv run mypy

Each function exercises a part of the public API with explicit annotations.
mypy will error if the inferred types contradict the declared ones.
"""

from __future__ import annotations

import io
from typing import Any, Callable

import yarutsk
from yarutsk import YamlDocument, YamlMapping, YamlSequence


# ── load / loads ──────────────────────────────────────────────────────────────

def check_load_from_stream() -> None:
    doc: YamlDocument | None = yarutsk.load(io.StringIO("key: val"))
    doc2: YamlDocument | None = yarutsk.load(io.BytesIO(b"key: val"))
    _ = doc, doc2


def check_loads_from_string() -> None:
    doc: YamlDocument | None = yarutsk.loads("key: val")
    empty: YamlDocument | None = yarutsk.loads("")
    _ = doc, empty


def check_load_all() -> None:
    docs: list[YamlDocument] = yarutsk.load_all(io.StringIO("---\na: 1\n---\nb: 2"))
    _ = docs


def check_loads_all() -> None:
    docs: list[YamlDocument] = yarutsk.loads_all("---\na: 1\n---\nb: 2")
    _ = docs


# ── dump / dumps ──────────────────────────────────────────────────────────────

def check_dump_to_stream(doc: YamlDocument) -> None:
    yarutsk.dump(doc, io.StringIO())


def check_dumps_to_string(doc: YamlDocument) -> None:
    text: str = yarutsk.dumps(doc)
    _ = text


def check_dump_all_to_stream(docs: list[YamlDocument]) -> None:
    yarutsk.dump_all(docs, io.StringIO())


def check_dumps_all_to_string(docs: list[YamlDocument]) -> None:
    text: str = yarutsk.dumps_all(docs)
    _ = text


# ── None-guard required before using the document ────────────────────────────

def check_none_narrowing() -> str:
    doc = yarutsk.loads("key: val")
    if doc is None:
        return ""
    # After the None check mypy knows doc: YamlDocument
    return yarutsk.dumps(doc)


# ── YamlDocument mapping interface ───────────────────────────────────────────

def check_document_mapping_access(doc: YamlDocument) -> None:
    val = doc["key"]           # YamlMapping | YamlSequence | int | float | bool | str | None
    doc["key"] = "new"
    doc["key"] = 42
    doc["key"] = True
    doc["key"] = None
    contained: bool = "key" in doc
    length: int = len(doc)
    keys: list[str] = doc.keys()
    _ = val, contained, length, keys


def check_document_sequence_access(doc: YamlDocument) -> None:
    item = doc[0]
    doc[0] = "replaced"
    doc[-1] = 99
    _ = item


def check_document_to_dict(doc: YamlDocument) -> None:
    d: Any = doc.to_dict()
    _ = d


# ── YamlMapping interface ─────────────────────────────────────────────────────

def check_mapping_access(m: YamlMapping) -> None:
    val = m["key"]             # YamlMapping | YamlSequence | int | float | bool | str | None
    m["key"] = "new"
    m["key"] = 42
    contained: bool = "key" in m
    length: int = len(m)
    keys: list[str] = m.keys()
    _ = val, contained, length, keys


def check_mapping_to_dict(m: YamlMapping) -> None:
    d: Any = m.to_dict()
    _ = d


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


def check_sequence_sort(s: YamlSequence) -> None:
    s.sort()
    s.sort(reverse=True)
    key_fn: Callable[[Any], int] = lambda v: len(str(v))
    s.sort(key=key_fn, reverse=True)


# ── Comments on YamlDocument ─────────────────────────────────────────────────

def check_document_comments(doc: YamlDocument) -> None:
    inline: str | None = doc.get_comment_inline("key")
    before: str | None = doc.get_comment_before("key")
    doc.set_comment_inline("key", "note")
    doc.set_comment_before("key", "header")
    _ = inline, before


# ── Sorting on YamlDocument ───────────────────────────────────────────────────

def check_document_sort_keys(doc: YamlDocument) -> None:
    doc.sort_keys()
    doc.sort_keys(reverse=True)
    key_fn: Callable[[str], int] = len
    doc.sort_keys(key=key_fn, reverse=True, recursive=True)


def check_document_sort(doc: YamlDocument) -> None:
    doc.sort()
    doc.sort(reverse=True)


# ── Type errors that mypy should catch (kept as comments to document intent) ──
#
#   yarutsk.dumps(yarutsk.loads("x: 1"))   # loads returns YamlDocument | None
#                                           # dumps requires YamlDocument
#
#   keys: list[int] = doc.keys()           # keys() returns list[str]
#
#   text: int = yarutsk.dumps(doc)         # dumps returns str
