"""Regression tests for plain-scalar quoting decisions.

Each case loads or constructs a doc, dumps it, re-loads, and asserts the
``to_python`` tree is preserved. These exercise gaps in
``needs_quoting`` / ``needs_quoting_for_key`` that were not covered by the
0.6.1 boundary-whitespace fix.
"""

from typing import Any

import pytest

import yarutsk


def _roundtrips(doc: Any, expected: Any) -> None:
    """Dump ``doc``, parse the output, and assert it matches ``expected``."""
    text = yarutsk.dumps(doc)
    parsed = yarutsk.loads(text)
    assert parsed is not None
    actual = parsed.to_python() if hasattr(parsed, "to_python") else parsed
    assert actual == expected, (
        f"roundtrip lost data\nemitted:\n{text!r}\nexpected: {expected!r}\ngot: {actual!r}"
    )


@pytest.mark.parametrize(
    "value",
    [" leading", "trailing ", "\tleading-tab", "trailing-tab\t", " ", "\t"],
)
def test_value_boundary_whitespace_roundtrips(value: str) -> None:
    _roundtrips({"k": value}, {"k": value})


@pytest.mark.parametrize(
    "value",
    ["a b", "a  b", "a   b", "a\tb", "a\t\tb"],
)
def test_value_internal_whitespace_roundtrips(value: str) -> None:
    _roundtrips({"k": value}, {"k": value})


@pytest.mark.parametrize(
    "value",
    ["a, b", "a,b", "[bracketed]", "{braced}", "a]b", "a}b", "x,y,z"],
)
def test_value_in_flow_sequence_roundtrips(value: str) -> None:
    seq = yarutsk.YamlSequence(style="flow")
    seq.append(value)
    seq.append("end")
    _roundtrips({"k": seq}, {"k": [value, "end"]})


@pytest.mark.parametrize(
    "value",
    ["a, b", "[bracketed]", "{braced}", "a]b", "x,y"],
)
def test_value_in_flow_mapping_roundtrips(value: str) -> None:
    inner = yarutsk.YamlMapping(style="flow")
    inner["a"] = value
    inner["b"] = "end"
    _roundtrips({"k": inner}, {"k": {"a": value, "b": "end"}})


@pytest.mark.parametrize(
    "key",
    [
        "-foo",  # leading dash → looks like a sequence indicator
        "null",  # YAML null keyword
        "Null",
        "NULL",
        "~",
        "true",  # YAML bool keyword
        "false",
        "yes",
        "no",
        "on",
        "off",
        " leading",  # boundary whitespace (covered by 0.6.1)
        "trailing ",
    ],
)
def test_key_keyword_or_indicator_roundtrips(key: str) -> None:
    _roundtrips({key: "v"}, {key: "v"})


def test_format_preserves_boundary_whitespace_in_value() -> None:
    doc = yarutsk.loads('k: " leading"')
    assert doc is not None
    doc.format()
    _roundtrips(doc, {"k": " leading"})


def test_format_preserves_flow_context_value_with_comma() -> None:
    doc = yarutsk.loads('seq: ["a, b", "c"]')
    assert doc is not None
    doc.format()
    # format() switches container style to block, so the comma is no longer
    # a flow indicator on emit. Round-trip must still be lossless.
    _roundtrips(doc, {"seq": ["a, b", "c"]})


def test_format_preserves_keyword_key() -> None:
    doc = yarutsk.loads("'null': 1\n'true': 2\n")
    assert doc is not None
    doc.format()
    _roundtrips(doc, {"null": 1, "true": 2})


def test_format_preserves_dash_leading_key() -> None:
    doc = yarutsk.loads("'-foo': bar\n")
    assert doc is not None
    doc.format()
    _roundtrips(doc, {"-foo": "bar"})
