"""Tests for the YamlScalar / YamlMapping / YamlSequence styled constructors.

Covers:
- All constructor arguments (value types, styles, tags, errors)
- Style/tag are readable back after construction
- Dump output reflects the requested style
- Styled values survive assignment into loaded documents (mapping and sequence)
- All mutation entry-points: __setitem__, append, insert, extend, update, setdefault
- Dumper protocol: returning styled nodes from add_dumper callbacks
- Tag precedence: tuple tag overrides any tag already set on the node
- to_python / __eq__ / __repr__ on YamlScalar
- Tag on YamlMapping / YamlSequence appears in dump output
"""

import datetime
from textwrap import dedent

import pytest

import yarutsk


class TestYamlScalarConstruction:
    def test_str_default(self):
        s = yarutsk.YamlScalar("hello")
        assert s.value == "hello"
        assert s.style == "plain"
        assert s.tag is None

    def test_int_value(self):
        s = yarutsk.YamlScalar(42)
        assert s.value == 42

    def test_float_value(self):
        s = yarutsk.YamlScalar(3.14)
        assert s.value == 3.14

    def test_bool_true(self):
        s = yarutsk.YamlScalar(True)
        assert s.value is True

    def test_bool_false(self):
        s = yarutsk.YamlScalar(False)
        assert s.value is False

    def test_none_value(self):
        s = yarutsk.YamlScalar(None)
        assert s.value is None

    def test_all_scalar_styles(self):
        for style in ("plain", "single", "double", "literal", "folded"):
            s = yarutsk.YamlScalar("text", style=style)
            assert s.style == style

    def test_tag_kwarg(self):
        s = yarutsk.YamlScalar("v", tag="!mytag")
        assert s.tag == "!mytag"

    def test_tag_and_style_together(self):
        s = yarutsk.YamlScalar("v", style="double", tag="!x")
        assert s.style == "double"
        assert s.tag == "!x"

    def test_tag_none_explicit(self):
        s = yarutsk.YamlScalar("v", tag=None)
        assert s.tag is None

    def test_rejects_list(self):
        with pytest.raises(TypeError):
            yarutsk.YamlScalar([1, 2])  # type: ignore[arg-type]

    def test_rejects_dict(self):
        with pytest.raises(TypeError):
            yarutsk.YamlScalar({"a": 1})  # type: ignore[arg-type]

    def test_rejects_unknown_style(self):
        with pytest.raises(ValueError, match="unknown style"):
            yarutsk.YamlScalar("v", style="bold")

    def test_style_mutable_after_construction(self):
        s = yarutsk.YamlScalar("v", style="plain")
        s.style = "double"
        assert s.style == "double"

    def test_tag_mutable_after_construction(self):
        s = yarutsk.YamlScalar("v")
        s.tag = "!foo"
        assert s.tag == "!foo"
        s.tag = None
        assert s.tag is None


class TestYamlScalarEquality:
    def test_eq_primitive(self):
        assert yarutsk.YamlScalar("hello") == "hello"
        assert yarutsk.YamlScalar(42) == 42
        assert yarutsk.YamlScalar(True) == True  # noqa: E712
        assert yarutsk.YamlScalar(None) == None  # noqa: E711

    def test_eq_other_scalar(self):
        assert yarutsk.YamlScalar("x") == yarutsk.YamlScalar("x")

    def test_neq_different_value(self):
        assert yarutsk.YamlScalar("a") != yarutsk.YamlScalar("b")

    def test_neq_different_type(self):
        assert yarutsk.YamlScalar(1) != "1"


class TestYamlScalarReprAndToDict:
    def test_repr(self):
        r = repr(yarutsk.YamlScalar("hi"))
        assert "YamlScalar" in r
        assert "hi" in r

    def test_to_python_returns_primitive(self):
        assert yarutsk.YamlScalar("x").to_python() == "x"
        assert yarutsk.YamlScalar(7).to_python() == 7
        assert yarutsk.YamlScalar(None).to_python() is None


class TestYamlScalarDumpStyle:
    def _doc_with(self, val):
        doc = yarutsk.YamlMapping()
        doc["k"] = val
        return yarutsk.dumps(doc)

    def test_plain_style(self):
        assert self._doc_with(yarutsk.YamlScalar("hello", style="plain")) == dedent("""\
            k: hello
        """)

    def test_double_style(self):
        assert self._doc_with(yarutsk.YamlScalar("hello", style="double")) == dedent("""\
            k: "hello"
        """)

    def test_single_style(self):
        assert self._doc_with(yarutsk.YamlScalar("hello", style="single")) == dedent("""\
            k: 'hello'
        """)

    def test_literal_style(self):
        out = self._doc_with(yarutsk.YamlScalar("a\nb\n", style="literal"))
        assert "|" in out

    def test_folded_style(self):
        out = self._doc_with(yarutsk.YamlScalar("a b c d", style="folded"))
        assert ">" in out

    def test_tag_in_output(self):
        out = self._doc_with(yarutsk.YamlScalar("42", tag="!myint"))
        assert "!myint" in out
        assert "42" in out

    def test_double_style_empty_string(self):
        out = self._doc_with(yarutsk.YamlScalar("", style="double"))
        assert '""' in out

    def test_int_style_not_quoted(self):
        # The emitter emits int/float/bool/null scalars in their native form
        # regardless of the requested style; only str values get quoted.
        assert self._doc_with(yarutsk.YamlScalar(42, style="double")) == dedent("""\
            k: 42
        """)


class TestYamlMappingConstruction:
    def test_default_style(self):
        m = yarutsk.YamlMapping()
        assert m.style == "block"
        assert m.tag is None
        assert len(m) == 0

    def test_flow_style(self):
        m = yarutsk.YamlMapping(style="flow")
        assert m.style == "flow"

    def test_block_style_explicit(self):
        m = yarutsk.YamlMapping(style="block")
        assert m.style == "block"

    def test_tag_kwarg(self):
        m = yarutsk.YamlMapping(tag="!mymap")
        assert m.tag == "!mymap"

    def test_tag_and_style(self):
        m = yarutsk.YamlMapping(style="flow", tag="!pt")
        assert m.style == "flow"
        assert m.tag == "!pt"

    def test_rejects_unknown_style(self):
        with pytest.raises(ValueError, match="unknown style"):
            yarutsk.YamlMapping(style="sideways")

    def test_is_dict_subclass(self):
        m = yarutsk.YamlMapping()
        assert isinstance(m, dict)

    def test_supports_dict_operations(self):
        m = yarutsk.YamlMapping(style="flow")
        m["a"] = 1
        m["b"] = 2
        assert m["a"] == 1
        assert list(m.keys()) == ["a", "b"]


class TestYamlMappingDumpStyle:
    def test_flow_mapping_emits_braces(self):
        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 1
        m["y"] = 2
        doc = yarutsk.YamlMapping()
        doc["p"] = m
        out = yarutsk.dumps(doc)
        assert "{" in out
        assert "x: 1" in out

    def test_block_mapping_no_braces(self):
        m = yarutsk.YamlMapping(style="block")
        m["x"] = 1
        doc = yarutsk.YamlMapping()
        doc["p"] = m
        out = yarutsk.dumps(doc)
        assert "{" not in out

    def test_tag_on_nested_mapping_in_output(self):
        m = yarutsk.YamlMapping(tag="!maptag")
        m["a"] = 1
        doc = yarutsk.YamlMapping()
        doc["v"] = m
        out = yarutsk.dumps(doc)
        assert "!maptag" in out

    def test_top_level_flow_mapping(self):
        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 1
        out = yarutsk.dumps(m)
        assert "{" in out

    def test_style_preserved_after_setitem_roundtrip(self):
        doc = yarutsk.loads(
            dedent("""\
            outer:
              x: 1
        """)
        )
        nested = yarutsk.YamlMapping(style="flow")
        nested["x"] = 1
        doc["outer"] = nested
        assert "{" in yarutsk.dumps(doc)


class TestYamlSequenceConstruction:
    def test_default_style(self):
        s = yarutsk.YamlSequence()
        assert s.style == "block"
        assert s.tag is None
        assert len(s) == 0

    def test_flow_style(self):
        s = yarutsk.YamlSequence(style="flow")
        assert s.style == "flow"

    def test_tag_kwarg(self):
        s = yarutsk.YamlSequence(tag="!myseq")
        assert s.tag == "!myseq"

    def test_tag_and_style(self):
        s = yarutsk.YamlSequence(style="flow", tag="!xs")
        assert s.style == "flow"
        assert s.tag == "!xs"

    def test_rejects_unknown_style(self):
        with pytest.raises(ValueError, match="unknown style"):
            yarutsk.YamlSequence(style="zigzag")

    def test_is_list_subclass(self):
        s = yarutsk.YamlSequence()
        assert isinstance(s, list)

    def test_supports_list_operations(self):
        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2, 3])
        assert list(s) == [1, 2, 3]


class TestYamlSequenceDumpStyle:
    def test_flow_sequence_emits_brackets(self):
        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2, 3])
        doc = yarutsk.YamlMapping()
        doc["nums"] = s
        out = yarutsk.dumps(doc)
        assert "[" in out

    def test_block_sequence_no_brackets(self):
        s = yarutsk.YamlSequence(style="block")
        s.extend([1, 2])
        doc = yarutsk.YamlMapping()
        doc["nums"] = s
        out = yarutsk.dumps(doc)
        assert "[" not in out

    def test_tag_on_nested_sequence_in_output(self):
        s = yarutsk.YamlSequence(tag="!seqtag")
        s.append(1)
        doc = yarutsk.YamlMapping()
        doc["v"] = s
        out = yarutsk.dumps(doc)
        assert "!seqtag" in out

    def test_top_level_flow_sequence(self):
        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2])
        out = yarutsk.dumps(s)
        assert "[" in out

    def test_style_preserved_after_setitem_roundtrip(self):
        doc = yarutsk.loads(
            dedent("""\
            items:
              - 1
              - 2
        """)
        )
        seq = yarutsk.YamlSequence(style="flow")
        seq.extend([1, 2])
        doc["items"] = seq
        assert "[" in yarutsk.dumps(doc)


class TestStyledScalarAssignmentPaths:
    """All the ways a styled YamlScalar can enter a mapping or sequence."""

    def test_mapping_setitem_new_key(self):
        doc = yarutsk.loads("a: 1")
        doc["b"] = yarutsk.YamlScalar("hi", style="double")
        assert '"hi"' in yarutsk.dumps(doc)

    def test_mapping_setitem_overwrite(self):
        doc = yarutsk.loads("a: old")
        doc["a"] = yarutsk.YamlScalar("new", style="single")
        assert "'new'" in yarutsk.dumps(doc)

    def test_mapping_update(self):
        doc = yarutsk.loads("a: 1")
        doc.update({"b": yarutsk.YamlScalar("x", style="double")})
        assert '"x"' in yarutsk.dumps(doc)

    def test_mapping_setdefault_inserts(self):
        doc = yarutsk.loads("a: 1")
        result = doc.setdefault("b", yarutsk.YamlScalar("y", style="single"))
        assert result == "y"
        assert "'y'" in yarutsk.dumps(doc)

    def test_mapping_setdefault_does_not_overwrite(self):
        doc = yarutsk.loads("a: existing")
        doc.setdefault("a", yarutsk.YamlScalar("ignored", style="double"))
        assert '"ignored"' not in yarutsk.dumps(doc)

    def test_sequence_setitem(self):
        doc = yarutsk.loads(
            dedent("""\
            - hello
            - world
        """)
        )
        doc[0] = yarutsk.YamlScalar("replaced", style="double")
        assert '"replaced"' in yarutsk.dumps(doc)

    def test_sequence_append(self):
        doc = yarutsk.loads("- a")
        doc.append(yarutsk.YamlScalar("b", style="single"))
        assert "'b'" in yarutsk.dumps(doc)

    def test_sequence_insert(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - c
        """)
        )
        doc.insert(1, yarutsk.YamlScalar("b", style="double"))
        assert '"b"' in yarutsk.dumps(doc)

    def test_sequence_extend(self):
        doc = yarutsk.loads("[]")
        doc.extend(
            [
                yarutsk.YamlScalar("x", style="single"),
                yarutsk.YamlScalar("y", style="double"),
            ]
        )
        out = yarutsk.dumps(doc)
        assert "'x'" in out
        assert '"y"' in out

    def test_nested_mapping_value(self):
        doc = yarutsk.loads(
            dedent("""\
            outer:
              a: 1
        """)
        )
        doc["outer"]["a"] = yarutsk.YamlScalar("styled", style="double")
        assert '"styled"' in yarutsk.dumps(doc)


class TestStyledContainerAssignmentPaths:
    def test_mapping_setitem_flow_mapping(self):
        doc = yarutsk.loads("p: placeholder")
        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 1
        doc["p"] = m
        assert "{" in yarutsk.dumps(doc)

    def test_mapping_setitem_flow_sequence(self):
        doc = yarutsk.loads("items: placeholder")
        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2])
        doc["items"] = s
        assert "[" in yarutsk.dumps(doc)

    def test_sequence_append_flow_mapping(self):
        doc = yarutsk.loads("[]")
        m = yarutsk.YamlMapping(style="flow")
        m["k"] = "v"
        doc.append(m)
        assert "{" in yarutsk.dumps(doc)

    def test_sequence_append_flow_sequence(self):
        doc = yarutsk.loads("[]")
        inner = yarutsk.YamlSequence(style="flow")
        inner.extend([1, 2])
        doc.append(inner)
        assert "[" in yarutsk.dumps(doc)

    def test_mapping_update_with_styled_container(self):
        doc = yarutsk.loads("a: 1")
        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 9
        doc.update({"sub": m})
        assert "{" in yarutsk.dumps(doc)


class TestDumperWithStyledNodes:
    """Dumper returns (tag, styled_node); style and tag must both appear."""

    def test_scalar_style_and_tuple_tag(self):
        class Foo:
            def __init__(self, v):
                self.v = v

        schema = yarutsk.Schema()
        schema.add_dumper(Foo, lambda obj: ("!foo", yarutsk.YamlScalar(str(obj.v), style="double")))
        doc = yarutsk.YamlMapping()
        doc["x"] = Foo("bar")
        out = yarutsk.dumps(doc, schema=schema)
        assert "!foo" in out
        assert '"bar"' in out

    def test_mapping_style_and_tuple_tag(self):
        class Point:
            def __init__(self, x, y):
                self.x, self.y = x, y

        def dump(p):
            m = yarutsk.YamlMapping(style="flow")
            m["x"] = p.x
            m["y"] = p.y
            return ("!point", m)

        schema = yarutsk.Schema()
        schema.add_dumper(Point, dump)
        doc = yarutsk.YamlMapping()
        doc["p"] = Point(1, 2)
        out = yarutsk.dumps(doc, schema=schema)
        assert "!point" in out
        assert "{" in out
        assert "x: 1" in out

    def test_sequence_style_and_tuple_tag(self):
        class Bag:
            def __init__(self, items):
                self.items = items

        def dump(b):
            s = yarutsk.YamlSequence(style="flow")
            s.extend(b.items)
            return ("!bag", s)

        schema = yarutsk.Schema()
        schema.add_dumper(Bag, dump)
        doc = yarutsk.YamlMapping()
        doc["b"] = Bag([1, 2, 3])
        out = yarutsk.dumps(doc, schema=schema)
        assert "!bag" in out
        assert "[" in out

    def test_tuple_tag_overrides_node_tag(self):
        """Tag from the (tag, data) tuple takes precedence over any tag set on the node."""

        class Bar:
            pass

        schema = yarutsk.Schema()
        schema.add_dumper(
            Bar,
            lambda obj: ("!tuple-tag", yarutsk.YamlScalar("v", tag="!node-tag")),
        )
        doc = yarutsk.YamlMapping()
        doc["x"] = Bar()
        out = yarutsk.dumps(doc, schema=schema)
        assert "!tuple-tag" in out
        assert "!node-tag" not in out

    def test_dumper_block_mapping(self):
        class Cfg:
            pass

        def dump(c):
            m = yarutsk.YamlMapping(style="block")
            m["debug"] = True
            return ("!cfg", m)

        schema = yarutsk.Schema()
        schema.add_dumper(Cfg, dump)
        doc = yarutsk.YamlMapping()
        doc["c"] = Cfg()
        out = yarutsk.dumps(doc, schema=schema)
        assert "!cfg" in out
        assert "{" not in out
        assert "debug" in out

    def test_dumper_scalar_single_quoted(self):
        class Name:
            def __init__(self, v):
                self.v = v

        schema = yarutsk.Schema()
        schema.add_dumper(Name, lambda obj: ("!name", yarutsk.YamlScalar(obj.v, style="single")))
        doc = yarutsk.YamlMapping()
        doc["n"] = Name("alice")
        out = yarutsk.dumps(doc, schema=schema)
        assert "'alice'" in out
        assert "!name" in out


class TestConstructorTagInOutput:
    def test_mapping_tag_nested(self):
        m = yarutsk.YamlMapping(tag="!cfg")
        m["debug"] = True
        doc = yarutsk.YamlMapping()
        doc["settings"] = m
        assert "!cfg" in yarutsk.dumps(doc)

    def test_sequence_tag_nested(self):
        s = yarutsk.YamlSequence(tag="!items")
        s.extend([1, 2])
        doc = yarutsk.YamlMapping()
        doc["data"] = s
        assert "!items" in yarutsk.dumps(doc)

    def test_scalar_tag_nested(self):
        doc = yarutsk.YamlMapping()
        doc["v"] = yarutsk.YamlScalar("x", tag="!scalar-tag")
        assert "!scalar-tag" in yarutsk.dumps(doc)


class TestCommentsFromScratch:
    """Comments attached to freshly-constructed nodes (no source document)."""

    def test_mapping_inline_comment_on_entry(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.node("a").comment_inline = "note"
        assert yarutsk.dumps(doc) == "a: 1  # note\n"

    def test_mapping_block_comment_on_entry(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.node("a").comment_before = "above"
        assert yarutsk.dumps(doc) == "# above\na: 1\n"

    def test_mapping_entry_both_comments(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.node("a").comment_before = "above"
        doc.node("a").comment_inline = "trailing"
        assert yarutsk.dumps(doc) == "# above\na: 1  # trailing\n"

    def test_multi_line_comment_before(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.node("a").comment_before = "line1\nline2\nline3"
        assert yarutsk.dumps(doc) == dedent("""\
            # line1
            # line2
            # line3
            a: 1
        """)

    def test_sequence_item_inline_comment(self):
        seq = yarutsk.YamlSequence()
        seq.extend([1, 2])
        seq.node(0).comment_inline = "one"
        seq.node(1).comment_inline = "two"
        assert yarutsk.dumps(seq) == "- 1  # one\n- 2  # two\n"

    def test_sequence_item_block_comment(self):
        seq = yarutsk.YamlSequence()
        seq.extend([1, 2])
        seq.node(0).comment_before = "first"
        assert yarutsk.dumps(seq) == "# first\n- 1\n- 2\n"

    def test_scalar_constructor_with_comments(self):
        doc = yarutsk.YamlMapping()
        s = yarutsk.YamlScalar("v", style="double")
        s.comment_before = "header"
        s.comment_inline = "note"
        doc["k"] = s
        assert yarutsk.dumps(doc) == '# header\nk: "v"  # note\n'

    def test_comment_clear_roundtrip(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.node("a").comment_inline = "temp"
        doc.node("a").comment_inline = None
        assert yarutsk.dumps(doc) == "a: 1\n"

    def test_nested_mapping_comment_on_child(self):
        outer = yarutsk.YamlMapping()
        inner = yarutsk.YamlMapping()
        inner["x"] = 1
        inner.node("x").comment_inline = "deep"
        outer["inner"] = inner
        out = yarutsk.dumps(outer)
        assert "# deep" in out
        assert "x: 1" in out

    def test_flow_mapping_comments_not_emitted(self):
        """Per-entry comments are dropped when the parent is flow-styled — known
        emitter behavior."""
        m = yarutsk.YamlMapping(style="flow")
        m["a"] = 1
        m.node("a").comment_inline = "ignored"
        doc = yarutsk.YamlMapping()
        doc["p"] = m
        assert "ignored" not in yarutsk.dumps(doc)


class TestAnchorsAndAliasesFromScratch:
    """Anchors and alias references built entirely from Python."""

    def test_anchor_on_scalar(self):
        doc = yarutsk.YamlMapping()
        s = yarutsk.YamlScalar("hi", style="double")
        s.anchor = "greeting"
        doc["a"] = s
        assert yarutsk.dumps(doc) == 'a: &greeting "hi"\n'

    def test_anchor_via_node_setter(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = "hello"
        doc.node("a").anchor = "g"
        assert yarutsk.dumps(doc) == "a: &g hello\n"

    def test_alias_reference_to_scalar(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = yarutsk.YamlScalar("value")
        doc.node("a").anchor = "ref"
        doc["b"] = "placeholder"
        doc.set_alias("b", "ref")
        assert yarutsk.dumps(doc) == "a: &ref value\nb: *ref\n"

    def test_anchor_on_mapping(self):
        inner = yarutsk.YamlMapping()
        inner["x"] = 1
        inner.anchor = "cfg"
        doc = yarutsk.YamlMapping()
        doc["a"] = inner
        out = yarutsk.dumps(doc)
        assert "&cfg" in out
        assert "x: 1" in out

    def test_alias_reference_to_mapping(self):
        inner = yarutsk.YamlMapping()
        inner["x"] = 1
        inner.anchor = "cfg"
        doc = yarutsk.YamlMapping()
        doc["a"] = inner
        doc["b"] = yarutsk.YamlMapping()
        doc.set_alias("b", "cfg")
        out = yarutsk.dumps(doc)
        assert "&cfg" in out
        assert "b: *cfg" in out

    def test_alias_in_sequence(self):
        doc = yarutsk.YamlMapping()
        doc["orig"] = "x"
        doc.node("orig").anchor = "a1"
        seq = yarutsk.YamlSequence()
        seq.append("placeholder")
        seq.set_alias(0, "a1")
        doc["refs"] = seq
        out = yarutsk.dumps(doc)
        assert "&a1" in out
        assert "*a1" in out

    def test_get_alias_returns_anchor_name(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = "v"
        doc.node("a").anchor = "x"
        doc["b"] = "v"
        doc.set_alias("b", "x")
        assert doc.get_alias("b") == "x"
        assert doc.get_alias("a") is None

    def test_anchor_clear_via_none(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = "v"
        doc.node("a").anchor = "n"
        doc.node("a").anchor = None
        assert "&" not in yarutsk.dumps(doc)


class TestBlankLinesFromScratch:
    def test_blank_lines_before_entry(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc["b"] = 2
        doc.node("b").blank_lines_before = 2
        assert yarutsk.dumps(doc) == "a: 1\n\n\nb: 2\n"

    def test_blank_lines_before_clamped_at_255(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc["b"] = 2
        doc.node("b").blank_lines_before = 10_000
        assert doc.node("b").blank_lines_before == 255

    def test_trailing_blank_lines_after_mapping(self):
        inner = yarutsk.YamlMapping()
        inner["x"] = 1
        inner.trailing_blank_lines = 2
        doc = yarutsk.YamlMapping()
        doc["nested"] = inner
        doc["other"] = 2
        out = yarutsk.dumps(doc)
        assert "\n\n\nother:" in out

    def test_trailing_blank_lines_after_sequence(self):
        seq = yarutsk.YamlSequence()
        seq.extend([1, 2])
        seq.trailing_blank_lines = 1
        doc = yarutsk.YamlMapping()
        doc["items"] = seq
        doc["next"] = "x"
        out = yarutsk.dumps(doc)
        assert "\n\nnext:" in out

    def test_blank_lines_before_sequence_item(self):
        seq = yarutsk.YamlSequence()
        seq.extend([1, 2])
        seq.node(1).blank_lines_before = 1
        assert yarutsk.dumps(seq) == "- 1\n\n- 2\n"


class TestDocumentMarkersFromScratch:
    def test_explicit_start_on_mapping(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.explicit_start = True
        assert yarutsk.dumps(doc) == "---\na: 1\n"

    def test_explicit_end_on_mapping(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.explicit_end = True
        assert yarutsk.dumps(doc).endswith("...\n")

    def test_yaml_version_directive(self):
        doc = yarutsk.YamlMapping()
        doc["a"] = 1
        doc.yaml_version = "1.2"
        out = yarutsk.dumps(doc)
        assert out.startswith("%YAML 1.2\n---\n")

    def test_yaml_version_rejects_invalid(self):
        doc = yarutsk.YamlMapping()
        with pytest.raises(ValueError):
            doc.yaml_version = "not-a-version"

    def test_tag_directive(self):
        doc = yarutsk.YamlMapping()
        doc["x"] = 1
        doc.tag_directives = [("!local!", "tag:example.com,2024:")]
        out = yarutsk.dumps(doc)
        assert "%TAG !local! tag:example.com,2024:" in out

    def test_all_document_markers_on_sequence(self):
        seq = yarutsk.YamlSequence()
        seq.append(1)
        seq.explicit_start = True
        seq.explicit_end = True
        seq.yaml_version = "1.2"
        out = yarutsk.dumps(seq)
        assert out.startswith("%YAML 1.2\n---\n")
        assert out.rstrip().endswith("...")

    def test_document_markers_on_scalar(self):
        s = yarutsk.YamlScalar("hello", style="double")
        s.explicit_start = True
        s.explicit_end = True
        out = yarutsk.dumps(s)
        assert "---" in out
        assert "..." in out


class TestSpecialScalarsFromScratch:
    def test_date_scalar(self):
        doc = yarutsk.YamlMapping()
        doc["when"] = yarutsk.YamlScalar(datetime.date(2024, 1, 15))
        out = yarutsk.dumps(doc)
        assert "!!timestamp 2024-01-15" in out

    def test_datetime_scalar(self):
        doc = yarutsk.YamlMapping()
        doc["at"] = yarutsk.YamlScalar(datetime.datetime(2024, 6, 30, 12, 34, 56))
        out = yarutsk.dumps(doc)
        assert "!!timestamp 2024-06-30T12:34:56" in out

    def test_bytes_scalar_base64_encoded(self):
        doc = yarutsk.YamlMapping()
        doc["blob"] = yarutsk.YamlScalar(b"hello")
        out = yarutsk.dumps(doc)
        assert "!!binary" in out
        assert "aGVsbG8=" in out  # base64 of "hello"

    def test_bytearray_scalar(self):
        doc = yarutsk.YamlMapping()
        doc["blob"] = yarutsk.YamlScalar(bytearray(b"abc"))
        out = yarutsk.dumps(doc)
        assert "!!binary" in out

    def test_date_roundtrip_via_load(self):
        """A date built from scratch re-loads as a datetime.date."""
        doc = yarutsk.YamlMapping()
        doc["d"] = yarutsk.YamlScalar(datetime.date(2024, 1, 1))
        reloaded = yarutsk.loads(yarutsk.dumps(doc))
        assert reloaded["d"] == datetime.date(2024, 1, 1)

    def test_bytes_roundtrip_via_load(self):
        doc = yarutsk.YamlMapping()
        doc["b"] = yarutsk.YamlScalar(b"payload")
        reloaded = yarutsk.loads(yarutsk.dumps(doc))
        assert reloaded["b"] == b"payload"

    def test_custom_tag_overrides_binary_default(self):
        doc = yarutsk.YamlMapping()
        doc["b"] = yarutsk.YamlScalar(b"x", tag="!raw")
        out = yarutsk.dumps(doc)
        assert "!raw" in out
        assert "!!binary" not in out


class TestCombinedFromScratch:
    """Build a document that exercises every surface in one go, then verify
    the emitted YAML and a load→compare round-trip."""

    def test_build_from_scratch_with_all_features(self):
        # Top-level mapping with document markers + %YAML directive.
        doc = yarutsk.YamlMapping()
        doc.explicit_start = True
        doc.yaml_version = "1.2"

        # Plain entry with a block comment.
        doc["name"] = "demo"
        doc.node("name").comment_before = "the demo config"

        # Scalar with a custom style + anchor.
        addr = yarutsk.YamlScalar("127.0.0.1", style="double")
        addr.anchor = "localhost"
        doc["primary"] = addr

        # Alias back to the anchored scalar, with a blank line between the two.
        doc["fallback"] = "placeholder"
        doc.set_alias("fallback", "localhost")
        doc.node("fallback").blank_lines_before = 1

        # Nested flow-style sequence with a tag.
        ports = yarutsk.YamlSequence(style="flow", tag="!ports")
        ports.extend([80, 443])
        doc["ports"] = ports

        # Nested block-style mapping with a trailing inline comment on an entry.
        meta = yarutsk.YamlMapping()
        meta["created"] = yarutsk.YamlScalar(datetime.date(2024, 1, 1))
        meta.node("created").comment_inline = "ISO"
        doc["meta"] = meta

        out = yarutsk.dumps(doc)

        # Spot-check every feature landed in the output.
        assert "%YAML 1.2" in out
        assert out.split("\n", 2)[1] == "---"
        assert "# the demo config" in out
        assert 'primary: &localhost "127.0.0.1"' in out
        assert "fallback: *localhost" in out
        assert "\n\nfallback:" in out  # blank_lines_before = 1
        assert "!ports" in out
        assert "[80, 443]" in out
        assert "!!timestamp 2024-01-01" in out
        assert "# ISO" in out

        # Re-load and confirm the structural + value view survives.
        loaded = yarutsk.loads(out)
        assert loaded["name"] == "demo"
        assert loaded["primary"] == "127.0.0.1"
        assert loaded["fallback"] == "127.0.0.1"  # alias resolved
        assert list(loaded["ports"]) == [80, 443]
        assert loaded["meta"]["created"] == datetime.date(2024, 1, 1)


class TestKnownLimitations:
    """Document known emitter limitations so they don't become surprise failures."""

    def test_top_level_mapping_tag_not_emitted(self):
        # Tags on a top-level mapping document are currently not emitted.
        # This is a pre-existing emitter limitation, not specific to constructors.
        m = yarutsk.YamlMapping(tag="!top")
        m["k"] = "v"
        assert "!top" not in yarutsk.dumps(m)  # tag is silently dropped by the emitter

    def test_top_level_sequence_tag_not_emitted(self):
        # Same limitation for top-level sequences.
        s = yarutsk.YamlSequence(tag="!top")
        s.append(1)
        assert "!top" not in yarutsk.dumps(s)

    def test_non_string_scalar_style_not_applied(self):
        # Requesting double-quoted style on an int/float/bool/null scalar
        # is ignored; the emitter always emits their native representation.
        doc = yarutsk.YamlMapping()
        doc["k"] = yarutsk.YamlScalar(42, style="double")
        assert yarutsk.dumps(doc) == dedent("""\
            k: 42
        """)
