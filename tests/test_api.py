"""Tests for the Python dict/list API surface: loads/dumps, to_dict, repr,
protocol compliance, sequence methods, negative indices, setdefault, errors, get."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestStringAPI:
    def test_loads_basic(self):
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_loads_empty(self):
        assert yarutsk.loads("") is None

    def test_loads_returns_first_doc(self):
        doc = yarutsk.loads(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
        assert doc["a"] == 1

    def test_loads_all_basic(self):
        docs = yarutsk.loads_all(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
        assert len(docs) == 2
        assert docs[0]["a"] == 1
        assert docs[1]["b"] == 2

    def test_loads_all_empty(self):
        assert yarutsk.loads_all("") == []

    def test_dumps_basic(self):
        doc = yarutsk.loads(
            dedent("""\
            name: John
            age: 30
        """)
        )
        result = yarutsk.dumps(doc)
        assert isinstance(result, str)
        assert "name: John" in result
        assert "age: 30" in result

    def test_dumps_preserves_comments(self):
        doc = yarutsk.loads("key: val  # note")
        result = yarutsk.dumps(doc)
        assert "# note" in result

    def test_dumps_all_basic(self):
        docs = yarutsk.loads_all(
            dedent("""\
            ---
            a: 1
            ---
            b: 2
        """)
        )
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
        original = dedent("""\
            name: Alice
            age: 30  # years
            city: Berlin
        """)
        doc = yarutsk.loads(original)
        result = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(result)
        assert doc2["name"] == "Alice"
        assert doc2["age"] == 30
        assert doc2.comment_inline("age") == "years"

    def test_loads_all_dumps_all_round_trip(self):
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
        assert docs2[0]["a"] == 1
        assert docs2[1]["b"] == 2
        assert docs2[2]["c"] == 3

    def test_loads_is_equivalent_to_load(self):
        yaml = dedent("""\
            x: 1
            y: 2
        """)
        doc_stream = yarutsk.load(io.StringIO(yaml))
        doc_str = yarutsk.loads(yaml)
        assert repr(doc_stream) == repr(doc_str)


class TestToDict:
    def test_to_dict_simple(self):
        content = io.StringIO(
            dedent("""\
            name: John
            age: 30
        """)
        )
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
    def test_repr_mapping(self):
        content = io.StringIO(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "mapping" in r.lower() or "YAML" in r

    def test_repr_sequence(self):
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "sequence" in r.lower() or "YAML" in r


class TestContains:
    def test_contains_existing_key(self):
        content = io.StringIO(
            dedent("""\
            name: John
            age: 30
        """)
        )
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
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        d = {**doc}
        assert d == {"a": 1, "b": 2}

    def test_dict_constructor(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        d = dict(doc)
        assert d["a"] == 1
        assert d["b"] == 2

    def test_list_unpacking(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        lst = [*doc]
        assert lst == [1, 2, 3]

    def test_list_constructor(self):
        doc = yarutsk.loads(
            dedent("""\
            - x
            - y
        """)
        )
        lst = list(doc)
        assert lst == ["x", "y"]

    def test_isinstance_dict(self):
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, dict)

    def test_isinstance_list(self):
        doc = yarutsk.loads("- a")
        assert isinstance(doc, list)

    def test_mapping_values(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
            c: 3
        """)
        )
        vals = list(doc.values())
        assert sorted(vals) == [1, 2, 3]

    def test_mapping_items(self):
        doc = yarutsk.loads(
            dedent("""\
            x: 10
            y: 20
        """)
        )
        items = dict(doc.items())
        assert items == {"x": 10, "y": 20}

    def test_sequence_iteration(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        total = sum(doc)
        assert total == 6


class TestNestedObjectIdentity:
    """Mutations to nested objects must be visible through the parent."""

    def test_nested_mutation_visible_via_parent(self):
        doc = yarutsk.loads(
            dedent("""\
            server:
              host: localhost
              port: 5432
        """)
        )
        server = doc["server"]
        server["host"] = "remote"
        assert doc["server"]["host"] == "remote"

    def test_nested_mutation_survives_dump(self):
        doc = yarutsk.loads(
            dedent("""\
            db:
              name: mydb
              port: 5432
        """)
        )
        doc["db"]["port"] = 9999
        out = yarutsk.dumps(doc)
        assert "9999" in out
        doc2 = yarutsk.loads(out)
        assert doc2["db"]["port"] == 9999

    def test_deeply_nested_mutation_visible(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              b:
                c: original
        """)
        )
        doc["a"]["b"]["c"] = "changed"
        out = yarutsk.dumps(doc)
        assert "changed" in out
        assert "original" not in out

    def test_sequence_item_mutation_visible(self):
        doc = yarutsk.loads(
            dedent("""\
            items:
              - x: 1
              - x: 2
        """)
        )
        item = doc["items"][0]
        item["x"] = 99
        assert doc["items"][0]["x"] == 99

    def test_two_references_same_object(self):
        doc = yarutsk.loads(
            dedent("""\
            cfg:
              val: 0
        """)
        )
        ref1 = doc["cfg"]
        ref2 = doc["cfg"]
        assert ref1 is ref2


class TestSequenceListMethods:
    """count(), index(), extend(), and friends on YamlSequence."""

    def test_count(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - a
            - c
            - a
        """)
        )
        assert doc.count("a") == 3
        assert doc.count("b") == 1
        assert doc.count("missing") == 0

    def test_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - x
            - y
            - z
        """)
        )
        assert doc.index("y") == 1

    def test_index_with_bounds(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
            - b
        """)
        )
        assert doc.index("b", 2) == 3

    def test_index_missing_raises(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        with pytest.raises(ValueError):
            doc.index("missing")

    def test_extend_appends_all(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        doc.extend(["c", "d"])
        assert len(doc) == 4
        assert doc[2] == "c"
        assert doc[3] == "d"

    def test_extend_empty_no_change(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        doc.extend([])
        assert len(doc) == 2

    def test_remove(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        doc.remove("b")
        assert len(doc) == 2
        assert list(doc) == ["a", "c"]

    def test_mixed_types_in_sequence(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - hello
            - true
            - null
            - 3.14
        """)
        )
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is True
        assert doc[3] is None
        assert doc[4] == pytest.approx(3.14)

    def test_contains_in_sequence(self):
        doc = yarutsk.loads(
            dedent("""\
            - foo
            - bar
        """)
        )
        assert "foo" in doc
        assert "baz" not in doc


class TestNegativeSequenceIndices:
    """Negative indices on YamlSequence should work like Python lists."""

    def test_getitem_negative(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert doc[-1] == "c"
        assert doc[-2] == "b"
        assert doc[-3] == "a"

    def test_setitem_negative(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        doc[-1] = "z"
        assert doc[2] == "z"

    def test_delitem_negative(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        del doc[-1]
        assert len(doc) == 2
        assert doc[-1] == "b"

    def test_pop_negative(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        val = doc.pop(-1)
        assert val == "c"
        assert len(doc) == 2

    def test_get_comment_negative_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c  # last
        """)
        )
        assert doc.comment_inline(-1) == "last"
        assert doc.comment_inline(-3) == "first"

    def test_set_comment_negative_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
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
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
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
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        with pytest.raises(IndexError):
            _ = doc[5]

    def test_delitem_out_of_range_raises(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
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
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
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


class TestSequenceScalarStyle:
    """scalar_style() on YamlSequence."""

    def test_set_single_quoted(self):
        doc = yarutsk.loads(
            dedent("""\
            - hello
            - world
        """)
        )
        doc.scalar_style(0, "single")
        out = yarutsk.dumps(doc)
        assert "'hello'" in out
        assert "world" in out

    def test_set_double_quoted(self):
        doc = yarutsk.loads("- hello\n")
        doc.scalar_style(0, "double")
        assert yarutsk.dumps(doc) == '- "hello"\n'

    def test_negative_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        doc.scalar_style(-1, "single")
        assert yarutsk.dumps(doc) == dedent("""\
            - a
            - b
            - 'c'
        """)

    def test_only_target_item_changes(self):
        doc = yarutsk.loads(
            dedent("""\
            - 'a'
            - b
        """)
        )
        doc.scalar_style(1, "double")
        out = yarutsk.dumps(doc)
        assert "'a'" in out
        assert '"b"' in out

    def test_invalid_style_raises(self):
        doc = yarutsk.loads("- hello\n")
        with pytest.raises(ValueError):
            doc.scalar_style(0, "bad")

    def test_out_of_range_raises(self):
        doc = yarutsk.loads("- hello\n")
        with pytest.raises(IndexError):
            doc.scalar_style(99, "plain")


class TestStyledConstructors:
    """Tests for YamlScalar/YamlMapping/YamlSequence constructors with style/tag."""

    def test_scalar_default_style(self):
        s = yarutsk.YamlScalar("hello")
        assert s.style == "plain"
        assert s.value == "hello"
        assert s.tag is None

    def test_scalar_double_style(self):
        s = yarutsk.YamlScalar("hello", style="double")
        assert s.style == "double"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        assert yarutsk.dumps(doc) == 'k: "hello"\n'

    def test_scalar_single_style(self):
        doc = yarutsk.YamlMapping()
        doc["k"] = yarutsk.YamlScalar("world", style="single")
        assert yarutsk.dumps(doc) == "k: 'world'\n"

    def test_scalar_literal_style(self):
        doc = yarutsk.YamlMapping()
        doc["k"] = yarutsk.YamlScalar("line1\nline2\n", style="literal")
        out = yarutsk.dumps(doc)
        assert "|-" in out or "|" in out

    def test_scalar_with_tag(self):
        s = yarutsk.YamlScalar("42", tag="!myint")
        assert s.tag == "!myint"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        out = yarutsk.dumps(doc)
        assert "!myint" in out
        assert "42" in out

    def test_scalar_int_value(self):
        s = yarutsk.YamlScalar(42, style="plain")
        assert s.value == 42

    def test_scalar_bool_value(self):
        s = yarutsk.YamlScalar(True)
        assert s.value is True

    def test_scalar_none_value(self):
        s = yarutsk.YamlScalar(None)
        assert s.value is None

    def test_scalar_rejects_non_primitive(self):
        with pytest.raises(TypeError):
            yarutsk.YamlScalar([1, 2, 3])

    def test_scalar_rejects_unknown_style(self):
        with pytest.raises(ValueError):
            yarutsk.YamlScalar("x", style="notathing")

    def test_scalar_assignment_preserves_style(self):
        doc = yarutsk.loads(
            dedent("""\
            a: old
            b: other
        """)
        )
        doc["a"] = yarutsk.YamlScalar("new", style="double")
        out = yarutsk.dumps(doc)
        assert 'a: "new"' in out

    def test_mapping_default_style(self):
        m = yarutsk.YamlMapping()
        assert m.style == "block"
        assert m.tag is None

    def test_mapping_flow_style(self):
        m = yarutsk.YamlMapping(style="flow")
        assert m.style == "flow"
        m["x"] = 1
        m["y"] = 2
        doc = yarutsk.YamlMapping()
        doc["point"] = m
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_mapping_with_tag(self):
        m = yarutsk.YamlMapping(tag="!mymap")
        assert m.tag == "!mymap"

    def test_mapping_rejects_unknown_style(self):
        with pytest.raises(ValueError):
            yarutsk.YamlMapping(style="notathing")

    def test_mapping_assignment_preserves_style(self):
        doc = yarutsk.loads(
            dedent("""\
            outer:
              a: 1
        """)
        )
        nested = yarutsk.YamlMapping(style="flow")
        nested["a"] = 1
        doc["outer"] = nested
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_sequence_default_style(self):
        s = yarutsk.YamlSequence()
        assert s.style == "block"
        assert s.tag is None

    def test_sequence_flow_style(self):
        s = yarutsk.YamlSequence(style="flow")
        assert s.style == "flow"
        s.extend([1, 2, 3])
        doc = yarutsk.YamlMapping()
        doc["nums"] = s
        out = yarutsk.dumps(doc)
        assert "[" in out

    def test_sequence_with_tag(self):
        s = yarutsk.YamlSequence(tag="!myseq")
        assert s.tag == "!myseq"

    def test_sequence_rejects_unknown_style(self):
        with pytest.raises(ValueError):
            yarutsk.YamlSequence(style="notathing")

    def test_dumper_returns_styled_scalar(self):
        class MyVal:
            def __init__(self, v):
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

    def test_dumper_returns_styled_mapping(self):
        class MyPoint:
            def __init__(self, x, y):
                self.x, self.y = x, y

        def dump_point(p):
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

    def test_dumper_returns_styled_sequence(self):
        class MyList:
            def __init__(self, items):
                self.items = items

        def dump_list(obj):
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

    def test_scalar_style_reset(self):
        doc = yarutsk.loads("key: 'single'")
        doc.format()
        assert doc.node("key").style == "plain"

    def test_container_style_reset(self):
        doc = yarutsk.loads("nested: {a: 1}")
        doc.format()
        assert "{" not in yarutsk.dumps(doc)

    def test_comments_cleared(self):
        src = dedent("""\
            # comment
            key: val  # inline
        """)
        doc = yarutsk.loads(src)
        doc.format()
        assert doc.comment_before("key") is None
        assert doc.comment_inline("key") is None

    def test_blank_lines_cleared(self):
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        doc.format()
        assert doc.blank_lines_before("b") == 0

    def test_tags_preserved(self):
        doc = yarutsk.loads("value: !!str 42")
        doc.format()
        assert "!!str" in yarutsk.dumps(doc)

    def test_recursive_nested_mapping(self):
        src = dedent("""\
            outer:
              inner: 'quoted'
        """)
        doc = yarutsk.loads(src)
        doc.format()
        assert "'" not in yarutsk.dumps(doc)

    def test_sequence_items_formatted(self):
        doc = yarutsk.loads("items: ['a', 'b']")
        doc.format()
        result = yarutsk.dumps(doc)
        assert "[" not in result
        assert "'" not in result

    def test_styles_false_preserves_scalar_style(self):
        doc = yarutsk.loads("key: 'single'")
        doc.format(styles=False)
        assert doc.node("key").style == "single"

    def test_styles_false_preserves_container_style(self):
        doc = yarutsk.loads("nested: {a: 1}")
        doc.format(styles=False)
        assert "{" in yarutsk.dumps(doc)

    def test_comments_false_preserves_comments(self):
        doc = yarutsk.loads("key: val  # inline")
        doc.format(comments=False)
        assert doc.comment_inline("key") == "inline"

    def test_blank_lines_false_preserves_blank_lines(self):
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        doc.format(blank_lines=False)
        assert doc.blank_lines_before("b") > 0

    def test_multiline_string_uses_literal_style(self):
        # A multiline string should become literal block style, not double-quoted with \n
        src = dedent("""\
            message: |
              line1
              line2
        """)
        doc = yarutsk.loads(src)
        # Force it to double-quoted so format() has something to reset
        doc.scalar_style("message", "double")
        doc.format()
        result = yarutsk.dumps(doc)
        assert "\\n" not in result
        assert "|" in result

    def test_roundtrip_clean(self):
        src = dedent("""\
            # Config
            server:
              host: 'localhost'  # inline
              port: 8080

              debug: true
        """)
        doc = yarutsk.loads(src)
        doc.format()
        result = yarutsk.dumps(doc)
        assert "#" not in result
        assert "'" not in result
        assert "\n\n" not in result

    def test_yaml_scalar_format(self):
        s = yarutsk.YamlScalar("hello", style="double")
        s.format()
        assert s.style == "plain"

    def test_yaml_scalar_format_preserves_tag(self):
        s = yarutsk.YamlScalar("42", style="single", tag="!!str")
        s.format()
        assert s.tag == "!!str"
        assert s.style == "plain"

    def test_sequence_blank_lines_cleared(self):
        src = dedent("""\
            items:
              - a

              - b
        """)
        doc = yarutsk.loads(src)
        seq = doc["items"]
        doc.format()
        assert seq.blank_lines_before(1) == 0

    def test_trailing_blank_lines_cleared(self):
        doc = yarutsk.loads("a: 1\nb: 2\n")
        doc.trailing_blank_lines = 3
        doc.format()
        assert doc.trailing_blank_lines == 0


class TestIndentParameter:
    """Test the indent= parameter on dump functions."""

    def test_dumps_indent_4(self):
        doc = yarutsk.loads("outer:\n  inner: 1\n")
        out = yarutsk.dumps(doc, indent=4)
        assert "    inner: 1" in out

    def test_dumps_indent_1(self):
        doc = yarutsk.loads("outer:\n  inner: 1\n")
        out = yarutsk.dumps(doc, indent=1)
        assert " inner: 1" in out

    def test_dumps_all_indent(self):
        docs = yarutsk.loads_all("a: 1\n---\nb: 2\n")
        out = yarutsk.dumps_all(docs, indent=4)
        assert "    " not in out  # top-level keys are not indented
        assert "a: 1" in out
        assert "b: 2" in out


class TestYamlVersionAndTagDirectives:
    """Test yaml_version and tag_directives properties."""

    def test_yaml_version_roundtrip(self):
        src = "%YAML 1.1\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc.yaml_version == "1.1"
        assert yarutsk.dumps(doc) == src

    def test_yaml_version_set(self):
        doc = yarutsk.loads("key: value\n")
        doc.yaml_version = "1.2"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.2" in out
        assert "---" in out

    def test_yaml_version_clear(self):
        src = "%YAML 1.1\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        doc.yaml_version = None
        out = yarutsk.dumps(doc)
        assert "%YAML" not in out

    def test_tag_directives_roundtrip(self):
        src = "%TAG ! tag:example.com,2024:\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        assert doc.tag_directives == [("!", "tag:example.com,2024:")]
        assert yarutsk.dumps(doc) == src

    def test_tag_directives_set(self):
        doc = yarutsk.loads("key: value\n")
        doc.explicit_start = True
        doc.tag_directives = [("!e!", "tag:example.com,2024:")]
        out = yarutsk.dumps(doc)
        assert "%TAG !e! tag:example.com,2024:" in out

    def test_tag_directives_clear(self):
        src = "%TAG ! tag:example.com,2024:\n---\nkey: value\n"
        doc = yarutsk.loads(src)
        doc.tag_directives = []
        out = yarutsk.dumps(doc)
        assert "%TAG" not in out

    def test_yaml_version_on_scalar(self):
        doc = yarutsk.loads("42\n")
        assert isinstance(doc, yarutsk.YamlScalar)
        doc.yaml_version = "1.1"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.1" in out

    def test_yaml_version_on_sequence(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.yaml_version = "1.2"
        doc.explicit_start = True
        out = yarutsk.dumps(doc)
        assert "%YAML 1.2" in out


class TestScalarStyleTypeError:
    """Test that scalar_style() raises TypeError on non-scalar values."""

    def test_mapping_scalar_style_on_nested_mapping_raises(self):
        doc = yarutsk.loads("outer:\n  inner: 1\n")
        with pytest.raises(TypeError, match="not a scalar"):
            doc.scalar_style("outer", "plain")

    def test_mapping_scalar_style_on_nested_sequence_raises(self):
        doc = yarutsk.loads("items:\n  - 1\n  - 2\n")
        with pytest.raises(TypeError, match="not a scalar"):
            doc.scalar_style("items", "plain")

    def test_sequence_scalar_style_on_nested_mapping_raises(self):
        doc = yarutsk.loads("- key: value\n")
        with pytest.raises(TypeError, match="not a scalar"):
            doc.scalar_style(0, "plain")

    def test_sequence_scalar_style_on_nested_sequence_raises(self):
        doc = yarutsk.loads("- - 1\n  - 2\n")
        with pytest.raises(TypeError, match="not a scalar"):
            doc.scalar_style(0, "plain")

    def test_mapping_scalar_style_on_scalar_still_works(self):
        doc = yarutsk.loads("key: value\n")
        doc.scalar_style("key", "double")
        assert '"value"' in yarutsk.dumps(doc)

    def test_sequence_scalar_style_on_scalar_still_works(self):
        doc = yarutsk.loads("- hello\n")
        doc.scalar_style(0, "single")
        assert "'hello'" in yarutsk.dumps(doc)


class TestAnchorProperty:
    """Test the anchor read/write property on all three node types."""

    def test_mapping_anchor_roundtrip(self):
        src = "base: &anchor\n  x: 1\nalias: *anchor\n"
        doc = yarutsk.loads(src)
        nested = doc.node("base")
        assert nested.anchor == "anchor"

    def test_mapping_set_anchor(self):
        doc = yarutsk.loads("key:\n  x: 1\n")
        nested = doc.node("key")
        assert isinstance(nested, yarutsk.YamlMapping)
        nested.anchor = "myanchor"
        assert nested.anchor == "myanchor"

    def test_scalar_anchor_roundtrip(self):
        src = "value: &val 42\n"
        doc = yarutsk.loads(src)
        node = doc.node("value")
        assert isinstance(node, yarutsk.YamlScalar)
        assert node.anchor == "val"

    def test_scalar_set_anchor(self):
        s = yarutsk.YamlScalar("hello")
        assert s.anchor is None
        s.anchor = "greeting"
        assert s.anchor == "greeting"
        s.anchor = None
        assert s.anchor is None

    def test_sequence_set_anchor(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.anchor = "mylist"
        assert doc.anchor == "mylist"
        out = yarutsk.dumps(doc)
        assert "&mylist" in out

    def test_mapping_set_anchor_emits(self):
        doc = yarutsk.loads("key: value\n")
        doc.anchor = "root"
        out = yarutsk.dumps(doc)
        assert "&root" in out


class TestConstructorFromExisting:
    """YamlMapping(mapping) and YamlSequence(iterable) constructors."""

    def test_mapping_from_plain_dict(self):
        m = yarutsk.YamlMapping({"a": 1, "b": 2})
        assert m["a"] == 1
        assert m["b"] == 2
        assert list(m.keys()) == ["a", "b"]

    def test_mapping_from_plain_dict_with_style(self):
        m = yarutsk.YamlMapping({"x": 1}, style="flow")
        assert m.style == "flow"
        assert m["x"] == 1

    def test_mapping_from_yaml_mapping_preserves_values(self):
        src = yarutsk.loads("x: 1\ny: 2\n")
        m = yarutsk.YamlMapping(src)
        assert m["x"] == 1
        assert m["y"] == 2
        assert list(m.keys()) == ["x", "y"]

    def test_mapping_from_yaml_mapping_with_override_style(self):
        src = yarutsk.loads("x: 1\ny: 2\n")
        m = yarutsk.YamlMapping(src, style="flow")
        assert m.style == "flow"
        assert m["x"] == 1

    def test_mapping_from_yaml_mapping_preserves_inner_metadata(self):
        src = yarutsk.loads("x: 1  # inline\ny: 2\n")
        m = yarutsk.YamlMapping(src)
        assert m.get_comment_inline("x") == "inline"

    def test_mapping_empty_no_arg(self):
        m = yarutsk.YamlMapping()
        assert len(m) == 0
        assert m.style == "block"

    def test_mapping_none_arg(self):
        m = yarutsk.YamlMapping(None)
        assert len(m) == 0

    def test_sequence_from_list(self):
        s = yarutsk.YamlSequence([1, 2, 3])
        assert list(s) == [1, 2, 3]

    def test_sequence_from_list_with_style(self):
        s = yarutsk.YamlSequence([1, 2, 3], style="flow")
        assert s.style == "flow"
        assert list(s) == [1, 2, 3]

    def test_sequence_from_range(self):
        s = yarutsk.YamlSequence(range(3))
        assert list(s) == [0, 1, 2]

    def test_sequence_from_yaml_sequence_preserves_values(self):
        src = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src)
        assert list(s) == [1, 2]

    def test_sequence_from_yaml_sequence_with_override_style(self):
        src = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src, style="flow")
        assert s.style == "flow"
        assert list(s) == [1, 2]

    def test_sequence_from_yaml_sequence_preserves_inner_metadata(self):
        src = yarutsk.loads("- a  # first\n- b\n")
        assert isinstance(src, yarutsk.YamlSequence)
        s = yarutsk.YamlSequence(src)
        assert s.get_comment_inline(0) == "first"

    def test_sequence_empty_no_arg(self):
        s = yarutsk.YamlSequence()
        assert len(s) == 0
        assert s.style == "block"

    def test_sequence_none_arg(self):
        s = yarutsk.YamlSequence(None)
        assert len(s) == 0


class TestAliasAPI:
    """Tests for alias_name() and set_alias() on YamlMapping and YamlSequence."""

    def test_mapping_alias_name_none_for_plain_value(self):
        doc = yarutsk.loads("key: value\n")
        assert doc.alias_name("key") is None

    def test_mapping_alias_name_parsed_alias(self):
        doc = yarutsk.loads("base: &anchor 1\nref: *anchor\n")
        assert doc.alias_name("ref") == "anchor"

    def test_mapping_alias_name_anchor_node_is_not_alias(self):
        doc = yarutsk.loads("base: &anchor 1\nref: *anchor\n")
        assert doc.alias_name("base") is None

    def test_mapping_alias_name_missing_key_raises(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(KeyError):
            doc.alias_name("missing")

    def test_mapping_set_alias_marks_value(self):
        doc = yarutsk.loads("base: &anchor hello\nother: hello\n")
        doc.set_alias("other", "anchor")
        assert doc.alias_name("other") == "anchor"

    def test_mapping_set_alias_resolved_value_accessible(self):
        doc = yarutsk.loads("base: &anchor 42\nother: 42\n")
        doc.set_alias("other", "anchor")
        assert doc["other"] == 42

    def test_mapping_set_alias_emits_star_anchor(self):
        doc = yarutsk.loads("base: &anchor hello\nother: hello\n")
        doc.set_alias("other", "anchor")
        out = yarutsk.dumps(doc)
        assert "*anchor" in out

    def test_mapping_set_alias_missing_key_raises(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(KeyError):
            doc.set_alias("missing", "anchor")

    def test_sequence_alias_name_none_for_plain_value(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.alias_name(0) is None

    def test_sequence_alias_name_parsed_alias(self):
        doc = yarutsk.loads("- &val 1\n- *val\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.alias_name(1) == "val"

    def test_sequence_alias_name_anchor_node_is_not_alias(self):
        doc = yarutsk.loads("- &val 1\n- *val\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.alias_name(0) is None

    def test_sequence_alias_name_out_of_range_raises(self):
        doc = yarutsk.loads("- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.alias_name(99)

    def test_sequence_set_alias_marks_value(self):
        doc = yarutsk.loads("- 1\n- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        assert doc.alias_name(1) == "val"

    def test_sequence_set_alias_resolved_value_accessible(self):
        doc = yarutsk.loads("- 42\n- 42\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        assert doc[1] == 42

    def test_sequence_set_alias_emits_star_anchor(self):
        doc = yarutsk.loads("- &val 1\n- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.set_alias(1, "val")
        out = yarutsk.dumps(doc)
        assert "*val" in out

    def test_sequence_set_alias_out_of_range_raises(self):
        doc = yarutsk.loads("- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.set_alias(99, "val")
