"""Tests for round-trip fidelity: scalar styles, container styles,
alias expansion, tags, and explicit document markers."""

import pytest
from textwrap import dedent

import yarutsk


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
        src = dedent("""\
            text: |
              line one
              line two
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "|\n" in out
        doc2 = yarutsk.loads(out)
        assert doc2["text"] == "line one\nline two\n"

    def test_folded_block_style_preserved(self):
        src = dedent("""\
            text: >
              folded line
        """)
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
        doc = yarutsk.loads(
            dedent("""\
            a: plain
            b: 'single'
            c: "double"
        """)
        )
        assert doc.node("a").style == "plain"
        assert doc.node("b").style == "single"
        assert doc.node("c").style == "double"

    def test_scalar_style_can_be_changed(self):
        """Changing scalar style via set_scalar_style affects how the value is emitted."""
        doc = yarutsk.loads("key: hello")
        doc.scalar_style("key", "double")
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
        src = dedent("""\
            items:
              - a
              - b
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "- a" in out
        assert "- b" in out
        assert "[" not in out

    def test_block_mapping_stays_block(self):
        src = dedent("""\
            nested:
              x: 1
              y: 2
        """)
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
        src = dedent("""\
            name: demo
            tags: [x, y]
            count: 3
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["name"] == "demo"
        assert list(doc2["tags"]) == ["x", "y"]
        assert doc2["count"] == 3


class TestRoundTripAliasExpansion:
    """Aliases are expanded in-place at load time (no *name in output)."""

    def test_alias_expands_to_value(self):
        src = dedent("""\
            default: &base 42
            actual: *base
        """)
        doc = yarutsk.loads(src)
        assert doc["actual"] == 42

    def test_alias_expands_independently(self):
        """Mutations to expanded alias do not affect the anchor site."""
        src = dedent("""\
            a: &anchor {x: 1}
            b: *anchor
        """)
        doc = yarutsk.loads(src)
        doc["b"]["x"] = 99
        assert doc["a"]["x"] == 1

    def test_alias_roundtrips_as_value(self):
        """Aliases are preserved in output: *name round-trips faithfully."""
        src = dedent("""\
            base: &b hello
            copy: *b
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert out == src
        doc2 = yarutsk.loads(out)
        assert doc2["copy"] == "hello"

    def test_merge_key_expands(self):
        """YAML merge keys (<<: *anchor) expand the referenced mapping."""
        src = dedent("""\
            defaults: &def
              timeout: 30
              retries: 3
            service:
              <<: *def
              name: api
        """)
        doc = yarutsk.loads(src)
        assert doc["service"]["name"] == "api"

    def test_sequence_alias_expands(self):
        src = dedent("""\
            orig: &items
              - a
              - b
            copy: *items
        """)
        doc = yarutsk.loads(src)
        assert list(doc["copy"]) == ["a", "b"]

    def test_alias_dump_is_reloadable(self):
        src = dedent("""\
            x: &v 100
            y: *v
            z: *v
        """)
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
        node = doc.node("value")
        assert node.tag is not None

    def test_mapping_tag_loaded(self):
        doc = yarutsk.loads("!!python/object:dict {a: 1}")
        assert doc.tag is not None

    def test_scalar_no_tag_returns_none(self):
        doc = yarutsk.loads("value: hello")
        node = doc.node("value")
        assert node.tag is None

    def test_set_tag_on_scalar(self):
        doc = yarutsk.loads("key: hello")
        node = doc.node("key")
        node.tag = "!!str"
        assert node.tag == "!!str"

    def test_set_tag_none_clears_tag(self):
        doc = yarutsk.loads("value: !!str 42")
        node = doc.node("value")
        node.tag = None
        assert node.tag is None

    def test_tags_emitted_in_dump(self):
        """Tags are preserved in emitted YAML for round-trip fidelity."""
        doc = yarutsk.loads("value: !!str 42")
        assert doc["value"] == "42"
        assert isinstance(doc["value"], str)
        out = yarutsk.dumps(doc)
        assert out == "value: !!str 42\n"
        doc2 = yarutsk.loads(out)
        assert doc2["value"] == "42"
        assert isinstance(doc2["value"], str)

    def test_set_tag_on_mapping(self):
        doc = yarutsk.loads("a: 1")
        doc.tag = "!!map"
        assert doc.tag == "!!map"

    def test_set_tag_on_sequence(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        doc.tag = "!!seq"
        assert doc.tag == "!!seq"


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
        doc = yarutsk.loads(
            dedent("""\
            ---
            - a
            - b
        """)
        )
        assert doc.explicit_start
        assert yarutsk.dumps(doc) == dedent("""\
            ---
            - a
            - b
        """)

    def test_marker_preserved_on_scalar(self):
        doc = yarutsk.loads("---\n42")
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        assert yarutsk.loads(out) == 42

    def test_marker_roundtrips(self):
        src = dedent("""\
            ---
            name: Alice
            age: 30
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_no_marker_roundtrips(self):
        src = dedent("""\
            name: Alice
            age: 30
        """)
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
        src = dedent("""\
            ---
            text: |
              line one
              line two
        """)
        doc = yarutsk.loads(src)
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        doc2 = yarutsk.loads(out)
        assert doc2["text"] == "line one\nline two\n"


class TestBlankLinePreservation:
    """Blank lines between mapping entries and sequence items are preserved."""

    def test_single_blank_line_between_keys(self):
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_multiple_blank_lines_between_keys(self):
        src = dedent("""\
            a: 1


            b: 2
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_no_blank_lines_unaffected(self):
        src = dedent("""\
            a: 1
            b: 2
            c: 3
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_between_some_keys(self):
        src = dedent("""\
            a: 1
            b: 2

            c: 3
            d: 4
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_blank_line_with_comment(self):
        src = dedent("""\
            x: 1

            # note
            y: 2
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_in_sequence(self):
        src = dedent("""\
            - a

            - b


            - c
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_in_nested_mapping(self):
        src = dedent("""\
            outer:
              a: 1

              b: 2
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_between_top_and_nested(self):
        src = dedent("""\
            section1:
              x: 1

            section2:
              y: 2
        """)
        doc = yarutsk.loads(src)
        assert yarutsk.dumps(doc) == src


class TestNonCanonicalScalarForms:
    """Non-canonical plain scalars round-trip as their original source text."""

    def test_null_tilde(self):
        src = "x: ~\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_null_capitalized(self):
        src = "x: Null\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_null_uppercase(self):
        src = "x: NULL\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_bool_yes(self):
        src = "x: yes\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_no(self):
        src = "x: no\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_on(self):
        src = "x: on\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_off(self):
        src = "x: off\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_capitalized_true(self):
        src = "x: True\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_capitalized_false(self):
        src = "x: False\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_uppercase_true(self):
        src = "x: TRUE\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_uppercase_false(self):
        src = "x: FALSE\n"
        doc = yarutsk.loads(src)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_hex_integer(self):
        src = "x: 0xFF\n"
        doc = yarutsk.loads(src)
        assert doc["x"] == 255
        assert yarutsk.dumps(doc) == src

    def test_hex_uppercase_prefix(self):
        src = "x: 0XFF\n"
        doc = yarutsk.loads(src)
        assert doc["x"] == 255
        assert yarutsk.dumps(doc) == src

    def test_octal_integer(self):
        src = "x: 0o77\n"
        doc = yarutsk.loads(src)
        assert doc["x"] == 63
        assert yarutsk.dumps(doc) == src

    def test_underscore_integer(self):
        # Underscore-separated integers are not parsed by Rust's i64::parse, so the
        # value is stored as a string — but the source form is preserved in the output.
        src = "x: 1_000_000\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_float_exponent_form(self):
        src = "x: 1.5e10\n"
        doc = yarutsk.loads(src)
        assert doc["x"] == 1.5e10
        assert yarutsk.dumps(doc) == src

    def test_canonical_null_unchanged(self):
        """Plain 'null' is canonical and should round-trip as 'null'."""
        src = "x: null\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_canonical_bool_unchanged(self):
        """Plain 'true'/'false' are canonical and should round-trip unchanged."""
        assert yarutsk.dumps(yarutsk.loads("x: true\n")) == "x: true\n"
        assert yarutsk.dumps(yarutsk.loads("x: false\n")) == "x: false\n"

    def test_non_canonical_in_sequence(self):
        """Non-canonical scalars inside a sequence are preserved."""
        src = dedent("""\
            - yes
            - no
            - ~
            - 0xFF
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src


class TestTagRoundTrip:
    """Tags are preserved through load → dump → load."""

    def test_str_tag_on_integer_looking_scalar(self):
        src = "x: !!str 42\n"
        doc = yarutsk.loads(src)
        assert doc["x"] == "42"
        assert isinstance(doc["x"], str)
        assert yarutsk.dumps(doc) == src

    def test_str_tag_roundtrips_value(self):
        src = "x: !!str 42\n"
        doc2 = yarutsk.loads(yarutsk.dumps(yarutsk.loads(src)))
        assert doc2["x"] == "42"
        assert isinstance(doc2["x"], str)

    def test_str_tag_on_bool_looking_scalar(self):
        src = "flag: !!str true\n"
        doc = yarutsk.loads(src)
        assert doc["flag"] == "true"
        assert isinstance(doc["flag"], str)
        assert yarutsk.dumps(doc) == src

    def test_tag_on_top_level_scalar(self):
        src = "!!str 42\n"
        doc = yarutsk.loads(src)
        assert doc == "42"
        assert yarutsk.dumps(doc) == src

    def test_custom_tag_on_flow_sequence(self):
        src = "x: !!python/tuple [1, 2]\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_tag_accessible_after_load(self):
        """get_node returns a snapshot; set_tag on it does not mutate the mapping."""
        doc = yarutsk.loads("value: !!str 42")
        node = doc.node("value")
        assert node.tag is not None
        node.tag = None
        # The change is local to the snapshot — the mapping still emits the tag.
        assert "!!" in yarutsk.dumps(doc)

    def test_tag_on_multiple_keys(self):
        src = dedent("""\
            a: !!str 1
            b: !!str 2
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_tag_on_top_level_sequence_accessible(self):
        """Tag on a top-level block sequence is parsed and accessible via get_tag()."""
        doc = yarutsk.loads(
            dedent("""\
            !!python/tuple
            - 1
            - 2
        """)
        )
        assert doc.tag is not None
        assert list(doc) == [1, 2]

    def test_tag_on_sequence_item_via_mapping(self):
        """Tag on a value that is a sequence inside a mapping."""
        src = "x: !!python/tuple [1, 2]\n"
        doc = yarutsk.loads(src)
        seq = doc["x"]
        assert seq.tag is not None
        assert yarutsk.dumps(doc) == src


class TestAnchorAliasRoundTrip:
    """Anchors (&name) and aliases (*name) are preserved through load → dump."""

    def test_scalar_anchor_and_alias(self):
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_alias_value_is_accessible(self):
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        doc = yarutsk.loads(src)
        assert doc["y"] == "value"

    def test_integer_anchor_and_alias(self):
        src = dedent("""\
            base: &n 42
            copy: *n
        """)
        doc = yarutsk.loads(src)
        assert doc["copy"] == 42
        assert yarutsk.dumps(doc) == src

    def test_multiple_aliases_same_anchor(self):
        src = dedent("""\
            x: &v 100
            y: *v
            z: *v
        """)
        doc = yarutsk.loads(src)
        assert doc["y"] == 100
        assert doc["z"] == 100
        assert yarutsk.dumps(doc) == src

    def test_flow_sequence_anchor(self):
        src = dedent("""\
            items: &mylist [1, 2, 3]
            ref: *mylist
        """)
        doc = yarutsk.loads(src)
        assert list(doc["ref"]) == [1, 2, 3]
        assert yarutsk.dumps(doc) == src

    def test_block_mapping_anchor(self):
        src = dedent("""\
            base: &base
              a: 1
              b: 2
            child: *base
        """)
        doc = yarutsk.loads(src)
        assert doc["child"]["a"] == 1
        assert yarutsk.dumps(doc) == src

    def test_block_sequence_anchor(self):
        src = dedent("""\
            orig: &items
              - a
              - b
            copy: *items
        """)
        doc = yarutsk.loads(src)
        assert list(doc["copy"]) == ["a", "b"]
        assert yarutsk.dumps(doc) == src

    def test_anchor_mutation_does_not_affect_alias(self):
        """Mutations to the anchor site do not affect the alias (they are independent)."""
        src = dedent("""\
            a: &anchor {x: 1}
            b: *anchor
        """)
        doc = yarutsk.loads(src)
        doc["b"]["x"] = 99
        assert doc["a"]["x"] == 1

    def test_alias_dump_is_reloadable(self):
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        doc2 = yarutsk.loads(yarutsk.dumps(yarutsk.loads(src)))
        assert doc2["y"] == "value"


class TestExplicitEndMarker:
    """The ... document-end marker is preserved and settable."""

    def test_end_marker_not_present_by_default(self):
        doc = yarutsk.loads("key: value")
        assert not doc.explicit_end
        assert "..." not in yarutsk.dumps(doc)

    def test_end_marker_preserved_on_load(self):
        doc = yarutsk.loads("key: value\n...")
        assert doc.explicit_end
        assert yarutsk.dumps(doc).endswith("...\n")

    def test_end_marker_roundtrips(self):
        src = "a: 1\n...\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_explicit_end_can_be_set(self):
        doc = yarutsk.loads("key: value")
        doc.explicit_end = True
        assert yarutsk.dumps(doc) == "key: value\n...\n"

    def test_explicit_end_can_be_cleared(self):
        doc = yarutsk.loads("key: value\n...")
        doc.explicit_end = False
        assert "..." not in yarutsk.dumps(doc)

    def test_both_markers_together(self):
        src = dedent("""\
            ---
            a: 1
            ...
        """)
        doc = yarutsk.loads(src)
        assert doc.explicit_start
        assert doc.explicit_end
        assert yarutsk.dumps(doc) == src

    def test_explicit_end_multidoc(self):
        src = dedent("""\
            ---
            a: 1
            ...
            ---
            b: 2
        """)
        docs = yarutsk.loads_all(src)
        assert docs[0].explicit_end
        assert not docs[1].explicit_end
        assert yarutsk.dumps_all(docs) == src


class TestKeyMetadataRoundTrip:
    """Key anchors and tags are preserved through load → dump → load."""

    def test_key_anchor_preserved(self):
        """Key anchor round-trips — the value is accessible after re-parse."""
        src = "&ka key: value\n"
        out = yarutsk.dumps(yarutsk.loads(src))
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "value"

    def test_key_tag_preserved(self):
        """Key tag (!!str) is emitted and the key is still accessible."""
        src = "!!str key: value\n"
        out = yarutsk.dumps(yarutsk.loads(src))
        assert "!!str" in out
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "value"

    def test_alias_as_key_preserved(self):
        """*alias used as a mapping key round-trips as an explicit-key form."""
        src = dedent("""\
            anchor: &ak value
            ? *ak
            : other
        """)
        out = yarutsk.dumps(yarutsk.loads(src))
        doc2 = yarutsk.loads(out)
        assert "anchor" in doc2


class TestBinaryTagRoundTrip:
    """!!binary scalars round-trip as Python bytes."""

    def test_binary_load_returns_bytes(self):
        doc = yarutsk.loads("data: !!binary aGVsbG8=\n")
        assert doc["data"] == b"hello"
        assert isinstance(doc["data"], bytes)

    def test_binary_roundtrip_preserves_source(self):
        src = "data: !!binary aGVsbG8=\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_binary_with_whitespace_in_value(self):
        # YAML binary values may contain whitespace (e.g. line-wrapped base64)
        doc = yarutsk.loads(
            dedent("""\
            data: !!binary aGVs
              bG8=
        """)
        )
        assert doc["data"] == b"hello"

    def test_binary_dump_from_bytes(self):
        import yarutsk as yr

        mapping = yr.loads("x: placeholder\n")
        mapping["x"] = b"hello"
        out = yr.dumps(mapping)
        assert "!!binary" in out
        doc2 = yr.loads(out)
        assert doc2["x"] == b"hello"


class TestTimestampTagRoundTrip:
    """!!timestamp scalars round-trip as Python datetime objects."""

    import datetime as _dt

    def test_timestamp_datetime_load(self):
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15T10:30:00\n")
        assert doc["ts"] == datetime.datetime(2024, 1, 15, 10, 30, 0)
        assert isinstance(doc["ts"], datetime.datetime)

    def test_timestamp_date_only_load(self):
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15\n")
        assert doc["ts"] == datetime.date(2024, 1, 15)
        assert isinstance(doc["ts"], datetime.date)

    def test_timestamp_roundtrip_preserves_source(self):
        src = "ts: !!timestamp 2024-01-15T10:30:00\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_timestamp_space_separator(self):
        # YAML allows space instead of T between date and time
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15 10:30:00\n")
        assert isinstance(doc["ts"], datetime.datetime)
        assert doc["ts"].year == 2024
        assert doc["ts"].hour == 10

    def test_timestamp_dump_from_datetime(self):
        import datetime
        import yarutsk as yr

        mapping = yr.loads("x: placeholder\n")
        mapping["x"] = datetime.datetime(2024, 1, 15, 10, 30, 0)
        out = yr.dumps(mapping)
        assert "!!timestamp" in out
        doc2 = yr.loads(out)
        assert doc2["x"] == datetime.datetime(2024, 1, 15, 10, 30, 0)


class TestContainerStyle:
    """style property: read block/flow from source and switch between them."""

    # ── YamlMapping ──────────────────────────────────────────────────────────

    def test_mapping_block_style_default(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert doc.style == "block"

    def test_mapping_flow_style_roundtrip(self):
        doc = yarutsk.loads("{a: 1, b: 2}")
        assert doc.style == "flow"
        assert yarutsk.dumps(doc) == "{a: 1, b: 2}\n"

    def test_mapping_block_to_flow(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc.style = "flow"
        out = yarutsk.dumps(doc)
        assert out.startswith("{")
        assert "a: 1" in out
        assert "b: 2" in out

    def test_mapping_flow_to_block(self):
        doc = yarutsk.loads("{a: 1, b: 2}")
        doc.style = "block"
        out = yarutsk.dumps(doc)
        assert "a: 1\n" in out
        assert "b: 2\n" in out

    def test_mapping_style_invalid_raises(self):
        doc = yarutsk.loads("a: 1\n")
        with pytest.raises(ValueError):
            doc.style = "invalid"

    # ── YamlSequence ─────────────────────────────────────────────────────────

    def test_sequence_block_style_default(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        assert doc.style == "block"

    def test_sequence_flow_style_roundtrip(self):
        doc = yarutsk.loads("[1, 2, 3]")
        assert doc.style == "flow"
        assert yarutsk.dumps(doc) == "[1, 2, 3]\n"

    def test_sequence_block_to_flow(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        doc.style = "flow"
        out = yarutsk.dumps(doc)
        assert out.startswith("[")

    def test_sequence_style_invalid_raises(self):
        doc = yarutsk.loads("- 1\n")
        with pytest.raises(ValueError):
            doc.style = "bad"


class TestContainerStyleSetter:
    """container_style(key/idx, style) sets the block/flow style of a nested
    mapping or sequence value directly, without going through node() clones."""

    # ── nested sequences inside a mapping ────────────────────────────────────

    def test_mapping_value_default_block_after_plain_list_assign(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = ["a", "b", "c"]
        assert doc.node("k").style == "block"

    def test_mapping_set_seq_value_to_flow(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = ["a", "b", "c"]
        doc.container_style("k", "flow")
        assert "[" in yarutsk.dumps(doc)

    def test_mapping_set_seq_value_back_to_block(self):
        doc = yarutsk.loads("k: [a, b, c]\n")
        assert doc.node("k").style == "flow"
        doc.container_style("k", "block")
        out = yarutsk.dumps(doc)
        assert "- a\n" in out

    def test_mapping_node_clone_does_not_affect_dump(self):
        """node() returns a clone — style mutations on it are silently ignored."""
        doc = yarutsk.loads("k: [1, 2]\n")
        clone = doc.node("k")
        clone.style = "block"
        # dump must still use the stored (flow) style
        assert "[" in yarutsk.dumps(doc)

    def test_mapping_container_style_key_error(self):
        doc = yarutsk.loads("a: 1\n")
        with pytest.raises(KeyError):
            doc.container_style("missing", "flow")

    def test_mapping_container_style_invalid_raises(self):
        doc = yarutsk.loads("k: [a, b]\n")
        with pytest.raises(ValueError):
            doc.container_style("k", "bad")

    def test_mapping_container_style_on_scalar_is_noop(self):
        """container_style on a scalar value silently does nothing."""
        doc = yarutsk.loads("k: hello\n")
        doc.container_style("k", "flow")
        assert yarutsk.dumps(doc) == "k: hello\n"

    # ── nested mappings inside a mapping ─────────────────────────────────────

    def test_mapping_nested_mapping_set_to_flow(self):
        doc = yarutsk.loads(
            dedent("""\
            k:
              a: 1
              b: 2
        """)
        )
        assert doc.node("k").style == "block"
        doc.container_style("k", "flow")
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_mapping_nested_mapping_set_to_block(self):
        doc = yarutsk.loads("k: {a: 1, b: 2}\n")
        doc.container_style("k", "block")
        out = yarutsk.dumps(doc)
        assert "a: 1\n" in out

    # ── nested containers inside a sequence ──────────────────────────────────

    def test_sequence_item_set_to_flow(self):
        doc = yarutsk.loads(
            dedent("""\
            - - a
              - b
        """)
        )
        assert doc.node(0).style == "block"
        doc.container_style(0, "flow")
        out = yarutsk.dumps(doc)
        assert "[" in out

    def test_sequence_item_set_to_block(self):
        doc = yarutsk.loads("- [a, b]\n")
        assert doc.node(0).style == "flow"
        doc.container_style(0, "block")
        out = yarutsk.dumps(doc)
        assert "- a\n" in out

    def test_sequence_container_style_negative_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - [a, b]
            - [c, d]
        """)
        )
        doc.container_style(-1, "block")
        out = yarutsk.dumps(doc)
        assert "[a, b]" in out  # first unchanged
        assert "- c\n" in out  # last converted

    def test_sequence_container_style_invalid_raises(self):
        doc = yarutsk.loads("- [a, b]\n")
        with pytest.raises(ValueError):
            doc.container_style(0, "bad")

    def test_sequence_container_style_on_scalar_is_noop(self):
        doc = yarutsk.loads("- hello\n")
        doc.container_style(0, "flow")
        assert yarutsk.dumps(doc) == "- hello\n"
