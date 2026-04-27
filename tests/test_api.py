"""Tests for the Python dict/list API surface: loads/dumps, to_python, repr,
protocol compliance, sequence methods, negative indices, setdefault, errors, get."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestStringAPI:
    def test_loads_basic(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_loads_empty(self) -> None:
        assert yarutsk.loads("") is None

    def test_loads_returns_first_doc(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] == 1

    def test_loads_all_basic(self) -> None:
        docs = yarutsk.loads_all(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2

    def test_loads_all_empty(self) -> None:
        assert yarutsk.loads_all("") == []

    def test_dumps_basic(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        assert doc is not None
        result = yarutsk.dumps(doc)
        assert isinstance(result, str)
        assert "name: John" in result
        assert "age: 30" in result

    def test_dumps_preserves_comments(self) -> None:
        doc = yarutsk.loads("key: val  # note")
        assert doc is not None
        result = yarutsk.dumps(doc)
        assert "# note" in result

    def test_dumps_all_basic(self) -> None:
        docs = yarutsk.loads_all(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        result = yarutsk.dumps_all(docs)
        assert isinstance(result, str)
        assert "---" in result
        assert "a: 1" in result
        assert "b: 2" in result

    def test_dumps_all_single_no_separator(self) -> None:
        docs = yarutsk.loads_all("x: 42")
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        result = yarutsk.dumps_all(docs)
        assert "---" not in result

    def test_loads_dumps_round_trip(self) -> None:
        original = dedent("""\
            name: Alice
            age: 30  # years
            city: Berlin
        """)
        doc = yarutsk.loads(original)
        assert doc is not None
        result = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(result)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["name"] == "Alice"
        assert doc2["age"] == 30
        assert doc2.node("age").comment_inline == "years"

    def test_loads_all_dumps_all_round_trip(self) -> None:
        original = dedent("""\
            ---
            a: 1
            ---
            b: 2
            ---
            c: 3
        """)
        docs = yarutsk.loads_all(original)
        result = yarutsk.dumps_all(docs)
        docs2 = yarutsk.loads_all(result)
        assert len(docs2) == 3
        d0, d1, d2 = docs2
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert isinstance(d2, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2
        assert d2["c"] == 3

    def test_loads_is_equivalent_to_load(self) -> None:
        yaml = dedent("""\
            x: 1
            y: 2
        """)
        doc_stream = yarutsk.load(io.StringIO(yaml))
        assert doc_stream is not None
        doc_str = yarutsk.loads(yaml)
        assert doc_str is not None
        assert repr(doc_stream) == repr(doc_str)


class TestToPython:
    def test_to_python_simple(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        assert doc is not None
        d = doc.to_python()
        assert d == {"name": "John", "age": 30}

    def test_to_python_nested(self) -> None:
        doc = yarutsk.loads("""
person:
  name: John
  age: 30
""")
        assert doc is not None
        d = doc.to_python()
        assert d == {"person": {"name": "John", "age": 30}}

    def test_to_python_recurses_into_every_level(self) -> None:
        # Critical post-C2: container children live as `Opaque(Py<…>)` in
        # `inner`, so `to_python()` MUST recurse through them — otherwise
        # `json.dumps(doc.to_python())` fails on the nested PyYamlMapping/
        # PyYamlSequence that are no longer dict/list subclasses.
        src = dedent("""\
            top:
              nested_map:
                inner_key: inner_val
                deep:
                  deeper: 42
              nested_seq:
                - 1
                - {x: 2, y: 3}
                - [a, b, [c, d]]
            also: bare
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        actual = doc.to_python()
        expected = {
            "top": {
                "nested_map": {
                    "inner_key": "inner_val",
                    "deep": {"deeper": 42},
                },
                "nested_seq": [
                    1,
                    {"x": 2, "y": 3},
                    ["a", "b", ["c", "d"]],
                ],
            },
            "also": "bare",
        }
        # Hard assertion: the result is plain Python (no yarutsk types
        # anywhere) and equals a hand-constructed dict/list literal.
        assert actual == expected
        # Spot-check: every container along the path is a plain `dict`/`list`,
        # not a yarutsk wrapper.
        assert isinstance(actual, dict)
        assert isinstance(actual["top"], dict)
        assert isinstance(actual["top"]["nested_map"], dict)
        assert isinstance(actual["top"]["nested_map"]["deep"], dict)
        assert isinstance(actual["top"]["nested_seq"], list)
        assert isinstance(actual["top"]["nested_seq"][1], dict)
        assert isinstance(actual["top"]["nested_seq"][2], list)
        assert isinstance(actual["top"]["nested_seq"][2][2], list)


class TestRepr:
    def test_repr_mapping(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert doc is not None
        r = repr(doc)
        assert "mapping" in r.lower() or "YAML" in r

    def test_repr_sequence(self) -> None:
        doc = yarutsk.loads("[a, b, c]")
        assert doc is not None
        r = repr(doc)
        assert "sequence" in r.lower() or "YAML" in r


class TestContains:
    def test_contains_existing_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "name" in doc
        assert "age" in doc

    def test_contains_missing_key(self) -> None:
        doc = yarutsk.loads("name: John")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "missing" not in doc


class TestDictProtocol:
    """Dict/list unpacking and protocol compliance."""

    def test_dict_unpacking(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        d = {**doc}
        assert d == {"a": 1, "b": 2}

    def test_dict_constructor(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        d = dict(doc)
        assert d["a"] == 1
        assert d["b"] == 2

    def test_list_unpacking(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        lst = [*doc]
        assert lst == [1, 2, 3]

    def test_list_constructor(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - x
            - y
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        lst = list(doc)
        assert lst == ["x", "y"]

    def test_not_isinstance_dict(self) -> None:
        # Standalone pyclass — `YamlMapping` deliberately does *not* extend
        # `dict`. Use `to_python()` to get a plain `dict` for interop with
        # APIs that need it (e.g. `json.dumps`).
        doc = yarutsk.loads("a: 1")
        assert doc is not None
        assert not isinstance(doc, dict)
        assert isinstance(doc, yarutsk.YamlMapping)

    def test_not_isinstance_list(self) -> None:
        doc = yarutsk.loads("- a")
        assert doc is not None
        assert not isinstance(doc, list)
        assert isinstance(doc, yarutsk.YamlSequence)

    def test_mapping_values(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
            c: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        vals = list(doc.values())
        assert sorted(vals) == [1, 2, 3]

    def test_mapping_items(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            x: 10
            y: 20
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        items = dict(doc.items())
        assert items == {"x": 10, "y": 20}

    def test_sequence_iteration(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        total = sum(doc)
        assert total == 6


class TestNestedObjectIdentity:
    """Mutations to nested objects must be visible through the parent."""

    def test_nested_mutation_visible_via_parent(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            server:
              host: localhost
              port: 5432
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        server = doc["server"]
        server["host"] = "remote"
        assert doc["server"]["host"] == "remote"

    def test_nested_mutation_survives_dump(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            db:
              name: mydb
              port: 5432
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["db"]["port"] = 9999
        out = yarutsk.dumps(doc)
        assert "9999" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["db"]["port"] == 9999

    def test_deeply_nested_mutation_visible(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              b:
                c: original
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["a"]["b"]["c"] = "changed"
        out = yarutsk.dumps(doc)
        assert "changed" in out
        assert "original" not in out

    def test_sequence_item_mutation_visible(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            items:
              - x: 1
              - x: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        item = doc["items"][0]
        item["x"] = 99
        assert doc["items"][0]["x"] == 99

    def test_two_references_same_object(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            cfg:
              val: 0
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        ref1 = doc["cfg"]
        ref2 = doc["cfg"]
        assert ref1 is ref2


class TestSequenceListMethods:
    """count(), index(), extend(), and friends on YamlSequence."""

    def test_count(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - a
            - c
            - a
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.count("a") == 3
        assert doc.count("b") == 1
        assert doc.count("missing") == 0

    def test_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - x
            - y
            - z
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.index("y") == 1

    def test_index_with_bounds(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.index("b", 2) == 3

    def test_index_missing_raises(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(ValueError):
            doc.index("missing")

    def test_extend_appends_all(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.extend(["c", "d"])
        assert len(doc) == 4
        assert doc[2] == "c"
        assert doc[3] == "d"

    def test_extend_empty_no_change(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.extend([])
        assert len(doc) == 2

    def test_remove(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.remove("b")
        assert len(doc) == 2
        assert list(doc) == ["a", "c"]

    def test_mixed_types_in_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - hello
            - true
            - null
            - 3.14
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is True
        assert doc[3] is None
        assert doc[4] == pytest.approx(3.14)

    def test_contains_in_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - foo
            - bar
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert "foo" in doc
        assert "baz" not in doc


class TestNegativeSequenceIndices:
    """Negative indices on YamlSequence should work like Python lists."""

    def test_getitem_negative(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[-1] == "c"
        assert doc[-2] == "b"
        assert doc[-3] == "a"

    def test_setitem_negative(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc[-1] = "z"
        assert doc[2] == "z"

    def test_delitem_negative(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        del doc[-1]
        assert len(doc) == 2
        assert doc[-1] == "b"

    def test_pop_negative(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        val = doc.pop(-1)
        assert val == "c"
        assert len(doc) == 2

    def test_get_comment_negative_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c  # last
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(-1).comment_inline == "last"
        assert doc.node(-3).comment_inline == "first"

    def test_set_comment_negative_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(-1).comment_inline = "tail note"
        out = yarutsk.dumps(doc)
        assert "# tail note" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert doc2.node(2).comment_inline == "tail note"


class TestSetDefault:
    """setdefault() return value and no-op behaviour."""

    def test_setdefault_missing_key_inserts(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        result = doc.setdefault("b", "default")
        assert result == "default"
        assert doc["b"] == "default"

    def test_setdefault_existing_key_returns_current(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        result = doc.setdefault("a", 99)
        assert result == 1
        assert doc["a"] == 1

    def test_setdefault_existing_none_returns_none(self) -> None:
        doc = yarutsk.loads("a: null")
        assert isinstance(doc, yarutsk.YamlMapping)
        result = doc.setdefault("a", "fallback")
        assert result is None
        assert doc["a"] is None

    def test_setdefault_preserves_order(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.setdefault("c", 3)
        assert list(doc.keys()) == ["a", "b", "c"]


class TestErrorCases:
    """KeyError / IndexError and safe-fallback behaviour."""

    def test_del_missing_key_raises(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            del doc["missing"]

    def test_pop_missing_key_raises(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.pop("missing")

    def test_pop_missing_key_with_default(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        result = doc.pop("missing", "fallback")
        assert result == "fallback"

    def test_getitem_missing_key_raises(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            _ = doc["missing"]

    def test_getitem_out_of_range_raises(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            _ = doc[5]

    def test_delitem_out_of_range_raises(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            del doc[5]

    def test_comment_inline_missing_key_raises(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.node("missing").comment_inline = "note"

    def test_comment_before_missing_key_raises(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.node("missing").comment_before = "note"


class TestGetMethod:
    """YamlMapping.get() edge cases."""

    def test_get_existing_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get("a") == 1

    def test_get_missing_key_default_none(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get("missing") is None

    def test_get_missing_key_custom_default(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get("missing", 42) == 42

    def test_get_key_with_none_value(self) -> None:
        doc = yarutsk.loads("a: null")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get("a") is None
        assert doc.get("a", "default") is None


class TestTagCoercion:
    """Standard schema tags coerce the Python type returned."""

    def test_float_tag_on_integer_literal(self) -> None:
        doc = yarutsk.loads("x: !!float 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["x"], float)
        assert doc["x"] == 1.0

    def test_float_tag_on_float_literal(self) -> None:
        doc = yarutsk.loads("x: !!float 1.5\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["x"], float)
        assert doc["x"] == 1.5

    def test_int_tag_on_plain_int(self) -> None:
        doc = yarutsk.loads("x: !!int 42\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["x"], int)
        assert doc["x"] == 42

    def test_bool_tag_on_plain_bool(self) -> None:
        doc = yarutsk.loads("x: !!bool true\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is True

    def test_null_tag_on_quoted_empty(self) -> None:
        doc = yarutsk.loads('x: !!null ""\n')
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is None

    def test_null_tag_on_plain_value(self) -> None:
        doc = yarutsk.loads("x: !!null something\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is None

    def test_int_tag_invalid_falls_back(self) -> None:
        # !!int on a non-integer value — graceful fallback, not an error
        doc = yarutsk.loads("x: !!int abc\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        # value is preserved as-is (str) since parse failed
        assert doc["x"] is not None


class TestSequenceScalarStyle:
    """scalar_style() on YamlSequence."""

    def test_set_single_quoted(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - hello
            - world
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).style = "single"
        out = yarutsk.dumps(doc)
        assert "'hello'" in out
        assert "world" in out

    def test_set_double_quoted(self) -> None:
        doc = yarutsk.loads("- hello\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).style = "double"
        assert yarutsk.dumps(doc) == '- "hello"\n'

    def test_negative_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(-1).style = "single"
        assert yarutsk.dumps(doc) == dedent("""\
            - a
            - b
            - 'c'
        """)

    def test_only_target_item_changes(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 'a'
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(1).style = "double"
        out = yarutsk.dumps(doc)
        assert "'a'" in out
        assert '"b"' in out

    def test_invalid_style_raises(self) -> None:
        doc = yarutsk.loads("- hello\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(ValueError):
            doc.node(0).style = "bad"

    def test_out_of_range_raises(self) -> None:
        doc = yarutsk.loads("- hello\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.node(99).style = "plain"


class TestStyledConstructors:
    """Tests for YamlScalar/YamlMapping/YamlSequence constructors with style/tag."""

    def test_scalar_default_style(self) -> None:
        s = yarutsk.YamlScalar("hello")
        assert s.style == "plain"
        assert s.value == "hello"
        assert s.tag is None

    def test_scalar_double_style(self) -> None:
        s = yarutsk.YamlScalar("hello", style="double")
        assert s.style == "double"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        assert yarutsk.dumps(doc) == 'k: "hello"\n'

    def test_scalar_single_style(self) -> None:
        doc = yarutsk.YamlMapping()
        doc["k"] = yarutsk.YamlScalar("world", style="single")
        assert yarutsk.dumps(doc) == "k: 'world'\n"

    def test_scalar_literal_style(self) -> None:
        doc = yarutsk.YamlMapping()
        doc["k"] = yarutsk.YamlScalar("line1\nline2\n", style="literal")
        out = yarutsk.dumps(doc)
        assert "|-" in out or "|" in out

    def test_scalar_with_tag(self) -> None:
        s = yarutsk.YamlScalar("42", tag="!myint")
        assert s.tag == "!myint"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        out = yarutsk.dumps(doc)
        assert "!myint" in out
        assert "42" in out

    def test_scalar_int_value(self) -> None:
        s = yarutsk.YamlScalar(42, style="plain")
        assert s.value == 42

    def test_scalar_bool_value(self) -> None:
        s = yarutsk.YamlScalar(True)
        assert s.value is True

    def test_scalar_none_value(self) -> None:
        s = yarutsk.YamlScalar(None)
        assert s.value is None

    def test_scalar_rejects_non_primitive(self) -> None:
        with pytest.raises(TypeError):
            yarutsk.YamlScalar([1, 2, 3])  # type: ignore[arg-type]

    def test_scalar_rejects_unknown_style(self) -> None:
        with pytest.raises(ValueError):
            yarutsk.YamlScalar("x", style="notathing")  # type: ignore[arg-type]

    def test_scalar_assignment_preserves_style(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: old
            b: other
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["a"] = yarutsk.YamlScalar("new", style="double")
        out = yarutsk.dumps(doc)
        assert 'a: "new"' in out

    def test_mapping_default_style(self) -> None:
        m = yarutsk.YamlMapping()
        assert m.style == "block"
        assert m.tag is None

    def test_mapping_flow_style(self) -> None:
        m = yarutsk.YamlMapping(style="flow")
        assert m.style == "flow"
        m["x"] = 1
        m["y"] = 2
        doc = yarutsk.YamlMapping()
        doc["point"] = m
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_mapping_with_tag(self) -> None:
        m = yarutsk.YamlMapping(tag="!mymap")
        assert m.tag == "!mymap"

    def test_mapping_rejects_unknown_style(self) -> None:
        with pytest.raises(ValueError):
            yarutsk.YamlMapping(style="notathing")  # type: ignore[arg-type]

    def test_mapping_assignment_preserves_style(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            outer:
              a: 1
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        nested = yarutsk.YamlMapping(style="flow")
        nested["a"] = 1
        doc["outer"] = nested
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_sequence_default_style(self) -> None:
        s = yarutsk.YamlSequence()
        assert s.style == "block"
        assert s.tag is None

    def test_sequence_flow_style(self) -> None:
        s = yarutsk.YamlSequence(style="flow")
        assert s.style == "flow"
        s.extend([1, 2, 3])
        doc = yarutsk.YamlMapping()
        doc["nums"] = s
        out = yarutsk.dumps(doc)
        assert "[" in out

    def test_sequence_with_tag(self) -> None:
        s = yarutsk.YamlSequence(tag="!myseq")
        assert s.tag == "!myseq"

    def test_sequence_rejects_unknown_style(self) -> None:
        with pytest.raises(ValueError):
            yarutsk.YamlSequence(style="notathing")  # type: ignore[arg-type]

    def test_dumper_returns_styled_scalar(self) -> None:
        class MyVal:
            def __init__(self, v: str) -> None:
                self.v = v

        schema = yarutsk.Schema()
        schema.add_dumper(
            MyVal,
            lambda obj: ("!myval", yarutsk.YamlScalar(str(obj.v), style="double")),
        )
        doc = yarutsk.YamlMapping()
        doc["k"] = MyVal("hello")
        out = yarutsk.dumps(doc, schema=schema)
        assert '!myval "hello"' in out

    def test_dumper_returns_styled_mapping(self) -> None:
        class MyPoint:
            def __init__(self, x: int, y: int) -> None:
                self.x, self.y = x, y

        def dump_point(p: MyPoint) -> tuple[str, yarutsk.YamlMapping]:
            m = yarutsk.YamlMapping(style="flow")
            m["x"] = p.x
            m["y"] = p.y
            return ("!point", m)

        schema = yarutsk.Schema()
        schema.add_dumper(MyPoint, dump_point)
        doc = yarutsk.YamlMapping()
        doc["p"] = MyPoint(1, 2)
        out = yarutsk.dumps(doc, schema=schema)
        assert "!point" in out
        assert "{" in out

    def test_dumper_returns_styled_sequence(self) -> None:
        class MyList:
            def __init__(self, items: list[int]) -> None:
                self.items = items

        def dump_list(obj: MyList) -> tuple[str, yarutsk.YamlSequence]:
            s = yarutsk.YamlSequence(style="flow")
            s.extend(obj.items)
            return ("!mylist", s)

        schema = yarutsk.Schema()
        schema.add_dumper(MyList, dump_list)
        doc = yarutsk.YamlMapping()
        doc["l"] = MyList([1, 2, 3])
        out = yarutsk.dumps(doc, schema=schema)
        assert "!mylist" in out


class TestFormat:
    """Tests for the format() method that resets cosmetic formatting to YAML defaults."""

    def test_scalar_style_reset(self) -> None:
        doc = yarutsk.loads("key: 'single'")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format()
        assert doc.node("key").style == "plain"

    def test_container_style_reset(self) -> None:
        doc = yarutsk.loads("nested: {a: 1}")
        assert doc is not None
        doc.format()
        assert "{" not in yarutsk.dumps(doc)

    def test_comments_cleared(self) -> None:
        src = dedent("""\
            # comment
            key: val  # inline
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format()
        assert doc.node("key").comment_before is None
        assert doc.node("key").comment_inline is None

    def test_blank_lines_cleared(self) -> None:
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format()
        assert doc.node("b").blank_lines_before == 0

    def test_tags_preserved(self) -> None:
        doc = yarutsk.loads("value: !!str 42")
        assert doc is not None
        doc.format()
        assert "!!str" in yarutsk.dumps(doc)

    def test_recursive_nested_mapping(self) -> None:
        src = dedent("""\
            outer:
              inner: 'quoted'
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.format()
        assert "'" not in yarutsk.dumps(doc)

    def test_sequence_items_formatted(self) -> None:
        doc = yarutsk.loads("items: ['a', 'b']")
        assert doc is not None
        doc.format()
        result = yarutsk.dumps(doc)
        assert "[" not in result
        assert "'" not in result

    def test_styles_false_preserves_scalar_style(self) -> None:
        doc = yarutsk.loads("key: 'single'")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format(styles=False)
        assert doc.node("key").style == "single"

    def test_styles_false_preserves_container_style(self) -> None:
        doc = yarutsk.loads("nested: {a: 1}")
        assert doc is not None
        doc.format(styles=False)
        assert "{" in yarutsk.dumps(doc)

    def test_comments_false_preserves_comments(self) -> None:
        doc = yarutsk.loads("key: val  # inline")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format(comments=False)
        assert doc.node("key").comment_inline == "inline"

    def test_blank_lines_false_preserves_blank_lines(self) -> None:
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format(blank_lines=False)
        assert doc.node("b").blank_lines_before > 0

    def test_multiline_string_uses_literal_style(self) -> None:
        # A multiline string should become literal block style, not double-quoted with \n
        src = dedent("""\
            message: |
              line1
              line2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        # Force it to double-quoted so format() has something to reset
        doc.node("message").style = "double"
        doc.format()
        result = yarutsk.dumps(doc)
        assert "\\n" not in result
        assert "|" in result

    def test_roundtrip_clean(self) -> None:
        src = dedent("""\
            # Config
            server:
              host: 'localhost'  # inline
              port: 8080

              debug: true
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.format()
        result = yarutsk.dumps(doc)
        assert "#" not in result
        assert "'" not in result
        assert "\n\n" not in result

    def test_yaml_scalar_format(self) -> None:
        s = yarutsk.YamlScalar("hello", style="double")
        s.format()
        assert s.style == "plain"

    def test_yaml_scalar_format_preserves_tag(self) -> None:
        s = yarutsk.YamlScalar("42", style="single", tag="!!str")
        s.format()
        assert s.tag == "!!str"
        assert s.style == "plain"

    def test_sequence_blank_lines_cleared(self) -> None:
        src = dedent("""\
            items:
              - a

              - b
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        seq = doc["items"]
        doc.format()
        assert seq.node(1).blank_lines_before == 0

    def test_trailing_blank_lines_cleared(self) -> None:
        doc = yarutsk.loads("a: 1\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.trailing_blank_lines = 3
        doc.format()
        assert doc.trailing_blank_lines == 0


class TestIndentParameter:
    def test_dumps_indent_4(self) -> None:
        doc = yarutsk.loads("outer:\n  inner: 1\n")
        assert doc is not None
        out = yarutsk.dumps(doc, indent=4)
        assert "    inner: 1" in out

    def test_dumps_indent_1(self) -> None:
        doc = yarutsk.loads("outer:\n  inner: 1\n")
        assert doc is not None
        out = yarutsk.dumps(doc, indent=1)
        assert " inner: 1" in out

    def test_dumps_all_indent(self) -> None:
        docs = yarutsk.loads_all("a: 1\n---\nb: 2\n")
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        out = yarutsk.dumps_all(docs, indent=4)
        assert "    " not in out  # top-level keys are not indented
        assert "a: 1" in out
        assert "b: 2" in out


class TestYamlVersionAndTagDirectives:
    def test_yaml_version_roundtrip(self) -> None:
        src = "%YAML 1.1\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        assert doc.yaml_version == "1.1"
        assert yarutsk.dumps(doc) == src

    def test_yaml_version_set(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert doc is not None
        doc.yaml_version = "1.2"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.2" in out
        assert "---" in out

    def test_yaml_version_clear(self) -> None:
        src = "%YAML 1.1\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.yaml_version = None
        out = yarutsk.dumps(doc)
        assert "%YAML" not in out

    def test_tag_directives_roundtrip(self) -> None:
        src = "%TAG ! tag:example.com,2024:\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        assert doc.tag_directives == [("!", "tag:example.com,2024:")]
        assert yarutsk.dumps(doc) == src

    def test_tag_directives_set(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert doc is not None
        doc.explicit_start = True
        doc.tag_directives = [("!e!", "tag:example.com,2024:")]
        out = yarutsk.dumps(doc)
        assert "%TAG !e! tag:example.com,2024:" in out

    def test_tag_directives_clear(self) -> None:
        src = "%TAG ! tag:example.com,2024:\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.tag_directives = []
        out = yarutsk.dumps(doc)
        assert "%TAG" not in out

    def test_yaml_version_on_scalar(self) -> None:
        doc = yarutsk.loads("42\n")
        assert isinstance(doc, yarutsk.YamlScalar)
        doc.yaml_version = "1.1"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.1" in out

    def test_yaml_version_on_sequence(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.yaml_version = "1.2"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.2" in out


class TestNodeStyleSetter:
    def test_mapping_scalar_style_on_scalar_child(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").style = "double"
        assert '"value"' in yarutsk.dumps(doc)

    def test_sequence_scalar_style_on_scalar_child(self) -> None:
        doc = yarutsk.loads("- hello\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).style = "single"
        assert "'hello'" in yarutsk.dumps(doc)


class TestStyleGetters:
    """Read node-level style, blank-line, and alias metadata."""

    def test_mapping_get_scalar_style(self) -> None:
        doc = yarutsk.loads("a: 'x'\nb: y\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").style == "single"
        assert doc.node("b").style == "plain"

    def test_mapping_get_container_style(self) -> None:
        doc = yarutsk.loads("a: {x: 1}\nb:\n  y: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").style == "flow"
        assert doc.node("b").style == "block"

    def test_mapping_get_blank_lines_before(self) -> None:
        doc = yarutsk.loads("a: 1\n\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").blank_lines_before == 0
        assert doc.node("b").blank_lines_before == 1
        with pytest.raises(KeyError):
            doc.node("missing")

    def test_mapping_get_alias(self) -> None:
        doc = yarutsk.loads("base: &anchor 1\nref: *anchor\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get_alias("ref") == "anchor"
        assert doc.get_alias("base") is None
        with pytest.raises(KeyError):
            doc.get_alias("missing")

    def test_sequence_get_scalar_style(self) -> None:
        doc = yarutsk.loads("- 'x'\n- y\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).style == "single"
        assert doc.node(1).style == "plain"

    def test_sequence_get_container_style(self) -> None:
        doc = yarutsk.loads("- {x: 1}\n- [1, 2]\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).style == "flow"
        assert doc.node(1).style == "flow"

    def test_sequence_get_blank_lines_before(self) -> None:
        doc = yarutsk.loads("- 1\n\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).blank_lines_before == 0
        assert doc.node(1).blank_lines_before == 1


class TestAnchorProperty:
    def test_mapping_anchor_roundtrip(self) -> None:
        src = "base: &anchor\n  x: 1\nalias: *anchor\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        nested = doc.node("base")
        assert nested.anchor == "anchor"

    def test_mapping_set_anchor(self) -> None:
        doc = yarutsk.loads("key:\n  x: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        nested = doc.node("key")
        assert isinstance(nested, yarutsk.YamlMapping)
        nested.anchor = "myanchor"
        assert nested.anchor == "myanchor"

    def test_scalar_anchor_roundtrip(self) -> None:
        src = "value: &val 42\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("value")
        assert isinstance(node, yarutsk.YamlScalar)
        assert node.anchor == "val"

    def test_scalar_set_anchor(self) -> None:
        s = yarutsk.YamlScalar("hello")
        assert s.anchor is None
        s.anchor = "greeting"
        assert s.anchor == "greeting"
        s.anchor = None
        assert s.anchor is None

    def test_sequence_set_anchor(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.anchor = "mylist"
        assert doc.anchor == "mylist"
        out = yarutsk.dumps(doc)
        assert "&mylist" in out

    def test_mapping_set_anchor_emits(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert doc is not None
        doc.anchor = "root"
        out = yarutsk.dumps(doc)
        assert "&root" in out


class TestConstructorFromExisting:
    """YamlMapping(mapping) and YamlSequence(iterable) constructors."""

    def test_mapping_from_plain_dict(self) -> None:
        m = yarutsk.YamlMapping({"a": 1, "b": 2})
        assert m["a"] == 1
        assert m["b"] == 2
        assert list(m.keys()) == ["a", "b"]

    def test_mapping_from_plain_dict_with_style(self) -> None:
        m = yarutsk.YamlMapping({"x": 1}, style="flow")
        assert m.style == "flow"
        assert m["x"] == 1

    def test_mapping_from_yaml_mapping_preserves_values(self) -> None:
        src = yarutsk.loads("x: 1\ny: 2\n")
        assert isinstance(src, yarutsk.YamlMapping)
        m = yarutsk.YamlMapping(src)
        assert m["x"] == 1
        assert m["y"] == 2
        assert list(m.keys()) == ["x", "y"]

    def test_mapping_from_yaml_mapping_with_override_style(self) -> None:
        src = yarutsk.loads("x: 1\ny: 2\n")
        assert isinstance(src, yarutsk.YamlMapping)
        m = yarutsk.YamlMapping(src, style="flow")
        assert m.style == "flow"
        assert m["x"] == 1

    def test_mapping_from_yaml_mapping_preserves_inner_metadata(self) -> None:
        src = yarutsk.loads("x: 1  # inline\ny: 2\n")
        assert isinstance(src, yarutsk.YamlMapping)
        m = yarutsk.YamlMapping(src)
        assert m.node("x").comment_inline == "inline"

    def test_mapping_empty_no_arg(self) -> None:
        m = yarutsk.YamlMapping()
        assert len(m) == 0
        assert m.style == "block"

    def test_mapping_none_arg(self) -> None:
        m = yarutsk.YamlMapping(None)
        assert len(m) == 0

    def test_sequence_constructor_with_list(self) -> None:
        s = yarutsk.YamlSequence([1, 2, 3])
        assert list(s) == [1, 2, 3]

    def test_sequence_constructor_with_style(self) -> None:
        s = yarutsk.YamlSequence([1, 2, 3], style="flow")
        assert s.style == "flow"
        assert list(s) == [1, 2, 3]

    def test_sequence_from_range(self) -> None:
        s = yarutsk.YamlSequence(range(3))
        assert list(s) == [0, 1, 2]

    def test_sequence_from_yaml_sequence_preserves_values(self) -> None:
        src = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src)
        assert list(s) == [1, 2]

    def test_sequence_from_yaml_sequence_with_override_style(self) -> None:
        src = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src, style="flow")
        assert s.style == "flow"
        assert list(s) == [1, 2]

    def test_sequence_from_yaml_sequence_preserves_inner_metadata(self) -> None:
        src = yarutsk.loads("- a  # first\n- b\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src)
        assert s.node(0).comment_inline == "first"

    def test_sequence_empty_no_arg(self) -> None:
        s = yarutsk.YamlSequence()
        assert len(s) == 0
        assert s.style == "block"

    def test_sequence_none_arg(self) -> None:
        s = yarutsk.YamlSequence(None)
        assert len(s) == 0


class TestAliasAPI:
    """Tests for alias_name() and set_alias() on YamlMapping and YamlSequence."""

    def test_mapping_alias_name_none_for_plain_value(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get_alias("key") is None

    def test_mapping_alias_name_parsed_alias(self) -> None:
        doc = yarutsk.loads("base: &anchor 1\nref: *anchor\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get_alias("ref") == "anchor"

    def test_mapping_alias_name_anchor_node_is_not_alias(self) -> None:
        doc = yarutsk.loads("base: &anchor 1\nref: *anchor\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.get_alias("base") is None

    def test_mapping_alias_name_missing_key_raises(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.get_alias("missing")

    def test_mapping_set_alias_marks_value(self) -> None:
        doc = yarutsk.loads("base: &anchor hello\nother: hello\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.set_alias("other", "anchor")
        assert doc.get_alias("other") == "anchor"

    def test_mapping_set_alias_resolved_value_accessible(self) -> None:
        doc = yarutsk.loads("base: &anchor 42\nother: 42\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.set_alias("other", "anchor")
        assert doc["other"] == 42

    def test_mapping_set_alias_emits_star_anchor(self) -> None:
        doc = yarutsk.loads("base: &anchor hello\nother: hello\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.set_alias("other", "anchor")
        out = yarutsk.dumps(doc)
        assert "*anchor" in out

    def test_mapping_set_alias_missing_key_raises(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.set_alias("missing", "anchor")

    def test_sequence_alias_name_none_for_plain_value(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.get_alias(0) is None

    def test_sequence_alias_name_parsed_alias(self) -> None:
        doc = yarutsk.loads("- &val 1\n- *val\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.get_alias(1) == "val"

    def test_sequence_alias_name_anchor_node_is_not_alias(self) -> None:
        doc = yarutsk.loads("- &val 1\n- *val\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.get_alias(0) is None

    def test_sequence_alias_name_out_of_range_raises(self) -> None:
        doc = yarutsk.loads("- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.get_alias(99)

    def test_sequence_set_alias_marks_value(self) -> None:
        doc = yarutsk.loads("- 1\n- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        assert doc.get_alias(1) == "val"

    def test_sequence_set_alias_resolved_value_accessible(self) -> None:
        doc = yarutsk.loads("- 42\n- 42\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        assert doc[1] == 42

    def test_sequence_set_alias_emits_star_anchor(self) -> None:
        doc = yarutsk.loads("- &val 1\n- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        out = yarutsk.dumps(doc)
        assert "*val" in out

    def test_sequence_set_alias_out_of_range_raises(self) -> None:
        doc = yarutsk.loads("- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.set_alias(99, "val")


class TestMappingConstructor:
    def test_basic(self) -> None:
        m = yarutsk.YamlMapping({"a": 1, "b": 2})
        assert isinstance(m, yarutsk.YamlMapping)
        assert m["a"] == 1
        assert m["b"] == 2

    def test_nested_dict_becomes_mapping(self) -> None:
        m = yarutsk.YamlMapping({"x": {"y": 3}})
        assert isinstance(m["x"], yarutsk.YamlMapping)
        assert m["x"]["y"] == 3

    def test_nested_list_becomes_sequence(self) -> None:
        m = yarutsk.YamlMapping({"items": [1, 2, 3]})
        assert isinstance(m["items"], yarutsk.YamlSequence)
        assert list(m["items"]) == [1, 2, 3]

    def test_round_trips(self) -> None:
        m = yarutsk.YamlMapping({"name": "Alice", "age": 30})
        out = yarutsk.dumps(m)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["name"] == "Alice"
        assert doc2["age"] == 30

    def test_non_dict_raises_type_error(self) -> None:
        with pytest.raises(TypeError):
            yarutsk.YamlMapping([1, 2, 3])  # type: ignore[list-item]

    def test_empty(self) -> None:
        m = yarutsk.YamlMapping({})
        assert isinstance(m, yarutsk.YamlMapping)
        assert len(m) == 0


class TestSequenceConstructor:
    def test_basic(self) -> None:
        s = yarutsk.YamlSequence([1, 2, 3])
        assert isinstance(s, yarutsk.YamlSequence)
        assert list(s) == [1, 2, 3]

    def test_nested_dict_becomes_mapping(self) -> None:
        s = yarutsk.YamlSequence([{"a": 1}, {"b": 2}])
        assert isinstance(s[0], yarutsk.YamlMapping)
        assert s[0]["a"] == 1

    def test_nested_list_becomes_sequence(self) -> None:
        s = yarutsk.YamlSequence([[1, 2], [3, 4]])
        assert isinstance(s[0], yarutsk.YamlSequence)
        assert list(s[0]) == [1, 2]

    def test_round_trips(self) -> None:
        s = yarutsk.YamlSequence(["x", "y", "z"])
        out = yarutsk.dumps(s)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert list(doc2) == ["x", "y", "z"]

    def test_non_iterable_raises_type_error(self) -> None:
        with pytest.raises((TypeError, RuntimeError)):
            yarutsk.YamlSequence(42)  # type: ignore[arg-type]

    def test_empty(self) -> None:
        s = yarutsk.YamlSequence([])
        assert isinstance(s, yarutsk.YamlSequence)
        assert len(s) == 0


class TestMappingNodes:
    def test_nodes_returns_list_of_pairs(self) -> None:
        doc = yarutsk.loads("x: 1\ny: hello\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        pairs = doc.nodes()
        assert [k for k, _ in pairs] == ["x", "y"]

    def test_nodes_scalar_values_are_yaml_scalar(self) -> None:
        doc = yarutsk.loads("a: 1\nb: true\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        nodes = {k: v for k, v in doc.nodes()}
        assert isinstance(nodes["a"], yarutsk.YamlScalar)
        assert isinstance(nodes["b"], yarutsk.YamlScalar)

    def test_nodes_nested_mapping_preserved(self) -> None:
        doc = yarutsk.loads("outer:\n  inner: 42\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        nodes = {k: v for k, v in doc.nodes()}
        assert isinstance(nodes["outer"], yarutsk.YamlMapping)
        assert nodes["outer"]["inner"] == 42

    def test_nodes_preserves_scalar_style(self) -> None:
        doc = yarutsk.loads('key: "quoted"\n')
        assert isinstance(doc, yarutsk.YamlMapping)
        nodes = {k: v for k, v in doc.nodes()}
        assert isinstance(nodes["key"], yarutsk.YamlScalar)
        assert nodes["key"].style == "double"

    def test_nodes_empty_mapping(self) -> None:
        doc = yarutsk.loads("{}\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.nodes() == []


class TestSequenceNodes:
    def test_node_returns_typed(self) -> None:
        doc = yarutsk.loads("- 1\n- foo\n- [a, b]\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert isinstance(doc.node(0), yarutsk.YamlScalar)
        assert isinstance(doc.node(2), yarutsk.YamlSequence)

    def test_nodes_list_preserves_order(self) -> None:
        doc = yarutsk.loads("- 1\n- hello\n- true\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        nodes = doc.nodes()
        values = [n.value for n in nodes if isinstance(n, yarutsk.YamlScalar)]
        assert values == [1, "hello", True]

    def test_nodes_preserves_scalar_style(self) -> None:
        doc = yarutsk.loads('- "a"\n- b\n')
        assert isinstance(doc, yarutsk.YamlSequence)
        nodes = doc.nodes()
        assert nodes[0].style == "double"
        assert nodes[1].style == "plain"

    def test_nodes_empty_sequence(self) -> None:
        doc = yarutsk.loads("[]\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.nodes() == []


class TestDeepCopy:
    def test_mapping_deepcopy_independence(self) -> None:
        import copy

        doc = yarutsk.loads("x: 1\ny:\n  z: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc2 = copy.deepcopy(doc)
        doc2["x"] = 99
        assert doc["x"] == 1

    def test_mapping_deepcopy_nested_independence(self) -> None:
        import copy

        doc = yarutsk.loads("outer:\n  inner: 10\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc2 = copy.deepcopy(doc)
        doc2["outer"]["inner"] = 99
        assert doc["outer"]["inner"] == 10

    def test_mapping_copy_is_yaml_mapping(self) -> None:
        import copy

        doc = yarutsk.loads("a: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc2 = copy.copy(doc)
        assert isinstance(doc2, yarutsk.YamlMapping)

    def test_mapping_deepcopy_preserves_style(self) -> None:
        import copy

        doc = yarutsk.loads('key: "quoted"\n')
        assert isinstance(doc, yarutsk.YamlMapping)
        doc2 = copy.deepcopy(doc)
        doc2.node("key").style = "plain"
        # Original should still have double-quoted style
        out = yarutsk.dumps(doc)
        assert '"quoted"' in out

    def test_sequence_deepcopy_independence(self) -> None:
        import copy

        doc = yarutsk.loads("- 1\n- 2\n- 3\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc2 = copy.deepcopy(doc)
        doc2[0] = 99
        assert doc[0] == 1

    def test_sequence_copy_is_yaml_sequence(self) -> None:
        import copy

        doc = yarutsk.loads("- a\n- b\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc2 = copy.copy(doc)
        assert isinstance(doc2, yarutsk.YamlSequence)


class TestMappingInnerSync:
    def test_clear_empties_inner(self) -> None:
        doc = yarutsk.loads("a: 1\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.clear()
        assert len(doc) == 0
        out = yarutsk.dumps(doc)
        assert "a" not in out
        assert "b" not in out

    def test_clear_returns_empty_mapping_on_dump(self) -> None:
        doc = yarutsk.loads("x: 10\ny: 20\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.clear()
        doc2 = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert len(doc2) == 0

    def test_clear_then_add_key(self) -> None:
        doc = yarutsk.loads("old: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.clear()
        doc["new"] = 2
        out = yarutsk.dumps(doc)
        assert "old" not in out
        assert "new: 2" in out

    def test_popitem_syncs_inner(self) -> None:
        doc = yarutsk.loads("a: 1\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        k, _v = doc.popitem()
        assert k not in doc
        out = yarutsk.dumps(doc)
        assert k not in out

    def test_popitem_returns_last_key(self) -> None:
        doc = yarutsk.loads("x: 10\ny: 20\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        k, v = doc.popitem()
        assert k == "y"
        assert v == 20

    def test_popitem_on_empty_raises(self) -> None:
        doc = yarutsk.loads("{}\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.popitem()

    def test_copy_returns_yaml_mapping(self) -> None:
        doc = yarutsk.loads("a: 1\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        c = doc.copy()
        assert isinstance(c, yarutsk.YamlMapping)

    def test_copy_content_matches(self) -> None:
        doc = yarutsk.loads("a: 1\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        c = doc.copy()
        assert yarutsk.dumps(c) == yarutsk.dumps(doc)

    def test_copy_is_independent(self) -> None:
        doc = yarutsk.loads("a: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        c = doc.copy()
        c["a"] = 99
        assert doc["a"] == 1


class TestSequenceInnerSync:
    def test_iadd_syncs_inner(self) -> None:
        seq = yarutsk.loads("- 1\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq += [2, 3]
        assert list(seq) == [1, 2, 3]
        out = yarutsk.dumps(seq)
        assert "- 2" in out
        assert "- 3" in out

    def test_iadd_returns_yaml_sequence(self) -> None:
        seq = yarutsk.loads("- a\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq += ["b"]
        assert isinstance(seq, yarutsk.YamlSequence)

    def test_iadd_preserves_existing_metadata(self) -> None:
        seq = yarutsk.loads("- x  # comment\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq += ["y"]
        assert seq.node(0).comment_inline == "comment"

    def test_slice_setitem_syncs_inner(self) -> None:
        seq = yarutsk.loads("- 1\n- 2\n- 3\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq[1:2] = [20, 21]
        assert list(seq) == [1, 20, 21, 3]
        out = yarutsk.dumps(seq)
        assert "- 20" in out
        assert "- 21" in out
        assert "- 2\n" not in out

    def test_slice_setitem_empty_replacement(self) -> None:
        seq = yarutsk.loads("- a\n- b\n- c\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq[1:2] = []
        assert list(seq) == ["a", "c"]
        out = yarutsk.dumps(seq)
        assert "- b\n" not in out

    def test_slice_setitem_insertion(self) -> None:
        seq = yarutsk.loads("- a\n- c\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        seq[1:1] = ["b"]
        assert list(seq) == ["a", "b", "c"]

    def test_slice_delitem_syncs_inner(self) -> None:
        seq = yarutsk.loads("- a\n- b\n- c\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        del seq[0:2]
        assert list(seq) == ["c"]
        assert yarutsk.dumps(seq) == "- c\n"

    def test_slice_delitem_empty_slice(self) -> None:
        seq = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        del seq[1:1]
        assert list(seq) == [1, 2]

    def test_slice_delitem_full(self) -> None:
        seq = yarutsk.loads("- x\n- y\n- z\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        del seq[:]
        assert list(seq) == []
        assert len(seq) == 0

    def test_extended_slice_setitem_raises(self) -> None:
        seq = yarutsk.loads("- 1\n- 2\n- 3\n- 4\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        with pytest.raises(NotImplementedError):
            seq[::2] = [10, 20]

    def test_extended_slice_delitem_raises(self) -> None:
        seq = yarutsk.loads("- 1\n- 2\n- 3\n- 4\n")
        assert isinstance(seq, yarutsk.YamlSequence)
        with pytest.raises(NotImplementedError):
            del seq[::2]


class TestNonStringKeys:
    """YAML keys that are not strings are preserved as their raw source text."""

    def test_integer_key_loaded_as_string(self) -> None:
        doc = yarutsk.loads("1: foo\n2: bar\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "1" in doc
        assert "2" in doc
        assert 1 not in doc

    def test_float_key_loaded_as_string(self) -> None:
        doc = yarutsk.loads("3.14: pi\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "3.14" in doc

    def test_bool_key_true_loaded_as_string(self) -> None:
        doc = yarutsk.loads("true: yes\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "true" in doc

    def test_bool_key_false_loaded_as_string(self) -> None:
        doc = yarutsk.loads("false: no\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "false" in doc

    def test_null_key_preserved_as_raw_text(self) -> None:
        doc = yarutsk.loads("null: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "null" in doc
        assert doc["null"] == "value"

    def test_null_key_tilde_form(self) -> None:
        doc = yarutsk.loads("~: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert "~" in doc
        assert doc["~"] == "value"

    def test_null_key_round_trips(self) -> None:
        doc = yarutsk.loads("null: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["null"] == "value"

    def test_integer_key_round_trips(self) -> None:
        doc = yarutsk.loads("42: answer\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["42"] == "answer"


class TestYamlIter:
    """iter_load_all() and iter_loads_all() lazily yield one document at a time."""

    def test_iter_loads_all_single_doc(self) -> None:
        docs = list(yarutsk.iter_loads_all("key: value\n"))
        assert len(docs) == 1
        d0 = docs[0]
        assert isinstance(d0, yarutsk.YamlMapping)
        assert d0["key"] == "value"

    def test_iter_loads_all_multi_doc(self) -> None:
        text = "a: 1\n---\nb: 2\n---\nc: 3\n"
        docs = list(yarutsk.iter_loads_all(text))
        assert len(docs) == 3
        d0, d1, d2 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert isinstance(d2, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2
        assert d2["c"] == 3

    def test_iter_loads_all_empty_string(self) -> None:
        docs = list(yarutsk.iter_loads_all(""))
        assert docs == []

    def test_iter_load_all_stringio(self) -> None:
        src = io.StringIO("x: 10\n---\ny: 20\n")
        docs = list(yarutsk.iter_load_all(src))
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["x"] == 10
        assert d1["y"] == 20

    def test_iter_load_all_bytesio(self) -> None:
        src = io.BytesIO(b"p: hello\n---\nq: world\n")
        docs = list(yarutsk.iter_load_all(src))
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["p"] == "hello"
        assert d1["q"] == "world"

    def test_iter_load_all_empty_stream(self) -> None:
        docs = list(yarutsk.iter_load_all(io.StringIO("")))
        assert docs == []

    def test_iter_is_iterator_protocol(self) -> None:
        it = yarutsk.iter_loads_all("a: 1\n---\nb: 2\n")
        assert iter(it) is it
        first = next(it)
        assert isinstance(first, yarutsk.YamlMapping)
        assert first["a"] == 1
        second = next(it)
        assert isinstance(second, yarutsk.YamlMapping)
        assert second["b"] == 2
        with pytest.raises(StopIteration):
            next(it)

    def test_iter_preserves_explicit_start(self) -> None:
        it = yarutsk.iter_loads_all("---\nkey: val\n")
        doc = next(it)
        assert doc.explicit_start is True

    def test_iter_preserves_comments(self) -> None:
        it = yarutsk.iter_loads_all("# comment\nkey: value\n")
        doc = next(it)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == "value"

    def test_iter_schema_applied(self) -> None:
        schema = yarutsk.Schema()
        schema.add_loader("!rev", lambda v: str(v)[::-1])
        it = yarutsk.iter_loads_all("val: !rev hello\n", schema=schema)
        doc = next(it)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["val"] == "olleh"

    def test_iter_loads_all_results_match_loads_all(self) -> None:
        text = "a: 1\nb: two\n---\nc: true\n"
        expected = yarutsk.loads_all(text)
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in expected
        )
        actual = list(yarutsk.iter_loads_all(text))
        assert len(actual) == len(expected)
        for exp, act in zip(expected, actual, strict=False):
            assert yarutsk.dumps(exp) == yarutsk.dumps(act)

    def test_iter_load_all_results_match_load_all(self) -> None:
        text = "x: 1\n---\ny: 2\n"
        expected = yarutsk.loads_all(text)
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in expected
        )
        actual = list(yarutsk.iter_load_all(io.StringIO(text)))
        assert len(actual) == len(expected)
        for exp, act in zip(expected, actual, strict=False):
            assert yarutsk.dumps(exp) == yarutsk.dumps(act)
