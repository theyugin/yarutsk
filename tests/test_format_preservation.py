"""Content-preservation tests for ``.format()``.

``.format()`` resets every cosmetic field (scalar style → plain, container
style → block, ``original`` cleared, comments dropped, blank lines zeroed).
The emitter is the safety net — without correct ``needs_quoting`` logic,
strings like ``"42"`` or ``"yes"`` would re-parse as the wrong type.

Each test follows the same shape:

    1. Build/load a doc whose content includes a value that looks risky.
    2. Call ``.format()`` (with default or explicit options).
    3. ``dumps`` → ``loads`` → ``to_python`` and assert it matches the expected
       Python tree.

Tests in ``test_api.py`` already check that ``.format()`` mutates the metadata
correctly. This file is exclusively about whether the document still says the
same thing after the round-trip.
"""

from typing import Any

import pytest

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import HealthCheck, given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402

import yarutsk  # noqa: E402


def _roundtrip_through_format(doc: Any, expected: Any, **opts: bool) -> None:
    """Call ``.format(**opts)``, then ``dumps``→``loads``→``to_python`` and assert."""
    if hasattr(doc, "format"):
        doc.format(**opts)
    text = yarutsk.dumps(doc)
    parsed = yarutsk.loads(text)
    actual = parsed.to_python() if hasattr(parsed, "to_python") else parsed
    assert actual == expected, (
        f"format() lost data\nemitted:\n{text!r}\nexpected: {expected!r}\ngot: {actual!r}"
    )


# ─── Numeric-looking strings ─────────────────────────────────────────────────
#
# When the input string ``original`` is cleared by ``format(styles=True)`` and
# the style is reset to Plain, the emitter must quote it or the parser will
# coerce it to int/float.


@pytest.mark.parametrize(
    "value",
    [
        "0",
        "1",
        "42",
        "-7",
        "+3",
        "0x10",
        "0X10",
        "0o17",
        "0O17",
        "3.14",
        "-2.5",
        "1e10",
        "1.5e10",
        "1E10",
        "1.5E-3",
        "1_000",  # underscored — currently re-parses as string
    ],
)
def test_numeric_string_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f'k: "{value}"')
    _roundtrip_through_format(doc, {"k": value})


@pytest.mark.parametrize("value", ["42", "0x10", "3.14", "1e10"])
def test_numeric_string_in_flow_seq_preserved(value: str) -> None:
    doc = yarutsk.loads(f'k: ["{value}", "end"]')
    _roundtrip_through_format(doc, {"k": [value, "end"]})


@pytest.mark.parametrize("value", ["42", "3.14", "1e10"])
def test_numeric_string_in_flow_map_preserved(value: str) -> None:
    doc = yarutsk.loads(f'k: {{a: "{value}", b: end}}')
    _roundtrip_through_format(doc, {"k": {"a": value, "b": "end"}})


# ─── Keyword strings (null/bool variants) ────────────────────────────────────


@pytest.mark.parametrize(
    "value",
    [
        # nulls
        "null",
        "Null",
        "NULL",
        "~",
        # booleans
        "true",
        "True",
        "TRUE",
        "false",
        "False",
        "FALSE",
        # YAML 1.1 booleans
        "yes",
        "Yes",
        "YES",
        "no",
        "No",
        "NO",
        "on",
        "On",
        "ON",
        "off",
        "Off",
        "OFF",
        # special floats
        ".inf",
        ".Inf",
        ".INF",
        "-.inf",
        "-.Inf",
        "-.INF",
        ".nan",
        ".NaN",
        ".NAN",
    ],
)
def test_keyword_string_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


@pytest.mark.parametrize(
    "value",
    ["null", "true", "false", "yes", "no", "~", ".inf", ".nan"],
)
def test_keyword_string_in_flow_seq_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: ['{value}', 'end']")
    _roundtrip_through_format(doc, {"k": [value, "end"]})


# ─── Strings with structural characters ──────────────────────────────────────


@pytest.mark.parametrize(
    "value",
    [
        "-foo",  # leading dash
        "?question",  # leading ?
        "&ref",  # leading &
        "*alias",  # leading *
        "!tag",  # leading !
        "#hash",  # leading #
        "|pipe",  # leading |
        ">gt",  # leading >
        "%percent",  # leading %
        "@at",  # leading @
        "`backtick",
        "{open",
        "}close",
        "[open",
        "]close",
        ",comma",
        "<lt",
        "=eq",
    ],
)
def test_leading_indicator_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


@pytest.mark.parametrize(
    "value",
    [
        "key: value",  # contains ": "
        "x: y: z",
        "trailing:",  # ends with `:`
        "with #hash",  # contains " #"
    ],
)
def test_colon_or_hash_in_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


@pytest.mark.parametrize("value", ["---", "..."])
def test_document_marker_string_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


# ─── Boundary and internal whitespace ────────────────────────────────────────


@pytest.mark.parametrize(
    "value",
    [
        " leading",
        "trailing ",
        "\tleading-tab",
        "trailing-tab\t",
        " ",
        "\t",
        "  many leading",
        "many trailing  ",
    ],
)
def test_boundary_whitespace_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


@pytest.mark.parametrize(
    "value",
    ["a b", "a  b", "a   b", "a\tb", "a \t b"],
)
def test_internal_whitespace_value_preserved(value: str) -> None:
    doc = yarutsk.loads(f"k: '{value}'")
    _roundtrip_through_format(doc, {"k": value})


# ─── Empty / whitespace-only strings ─────────────────────────────────────────


def test_empty_string_value_preserved() -> None:
    doc = yarutsk.loads("k: ''")
    _roundtrip_through_format(doc, {"k": ""})


def test_empty_string_in_flow_seq_preserved() -> None:
    doc = yarutsk.loads("k: ['', 'end']")
    _roundtrip_through_format(doc, {"k": ["", "end"]})


# ─── Multiline strings (force literal style on format) ───────────────────────


@pytest.mark.parametrize(
    "value",
    [
        "line1\nline2",
        "line1\nline2\n",
        "single\n\nblank-between",
        "triple\nlines\nhere",
        "with: colon\nand: more",
    ],
)
def test_multiline_string_preserved(value: str) -> None:
    """Multiline strings get coerced to literal block style on format()."""
    doc = yarutsk.YamlMapping()
    doc["k"] = yarutsk.YamlScalar(value, style="double")
    _roundtrip_through_format(doc, {"k": value})


def test_multiline_with_trailing_newline_preserved() -> None:
    """Trailing newline determines block-scalar chomping (clip vs strip vs keep)."""
    value = "abc\ndef\n"
    doc = yarutsk.YamlMapping()
    doc["k"] = yarutsk.YamlScalar(value, style="double")
    _roundtrip_through_format(doc, {"k": value})


def test_multiline_no_trailing_newline_preserved() -> None:
    value = "abc\ndef"
    doc = yarutsk.YamlMapping()
    doc["k"] = yarutsk.YamlScalar(value, style="double")
    _roundtrip_through_format(doc, {"k": value})


# ─── Container shape (flow → block) ──────────────────────────────────────────


def test_flow_mapping_format_to_block_preserves_content() -> None:
    doc = yarutsk.loads("k: {a: 1, b: 2, c: 3}")
    _roundtrip_through_format(doc, {"k": {"a": 1, "b": 2, "c": 3}})


def test_flow_sequence_format_to_block_preserves_content() -> None:
    doc = yarutsk.loads("k: [1, 2, 3, 4]")
    _roundtrip_through_format(doc, {"k": [1, 2, 3, 4]})


def test_nested_flow_format_to_block_preserves_content() -> None:
    doc = yarutsk.loads("k: {a: [1, {b: [2, 3]}], c: 4}")
    _roundtrip_through_format(
        doc,
        {"k": {"a": [1, {"b": [2, 3]}], "c": 4}},
    )


def test_empty_flow_mapping_preserved() -> None:
    doc = yarutsk.loads("k: {}")
    _roundtrip_through_format(doc, {"k": {}})


def test_empty_flow_sequence_preserved() -> None:
    doc = yarutsk.loads("k: []")
    _roundtrip_through_format(doc, {"k": []})


# ─── Mapping keys ────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "key",
    [
        "-foo",
        "?foo",
        "&foo",
        "*foo",
        "!foo",
        "null",
        "Null",
        "NULL",
        "~",
        "true",
        "false",
        "yes",
        "no",
        " leading",
        "trailing ",
        "\tleading-tab",
        "trailing-tab\t",
        ".inf",
        ".nan",
        "---",
        "...",
        "with: colon",
        "trailing:",
    ],
)
def test_risky_key_preserved(key: str) -> None:
    doc = yarutsk.YamlMapping()
    doc[key] = "value"
    _roundtrip_through_format(doc, {key: "value"})


def test_keys_in_flow_mapping_preserved() -> None:
    src = "{a: 1, b: 2, '-foo': 3, 'null': 4}"
    doc = yarutsk.loads(src)
    _roundtrip_through_format(doc, {"a": 1, "b": 2, "-foo": 3, "null": 4})


# ─── Tags / anchors / aliases preserved through format() ─────────────────────


def test_tag_on_scalar_preserved() -> None:
    doc = yarutsk.loads("v: !!str 42")
    doc.format()
    text = yarutsk.dumps(doc)
    assert "!!str" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"v": "42"}


def test_tag_on_mapping_preserved() -> None:
    doc = yarutsk.loads("v: !!map {a: 1}")
    doc.format()
    text = yarutsk.dumps(doc)
    assert "!!map" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"v": {"a": 1}}


def test_tag_on_sequence_preserved() -> None:
    doc = yarutsk.loads("v: !!seq [1, 2, 3]")
    doc.format()
    text = yarutsk.dumps(doc)
    assert "!!seq" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"v": [1, 2, 3]}


def test_custom_tag_preserved() -> None:
    doc = yarutsk.loads("v: !custom value")
    doc.format()
    text = yarutsk.dumps(doc)
    assert "!custom" in text


def test_anchor_alias_preserved() -> None:
    src = "a: &name value\nb: *name\n"
    doc = yarutsk.loads(src)
    doc.format()
    text = yarutsk.dumps(doc)
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"a": "value", "b": "value"}
    assert "&name" in text
    assert "*name" in text


def test_anchor_on_container_preserved() -> None:
    src = "a: &grp\n  x: 1\n  y: 2\nb: *grp\n"
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    expected = {"x": 1, "y": 2}
    assert parsed.to_python() == {"a": expected, "b": expected}


# ─── Float precision and special floats ──────────────────────────────────────


@pytest.mark.parametrize(
    "src,expected",
    [
        ("v: 3.14", {"v": 3.14}),
        ("v: -2.5", {"v": -2.5}),
        ("v: 1.5e10", {"v": 1.5e10}),
        ("v: 1.0e-5", {"v": 1.0e-5}),
        ("v: 0.0", {"v": 0.0}),
        ("v: .inf", {"v": float("inf")}),
        ("v: -.inf", {"v": float("-inf")}),
    ],
)
def test_float_value_preserved(src: str, expected: dict[str, Any]) -> None:
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == expected


def test_nan_value_remains_nan() -> None:
    """NaN is not equal to itself, so check via isnan()."""
    import math

    doc = yarutsk.loads("v: .nan")
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    val = parsed.to_python()["v"]
    assert isinstance(val, float) and math.isnan(val)


# ─── Integer canonicalisation (hex/octal → decimal is OK semantically) ───────


@pytest.mark.parametrize(
    "src,expected",
    [
        ("v: 0x10", {"v": 16}),
        ("v: 0o17", {"v": 15}),
        ("v: -42", {"v": -42}),
        ("v: 0", {"v": 0}),
    ],
)
def test_int_value_preserved(src: str, expected: dict[str, Any]) -> None:
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == expected


# ─── Idempotency ─────────────────────────────────────────────────────────────


def test_format_twice_equals_once() -> None:
    src = """
    # comment
    a: 'single'
    b: "double"
    nested: {x: 1, y: [1, 2]}
    """
    doc1 = yarutsk.loads(src)
    doc1.format()
    out1 = yarutsk.dumps(doc1)

    doc2 = yarutsk.loads(src)
    doc2.format()
    doc2.format()
    out2 = yarutsk.dumps(doc2)

    assert out1 == out2


def test_dump_after_format_is_fixed_point() -> None:
    """dumps(format(loads(out))) == dumps(format(loads(dumps(format(loads(out))))))"""
    src = "a: 'one'\nb: [1, 2]\nc: {x: 'y'}\n"
    doc = yarutsk.loads(src)
    doc.format()
    once = yarutsk.dumps(doc)

    doc2 = yarutsk.loads(once)
    doc2.format()
    twice = yarutsk.dumps(doc2)

    assert once == twice


# ─── Format-option toggles ───────────────────────────────────────────────────


def test_styles_false_keeps_quoted_string() -> None:
    """With styles=False, original style is kept; content still preserved."""
    doc = yarutsk.loads('k: "42"')
    doc.format(styles=False)
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"k": "42"}


def test_comments_false_preserves_inline_comment_in_dump() -> None:
    doc = yarutsk.loads("k: v  # hello")
    doc.format(comments=False)
    text = yarutsk.dumps(doc)
    assert "# hello" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"k": "v"}


def test_blank_lines_false_preserves_separator_in_dump() -> None:
    src = "a: 1\n\n\nb: 2\n"
    doc = yarutsk.loads(src)
    doc.format(blank_lines=False)
    text = yarutsk.dumps(doc)
    assert "\n\n" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"a": 1, "b": 2}


def test_styles_false_keeps_flow_container() -> None:
    doc = yarutsk.loads("k: {a: 1, b: 2}")
    doc.format(styles=False)
    text = yarutsk.dumps(doc)
    assert "{" in text
    parsed = yarutsk.loads(text)
    assert parsed.to_python() == {"k": {"a": 1, "b": 2}}


# ─── format() on a YamlScalar standalone ─────────────────────────────────────


@pytest.mark.parametrize(
    "value",
    ["42", "true", "null", "yes", "-foo", " leading", "trailing ", ""],
)
def test_yamlscalar_format_preserves_content(value: str) -> None:
    s = yarutsk.YamlScalar(value, style="double")
    s.format()
    doc = yarutsk.YamlMapping()
    doc["k"] = s
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"k": value}


def test_yamlscalar_format_multiline_to_literal() -> None:
    s = yarutsk.YamlScalar("a\nb\n", style="double")
    s.format()
    assert s.style == "literal"
    doc = yarutsk.YamlMapping()
    doc["k"] = s
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"k": "a\nb\n"}


# ─── Deeply nested structures ────────────────────────────────────────────────


def test_deep_nesting_preserved() -> None:
    src = """
    a:
      b:
        c:
          d:
            e: 'deep'
    """
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"a": {"b": {"c": {"d": {"e": "deep"}}}}}


def test_mixed_nesting_preserved() -> None:
    src = """
    items:
      - {name: 'a', vals: [1, 2]}
      - name: 'b'
        vals: [3, 4]
      - 'plain'
    """
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {
        "items": [
            {"name": "a", "vals": [1, 2]},
            {"name": "b", "vals": [3, 4]},
            "plain",
        ],
    }


# ─── Big mixed-content stress case ───────────────────────────────────────────


def test_all_categories_in_one_doc() -> None:
    src = """
    # leading block comment
    nums:
      int: 42
      hex: 0x10
      float: 3.14
      sci: 1.5e10
      neg: -7
    risky_strings:
      num_str: '42'
      bool_str: 'yes'
      null_str: 'null'
      dash: '-foo'
      colon: 'a: b'
      space: ' leading'
      empty: ''
    multiline: "first\\nsecond\\nthird"
    flow_map: {a: 1, b: [2, 3]}
    flow_seq: ['x', 'y', {z: 1}]
    tags:
      string: !!str 42
      mapping: !!map {a: 1}
    """
    doc = yarutsk.loads(src)
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    expected = {
        "nums": {
            "int": 42,
            "hex": 16,
            "float": 3.14,
            "sci": 1.5e10,
            "neg": -7,
        },
        "risky_strings": {
            "num_str": "42",
            "bool_str": "yes",
            "null_str": "null",
            "dash": "-foo",
            "colon": "a: b",
            "space": " leading",
            "empty": "",
        },
        "multiline": "first\nsecond\nthird",
        "flow_map": {"a": 1, "b": [2, 3]},
        "flow_seq": ["x", "y", {"z": 1}],
        "tags": {"string": "42", "mapping": {"a": 1}},
    }
    assert parsed.to_python() == expected


# ─── Hypothesis property: format() never loses risky content ─────────────────
#
# The default property test in ``test_roundtrip_property.py`` uses a "safe"
# alphabet. Here we deliberately target inputs the emitter has to *quote* to
# survive ``format()``: numerics, keywords, leading indicators, structural
# punctuation.


_RISKY_SAMPLED = st.sampled_from(
    [
        # numerics
        "0",
        "1",
        "42",
        "-7",
        "+3",
        "0x10",
        "0o17",
        "3.14",
        "1e10",
        "1.5E-3",
        "1_000",
        # keywords
        "null",
        "Null",
        "NULL",
        "~",
        "true",
        "True",
        "TRUE",
        "false",
        "False",
        "FALSE",
        "yes",
        "Yes",
        "YES",
        "no",
        "No",
        "NO",
        "on",
        "On",
        "off",
        "Off",
        ".inf",
        "-.inf",
        ".nan",
        # leading indicators
        "-x",
        "?x",
        "&x",
        "*x",
        "!x",
        "#x",
        "|x",
        ">x",
        "%x",
        "@x",
        "`x",
        "{x",
        "}x",
        "[x",
        "]x",
        ",x",
        "<x",
        "=x",
        # structural
        "a: b",
        "x: y: z",
        "trailing:",
        "with #hash",
        "---",
        "...",
        # whitespace boundaries
        " leading",
        "trailing ",
        "\tlt",
        "tt\t",
        " ",
        "\t",
        # internal whitespace
        "a b",
        "a  b",
        "a\tb",
        # empty
        "",
    ],
)

_SAFE_LEAVES = (
    st.integers(min_value=-(2**31), max_value=2**31 - 1)
    | st.floats(allow_nan=False, allow_infinity=False, width=32)
    | st.booleans()
    | st.none()
)
_LEAF = st.one_of(_SAFE_LEAVES, _RISKY_SAMPLED)
_SAFE_KEY = st.text(
    alphabet=st.characters(whitelist_categories=("Ll", "Lu", "Nd"), whitelist_characters="_-"),
    min_size=1,
    max_size=8,
)


def _trees() -> st.SearchStrategy[Any]:
    return st.recursive(
        _LEAF,
        lambda inner: st.lists(inner, max_size=4) | st.dictionaries(_SAFE_KEY, inner, max_size=4),
        max_leaves=8,
    )


@given(tree=_trees())
@settings(max_examples=200, deadline=None, suppress_health_check=[HealthCheck.too_slow])
def test_format_then_roundtrip_risky_leaves(tree: Any) -> None:
    """Build doc → format() → dump → load → assert tree equality."""
    doc = yarutsk.loads(yarutsk.dumps(tree))
    if hasattr(doc, "format"):
        doc.format()
    again = yarutsk.loads(yarutsk.dumps(doc))
    actual = again.to_python() if hasattr(again, "to_python") else again
    assert actual == tree


_RISKY_KEY = st.one_of(_SAFE_KEY, _RISKY_SAMPLED.filter(lambda s: s != ""))


@given(
    items=st.lists(st.tuples(_RISKY_KEY, _LEAF), min_size=1, max_size=6, unique_by=lambda t: t[0])
)
@settings(max_examples=150, deadline=None, suppress_health_check=[HealthCheck.too_slow])
def test_format_preserves_risky_keys(items: list[tuple[str, Any]]) -> None:
    """Hypothesis-generated risky keys round-trip through format() losslessly."""
    tree = dict(items)
    doc = yarutsk.loads(yarutsk.dumps(tree))
    if hasattr(doc, "format"):
        doc.format()
    again = yarutsk.loads(yarutsk.dumps(doc))
    actual = again.to_python() if hasattr(again, "to_python") else again
    assert actual == tree


@given(values=st.lists(_LEAF, min_size=1, max_size=6))
@settings(max_examples=150, deadline=None, suppress_health_check=[HealthCheck.too_slow])
def test_format_flow_seq_with_risky_values(values: list[Any]) -> None:
    """Flow-sequence values that include risky strings survive format()."""
    seq = yarutsk.YamlSequence(style="flow")
    for v in values:
        seq.append(v)
    doc = yarutsk.YamlMapping()
    doc["k"] = seq
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"k": values}


@given(items=st.dictionaries(_SAFE_KEY, _LEAF, min_size=1, max_size=6))
@settings(max_examples=150, deadline=None, suppress_health_check=[HealthCheck.too_slow])
def test_format_flow_map_with_risky_values(items: dict[str, Any]) -> None:
    m = yarutsk.YamlMapping(style="flow")
    for k, v in items.items():
        m[k] = v
    doc = yarutsk.YamlMapping()
    doc["k"] = m
    doc.format()
    parsed = yarutsk.loads(yarutsk.dumps(doc))
    assert parsed.to_python() == {"k": items}
