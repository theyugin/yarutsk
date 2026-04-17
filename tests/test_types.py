"""Tests for YAML type coercion: quoted lookalikes, special floats, block scalars,
special-character strings."""

from textwrap import dedent

import yarutsk


class TestQuotedTypeLookalikes:
    """Quoted scalars that look like other types must stay as strings."""

    def test_quoted_true_is_str(self):
        doc = yarutsk.loads('key: "true"')
        assert doc["key"] == "true"
        assert isinstance(doc["key"], str)

    def test_quoted_false_is_str(self):
        doc = yarutsk.loads("key: 'false'")
        assert doc["key"] == "false"
        assert isinstance(doc["key"], str)

    def test_quoted_null_is_str(self):
        doc = yarutsk.loads('key: "null"')
        assert doc["key"] == "null"
        assert isinstance(doc["key"], str)

    def test_quoted_integer_is_str(self):
        doc = yarutsk.loads('key: "42"')
        assert doc["key"] == "42"
        assert isinstance(doc["key"], str)

    def test_quoted_float_is_str(self):
        doc = yarutsk.loads("key: '3.14'")
        assert doc["key"] == "3.14"
        assert isinstance(doc["key"], str)

    def test_quoted_zero_is_str(self):
        doc = yarutsk.loads('key: "0"')
        assert doc["key"] == "0"
        assert isinstance(doc["key"], str)

    def test_quoted_yes_is_str(self):
        """'yes' is a bool in YAML 1.1 — but only unquoted."""
        doc = yarutsk.loads('key: "yes"')
        assert doc["key"] == "yes"
        assert isinstance(doc["key"], str)

    def test_plain_true_is_bool(self):
        doc = yarutsk.loads("key: true")
        assert doc["key"] is True

    def test_plain_false_is_bool(self):
        doc = yarutsk.loads("key: false")
        assert doc["key"] is False

    def test_plain_null_is_none(self):
        doc = yarutsk.loads("key: null")
        assert doc["key"] is None

    def test_tilde_is_none(self):
        doc = yarutsk.loads("key: ~")
        assert doc["key"] is None

    def test_plain_integer_is_int(self):
        doc = yarutsk.loads("key: 42")
        assert doc["key"] == 42
        assert isinstance(doc["key"], int)


class TestSpecialFloats:
    """Special float literals: .inf, -.inf, .nan."""

    def test_inf(self):
        import math

        doc = yarutsk.loads("key: .inf")
        assert math.isinf(doc["key"])
        assert doc["key"] > 0

    def test_negative_inf(self):
        import math

        doc = yarutsk.loads("key: -.inf")
        assert math.isinf(doc["key"])
        assert doc["key"] < 0

    def test_nan(self):
        import math

        doc = yarutsk.loads("key: .nan")
        assert math.isnan(doc["key"])

    def test_inf_round_trip(self):
        import math

        doc = yarutsk.loads("key: .inf")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert math.isinf(doc2["key"])


class TestBlockScalars:
    """Literal | and folded > block scalars."""

    def test_literal_block_preserves_newlines(self):
        yaml = dedent("""\
            text: |
              line one
              line two
        """)
        doc = yarutsk.loads(yaml)
        assert "line one" in doc["text"]
        assert "line two" in doc["text"]
        assert "\n" in doc["text"]

    def test_folded_block_is_string(self):
        yaml = dedent("""\
            text: >
              folded
              text
        """)
        doc = yarutsk.loads(yaml)
        assert isinstance(doc["text"], str)
        assert "folded" in doc["text"]

    def test_literal_block_value_is_string(self):
        """Block scalar value is a plain Python str after loading."""
        yaml = dedent("""\
            text: |
              hello
              world
        """)
        doc = yarutsk.loads(yaml)
        assert isinstance(doc["text"], str)
        assert doc["text"].startswith("hello")


class TestSpecialStringRoundTrips:
    """Strings containing YAML-special characters survive dump/load."""

    def test_string_with_colon(self):
        doc = yarutsk.loads("url: 'http://example.com:8080/path'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["url"] == "http://example.com:8080/path"

    def test_string_with_hash(self):
        doc = yarutsk.loads("comment: 'color: #fff'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["comment"] == "color: #fff"

    def test_string_with_leading_spaces(self):
        doc = yarutsk.loads("key: '  leading spaces'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "  leading spaces"

    def test_string_with_newline(self):
        doc = yarutsk.loads("key: 'line1\\nline2'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == doc["key"]

    def test_empty_string_key(self):
        """An empty string value on a non-empty key round-trips correctly."""
        doc = yarutsk.loads("key: ''")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == ""
        assert isinstance(doc2["key"], str)
