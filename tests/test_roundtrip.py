"""Tests for round-trip fidelity: scalar styles, container styles,
alias expansion, tags, and explicit document markers."""

import pytest

try:
    import yarutsk

    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

pytestmark = pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk module not built")


class TestRoundTripScalarStyles:
    """Scalar quoting/block styles are preserved through load → dump → load."""

    def test_plain_style_unchanged(self):
        doc = yarutsk.loads("key: hello")
        out = yarutsk.dumps(doc)
        assert out == "key: hello\n"

    def test_single_quoted_style_preserved(self):
        doc = yarutsk.loads("key: 'hello'")
        out = yarutsk.dumps(doc)
        assert out == "key: 'hello'\n"

    def test_double_quoted_style_preserved(self):
        doc = yarutsk.loads('key: "hello"')
        out = yarutsk.dumps(doc)
        assert out == 'key: "hello"\n'

    def test_single_quoted_type_lookalike_preserved(self):
        """'true' as a string should stay single-quoted so it round-trips as str."""
        doc = yarutsk.loads("key: 'true'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "true"
        assert isinstance(doc2["key"], str)

    def test_double_quoted_type_lookalike_preserved(self):
        doc = yarutsk.loads('key: "42"')
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "42"
        assert isinstance(doc2["key"], str)

    def test_literal_block_style_preserved(self):
        src = "text: |\n  line one\n  line two\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "|\n" in out
        doc2 = yarutsk.loads(out)
        assert doc2["text"] == "line one\nline two\n"

    def test_folded_block_style_preserved(self):
        src = "text: >\n  folded line\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert ">\n" in out
        doc2 = yarutsk.loads(out)
        assert doc2["text"] == "folded line\n"

    def test_plain_integer_roundtrips(self):
        doc = yarutsk.loads("n: 42")
        out = yarutsk.dumps(doc)
        assert out == "n: 42\n"
        doc2 = yarutsk.loads(out)
        assert doc2["n"] == 42

    def test_plain_bool_roundtrips(self):
        doc = yarutsk.loads("flag: true")
        out = yarutsk.dumps(doc)
        assert out == "flag: true\n"
        doc2 = yarutsk.loads(out)
        assert doc2["flag"] is True

    def test_plain_null_roundtrips(self):
        doc = yarutsk.loads("nothing: null")
        out = yarutsk.dumps(doc)
        assert out == "nothing: null\n"
        doc2 = yarutsk.loads(out)
        assert doc2["nothing"] is None

    def test_scalar_style_attribute(self):
        """YamlScalar.style attribute reflects the source quoting style."""
        doc = yarutsk.loads("a: plain\nb: 'single'\nc: \"double\"")
        assert doc.get_node("a").style == "plain"
        assert doc.get_node("b").style == "single"
        assert doc.get_node("c").style == "double"

    def test_scalar_style_can_be_changed(self):
        """Changing scalar style via set_scalar_style affects how the value is emitted."""
        doc = yarutsk.loads("key: hello")
        doc.set_scalar_style("key", "double")
        out = yarutsk.dumps(doc)
        assert out == 'key: "hello"\n'


class TestRoundTripContainerStyles:
    """Flow vs block container style is preserved through load → dump → load."""

    def test_flow_sequence_value_emitted_inline(self):
        src = "tags: [a, b, c]\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert out == src

    def test_flow_mapping_value_emitted_inline(self):
        src = "point: {x: 1, y: 2}\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert out == src

    def test_flow_sequence_roundtrips(self):
        src = "tags: [a, b, c]\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert list(doc2["tags"]) == ["a", "b", "c"]

    def test_flow_mapping_roundtrips(self):
        src = "point: {x: 1, y: 2}\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["point"]["x"] == 1
        assert doc2["point"]["y"] == 2

    def test_block_sequence_stays_block(self):
        src = "items:\n  - a\n  - b\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "- a" in out
        assert "- b" in out
        assert "[" not in out

    def test_block_mapping_stays_block(self):
        src = "nested:\n  x: 1\n  y: 2\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "{" not in out
        assert "x: 1" in out

    def test_top_level_flow_sequence(self):
        src = "[a, b, c]"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert list(doc2) == ["a", "b", "c"]

    def test_top_level_flow_mapping(self):
        src = "{x: 1, y: 2}"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["x"] == 1
        assert doc2["y"] == 2

    def test_empty_flow_mapping_value_inline(self):
        doc = yarutsk.loads("key: {}")
        out = yarutsk.dumps(doc)
        assert "{}" in out
        assert out == "key: {}\n"

    def test_empty_flow_sequence_value_inline(self):
        doc = yarutsk.loads("key: []")
        out = yarutsk.dumps(doc)
        assert out == "key: []\n"

    def test_nested_flow_in_block_roundtrips(self):
        """A block mapping with a flow sequence value round-trips correctly."""
        src = "name: demo\ntags: [x, y]\ncount: 3\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["name"] == "demo"
        assert list(doc2["tags"]) == ["x", "y"]
        assert doc2["count"] == 3


class TestRoundTripAliasExpansion:
    """Aliases are expanded in-place at load time (no *name in output)."""

    def test_alias_expands_to_value(self):
        src = "default: &base 42\nactual: *base\n"
        doc = yarutsk.loads(src)
        assert doc["actual"] == 42

    def test_alias_expands_independently(self):
        """Mutations to expanded alias do not affect the anchor site."""
        src = "a: &anchor {x: 1}\nb: *anchor\n"
        doc = yarutsk.loads(src)
        doc["b"]["x"] = 99
        assert doc["a"]["x"] == 1

    def test_alias_roundtrips_as_value(self):
        """After expansion, dumps produces the full values (no *alias syntax)."""
        src = "base: &b hello\ncopy: *b\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "*" not in out
        doc2 = yarutsk.loads(out)
        assert doc2["copy"] == "hello"

    def test_merge_key_expands(self):
        """YAML merge keys (<<: *anchor) expand the referenced mapping."""
        src = "defaults: &def\n  timeout: 30\n  retries: 3\nservice:\n  <<: *def\n  name: api\n"
        doc = yarutsk.loads(src)
        assert doc["service"]["name"] == "api"

    def test_sequence_alias_expands(self):
        src = "orig: &items\n  - a\n  - b\ncopy: *items\n"
        doc = yarutsk.loads(src)
        assert list(doc["copy"]) == ["a", "b"]

    def test_alias_dump_is_reloadable(self):
        src = "x: &v 100\ny: *v\nz: *v\n"
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["x"] == 100
        assert doc2["y"] == 100
        assert doc2["z"] == 100


class TestRoundTripTagAccess:
    """Tags are accessible via get_tag() but are not emitted into the dump text."""

    def test_scalar_tag_loaded(self):
        doc = yarutsk.loads("value: !!str 42")
        node = doc.get_node("value")
        assert node.get_tag() is not None

    def test_mapping_tag_loaded(self):
        doc = yarutsk.loads("!!python/object:dict {a: 1}")
        assert doc.get_tag() is not None

    def test_scalar_no_tag_returns_none(self):
        doc = yarutsk.loads("value: hello")
        node = doc.get_node("value")
        assert node.get_tag() is None

    def test_set_tag_on_scalar(self):
        doc = yarutsk.loads("key: hello")
        node = doc.get_node("key")
        node.set_tag("!!str")
        assert node.get_tag() == "!!str"

    def test_set_tag_none_clears_tag(self):
        doc = yarutsk.loads("value: !!str 42")
        node = doc.get_node("value")
        node.set_tag(None)
        assert node.get_tag() is None

    def test_tags_not_emitted_in_dump(self):
        """Tags do not appear in the emitted YAML text.
        !!str forces the scalar to be a string, so !!str 42 loads as "42" (str)."""
        doc = yarutsk.loads("value: !!str 42")
        assert doc["value"] == "42"
        assert isinstance(doc["value"], str)
        out = yarutsk.dumps(doc)
        assert "!!" not in out
        doc2 = yarutsk.loads(out)
        assert doc2["value"] == "42"
        assert isinstance(doc2["value"], str)

    def test_set_tag_on_mapping(self):
        doc = yarutsk.loads("a: 1")
        doc.set_tag("!!map")
        assert doc.get_tag() == "!!map"

    def test_set_tag_on_sequence(self):
        doc = yarutsk.loads("- 1\n- 2")
        doc.set_tag("!!seq")
        assert doc.get_tag() == "!!seq"


class TestExplicitDocumentMarker:
    """The --- document-start marker is preserved through load → dump."""

    def test_no_marker_not_emitted(self):
        doc = yarutsk.loads("key: value")
        assert not doc.explicit_start
        assert yarutsk.dumps(doc) == "key: value\n"

    def test_marker_preserved_on_mapping(self):
        doc = yarutsk.loads("---\nkey: value")
        assert doc.explicit_start
        assert yarutsk.dumps(doc) == "---\nkey: value\n"

    def test_marker_preserved_on_sequence(self):
        doc = yarutsk.loads("---\n- a\n- b")
        assert doc.explicit_start
        assert yarutsk.dumps(doc) == "---\n- a\n- b\n"

    def test_marker_preserved_on_scalar(self):
        doc = yarutsk.loads("---\n42")
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        assert yarutsk.loads(out) == 42

    def test_marker_roundtrips(self):
        src = "---\nname: Alice\nage: 30\n"
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_no_marker_roundtrips(self):
        src = "name: Alice\nage: 30\n"
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_explicit_start_can_be_set(self):
        """Setting explicit_start=True adds --- on next dump."""
        doc = yarutsk.loads("key: value")
        assert not doc.explicit_start
        doc.explicit_start = True
        assert yarutsk.dumps(doc) == "---\nkey: value\n"

    def test_explicit_start_can_be_cleared(self):
        """Setting explicit_start=False removes --- from dump."""
        doc = yarutsk.loads("---\nkey: value")
        doc.explicit_start = False
        assert yarutsk.dumps(doc) == "key: value\n"

    def test_multiline_value_with_marker(self):
        src = "---\ntext: |\n  line one\n  line two\n"
        doc = yarutsk.loads(src)
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        doc2 = yarutsk.loads(out)
        assert doc2["text"] == "line one\nline two\n"
