"""Tests for YAML loading: basic parsing, type preservation, insertion order."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestBasicLoading:
    def test_load_from_stringio(self) -> None:
        content = io.StringIO(
            dedent("""\
            name: John
            age: 30
        """)
        )
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_from_bytesio(self) -> None:
        content = io.BytesIO(
            dedent("""\
            name: John
            age: 30
        """).encode()
        )
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_nested_mapping(self) -> None:
        content = io.StringIO("""
person:
  name: John
  age: 30
  address:
    city: New York
""")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["person"]["name"] == "John"
        assert doc["person"]["age"] == 30
        assert doc["person"]["address"]["city"] == "New York"

    def test_load_sequence(self) -> None:
        content = io.StringIO("""
items:
  - first
  - second
  - third
""")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        items = doc["items"]
        assert items[0] == "first"
        assert items[1] == "second"
        assert items[2] == "third"

    def test_load_flow_sequence(self) -> None:
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[0] == "a"
        assert doc[1] == "b"
        assert doc[2] == "c"

    def test_load_flow_mapping(self) -> None:
        content = io.StringIO("{a: 1, b: 2}")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] == 1
        assert doc["b"] == 2


class TestLoadsBytes:
    def test_loads_bytes(self) -> None:
        doc = yarutsk.loads(b"a: 1\nb: two\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] == 1
        assert doc["b"] == "two"

    def test_loads_bytearray(self) -> None:
        doc = yarutsk.loads(bytearray(b"x: [1, 2, 3]\n"))
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc["x"]) == [1, 2, 3]

    def test_loads_bytes_utf8_multibyte(self) -> None:
        doc = yarutsk.loads("greeting: héllo\n".encode())
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["greeting"] == "héllo"

    def test_loads_all_bytes(self) -> None:
        docs = yarutsk.loads_all(b"a: 1\n---\nb: 2\n")
        assert all(isinstance(d, yarutsk.YamlMapping) for d in docs)
        assert [d.to_python() for d in docs] == [{"a": 1}, {"b": 2}]

    def test_iter_loads_all_bytes(self) -> None:
        docs = list(yarutsk.iter_loads_all(b"a: 1\n---\nb: 2\n"))
        assert all(isinstance(d, yarutsk.YamlMapping) for d in docs)
        assert [d.to_python() for d in docs] == [{"a": 1}, {"b": 2}]

    def test_loads_invalid_utf8_raises_unicode_decode_error(self) -> None:
        with pytest.raises(UnicodeDecodeError):
            yarutsk.loads(b"\xff\xfe bad")

    def test_loads_rejects_non_text_type(self) -> None:
        with pytest.raises(TypeError):
            yarutsk.loads(123)  # type: ignore[arg-type]


class TestTypePreservation:
    def test_integer(self) -> None:
        content = io.StringIO("value: 42")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] == 42
        assert isinstance(doc["value"], int)

    def test_float(self) -> None:
        content = io.StringIO("value: 3.14")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] == pytest.approx(3.14)
        assert isinstance(doc["value"], float)

    def test_boolean_true(self) -> None:
        content = io.StringIO("value: true")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] is True

    def test_boolean_false(self) -> None:
        content = io.StringIO("value: false")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] is False

    def test_null(self) -> None:
        content = io.StringIO("value: null")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] is None

    def test_string(self) -> None:
        content = io.StringIO('value: "hello world"')
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] == "hello world"
        assert isinstance(doc["value"], str)

    def test_quoted_string(self) -> None:
        content = io.StringIO("value: 'quoted string'")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] == "quoted string"

    def test_empty_double_quoted_string(self) -> None:
        doc = yarutsk.loads('key: ""')
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_single_quoted_string(self) -> None:
        doc = yarutsk.loads("key: ''")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_quoted_strings_in_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - ""
            - ''
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[0] == ""
        assert doc[1] == ""
        assert isinstance(doc[0], str)
        assert isinstance(doc[1], str)

    def test_empty_quoted_vs_bare_null(self) -> None:
        """Bare empty value and ~ are null; quoted empty is an empty string."""
        doc = yarutsk.loads(
            dedent("""\
            bare:
            null_tilde: ~
            quoted: ""
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["bare"] is None
        assert doc["null_tilde"] is None
        assert doc["quoted"] == ""

    def test_empty_quoted_round_trips(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: ""
            b: ''
        """)
        )
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["a"] == ""
        assert doc2["b"] == ""


class TestInsertionOrderPreservation:
    def test_order_preserved_on_load(self) -> None:
        content = io.StringIO(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc.keys()) == ["z", "a", "m"]

    def test_order_preserved_on_insert(self) -> None:
        content = io.StringIO(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["z"] = 3
        assert list(doc.keys()) == ["a", "b", "z"]

    def test_nested_order_preserved(self) -> None:
        content = io.StringIO("""
outer:
  z: 1
  a: 2
  m: 3
""")
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc["outer"].keys()) == ["z", "a", "m"]

    def test_round_trip_order(self) -> None:
        content = io.StringIO(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        doc = yarutsk.load(content)
        assert isinstance(doc, yarutsk.YamlMapping)
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

    def test_load_stringio(self) -> None:
        doc = yarutsk.load(io.StringIO("key: value\n"))
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == "value"

    def test_load_bytesio(self) -> None:
        doc = yarutsk.load(io.BytesIO(b"key: value\n"))
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == "value"

    def test_load_all_stringio(self) -> None:
        src = io.StringIO("a: 1\n---\nb: 2\n")
        docs = yarutsk.load_all(src)
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2

    def test_load_all_bytesio(self) -> None:
        src = io.BytesIO(b"x: 10\n---\ny: 20\n")
        docs = yarutsk.load_all(src)
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["x"] == 10
        assert d1["y"] == 20

    def test_load_empty_stream(self) -> None:
        doc = yarutsk.load(io.StringIO(""))
        assert doc is None

    def test_load_preserves_comments(self) -> None:
        src = io.StringIO("# comment\nkey: value\n")
        doc = yarutsk.load(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == "value"

    def test_load_preserves_explicit_start(self) -> None:
        src = io.StringIO("---\nkey: value\n")
        doc = yarutsk.load(src)
        assert doc is not None
        assert doc.explicit_start is True

    def test_load_schema_applied(self) -> None:
        schema = yarutsk.Schema()
        schema.add_loader("!upper", lambda v: str(v).upper())
        src = io.StringIO("val: !upper hello\n")
        doc = yarutsk.load(src, schema=schema)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["val"] == "HELLO"
