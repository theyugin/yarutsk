"""Tests for round-trip fidelity: scalar styles, container styles,
alias expansion, tags, and explicit document markers."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestRoundTripScalarStyles:
    """Scalar quoting/block styles are preserved through load → dump → load."""

    def test_plain_style_unchanged(self) -> None:
        doc = yarutsk.loads("key: hello")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "key: hello\n"

    def test_single_quoted_style_preserved(self) -> None:
        doc = yarutsk.loads("key: 'hello'")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "key: 'hello'\n"

    def test_double_quoted_style_preserved(self) -> None:
        doc = yarutsk.loads('key: "hello"')
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == 'key: "hello"\n'

    def test_single_quoted_type_lookalike_preserved(self) -> None:
        """'true' as a string should stay single-quoted so it round-trips as str."""
        doc = yarutsk.loads("key: 'true'")
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["key"] == "true"
        assert isinstance(doc2["key"], str)

    def test_double_quoted_type_lookalike_preserved(self) -> None:
        doc = yarutsk.loads('key: "42"')
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["key"] == "42"
        assert isinstance(doc2["key"], str)

    def test_literal_block_style_preserved(self) -> None:
        src = dedent("""\
            text: |
              line one
              line two
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert "|\n" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["text"] == "line one\nline two\n"

    def test_folded_block_style_preserved(self) -> None:
        src = dedent("""\
            text: >
              folded line
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert ">\n" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["text"] == "folded line\n"

    def test_plain_integer_roundtrips(self) -> None:
        doc = yarutsk.loads("n: 42")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "n: 42\n"
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["n"] == 42

    def test_plain_bool_roundtrips(self) -> None:
        doc = yarutsk.loads("flag: true")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "flag: true\n"
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["flag"] is True

    def test_plain_null_roundtrips(self) -> None:
        doc = yarutsk.loads("nothing: null")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "nothing: null\n"
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["nothing"] is None

    def test_scalar_style_attribute(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: plain
            b: 'single'
            c: "double"
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").style == "plain"
        assert doc.node("b").style == "single"
        assert doc.node("c").style == "double"

    def test_scalar_style_can_be_changed(self) -> None:
        doc = yarutsk.loads("key: hello")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").style = "double"
        out = yarutsk.dumps(doc)
        assert out == 'key: "hello"\n'


class TestRoundTripContainerStyles:
    """Flow vs block container style is preserved through load → dump → load."""

    def test_flow_sequence_value_emitted_inline(self) -> None:
        src = "tags: [a, b, c]\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == src

    def test_flow_mapping_value_emitted_inline(self) -> None:
        src = "point: {x: 1, y: 2}\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == src

    def test_flow_sequence_roundtrips(self) -> None:
        src = "tags: [a, b, c]\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert list(doc2["tags"]) == ["a", "b", "c"]

    def test_flow_mapping_roundtrips(self) -> None:
        src = "point: {x: 1, y: 2}\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["point"]["x"] == 1
        assert doc2["point"]["y"] == 2

    def test_block_sequence_stays_block(self) -> None:
        src = dedent("""\
            items:
              - a
              - b
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert "- a" in out
        assert "- b" in out
        assert "[" not in out

    def test_block_mapping_stays_block(self) -> None:
        src = dedent("""\
            nested:
              x: 1
              y: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert "{" not in out
        assert "x: 1" in out

    def test_top_level_flow_sequence(self) -> None:
        src = "[a, b, c]"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert list(doc2) == ["a", "b", "c"]

    def test_top_level_flow_mapping(self) -> None:
        src = "{x: 1, y: 2}"
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["x"] == 1
        assert doc2["y"] == 2

    def test_empty_flow_mapping_value_inline(self) -> None:
        doc = yarutsk.loads("key: {}")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert "{}" in out
        assert out == "key: {}\n"

    def test_empty_flow_sequence_value_inline(self) -> None:
        doc = yarutsk.loads("key: []")
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == "key: []\n"

    def test_nested_flow_in_block_roundtrips(self) -> None:
        src = dedent("""\
            name: demo
            tags: [x, y]
            count: 3
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["name"] == "demo"
        assert list(doc2["tags"]) == ["x", "y"]
        assert doc2["count"] == 3


class TestRoundTripAliasExpansion:
    """Aliases are expanded in-place at load time (no *name in output)."""

    def test_alias_expands_to_value(self) -> None:
        src = dedent("""\
            default: &base 42
            actual: *base
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["actual"] == 42

    def test_alias_shares_identity_with_anchor(self) -> None:
        """Aliases share Python identity with the anchored container, so
        mutations through one are visible through the other (matches the
        reference semantics of plain Python dicts/lists)."""
        src = dedent("""\
            a: &anchor {x: 1}
            b: *anchor
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] is doc["b"]
        doc["b"]["x"] = 99
        assert doc["a"]["x"] == 99

    def test_alias_roundtrips_as_value(self) -> None:
        """Aliases are preserved in output: *name round-trips faithfully."""
        src = dedent("""\
            base: &b hello
            copy: *b
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert out == src
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["copy"] == "hello"

    def test_merge_key_expands(self) -> None:
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
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["service"]["name"] == "api"

    def test_sequence_alias_expands(self) -> None:
        src = dedent("""\
            orig: &items
              - a
              - b
            copy: *items
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc["copy"]) == ["a", "b"]

    def test_alias_dump_is_reloadable(self) -> None:
        src = dedent("""\
            x: &v 100
            y: *v
            z: *v
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["x"] == 100
        assert doc2["y"] == 100
        assert doc2["z"] == 100


class TestRoundTripTagAccess:
    """Tags are accessible via get_tag() but are not emitted into the dump text."""

    def test_scalar_tag_loaded(self) -> None:
        doc = yarutsk.loads("value: !!str 42")
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("value")
        assert node.tag is not None

    def test_mapping_tag_loaded(self) -> None:
        doc = yarutsk.loads("!!python/object:dict {a: 1}")
        assert doc is not None
        assert doc.tag is not None

    def test_scalar_no_tag_returns_none(self) -> None:
        doc = yarutsk.loads("value: hello")
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("value")
        assert node.tag is None

    def test_set_tag_on_scalar(self) -> None:
        doc = yarutsk.loads("key: hello")
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("key")
        node.tag = "!!str"
        assert node.tag == "!!str"

    def test_set_tag_none_clears_tag(self) -> None:
        doc = yarutsk.loads("value: !!str 42")
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("value")
        node.tag = None
        assert node.tag is None

    def test_tags_emitted_in_dump(self) -> None:
        doc = yarutsk.loads("value: !!str 42")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["value"] == "42"
        assert isinstance(doc["value"], str)
        out = yarutsk.dumps(doc)
        assert out == "value: !!str 42\n"
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["value"] == "42"
        assert isinstance(doc2["value"], str)

    def test_set_tag_on_mapping(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert doc is not None
        doc.tag = "!!map"
        assert doc.tag == "!!map"

    def test_set_tag_on_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        assert doc is not None
        doc.tag = "!!seq"
        assert doc.tag == "!!seq"


class TestExplicitDocumentMarker:
    """The --- document-start marker is preserved through load → dump."""

    def test_no_marker_not_emitted(self) -> None:
        doc = yarutsk.loads("key: value")
        assert doc is not None
        assert not doc.explicit_start
        assert yarutsk.dumps(doc) == "key: value\n"

    def test_marker_preserved_on_mapping(self) -> None:
        doc = yarutsk.loads("---\nkey: value")
        assert doc is not None
        assert doc.explicit_start
        assert yarutsk.dumps(doc) == "---\nkey: value\n"

    def test_marker_preserved_on_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            ---
            - a
            - b
        """)
        )
        assert doc is not None
        assert doc.explicit_start
        assert yarutsk.dumps(doc) == dedent("""\
            ---
            - a
            - b
        """)

    def test_marker_preserved_on_scalar(self) -> None:
        doc = yarutsk.loads("---\n42")
        assert doc is not None
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        assert yarutsk.loads(out) == 42

    def test_marker_roundtrips(self) -> None:
        src = dedent("""\
            ---
            name: Alice
            age: 30
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_no_marker_roundtrips(self) -> None:
        src = dedent("""\
            name: Alice
            age: 30
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_explicit_start_can_be_set(self) -> None:
        doc = yarutsk.loads("key: value")
        assert doc is not None
        assert not doc.explicit_start
        doc.explicit_start = True
        assert yarutsk.dumps(doc) == "---\nkey: value\n"

    def test_explicit_start_can_be_cleared(self) -> None:
        doc = yarutsk.loads("---\nkey: value")
        assert doc is not None
        doc.explicit_start = False
        assert yarutsk.dumps(doc) == "key: value\n"

    def test_multiline_value_with_marker(self) -> None:
        src = dedent("""\
            ---
            text: |
              line one
              line two
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert doc.explicit_start
        out = yarutsk.dumps(doc)
        assert out.startswith("---\n")
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["text"] == "line one\nline two\n"


class TestBlankLinePreservation:
    """Blank lines between mapping entries and sequence items are preserved."""

    def test_single_blank_line_between_keys(self) -> None:
        src = dedent("""\
            a: 1

            b: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_multiple_blank_lines_between_keys(self) -> None:
        src = dedent("""\
            a: 1


            b: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_no_blank_lines_unaffected(self) -> None:
        src = dedent("""\
            a: 1
            b: 2
            c: 3
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_between_some_keys(self) -> None:
        src = dedent("""\
            a: 1
            b: 2

            c: 3
            d: 4
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_blank_line_with_comment(self) -> None:
        src = dedent("""\
            x: 1

            # note
            y: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_in_sequence(self) -> None:
        src = dedent("""\
            - a

            - b


            - c
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_in_nested_mapping(self) -> None:
        src = dedent("""\
            outer:
              a: 1

              b: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_blank_lines_between_top_and_nested(self) -> None:
        src = dedent("""\
            section1:
              x: 1

            section2:
              y: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_null_sequence_item_does_not_accumulate_blanks(self) -> None:
        # Regression: yaml-rust2 reports empty plain scalars (implicit nulls)
        # at the position of the next token, not where the `-` actually is.
        # Without correction, re-parsing the emitter's output saw phantom blank
        # lines before null items and re-emit drifted (found by idempotent_emit
        # fuzz target on input ": -").
        src = ": -\n"
        out1 = yarutsk.dumps(yarutsk.loads(src))
        out2 = yarutsk.dumps(yarutsk.loads(out1))
        assert out1 == out2

    def test_empty_mapping_key_does_not_accumulate_blanks(self) -> None:
        # Regression: same root cause as the null-sequence-item case, but for
        # an empty plain scalar used as a mapping key (found by idempotent_emit
        # fuzz target on input " ?\n #*").
        src = " ?\n #*"
        out1 = yarutsk.dumps(yarutsk.loads(src))
        out2 = yarutsk.dumps(yarutsk.loads(out1))
        assert out1 == out2

    def test_mapping_setitem_preserves_blank_lines(self) -> None:
        # Reassigning an existing key via __setitem__ preserves the blank line
        # that preceded it — parity with sequence __setitem__.
        doc = yarutsk.loads("a: 1\n\nb: 2\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["b"] = 99
        assert yarutsk.dumps(doc) == "a: 1\n\nb: 99\n"


class TestNonCanonicalScalarForms:
    """Non-canonical plain scalars round-trip as their original source text."""

    def test_null_tilde(self) -> None:
        src = "x: ~\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_null_capitalized(self) -> None:
        src = "x: Null\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_null_uppercase(self) -> None:
        src = "x: NULL\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_bool_yes(self) -> None:
        src = "x: yes\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_no(self) -> None:
        src = "x: no\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_on(self) -> None:
        src = "x: on\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_off(self) -> None:
        src = "x: off\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_capitalized_true(self) -> None:
        src = "x: True\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_capitalized_false(self) -> None:
        src = "x: False\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_bool_uppercase_true(self) -> None:
        src = "x: TRUE\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is True
        assert yarutsk.dumps(doc) == src

    def test_bool_uppercase_false(self) -> None:
        src = "x: FALSE\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] is False
        assert yarutsk.dumps(doc) == src

    def test_hex_integer(self) -> None:
        src = "x: 0xFF\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] == 255
        assert yarutsk.dumps(doc) == src

    def test_hex_uppercase_prefix(self) -> None:
        src = "x: 0XFF\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] == 255
        assert yarutsk.dumps(doc) == src

    def test_octal_integer(self) -> None:
        src = "x: 0o77\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] == 63
        assert yarutsk.dumps(doc) == src

    def test_underscore_integer(self) -> None:
        # Underscore-separated integers are not parsed by Rust's i64::parse, so the
        # value is stored as a string — but the source form is preserved in the output.
        src = "x: 1_000_000\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_float_exponent_form(self) -> None:
        src = "x: 1.5e10\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] == 1.5e10
        assert yarutsk.dumps(doc) == src

    def test_canonical_null_unchanged(self) -> None:
        src = "x: null\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_canonical_bool_unchanged(self) -> None:
        assert yarutsk.dumps(yarutsk.loads("x: true\n")) == "x: true\n"
        assert yarutsk.dumps(yarutsk.loads("x: false\n")) == "x: false\n"

    def test_non_canonical_in_sequence(self) -> None:
        src = dedent("""\
            - yes
            - no
            - ~
            - 0xFF
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src


class TestTagRoundTrip:
    """Tags are preserved through load → dump → load."""

    def test_str_tag_on_integer_looking_scalar(self) -> None:
        src = "x: !!str 42\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["x"] == "42"
        assert isinstance(doc["x"], str)
        assert yarutsk.dumps(doc) == src

    def test_str_tag_roundtrips_value(self) -> None:
        src = "x: !!str 42\n"
        doc2 = yarutsk.loads(yarutsk.dumps(yarutsk.loads(src)))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["x"] == "42"
        assert isinstance(doc2["x"], str)

    def test_str_tag_on_bool_looking_scalar(self) -> None:
        src = "flag: !!str true\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["flag"] == "true"
        assert isinstance(doc["flag"], str)
        assert yarutsk.dumps(doc) == src

    def test_tag_on_top_level_scalar(self) -> None:
        src = "!!str 42\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        assert doc == "42"
        assert yarutsk.dumps(doc) == src

    def test_custom_tag_on_flow_sequence(self) -> None:
        src = "x: !!python/tuple [1, 2]\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_tag_mutation_via_node_propagates(self) -> None:
        """node() is a write-through proxy: tag mutations land on the parent."""
        doc = yarutsk.loads("value: !!str 42")
        assert isinstance(doc, yarutsk.YamlMapping)
        node = doc.node("value")
        assert node.tag is not None
        node.tag = None
        assert "!!" not in yarutsk.dumps(doc)

    def test_tag_on_multiple_keys(self) -> None:
        src = dedent("""\
            a: !!str 1
            b: !!str 2
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_tag_on_top_level_sequence_accessible(self) -> None:
        """Tag on a top-level block sequence is parsed and accessible via get_tag()."""
        doc = yarutsk.loads(
            dedent("""\
            !!python/tuple
            - 1
            - 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.tag is not None
        assert list(doc) == [1, 2]

    def test_tag_on_sequence_item_via_mapping(self) -> None:
        """Tag on a value that is a sequence inside a mapping."""
        src = "x: !!python/tuple [1, 2]\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        seq = doc["x"]
        assert seq.tag is not None
        assert yarutsk.dumps(doc) == src

    def test_inline_comment_after_quoted_seq_scalar(self) -> None:
        # Regression: the scanner reads past quoted scalars (and any trailing
        # `# …`) before emitting the Scalar event, so the comment was drained
        # into the builder before the sequence item existed and was silently
        # dropped. Found by idempotent_emit fuzz target.
        src = "- 'foo'  # bar\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).comment_inline == "bar"
        assert yarutsk.dumps(doc) == src

    def test_tag_with_pct_escape_is_reencoded(self) -> None:
        # Regression: the scanner decodes `%XX` escapes on input, so a tag like
        # `!0~%099` enters the emitter as `!0~\t9`. Emitting verbatim breaks
        # round-trip because the tab terminates the tag on reparse. Found by
        # idempotent_emit fuzz target.
        src = "!0~%099"
        out1 = yarutsk.dumps(yarutsk.loads(src))
        out2 = yarutsk.dumps(yarutsk.loads(out1))
        assert out1 == out2


class TestAnchorAliasRoundTrip:
    """Anchors (&name) and aliases (*name) are preserved through load → dump."""

    def test_scalar_anchor_and_alias(self) -> None:
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_alias_value_is_accessible(self) -> None:
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["y"] == "value"

    def test_integer_anchor_and_alias(self) -> None:
        src = dedent("""\
            base: &n 42
            copy: *n
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["copy"] == 42
        assert yarutsk.dumps(doc) == src

    def test_multiple_aliases_same_anchor(self) -> None:
        src = dedent("""\
            x: &v 100
            y: *v
            z: *v
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["y"] == 100
        assert doc["z"] == 100
        assert yarutsk.dumps(doc) == src

    def test_flow_sequence_anchor(self) -> None:
        src = dedent("""\
            items: &mylist [1, 2, 3]
            ref: *mylist
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc["ref"]) == [1, 2, 3]
        assert yarutsk.dumps(doc) == src

    def test_block_mapping_anchor(self) -> None:
        src = dedent("""\
            base: &base
              a: 1
              b: 2
            child: *base
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["child"]["a"] == 1
        assert yarutsk.dumps(doc) == src

    def test_block_sequence_anchor(self) -> None:
        src = dedent("""\
            orig: &items
              - a
              - b
            copy: *items
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert list(doc["copy"]) == ["a", "b"]
        assert yarutsk.dumps(doc) == src

    def test_anchor_and_alias_share_identity(self) -> None:
        """The anchored container and every alias to it surface as the same
        Python object — mutating through one is visible through the others
        (Python dict/list reference semantics, intentional in B1)."""
        src = dedent("""\
            a: &anchor {x: 1}
            b: *anchor
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] is doc["b"]
        doc["b"]["x"] = 99
        assert doc["a"]["x"] == 99

    def test_alias_dump_is_reloadable(self) -> None:
        src = dedent("""\
            x: &anchor value
            y: *anchor
        """)
        doc2 = yarutsk.loads(yarutsk.dumps(yarutsk.loads(src)))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["y"] == "value"


class TestExplicitEndMarker:
    """The ... document-end marker is preserved and settable."""

    def test_end_marker_not_present_by_default(self) -> None:
        doc = yarutsk.loads("key: value")
        assert doc is not None
        assert not doc.explicit_end
        assert "..." not in yarutsk.dumps(doc)

    def test_end_marker_preserved_on_load(self) -> None:
        doc = yarutsk.loads("key: value\n...")
        assert doc is not None
        assert doc.explicit_end
        assert yarutsk.dumps(doc).endswith("...\n")

    def test_end_marker_roundtrips(self) -> None:
        src = "a: 1\n...\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_explicit_end_can_be_set(self) -> None:
        doc = yarutsk.loads("key: value")
        assert doc is not None
        doc.explicit_end = True
        assert yarutsk.dumps(doc) == "key: value\n...\n"

    def test_explicit_end_can_be_cleared(self) -> None:
        doc = yarutsk.loads("key: value\n...")
        assert doc is not None
        doc.explicit_end = False
        assert "..." not in yarutsk.dumps(doc)

    def test_both_markers_together(self) -> None:
        src = dedent("""\
            ---
            a: 1
            ...
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        assert doc.explicit_start
        assert doc.explicit_end
        assert yarutsk.dumps(doc) == src

    def test_explicit_end_multidoc(self) -> None:
        src = dedent("""\
            ---
            a: 1
            ...
            ---
            b: 2
        """)
        docs = yarutsk.loads_all(src)
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        assert docs[0].explicit_end
        assert not docs[1].explicit_end
        assert yarutsk.dumps_all(docs) == src


class TestKeyMetadataRoundTrip:
    """Key anchors and tags are preserved through load → dump → load."""

    def test_key_anchor_preserved(self) -> None:
        src = "&ka key: value\n"
        out = yarutsk.dumps(yarutsk.loads(src))
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["key"] == "value"

    def test_key_tag_preserved(self) -> None:
        src = "!!str key: value\n"
        out = yarutsk.dumps(yarutsk.loads(src))
        assert "!!str" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["key"] == "value"

    def test_alias_as_key_preserved(self) -> None:
        """*alias used as a mapping key round-trips as an explicit-key form."""
        src = dedent("""\
            anchor: &ak value
            ? *ak
            : other
        """)
        out = yarutsk.dumps(yarutsk.loads(src))
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert "anchor" in doc2


class TestBinaryTagRoundTrip:
    """!!binary scalars round-trip as Python bytes."""

    def test_binary_load_returns_bytes(self) -> None:
        doc = yarutsk.loads("data: !!binary aGVsbG8=\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["data"] == b"hello"
        assert isinstance(doc["data"], bytes)

    def test_binary_roundtrip_preserves_source(self) -> None:
        src = "data: !!binary aGVsbG8=\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_binary_with_whitespace_in_value(self) -> None:
        # YAML binary values may contain whitespace (e.g. line-wrapped base64)
        doc = yarutsk.loads(
            dedent("""\
            data: !!binary aGVs
              bG8=
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["data"] == b"hello"

    def test_binary_dump_from_bytes(self) -> None:
        import yarutsk as yr

        mapping = yr.loads("x: placeholder\n")
        assert isinstance(mapping, yr.YamlMapping)
        mapping["x"] = b"hello"
        out = yr.dumps(mapping)
        assert "!!binary" in out
        doc2 = yr.loads(out)
        assert isinstance(doc2, yr.YamlMapping)
        assert doc2["x"] == b"hello"


class TestTimestampTagRoundTrip:
    """!!timestamp scalars round-trip as Python datetime objects."""

    import datetime as _dt

    def test_timestamp_datetime_load(self) -> None:
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15T10:30:00\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["ts"] == datetime.datetime(2024, 1, 15, 10, 30, 0)
        assert isinstance(doc["ts"], datetime.datetime)

    def test_timestamp_date_only_load(self) -> None:
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["ts"] == datetime.date(2024, 1, 15)
        assert isinstance(doc["ts"], datetime.date)

    def test_timestamp_roundtrip_preserves_source(self) -> None:
        src = "ts: !!timestamp 2024-01-15T10:30:00\n"
        assert yarutsk.dumps(yarutsk.loads(src)) == src

    def test_timestamp_space_separator(self) -> None:
        # YAML allows space instead of T between date and time
        import datetime

        doc = yarutsk.loads("ts: !!timestamp 2024-01-15 10:30:00\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["ts"], datetime.datetime)
        assert doc["ts"].year == 2024
        assert doc["ts"].hour == 10

    def test_timestamp_dump_from_datetime(self) -> None:
        import datetime

        import yarutsk as yr

        mapping = yr.loads("x: placeholder\n")
        assert isinstance(mapping, yr.YamlMapping)
        mapping["x"] = datetime.datetime(2024, 1, 15, 10, 30, 0)
        out = yr.dumps(mapping)
        assert "!!timestamp" in out
        doc2 = yr.loads(out)
        assert isinstance(doc2, yr.YamlMapping)
        assert doc2["x"] == datetime.datetime(2024, 1, 15, 10, 30, 0)


class TestContainerStyle:
    """style property: read block/flow from source and switch between them."""

    def test_mapping_block_style_default(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert doc is not None
        assert doc.style == "block"

    def test_mapping_flow_style_roundtrip(self) -> None:
        doc = yarutsk.loads("{a: 1, b: 2}")
        assert doc is not None
        assert doc.style == "flow"
        assert yarutsk.dumps(doc) == "{a: 1, b: 2}\n"

    def test_mapping_block_to_flow(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert doc is not None
        doc.style = "flow"
        out = yarutsk.dumps(doc)
        assert out.startswith("{")
        assert "a: 1" in out
        assert "b: 2" in out

    def test_mapping_flow_to_block(self) -> None:
        doc = yarutsk.loads("{a: 1, b: 2}")
        assert doc is not None
        doc.style = "block"
        out = yarutsk.dumps(doc)
        assert "a: 1\n" in out
        assert "b: 2\n" in out

    def test_mapping_style_invalid_raises(self) -> None:
        doc = yarutsk.loads("a: 1\n")
        assert doc is not None
        with pytest.raises(ValueError):
            doc.style = "invalid"  # type: ignore[assignment]

    def test_sequence_block_style_default(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        assert doc is not None
        assert doc.style == "block"

    def test_sequence_flow_style_roundtrip(self) -> None:
        doc = yarutsk.loads("[1, 2, 3]")
        assert doc is not None
        assert doc.style == "flow"
        assert yarutsk.dumps(doc) == "[1, 2, 3]\n"

    def test_sequence_block_to_flow(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        assert doc is not None
        doc.style = "flow"
        out = yarutsk.dumps(doc)
        assert out.startswith("[")

    def test_sequence_style_invalid_raises(self) -> None:
        doc = yarutsk.loads("- 1\n")
        assert doc is not None
        with pytest.raises(ValueError):
            doc.style = "bad"  # type: ignore[assignment]


class TestContainerStyleSetter:
    """container_style(key/idx, style) sets the block/flow style of a nested
    mapping or sequence value directly, without going through node() clones."""

    def test_mapping_value_default_block_after_plain_list_assign(self) -> None:
        doc = yarutsk.loads("k: placeholder\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["k"] = ["a", "b", "c"]
        assert doc.node("k").style == "block"

    def test_mapping_set_seq_value_to_flow(self) -> None:
        doc = yarutsk.loads("k: placeholder\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["k"] = ["a", "b", "c"]
        doc.node("k").style = "flow"
        assert "[" in yarutsk.dumps(doc)

    def test_mapping_set_seq_value_back_to_block(self) -> None:
        doc = yarutsk.loads("k: [a, b, c]\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("k").style == "flow"
        doc.node("k").style = "block"
        out = yarutsk.dumps(doc)
        assert "- a\n" in out

    def test_mapping_node_mutation_propagates(self) -> None:
        """node() is a write-through proxy: style mutations land on the parent."""
        doc = yarutsk.loads("k: [1, 2]\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        child = doc.node("k")
        child.style = "block"
        assert "[" not in yarutsk.dumps(doc)

    def test_mapping_container_style_key_error(self) -> None:
        doc = yarutsk.loads("a: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.node("missing").style = "flow"

    def test_mapping_container_style_invalid_raises(self) -> None:
        doc = yarutsk.loads("k: [a, b]\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(ValueError):
            doc.node("k").style = "bad"  # type: ignore[assignment]

    def test_mapping_nested_mapping_set_to_flow(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            k:
              a: 1
              b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("k").style == "block"
        doc.node("k").style = "flow"
        out = yarutsk.dumps(doc)
        assert "{" in out

    def test_mapping_nested_mapping_set_to_block(self) -> None:
        doc = yarutsk.loads("k: {a: 1, b: 2}\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("k").style = "block"
        out = yarutsk.dumps(doc)
        assert "a: 1\n" in out

    def test_sequence_item_set_to_flow(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - - a
              - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).style == "block"
        doc.node(0).style = "flow"
        out = yarutsk.dumps(doc)
        assert "[" in out

    def test_sequence_item_set_to_block(self) -> None:
        doc = yarutsk.loads("- [a, b]\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).style == "flow"
        doc.node(0).style = "block"
        out = yarutsk.dumps(doc)
        assert "- a\n" in out

    def test_sequence_container_style_negative_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - [a, b]
            - [c, d]
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(-1).style = "block"
        out = yarutsk.dumps(doc)
        assert "[a, b]" in out
        assert "- c\n" in out

    def test_sequence_container_style_invalid_raises(self) -> None:
        doc = yarutsk.loads("- [a, b]\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(ValueError):
            doc.node(0).style = "bad"  # type: ignore[assignment]


class TestStreamRoundTrip:
    """load(stream) -> dump(stream) loops should preserve the source byte-for-byte."""

    def test_stringio_roundtrip(self) -> None:
        src = dedent("""\
            # a greeting
            hello: world
            nums: [1, 2, 3]
        """)
        doc = yarutsk.load(io.StringIO(src))
        assert doc is not None
        buf = io.StringIO()
        yarutsk.dump(doc, buf)
        assert buf.getvalue() == src

    def test_bytesio_roundtrip(self) -> None:
        src = b"# a greeting\nhello: world\nnums: [1, 2, 3]\n"
        doc = yarutsk.load(io.BytesIO(src))
        assert doc is not None
        buf = io.BytesIO()
        yarutsk.dump(doc, buf)
        assert buf.getvalue() == src

    def test_multidoc_stream_roundtrip(self) -> None:
        src = b"---\na: 1\n...\n---\nb: 2\n...\n"
        docs = list(yarutsk.iter_load_all(io.BytesIO(src)))
        buf = io.BytesIO()
        yarutsk.dump_all(docs, buf)
        assert buf.getvalue() == src
