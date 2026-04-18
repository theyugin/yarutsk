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
    doc["x"] = Point(x, y)
    out = yarutsk.dumps(doc, schema=schema)
    doc2 = yarutsk.loads(out, schema=schema)
    assert doc2["x"] == Point(x, y)


@given(data=st.binary(max_size=1024))
@settings(max_examples=100, deadline=None)
def test_binary_roundtrip(data: bytes) -> None:
    doc = yarutsk.loads("x: placeholder\n")
    doc["x"] = data
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
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
    doc["x"] = dt
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
    assert doc2["x"] == dt


@given(d=st.dates(min_value=datetime.date(1900, 1, 1), max_value=datetime.date(2100, 1, 1)))
@settings(max_examples=50, deadline=None)
def test_date_roundtrip(d: datetime.date) -> None:
    doc = yarutsk.loads("x: placeholder\n")
    doc["x"] = d
    out = yarutsk.dumps(doc)
    doc2 = yarutsk.loads(out)
    assert doc2["x"] == d


@given(n=st.integers(min_value=-(2**63), max_value=2**63 - 1))
@settings(max_examples=100, deadline=None)
def test_int_raw_tag_bypass(n: int) -> None:
    """A registered !!int loader must receive the raw string, not the coerced int."""
    received: list[str] = []
    schema = yarutsk.Schema()
    schema.add_loader("!!int", lambda raw: received.append(raw) or int(raw))

    doc = yarutsk.loads(f"x: !!int {n}\n", schema=schema)
    assert doc["x"] == n
    assert received == [str(n)]
