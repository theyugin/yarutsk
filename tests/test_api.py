"""Tests for the Python dict/list API surface: loads/dumps, to_dict, repr,
protocol compliance, sequence methods, negative indices, setdefault, errors, get."""

import io

import pytest

try:
    import yarutsk

    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

pytestmark = pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk module not built")


class TestStringAPI:
    """Test loads/dumps string-based API."""

    def test_loads_basic(self):
        doc = yarutsk.loads("name: John\nage: 30")
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_loads_empty(self):
        assert yarutsk.loads("") is None

    def test_loads_returns_first_doc(self):
        doc = yarutsk.loads("---\na: 1\n---\nb: 2")
        assert doc["a"] == 1

    def test_loads_all_basic(self):
        docs = yarutsk.loads_all("---\na: 1\n---\nb: 2")
        assert len(docs) == 2
        assert docs[0]["a"] == 1
        assert docs[1]["b"] == 2

    def test_loads_all_empty(self):
        assert yarutsk.loads_all("") == []

    def test_dumps_basic(self):
        doc = yarutsk.loads("name: John\nage: 30")
        result = yarutsk.dumps(doc)
        assert isinstance(result, str)
        assert "name: John" in result
        assert "age: 30" in result

    def test_dumps_preserves_comments(self):
        doc = yarutsk.loads("key: val  # note")
        result = yarutsk.dumps(doc)
        assert "# note" in result

    def test_dumps_all_basic(self):
        docs = yarutsk.loads_all("---\na: 1\n---\nb: 2")
        result = yarutsk.dumps_all(docs)
        assert isinstance(result, str)
        assert "---" in result
        assert "a: 1" in result
        assert "b: 2" in result

    def test_dumps_all_single_no_separator(self):
        docs = yarutsk.loads_all("x: 42")
        result = yarutsk.dumps_all(docs)
        assert "---" not in result

    def test_loads_dumps_round_trip(self):
        original = "name: Alice\nage: 30  # years\ncity: Berlin"
        doc = yarutsk.loads(original)
        result = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(result)
        assert doc2["name"] == "Alice"
        assert doc2["age"] == 30
        assert doc2.comment_inline("age") == "years"

    def test_loads_all_dumps_all_round_trip(self):
        original = "---\na: 1\n---\nb: 2\n---\nc: 3"
        docs = yarutsk.loads_all(original)
        result = yarutsk.dumps_all(docs)
        docs2 = yarutsk.loads_all(result)
        assert len(docs2) == 3
        assert docs2[0]["a"] == 1
        assert docs2[1]["b"] == 2
        assert docs2[2]["c"] == 3

    def test_loads_is_equivalent_to_load(self):
        yaml = "x: 1\ny: 2"
        doc_stream = yarutsk.load(io.StringIO(yaml))
        doc_str = yarutsk.loads(yaml)
        assert repr(doc_stream) == repr(doc_str)


class TestToDict:
    """Test to_dict conversion."""

    def test_to_dict_simple(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        d = doc.to_dict()
        assert d == {"name": "John", "age": 30}

    def test_to_dict_nested(self):
        content = io.StringIO("""
person:
  name: John
  age: 30
""")
        doc = yarutsk.load(content)
        d = doc.to_dict()
        assert d == {"person": {"name": "John", "age": 30}}


class TestRepr:
    """Test __repr__ functionality."""

    def test_repr_mapping(self):
        content = io.StringIO("a: 1\nb: 2")
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "mapping" in r.lower() or "YAML" in r

    def test_repr_sequence(self):
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "sequence" in r.lower() or "YAML" in r


class TestContains:
    """Test __contains__ functionality."""

    def test_contains_existing_key(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert "name" in doc
        assert "age" in doc

    def test_contains_missing_key(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        assert "missing" not in doc


class TestDictProtocol:
    """Dict/list unpacking and protocol compliance."""

    def test_dict_unpacking(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        d = {**doc}
        assert d == {"a": 1, "b": 2}

    def test_dict_constructor(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        d = dict(doc)
        assert d["a"] == 1
        assert d["b"] == 2

    def test_list_unpacking(self):
        doc = yarutsk.loads("- 1\n- 2\n- 3")
        lst = [*doc]
        assert lst == [1, 2, 3]

    def test_list_constructor(self):
        doc = yarutsk.loads("- x\n- y")
        lst = list(doc)
        assert lst == ["x", "y"]

    def test_isinstance_dict(self):
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, dict)

    def test_isinstance_list(self):
        doc = yarutsk.loads("- a")
        assert isinstance(doc, list)

    def test_mapping_values(self):
        doc = yarutsk.loads("a: 1\nb: 2\nc: 3")
        vals = list(doc.values())
        assert sorted(vals) == [1, 2, 3]

    def test_mapping_items(self):
        doc = yarutsk.loads("x: 10\ny: 20")
        items = dict(doc.items())
        assert items == {"x": 10, "y": 20}

    def test_sequence_iteration(self):
        doc = yarutsk.loads("- 1\n- 2\n- 3")
        total = sum(doc)
        assert total == 6


class TestNestedObjectIdentity:
    """Mutations to nested objects must be visible through the parent."""

    def test_nested_mutation_visible_via_parent(self):
        doc = yarutsk.loads("server:\n  host: localhost\n  port: 5432")
        server = doc["server"]
        server["host"] = "remote"
        assert doc["server"]["host"] == "remote"

    def test_nested_mutation_survives_dump(self):
        doc = yarutsk.loads("db:\n  name: mydb\n  port: 5432")
        doc["db"]["port"] = 9999
        out = yarutsk.dumps(doc)
        assert "9999" in out
        doc2 = yarutsk.loads(out)
        assert doc2["db"]["port"] == 9999

    def test_deeply_nested_mutation_visible(self):
        doc = yarutsk.loads("a:\n  b:\n    c: original")
        doc["a"]["b"]["c"] = "changed"
        out = yarutsk.dumps(doc)
        assert "changed" in out
        assert "original" not in out

    def test_sequence_item_mutation_visible(self):
        doc = yarutsk.loads("items:\n  - x: 1\n  - x: 2")
        item = doc["items"][0]
        item["x"] = 99
        assert doc["items"][0]["x"] == 99

    def test_two_references_same_object(self):
        doc = yarutsk.loads("cfg:\n  val: 0")
        ref1 = doc["cfg"]
        ref2 = doc["cfg"]
        assert ref1 is ref2


class TestSequenceListMethods:
    """count(), index(), extend(), and friends on YamlSequence."""

    def test_count(self):
        doc = yarutsk.loads("- a\n- b\n- a\n- c\n- a")
        assert doc.count("a") == 3
        assert doc.count("b") == 1
        assert doc.count("missing") == 0

    def test_index(self):
        doc = yarutsk.loads("- x\n- y\n- z")
        assert doc.index("y") == 1

    def test_index_with_bounds(self):
        doc = yarutsk.loads("- a\n- b\n- c\n- b")
        assert doc.index("b", 2) == 3

    def test_index_missing_raises(self):
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(ValueError):
            doc.index("missing")

    def test_extend_appends_all(self):
        doc = yarutsk.loads("- a\n- b")
        doc.extend(["c", "d"])
        assert len(doc) == 4
        assert doc[2] == "c"
        assert doc[3] == "d"

    def test_extend_empty_no_change(self):
        doc = yarutsk.loads("- a\n- b")
        doc.extend([])
        assert len(doc) == 2

    def test_remove(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.remove("b")
        assert len(doc) == 2
        assert list(doc) == ["a", "c"]

    def test_mixed_types_in_sequence(self):
        doc = yarutsk.loads("- 1\n- hello\n- true\n- null\n- 3.14")
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is True
        assert doc[3] is None
        assert doc[4] == pytest.approx(3.14)

    def test_contains_in_sequence(self):
        doc = yarutsk.loads("- foo\n- bar")
        assert "foo" in doc
        assert "baz" not in doc


class TestNegativeSequenceIndices:
    """Negative indices on YamlSequence should work like Python lists."""

    def test_getitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        assert doc[-1] == "c"
        assert doc[-2] == "b"
        assert doc[-3] == "a"

    def test_setitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc[-1] = "z"
        assert doc[2] == "z"

    def test_delitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        del doc[-1]
        assert len(doc) == 2
        assert doc[-1] == "b"

    def test_pop_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        val = doc.pop(-1)
        assert val == "c"
        assert len(doc) == 2

    def test_get_comment_negative_index(self):
        doc = yarutsk.loads("- a  # first\n- b\n- c  # last")
        assert doc.comment_inline(-1) == "last"
        assert doc.comment_inline(-3) == "first"

    def test_set_comment_negative_index(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.comment_inline(-1, "tail note")
        out = yarutsk.dumps(doc)
        assert "# tail note" in out
        doc2 = yarutsk.loads(out)
        assert doc2.comment_inline(2) == "tail note"


class TestSetDefault:
    """setdefault() return value and no-op behaviour."""

    def test_setdefault_missing_key_inserts(self):
        doc = yarutsk.loads("a: 1")
        result = doc.setdefault("b", "default")
        assert result == "default"
        assert doc["b"] == "default"

    def test_setdefault_existing_key_returns_current(self):
        doc = yarutsk.loads("a: 1")
        result = doc.setdefault("a", 99)
        assert result == 1
        assert doc["a"] == 1

    def test_setdefault_existing_none_returns_none(self):
        doc = yarutsk.loads("a: null")
        result = doc.setdefault("a", "fallback")
        assert result is None
        assert doc["a"] is None

    def test_setdefault_preserves_order(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        doc.setdefault("c", 3)
        assert list(doc.keys()) == ["a", "b", "c"]


class TestErrorCases:
    """KeyError / IndexError and safe-fallback behaviour."""

    def test_del_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            del doc["missing"]

    def test_pop_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            doc.pop("missing")

    def test_pop_missing_key_with_default(self):
        doc = yarutsk.loads("a: 1")
        result = doc.pop("missing", "fallback")
        assert result == "fallback"

    def test_getitem_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            _ = doc["missing"]

    def test_getitem_out_of_range_raises(self):
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(IndexError):
            _ = doc[5]

    def test_delitem_out_of_range_raises(self):
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(IndexError):
            del doc[5]

    def test_comment_inline_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            doc.comment_inline("missing", "note")

    def test_comment_before_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            doc.comment_before("missing", "note")

    def test_comment_inline_missing_key_returns_none(self):
        doc = yarutsk.loads("a: 1")
        assert doc.comment_inline("missing") is None

    def test_comment_before_missing_key_returns_none(self):
        doc = yarutsk.loads("a: 1")
        assert doc.comment_before("missing") is None


class TestGetMethod:
    """YamlMapping.get() edge cases."""

    def test_get_existing_key(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        assert doc.get("a") == 1

    def test_get_missing_key_default_none(self):
        doc = yarutsk.loads("a: 1")
        assert doc.get("missing") is None

    def test_get_missing_key_custom_default(self):
        doc = yarutsk.loads("a: 1")
        assert doc.get("missing", 42) == 42

    def test_get_key_with_none_value(self):
        doc = yarutsk.loads("a: null")
        assert doc.get("a") is None
        assert doc.get("a", "default") is None


class TestTagCoercion:
    """Standard schema tags coerce the Python type returned."""

    def test_float_tag_on_integer_literal(self):
        doc = yarutsk.loads("x: !!float 1\n")
        assert isinstance(doc["x"], float)
        assert doc["x"] == 1.0

    def test_float_tag_on_float_literal(self):
        doc = yarutsk.loads("x: !!float 1.5\n")
        assert isinstance(doc["x"], float)
        assert doc["x"] == 1.5

    def test_int_tag_on_plain_int(self):
        doc = yarutsk.loads("x: !!int 42\n")
        assert isinstance(doc["x"], int)
        assert doc["x"] == 42

    def test_bool_tag_on_plain_bool(self):
        doc = yarutsk.loads("x: !!bool true\n")
        assert doc["x"] is True

    def test_null_tag_on_quoted_empty(self):
        doc = yarutsk.loads('x: !!null ""\n')
        assert doc["x"] is None

    def test_null_tag_on_plain_value(self):
        doc = yarutsk.loads("x: !!null something\n")
        assert doc["x"] is None

    def test_int_tag_invalid_falls_back(self):
        # !!int on a non-integer value — graceful fallback, not an error
        doc = yarutsk.loads("x: !!int abc\n")
        # value is preserved as-is (str) since parse failed
        assert doc["x"] is not None
