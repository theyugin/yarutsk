"""Tests for YAML loading: basic parsing, type preservation, insertion order."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestBasicLoading:
    def test_load_from_stringio(self):
        content = io.StringIO(
            dedent("""\
            name: John
            age: 30
        """)
        )
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_from_bytesio(self):
        content = io.BytesIO(
            dedent("""\
            name: John
            age: 30
        """).encode()
        )
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_nested_mapping(self):
        content = io.StringIO("""
person:
  name: John
  age: 30
  address:
    city: New York
""")
        doc = yarutsk.load(content)
        assert doc["person"]["name"] == "John"
        assert doc["person"]["age"] == 30
        assert doc["person"]["address"]["city"] == "New York"

    def test_load_sequence(self):
        content = io.StringIO("""
items:
  - first
  - second
  - third
""")
        doc = yarutsk.load(content)
        items = doc["items"]
        assert items[0] == "first"
        assert items[1] == "second"
        assert items[2] == "third"

    def test_load_flow_sequence(self):
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        assert doc[0] == "a"
        assert doc[1] == "b"
        assert doc[2] == "c"

    def test_load_flow_mapping(self):
        content = io.StringIO("{a: 1, b: 2}")
        doc = yarutsk.load(content)
        assert doc["a"] == 1
        assert doc["b"] == 2


class TestTypePreservation:
    def test_integer(self):
        content = io.StringIO("value: 42")
        doc = yarutsk.load(content)
        assert doc["value"] == 42
        assert isinstance(doc["value"], int)

    def test_float(self):
        content = io.StringIO("value: 3.14")
        doc = yarutsk.load(content)
        assert doc["value"] == pytest.approx(3.14)
        assert isinstance(doc["value"], float)

    def test_boolean_true(self):
        content = io.StringIO("value: true")
        doc = yarutsk.load(content)
        assert doc["value"] is True

    def test_boolean_false(self):
        content = io.StringIO("value: false")
        doc = yarutsk.load(content)
        assert doc["value"] is False

    def test_null(self):
        content = io.StringIO("value: null")
        doc = yarutsk.load(content)
        assert doc["value"] is None

    def test_string(self):
        content = io.StringIO('value: "hello world"')
        doc = yarutsk.load(content)
        assert doc["value"] == "hello world"
        assert isinstance(doc["value"], str)

    def test_quoted_string(self):
        content = io.StringIO("value: 'quoted string'")
        doc = yarutsk.load(content)
        assert doc["value"] == "quoted string"

    def test_empty_double_quoted_string(self):
        """Empty double-quoted string "" must be an empty str, not None."""
        doc = yarutsk.loads('key: ""')
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_single_quoted_string(self):
        """Empty single-quoted string '' must be an empty str, not None."""
        doc = yarutsk.loads("key: ''")
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_quoted_strings_in_sequence(self):
        """Empty quoted strings inside a sequence are preserved as empty strings."""
        doc = yarutsk.loads(
            dedent("""\
            - ""
            - ''
        """)
        )
        assert doc[0] == ""
        assert doc[1] == ""
        assert isinstance(doc[0], str)
        assert isinstance(doc[1], str)

    def test_empty_quoted_vs_bare_null(self):
        """Bare empty value and ~ are null; quoted empty is an empty string."""
        doc = yarutsk.loads(
            dedent("""\
            bare:
            null_tilde: ~
            quoted: ""
        """)
        )
        assert doc["bare"] is None
        assert doc["null_tilde"] is None
        assert doc["quoted"] == ""

    def test_empty_quoted_round_trips(self):
        """Empty quoted string survives a dump/load cycle as an empty string."""
        doc = yarutsk.loads(
            dedent("""\
            a: ""
            b: ''
        """)
        )
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["a"] == ""
        assert doc2["b"] == ""


class TestInsertionOrderPreservation:
    def test_order_preserved_on_load(self):
        """Keys appear in same order as input YAML."""
        content = io.StringIO(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        doc = yarutsk.load(content)
        assert list(doc.keys()) == ["z", "a", "m"]

    def test_order_preserved_on_insert(self):
        """New keys appended at end."""
        content = io.StringIO(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc = yarutsk.load(content)
        doc["z"] = 3
        assert list(doc.keys()) == ["a", "b", "z"]

    def test_nested_order_preserved(self):
        """Nested mappings also preserve order."""
        content = io.StringIO("""
outer:
  z: 1
  a: 2
  m: 3
""")
        doc = yarutsk.load(content)
        assert list(doc["outer"].keys()) == ["z", "a", "m"]

    def test_round_trip_order(self):
        """Order preserved through parse-modify-serialize cycle."""
        content = io.StringIO(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        doc = yarutsk.load(content)
        doc["b"] = 4
        output = io.StringIO()
        yarutsk.dump(doc, output)
        result = output.getvalue()
        assert result.index("z:") < result.index("a:")
        assert result.index("a:") < result.index("m:")
        assert result.index("m:") < result.index("b:")


class TestStreamingLoad:
    """load() and load_all() stream IO objects in chunks without reading
    the entire file at once."""

    def test_load_stringio(self):
        doc = yarutsk.load(io.StringIO("key: value\n"))
        assert doc["key"] == "value"

    def test_load_bytesio(self):
        doc = yarutsk.load(io.BytesIO(b"key: value\n"))
        assert doc["key"] == "value"

    def test_load_all_stringio(self):
        src = io.StringIO("a: 1\n---\nb: 2\n")
        docs = yarutsk.load_all(src)
        assert len(docs) == 2
        assert docs[0]["a"] == 1
        assert docs[1]["b"] == 2

    def test_load_all_bytesio(self):
        src = io.BytesIO(b"x: 10\n---\ny: 20\n")
        docs = yarutsk.load_all(src)
        assert len(docs) == 2
        assert docs[0]["x"] == 10
        assert docs[1]["y"] == 20

    def test_load_empty_stream(self):
        doc = yarutsk.load(io.StringIO(""))
        assert doc is None

    def test_load_preserves_comments(self):
        src = io.StringIO("# comment\nkey: value\n")
        doc = yarutsk.load(src)
        assert doc["key"] == "value"

    def test_load_preserves_explicit_start(self):
        src = io.StringIO("---\nkey: value\n")
        doc = yarutsk.load(src)
        assert doc.explicit_start is True

    def test_load_schema_applied(self):
        schema = yarutsk.Schema()
        schema.add_loader("!upper", lambda v: str(v).upper())
        src = io.StringIO("val: !upper hello\n")
        doc = yarutsk.load(src, schema=schema)
        assert doc["val"] == "HELLO"
