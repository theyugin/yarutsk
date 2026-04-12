"""Tests for YAML loading: basic parsing, type preservation, insertion order."""

import io

import pytest

try:
    import yarutsk

    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

pytestmark = pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk module not built")


class TestBasicLoading:
    """Test basic YAML loading functionality."""

    def test_load_from_stringio(self):
        """Test loading from StringIO."""
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_from_bytesio(self):
        """Test loading from BytesIO."""
        content = io.BytesIO(b"name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_nested_mapping(self):
        """Test loading nested mappings."""
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
        """Test loading sequences."""
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
        """Test loading flow sequences."""
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        assert doc[0] == "a"
        assert doc[1] == "b"
        assert doc[2] == "c"

    def test_load_flow_mapping(self):
        """Test loading flow mappings."""
        content = io.StringIO("{a: 1, b: 2}")
        doc = yarutsk.load(content)
        assert doc["a"] == 1
        assert doc["b"] == 2


class TestTypePreservation:
    """Test that YAML types are correctly preserved."""

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
        doc = yarutsk.loads("- \"\"\n- ''")
        assert doc[0] == ""
        assert doc[1] == ""
        assert isinstance(doc[0], str)
        assert isinstance(doc[1], str)

    def test_empty_quoted_vs_bare_null(self):
        """Bare empty value and ~ are null; quoted empty is an empty string."""
        doc = yarutsk.loads('bare:\nnull_tilde: ~\nquoted: ""')
        assert doc["bare"] is None
        assert doc["null_tilde"] is None
        assert doc["quoted"] == ""

    def test_empty_quoted_round_trips(self):
        """Empty quoted string survives a dump/load cycle as an empty string."""
        doc = yarutsk.loads("a: \"\"\nb: ''")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["a"] == ""
        assert doc2["b"] == ""


class TestInsertionOrderPreservation:
    """Test that insertion order is preserved."""

    def test_order_preserved_on_load(self):
        """Keys appear in same order as input YAML."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        assert list(doc.keys()) == ["z", "a", "m"]

    def test_order_preserved_on_insert(self):
        """New keys appended at end."""
        content = io.StringIO("a: 1\nb: 2")
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
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        doc["b"] = 4
        output = io.StringIO()
        yarutsk.dump(doc, output)
        result = output.getvalue()
        assert result.index("z:") < result.index("a:")
        assert result.index("a:") < result.index("m:")
        assert result.index("m:") < result.index("b:")
