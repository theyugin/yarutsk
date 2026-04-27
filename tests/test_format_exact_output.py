"""Exact-output tests for ``.format()`` that also verify data preservation.

Each test follows the same shape:

    1. Load (or build) a document with some cosmetic formatting — quoting,
       comments, blank lines, flow style, etc.
    2. Call ``.format()`` (optionally with ``styles=``/``comments=``/``blank_lines=``
       toggles).
    3. Assert the emitted YAML matches a ``dedent``-ed expected string so a
       human reader can see the canonical output the emitter produces.
    4. Round-trip that output through ``loads`` and assert ``.to_python()``
       matches the expected Python tree — proving ``format()`` didn't silently
       drop or mangle data while stripping the styling.

``test_format_preservation.py`` covers the same territory with Hypothesis and
parametrised risky strings. This file is the human-readable counterpart: you
can read the expected output and see *exactly* what canonical form the
emitter lands on for each case.
"""

from textwrap import dedent
from typing import Any

import yarutsk


def _format_and_assert(src: str, expected_yaml: str, expected_data: Any) -> None:
    """Load *src*, call ``.format()``, assert exact YAML output, then verify the
    emitted text round-trips to *expected_data*.
    """
    doc = yarutsk.loads(src)
    assert doc is not None
    doc.format()
    out = yarutsk.dumps(doc)
    assert out == expected_yaml, f"\n--expected--\n{expected_yaml}\n--actual--\n{out}"
    parsed = yarutsk.loads(out)
    assert parsed is not None
    actual = parsed.to_python() if hasattr(parsed, "to_python") else parsed
    assert actual == expected_data, f"data mismatch: {actual!r} != {expected_data!r}"


class TestFormatBaseline:
    def test_everything_at_once(self) -> None:
        src = dedent("""\
            # header comment
            a: 'one'  # trailing
            b: "two"


            c: {x: 1, y: 2}
            d:
              - 'first'
              - "second"
        """)
        expected = dedent("""\
            a: one
            b: two
            c:
              x: 1
              y: 2
            d:
              - first
              - second
        """)
        _format_and_assert(
            src,
            expected,
            {
                "a": "one",
                "b": "two",
                "c": {"x": 1, "y": 2},
                "d": ["first", "second"],
            },
        )

    def test_already_canonical_is_unchanged(self) -> None:
        src = dedent("""\
            a: 1
            b: two
            c:
              - x
              - y
        """)
        _format_and_assert(src, src, {"a": 1, "b": "two", "c": ["x", "y"]})


class TestFormatScalarReQuoting:
    """After ``format()`` every scalar style resets to plain — but the emitter
    adds back single quotes when the plain form would re-parse as a different
    YAML type (bool, null, int, float).
    """

    def test_numeric_string_is_requoted(self) -> None:
        _format_and_assert('k: "42"\n', "k: '42'\n", {"k": "42"})

    def test_bool_lookalike_is_requoted(self) -> None:
        _format_and_assert("k: 'yes'\n", "k: 'yes'\n", {"k": "yes"})

    def test_null_lookalike_is_requoted(self) -> None:
        _format_and_assert('k: "null"\n', "k: 'null'\n", {"k": "null"})

    def test_tilde_is_requoted(self) -> None:
        _format_and_assert('k: "~"\n', "k: '~'\n", {"k": "~"})

    def test_inf_lookalike_is_requoted(self) -> None:
        _format_and_assert('k: ".inf"\n', "k: '.inf'\n", {"k": ".inf"})

    def test_nan_lookalike_is_requoted(self) -> None:
        _format_and_assert('k: ".nan"\n', "k: '.nan'\n", {"k": ".nan"})

    def test_hex_lookalike_is_requoted(self) -> None:
        _format_and_assert('k: "0x10"\n', "k: '0x10'\n", {"k": "0x10"})

    def test_float_lookalike_is_requoted(self) -> None:
        _format_and_assert('k: "3.14"\n', "k: '3.14'\n", {"k": "3.14"})

    def test_empty_string_is_requoted(self) -> None:
        _format_and_assert("k: ''\n", "k: ''\n", {"k": ""})

    def test_leading_whitespace_is_requoted(self) -> None:
        _format_and_assert("k: ' leading'\n", "k: ' leading'\n", {"k": " leading"})

    def test_plain_non_risky_string_becomes_bare(self) -> None:
        """A quoted string that has no risky content drops the quotes entirely."""
        _format_and_assert("k: 'hello'\n", "k: hello\n", {"k": "hello"})
        _format_and_assert('k: "world"\n', "k: world\n", {"k": "world"})

    def test_dash_prefix_plain_value(self) -> None:
        """`-foo` is not a YAML indicator in value position so it stays plain."""
        _format_and_assert("k: '-foo'\n", "k: -foo\n", {"k": "-foo"})

    def test_risky_key_is_requoted(self) -> None:
        _format_and_assert("'42': value\n", "'42': value\n", {"42": "value"})


class TestFormatMultilineScalars:
    def test_multiline_with_trailing_newline_becomes_literal_clip(self) -> None:
        # Double-quoted "line1\nline2\n" (with real newlines) → |
        src = 'k: "line1\\nline2\\n"\n'
        expected = dedent("""\
            k: |
              line1
              line2
        """)
        _format_and_assert(src, expected, {"k": "line1\nline2\n"})

    def test_multiline_without_trailing_newline_becomes_literal_strip(self) -> None:
        src = 'k: "line1\\nline2"\n'
        expected = dedent("""\
            k: |-
              line1
              line2
        """)
        _format_and_assert(src, expected, {"k": "line1\nline2"})

    def test_multiline_three_lines(self) -> None:
        src = 'multiline: "first\\nsecond\\nthird"\n'
        expected = dedent("""\
            multiline: |-
              first
              second
              third
        """)
        _format_and_assert(src, expected, {"multiline": "first\nsecond\nthird"})


class TestFormatFlowToBlock:
    def test_flow_mapping_becomes_block(self) -> None:
        src = "k: {a: 1, b: 2}\n"
        expected = dedent("""\
            k:
              a: 1
              b: 2
        """)
        _format_and_assert(src, expected, {"k": {"a": 1, "b": 2}})

    def test_flow_sequence_becomes_block(self) -> None:
        src = "k: [1, 2, 3]\n"
        expected = dedent("""\
            k:
              - 1
              - 2
              - 3
        """)
        _format_and_assert(src, expected, {"k": [1, 2, 3]})

    def test_nested_flow_becomes_block(self) -> None:
        src = "k: {a: [1, 2], b: {c: 3}}\n"
        expected = dedent("""\
            k:
              a:
                - 1
                - 2
              b:
                c: 3
        """)
        _format_and_assert(src, expected, {"k": {"a": [1, 2], "b": {"c": 3}}})


class TestFormatStripsComments:
    def test_leading_inline_and_before_comments_removed(self) -> None:
        src = dedent("""\
            # top
            a: 1  # inline
            # before b
            b: 2
        """)
        expected = dedent("""\
            a: 1
            b: 2
        """)
        _format_and_assert(src, expected, {"a": 1, "b": 2})

    def test_comment_inside_flow_removed(self) -> None:
        src = dedent("""\
            items:
              - 1  # one
              - 2  # two
        """)
        expected = dedent("""\
            items:
              - 1
              - 2
        """)
        _format_and_assert(src, expected, {"items": [1, 2]})


class TestFormatStripsBlankLines:
    def test_runs_of_blanks_removed(self) -> None:
        src = "a: 1\n\n\n\nb: 2\n"
        expected = "a: 1\nb: 2\n"
        _format_and_assert(src, expected, {"a": 1, "b": 2})

    def test_blanks_around_nested_block_removed(self) -> None:
        src = dedent("""\
            a: 1

            b:

              c: 2

            d: 3
        """)
        expected = dedent("""\
            a: 1
            b:
              c: 2
            d: 3
        """)
        _format_and_assert(src, expected, {"a": 1, "b": {"c": 2}, "d": 3})


class TestFormatPreservesTags:
    def test_str_tag_preserved_value_requoted(self) -> None:
        src = "v: !!str 42\n"
        expected = "v: !!str '42'\n"
        _format_and_assert(src, expected, {"v": "42"})

    def test_map_tag_preserved_flow_coerced_to_block(self) -> None:
        src = "v: !!map {a: 1, b: 2}\n"
        expected = dedent("""\
            v: !!map
              a: 1
              b: 2
        """)
        _format_and_assert(src, expected, {"v": {"a": 1, "b": 2}})

    def test_seq_tag_preserved_flow_coerced_to_block(self) -> None:
        src = "v: !!seq [1, 2, 3]\n"
        expected = dedent("""\
            v: !!seq
              - 1
              - 2
              - 3
        """)
        _format_and_assert(src, expected, {"v": [1, 2, 3]})

    def test_custom_tag_preserved(self) -> None:
        src = "v: !custom value\n"
        expected = "v: !custom value\n"
        _format_and_assert(src, expected, {"v": "value"})


class TestFormatPreservesAnchorsAliases:
    def test_anchor_and_alias_on_scalar_preserved(self) -> None:
        src = dedent("""\
            a: &name value
            b: *name
        """)
        _format_and_assert(src, src, {"a": "value", "b": "value"})

    def test_anchor_on_block_mapping_preserved(self) -> None:
        src = dedent("""\
            a: &grp
              x: 1
              y: 2
            b: *grp
        """)
        _format_and_assert(
            src,
            src,
            {"a": {"x": 1, "y": 2}, "b": {"x": 1, "y": 2}},
        )


class TestFormatPreservesDocumentMarkers:
    def test_explicit_start_and_end_preserved(self) -> None:
        src = dedent("""\
            ---
            a: 1
            ...
        """)
        _format_and_assert(src, src, {"a": 1})

    def test_yaml_version_preserved(self) -> None:
        src = dedent("""\
            %YAML 1.2
            ---
            a: 1
        """)
        _format_and_assert(src, src, {"a": 1})


class TestFormatOptions:
    def test_styles_false_keeps_quotes_but_drops_comments_and_blanks(self) -> None:
        src = dedent("""\
            # top
            a: 'one'  # inline


            b: "two"
        """)
        expected = dedent("""\
            a: 'one'
            b: "two"
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.format(styles=False)
        out = yarutsk.dumps(doc)
        assert out == expected
        out_parsed = yarutsk.loads(out)
        assert isinstance(out_parsed, yarutsk.YamlMapping)
        assert out_parsed.to_python() == {"a": "one", "b": "two"}

    def test_comments_false_keeps_comments_but_resets_styles_and_blanks(self) -> None:
        src = dedent("""\
            # top
            a: 'one'  # inline
            b: "two"


            c: 3
        """)
        expected = dedent("""\
            # top
            a: one  # inline
            b: two
            c: 3
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format(comments=False)
        out = yarutsk.dumps(doc)
        assert out == expected
        out_parsed = yarutsk.loads(out)
        assert isinstance(out_parsed, yarutsk.YamlMapping)
        assert out_parsed.to_python() == {"a": "one", "b": "two", "c": 3}

    def test_blank_lines_false_keeps_blanks_but_resets_styles_and_comments(self) -> None:
        src = dedent("""\
            # top
            a: 'one'  # inline


            b: "two"
        """)
        expected = dedent("""\
            a: one


            b: two
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.format(blank_lines=False)
        out = yarutsk.dumps(doc)
        assert out == expected
        out_parsed = yarutsk.loads(out)
        assert isinstance(out_parsed, yarutsk.YamlMapping)
        assert out_parsed.to_python() == {"a": "one", "b": "two"}


class TestFormatDeepAndMixed:
    def test_deep_nested_mapping(self) -> None:
        src = dedent("""\
            a:
              b:
                c:
                  d: 'deep'
        """)
        expected = dedent("""\
            a:
              b:
                c:
                  d: deep
        """)
        _format_and_assert(src, expected, {"a": {"b": {"c": {"d": "deep"}}}})

    def test_sequence_of_mixed_nodes(self) -> None:
        src = dedent("""\
            items:
              - {name: 'a', vals: [1, 2]}
              - 'plain'
        """)
        expected = dedent("""\
            items:
              - name: a
                vals:
                  - 1
                  - 2
              - plain
        """)
        _format_and_assert(
            src,
            expected,
            {"items": [{"name": "a", "vals": [1, 2]}, "plain"]},
        )

    def test_big_mixed_document(self) -> None:
        src = dedent("""\
            # leading comment
            nums:
              int: 42
              neg: -7

            risky:
              num_str: '42'
              bool_str: 'yes'
              dash: '-foo'
            multiline: "first\\nsecond\\nthird"
            flow: {a: 1, b: [2, 3]}
        """)
        expected = dedent("""\
            nums:
              int: 42
              neg: -7
            risky:
              num_str: '42'
              bool_str: 'yes'
              dash: -foo
            multiline: |-
              first
              second
              third
            flow:
              a: 1
              b:
                - 2
                - 3
        """)
        _format_and_assert(
            src,
            expected,
            {
                "nums": {"int": 42, "neg": -7},
                "risky": {"num_str": "42", "bool_str": "yes", "dash": "-foo"},
                "multiline": "first\nsecond\nthird",
                "flow": {"a": 1, "b": [2, 3]},
            },
        )


class TestFormatIdempotent:
    def test_format_twice_gives_same_output(self) -> None:
        src = dedent("""\
            # header
            a: 'x'
            b: {c: 1, d: [2, 3]}


            e: "multi\\nline"
        """)
        doc1 = yarutsk.loads(src)
        assert doc1 is not None
        doc1.format()
        once = yarutsk.dumps(doc1)

        doc2 = yarutsk.loads(once)
        assert doc2 is not None
        doc2.format()
        twice = yarutsk.dumps(doc2)

        assert once == twice
        once_parsed = yarutsk.loads(once)
        assert isinstance(once_parsed, yarutsk.YamlMapping)
        assert once_parsed.to_python() == {
            "a": "x",
            "b": {"c": 1, "d": [2, 3]},
            "e": "multi\nline",
        }

    def test_formatted_output_reads_back_canonical(self) -> None:
        """The output of format() must itself be the canonical form — i.e.
        parsing it back and formatting again is a no-op at the text level.
        """
        src = "k: {a: 'one', b: [1, 2]}\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        doc.format()
        first = yarutsk.dumps(doc)
        assert first == dedent("""\
            k:
              a: one
              b:
                - 1
                - 2
        """)
        # Re-load and re-format: nothing changes.
        doc2 = yarutsk.loads(first)
        assert doc2 is not None
        doc2.format()
        assert yarutsk.dumps(doc2) == first


class TestYamlScalarFormat:
    def test_scalar_plain_output_after_format(self) -> None:
        s = yarutsk.YamlScalar("hello", style="double")
        s.format()
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        assert yarutsk.dumps(doc) == "k: hello\n"
        reparsed = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(reparsed, yarutsk.YamlMapping)
        assert reparsed.to_python() == {"k": "hello"}

    def test_scalar_risky_is_requoted_after_format(self) -> None:
        s = yarutsk.YamlScalar("42", style="double")
        s.format()
        assert s.style == "plain"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        # style=plain but emitter adds quotes because plain '42' would re-parse as int.
        assert yarutsk.dumps(doc) == "k: '42'\n"
        reparsed = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(reparsed, yarutsk.YamlMapping)
        assert reparsed.to_python() == {"k": "42"}

    def test_scalar_multiline_becomes_literal_after_format(self) -> None:
        s = yarutsk.YamlScalar("line1\nline2\n", style="double")
        s.format()
        assert s.style == "literal"
        doc = yarutsk.YamlMapping()
        doc["k"] = s
        assert yarutsk.dumps(doc) == dedent("""\
            k: |
              line1
              line2
        """)
        reparsed = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(reparsed, yarutsk.YamlMapping)
        assert reparsed.to_python() == {"k": "line1\nline2\n"}
