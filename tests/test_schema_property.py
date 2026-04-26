"""Property-based tests for Schema loaders/dumpers and built-in tag handlers.

Complements the hand-written cases in ``test_schema.py`` by exercising the
load/dump round-trip against hypothesis-generated inputs. Finds classes of bugs
that example-based tests miss (boundary values, escape sequences, unusual
datetimes, etc.).
"""

import datetime

import pytest

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402

import yarutsk  # noqa: E402


class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Point) and self.x == other.x and self.y == other.y

    def __repr__(self) -> str:
        return f"Point({self.x}, {self.y})"


def _point_schema() -> yarutsk.Schema:
    s = yarutsk.Schema()
    s.add_loader("!pt", lambda d: Point(d["x"], d["y"]))
    s.add_dumper(Point, lambda p: ("!pt", {"x": p.x, "y": p.y}))
    return s


# Values outside i64 range are emitted as-is but re-parse as floats (see
# CLAUDE.md: ScalarValue coercion). Keep ints inside i64 for a clean round-trip.
I64_INTS = st.integers(min_value=-(2**63), max_value=2**63 - 1)


@given(x=I64_INTS, y=I64_INTS)
@settings(max_examples=100, deadline=None)
def test_custom_tag_roundtrip(x: int, y: int) -> None:
    schema = _point_schema()
    doc = yarutsk.loads("x: placeholder\n")
    assert isinstance(doc, yarutsk.YamlMapping)
    doc["x"] = Point(x, y)
    out = yarutsk.dumps(doc, schema=schema)
    doc2 = yarutsk.loads(out, schema=schema)
    assert isinstance(doc2, yarutsk.YamlMapping)
    assert doc2["x"] == Point(x, y)


@given(data=st.binary(max_size=1024))
@settings(max_examples=100, deadline=None)
def test_binary_roundtrip(data: bytes) -> None:
    doc = yarutsk.loads("x: placeholder\n")
    assert isinstance(doc, yarutsk.YamlMapping)
    doc["x"] = data
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
    assert isinstance(doc2, yarutsk.YamlMapping)
    assert doc2["x"] == data


@given(
    dt=st.datetimes(
        min_value=datetime.datetime(1900, 1, 1),
        max_value=datetime.datetime(2100, 1, 1),
    )
)
@settings(max_examples=100, deadline=None)
def test_datetime_roundtrip(dt: datetime.datetime) -> None:
    doc = yarutsk.loads("x: placeholder\n")
    assert isinstance(doc, yarutsk.YamlMapping)
    doc["x"] = dt
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
    assert isinstance(doc2, yarutsk.YamlMapping)
    assert doc2["x"] == dt


@given(d=st.dates(min_value=datetime.date(1900, 1, 1), max_value=datetime.date(2100, 1, 1)))
@settings(max_examples=50, deadline=None)
def test_date_roundtrip(d: datetime.date) -> None:
    doc = yarutsk.loads("x: placeholder\n")
    assert isinstance(doc, yarutsk.YamlMapping)
    doc["x"] = d
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
    assert isinstance(doc2, yarutsk.YamlMapping)
    assert doc2["x"] == d


@given(n=st.integers(min_value=-(2**63), max_value=2**63 - 1))
@settings(max_examples=100, deadline=None)
def test_int_raw_tag_bypass(n: int) -> None:
    """A registered !!int loader must receive the raw string, not the coerced int."""
    received: list[str] = []
    schema = yarutsk.Schema()

    def load_int(raw: str) -> int:
        received.append(raw)
        return int(raw)

    schema.add_loader("!!int", load_int)

    doc = yarutsk.loads(f"x: !!int {n}\n", schema=schema)
    assert isinstance(doc, yarutsk.YamlMapping)
    assert doc["x"] == n
    assert received == [str(n)]


@given(s=st.sampled_from(["null", "Null", "NULL", "~", "anything", "yes", "0"]))
@settings(max_examples=50, deadline=None)
def test_null_raw_tag_bypass(s: str) -> None:
    """A registered !!null loader must receive the raw string verbatim."""
    received: list[str] = []
    schema = yarutsk.Schema()

    def load_null(raw: str) -> None:
        received.append(raw)
        return None

    schema.add_loader("!!null", load_null)

    doc = yarutsk.loads(f"x: !!null {s}\n", schema=schema)
    assert isinstance(doc, yarutsk.YamlMapping)
    assert doc["x"] is None
    assert received == [s]


@given(
    s=st.sampled_from(
        ["true", "True", "TRUE", "false", "False", "yes", "no", "on", "off", "y", "n"]
    )
)
@settings(max_examples=50, deadline=None)
def test_bool_raw_tag_bypass(s: str) -> None:
    """A registered !!bool loader must receive the raw string, not the coerced bool."""
    received: list[str] = []
    schema = yarutsk.Schema()

    def load_bool(raw: str) -> bool:
        received.append(raw)
        return raw.lower() in {"true", "yes", "on", "y"}

    schema.add_loader("!!bool", load_bool)

    doc = yarutsk.loads(f"x: !!bool {s}\n", schema=schema)
    assert isinstance(doc, yarutsk.YamlMapping)
    assert doc["x"] == (s.lower() in {"true", "yes", "on", "y"})
    assert received == [s]


@given(f=st.floats(allow_nan=False, allow_infinity=False, width=64))
@settings(max_examples=50, deadline=None)
def test_float_raw_tag_bypass(f: float) -> None:
    """A registered !!float loader must receive the raw string, not the coerced float."""
    received: list[str] = []
    schema = yarutsk.Schema()

    def load_float(raw: str) -> float:
        received.append(raw)
        return float(raw)

    schema.add_loader("!!float", load_float)

    text = repr(f)
    doc = yarutsk.loads(f'x: !!float "{text}"\n', schema=schema)
    assert isinstance(doc, yarutsk.YamlMapping)
    assert doc["x"] == f
    assert received == [text]


@given(
    s=st.text(
        alphabet=st.characters(whitelist_categories=("Ll", "Lu", "Nd"), whitelist_characters="_-"),
        min_size=1,
        max_size=40,
    )
)
@settings(max_examples=50, deadline=None)
def test_str_raw_tag_bypass(s: str) -> None:
    """A registered !!str loader must receive the raw string verbatim."""
    received: list[str] = []
    schema = yarutsk.Schema()

    def load_str(raw: str) -> str:
        received.append(raw)
        return raw

    schema.add_loader("!!str", load_str)

    doc = yarutsk.loads(f'x: !!str "{s}"\n', schema=schema)
    assert isinstance(doc, yarutsk.YamlMapping)
    assert doc["x"] == s
    assert received == [s]
