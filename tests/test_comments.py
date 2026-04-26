"""Tests for comment preservation, edge cases, and mutations."""

from textwrap import dedent

import pytest

import yarutsk


class TestCommentPreservation:
    def test_inline_comment_preserved(self) -> None:
        doc = yarutsk.loads("name: John  # inline comment")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("name").comment_inline == "inline comment"

    def test_leading_comment_preserved(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # Leading comment
            name: John
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("name").comment_before == "Leading comment"

    def test_multiple_leading_comments(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # Line 1
            # Line 2
            name: John
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        before = doc.node("name").comment_before
        assert before is not None
        assert "Line 1" in before
        assert "Line 2" in before

    def test_comment_in_serialized_output(self) -> None:
        doc = yarutsk.loads("name: John  # inline comment")
        assert doc is not None
        assert "# inline comment" in yarutsk.dumps(doc)

    def test_comment_inline_set(self) -> None:
        doc = yarutsk.loads("name: John")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("name").comment_inline = "new comment"
        assert "# new comment" in yarutsk.dumps(doc)

    def test_comment_before_set(self) -> None:
        doc = yarutsk.loads("name: John")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("name").comment_before = "Header comment"
        assert "# Header comment" in yarutsk.dumps(doc)


class TestCommentEdgeCases:
    """Tests for unusual comment placement and whitespace in comments."""

    def test_inline_no_space_after_hash(self) -> None:
        doc = yarutsk.loads("key: val  #nospace")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_inline == "nospace"

    def test_inline_leading_spaces_inside_comment(self) -> None:
        """Spaces after the # are part of the comment text."""
        doc = yarutsk.loads("key: val  #   padded")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_inline == "  padded"

    def test_inline_on_null_value(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            key:  # empty
            other: x
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_inline == "empty"
        assert doc.node("other").comment_before is None

    def test_inline_only_on_last_key_in_block(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2  # last
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").comment_inline is None
        assert doc.node("b").comment_inline == "last"

    def test_multiple_keys_each_has_own_inline(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1  # c1
            b: 2  # c2
            c: 3  # c3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").comment_inline == "c1"
        assert doc.node("b").comment_inline == "c2"
        assert doc.node("c").comment_inline == "c3"

    def test_inline_does_not_bleed_to_next_key(self) -> None:
        """An inline comment on key N is not treated as before-comment for key N+1."""
        doc = yarutsk.loads(
            dedent("""\
            a: 1  # only-a
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").comment_inline == "only-a"
        assert doc.node("b").comment_before is None

    def test_before_comment_on_second_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            # before b
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").comment_before is None
        assert doc.node("b").comment_before == "before b"

    def test_before_comment_on_every_key(self) -> None:
        yaml = dedent("""\
            # c-a
            a: 1
            # c-b
            b: 2
            # c-c
            c: 3
        """)
        doc = yarutsk.loads(yaml)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").comment_before == "c-a"
        assert doc.node("b").comment_before == "c-b"
        assert doc.node("c").comment_before == "c-c"

    def test_before_comment_blank_line_between_comment_and_key(self) -> None:
        """A blank line between the comment and the key still associates them."""
        doc = yarutsk.loads(
            dedent("""\
            # header

            key: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_before == "header"

    def test_multiple_blank_lines_dont_lose_comment(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # note


            key: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_before == "note"

    def test_multi_line_before_comment_joined(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # L1
            # L2
            # L3
            key: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        before = doc.node("key").comment_before
        assert before == "L1\nL2\nL3"

    def test_inline_on_nested_key_not_outer(self) -> None:
        """An inline comment on a nested value is on the inner key, not the outer."""
        doc = yarutsk.loads(
            dedent("""\
            outer:
              inner: val  # deep
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["outer"].node("inner").comment_inline == "deep"
        assert doc.node("outer").comment_inline is None

    def test_before_comment_on_nested_key(self) -> None:
        """A comment before an indented key belongs to that key."""
        doc = yarutsk.loads(
            dedent("""\
            outer:
              # before inner
              inner: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["outer"].node("inner").comment_before == "before inner"

    def test_inline_on_deeply_nested_key(self) -> None:
        yaml = dedent("""\
            l1:
              l2:
                l3: v  # deep inline
        """)
        doc = yarutsk.loads(yaml)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["l1"]["l2"].node("l3").comment_inline == "deep inline"

    def test_before_comment_on_sequence_item_round_trips(self) -> None:
        yaml = dedent("""\
            items:
              # first item
              - foo
              - bar
        """)
        doc = yarutsk.loads(yaml)
        assert doc is not None
        result = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(result)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert "# first item" in result
        assert doc2["items"][0] == "foo"

    def test_append_then_comment_survives_dump(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            items:
              - foo
              - bar
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        items = doc["items"]
        items.append("baz")
        items.node(2).comment_inline = "newly added"
        result = yarutsk.dumps(doc)
        assert "baz" in result
        assert "# newly added" in result
        doc2 = yarutsk.loads(result)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["items"][2] == "baz"
        assert doc2["items"].node(2).comment_inline == "newly added"

    def test_nested_mapping_mutation_then_comment_survives_dump(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            server:
              host: localhost
              port: 5432
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        server = doc["server"]
        server["port"] = 5433
        server.node("port").comment_inline = "changed"
        result = yarutsk.dumps(doc)
        assert "5433" in result
        assert "# changed" in result
        doc2 = yarutsk.loads(result)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["server"]["port"] == 5433
        assert doc2["server"].node("port").comment_inline == "changed"

    def test_inline_on_sequence_item_does_not_attach_to_parent_key(self) -> None:
        """Inline comment on a sequence item is NOT on the mapping key above."""
        doc = yarutsk.loads(
            dedent("""\
            items:
              - foo  # item comment
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("items").comment_inline is None

    def test_comment_text_trailing_spaces_preserved_by_emitter(self) -> None:
        doc = yarutsk.loads("key: val")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_inline = "text with spaces  "
        assert "# text with spaces  " in yarutsk.dumps(doc)

    def test_multiline_before_comment_round_trips(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # line one
            # line two
            key: val
        """)
        )
        assert doc is not None
        doc2 = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(doc2, yarutsk.YamlMapping)
        before = doc2.node("key").comment_before
        assert before is not None
        assert "line one" in before
        assert "line two" in before

    def test_set_multiline_before_comment_emits_multiple_hash_lines(self) -> None:
        doc = yarutsk.loads("key: val")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = "first line\nsecond line"
        result = yarutsk.dumps(doc)
        assert "# first line" in result
        assert "# second line" in result


class TestCommentMutations:
    """Tests for comment behaviour when values or structure are mutated."""

    def test_overwrite_inline_comment(self) -> None:
        doc = yarutsk.loads("key: val  # original")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_inline = "updated"
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_overwrite_before_comment(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # original
            key: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = "updated"
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_clear_inline_comment_with_none(self) -> None:
        doc = yarutsk.loads("key: val  # remove me")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_inline = None
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_clear_before_comment_with_none(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # remove me
            key: val
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = None
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_inline_and_before_on_same_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # above
            key: val  # beside
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_before == "above"
        assert doc.node("key").comment_inline == "beside"
        out = yarutsk.dumps(doc)
        assert "# above" in out
        assert "# beside" in out

    def test_set_both_comment_types_then_round_trip(self) -> None:
        doc = yarutsk.loads("key: val")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = "header"
        doc.node("key").comment_inline = "side"
        doc2 = yarutsk.loads(yarutsk.dumps(doc))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2.node("key").comment_before == "header"
        assert doc2.node("key").comment_inline == "side"

    def test_inline_comment_survives_value_change(self) -> None:
        doc = yarutsk.loads("port: 5432  # db port")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_before_comment_survives_value_change(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            # db port
            port: 5432
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_comment_gone_after_del(self) -> None:
        doc = yarutsk.loads("a: 1  # keep\nb: 2  # gone")
        assert isinstance(doc, yarutsk.YamlMapping)
        del doc["b"]
        out = yarutsk.dumps(doc)
        assert "# keep" in out
        assert "# gone" not in out

    def test_comment_gone_after_pop(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            # before b
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.pop("b")
        out = yarutsk.dumps(doc)
        assert "before b" not in out

    def test_update_preserves_comments_on_untouched_keys(self) -> None:
        doc = yarutsk.loads("a: 1  # side\nb: 2")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.update({"b": 99})
        out = yarutsk.dumps(doc)
        assert "# side" in out
        assert "99" in out

    def test_update_with_new_key_no_comment(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.update({"b": 2})
        assert doc.node("b").comment_inline is None
        assert doc.node("b").comment_before is None

    def test_add_comment_to_new_key(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["b"] = 2
        doc.node("b").comment_inline = "new key"
        out = yarutsk.dumps(doc)
        assert "b: 2" in out
        assert "# new key" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2.node("b").comment_inline == "new key"


class TestCommentSequenceMutations:
    """Tests for comment behaviour on sequence items after mutation."""

    def test_set_inline_on_sequence_item(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(1).comment_inline = "middle"
        out = yarutsk.dumps(doc)
        assert "# middle" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert doc2.node(1).comment_inline == "middle"

    def test_set_before_on_sequence_item(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(2).comment_before = "last item"
        out = yarutsk.dumps(doc)
        assert "# last item" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert doc2.node(2).comment_before == "last item"

    def test_overwrite_inline_on_sequence_item(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a  # old
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).comment_inline = "new"
        out = yarutsk.dumps(doc)
        assert "# new" in out
        assert "old" not in out

    def test_clear_inline_on_sequence_item(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a  # remove
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).comment_inline = None
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_insert_shifts_item_with_comment(self) -> None:
        """insert(0, …) shifts existing items; the comment travels with them."""
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b  # on b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.insert(0, "z")
        assert doc[2] == "b"
        assert doc.node(2).comment_inline == "on b"

    def test_insert_new_item_has_no_comment(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.insert(1, "new")
        assert doc.node(1).comment_inline is None
        assert doc.node(1).comment_before is None

    def test_pop_removes_comment_from_output(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.pop(0)
        out = yarutsk.dumps(doc)
        assert "# first" not in out

    def test_pop_shifts_remaining_comments(self) -> None:
        """After pop(0), what was item 1 is now item 0 and keeps its comment."""
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b  # on b
            - c
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.pop(0)
        assert doc[0] == "b"
        assert doc.node(0).comment_inline == "on b"

    def test_reverse_keeps_comments_with_items(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c  # last
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.reverse()
        assert doc[0] == "c"
        assert doc.node(0).comment_inline == "last"
        assert doc[2] == "a"
        assert doc.node(2).comment_inline == "first"


class TestBlankLines:
    """blank_lines_before and trailing_blank_lines APIs on mappings and sequences."""

    def test_mapping_blank_lines_before_roundtrip(self) -> None:
        src = dedent("""\
            a: 1


            b: 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("a").blank_lines_before == 0
        assert doc.node("b").blank_lines_before == 2
        assert yarutsk.dumps(doc) == src

    def test_mapping_blank_lines_before_set(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("b").blank_lines_before = 1
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1

            b: 2
        """)

    def test_mapping_blank_lines_before_clear(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1

            b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("b").blank_lines_before = 0
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1
            b: 2
        """)

    def test_mapping_blank_lines_before_key_error(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.node("missing")

    def test_mapping_trailing_blank_lines_roundtrip(self) -> None:
        src = dedent("""\
            a: 1
            b: 2


        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.trailing_blank_lines == 2
        assert yarutsk.dumps(doc) == src

    def test_mapping_trailing_blank_lines_set(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.trailing_blank_lines = 2
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1


        """)

    def test_mapping_trailing_blank_lines_clear(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1


        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.trailing_blank_lines = 0
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1
        """)

    def test_sequence_blank_lines_before_roundtrip(self) -> None:
        src = dedent("""\
            - 1


            - 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).blank_lines_before == 0
        assert doc.node(1).blank_lines_before == 2
        assert yarutsk.dumps(doc) == src

    def test_sequence_blank_lines_before_set(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(1).blank_lines_before = 1
        assert yarutsk.dumps(doc) == dedent("""\
            - 1

            - 2
        """)

    def test_sequence_blank_lines_before_negative_index(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(-1).blank_lines_before == 0
        doc.node(-1).blank_lines_before = 2
        assert yarutsk.dumps(doc) == dedent("""\
            - 1
            - 2


            - 3
        """)

    def test_sequence_trailing_blank_lines_roundtrip(self) -> None:
        src = dedent("""\
            - 1
            - 2

        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.trailing_blank_lines == 1
        assert yarutsk.dumps(doc) == src

    def test_sequence_trailing_blank_lines_set(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - 1
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.trailing_blank_lines = 3
        assert yarutsk.dumps(doc) == dedent("""\
            - 1



        """)


class TestExplicitCommentMethods:
    """Tests for the ``comment_inline`` / ``comment_before`` properties."""

    def test_mapping_get_comment_inline_unset(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_inline is None

    def test_mapping_get_comment_inline_set(self) -> None:
        src = "key: value  # note\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_inline == "note"

    def test_mapping_set_comment_inline(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_inline = "added comment"
        assert doc.node("key").comment_inline == "added comment"
        assert "# added comment" in yarutsk.dumps(doc)

    def test_mapping_set_comment_inline_clear(self) -> None:
        src = "key: value  # note\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_inline = None
        assert doc.node("key").comment_inline is None
        assert "# note" not in yarutsk.dumps(doc)

    def test_mapping_comment_inline_missing_key(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        with pytest.raises(KeyError):
            doc.node("missing")

    def test_mapping_get_comment_before_unset(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_before is None

    def test_mapping_get_comment_before_set(self) -> None:
        src = dedent("""\
            # above
            key: value
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("key").comment_before == "above"

    def test_mapping_set_comment_before(self) -> None:
        doc = yarutsk.loads("key: value\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = "header"
        assert doc.node("key").comment_before == "header"
        assert "# header" in yarutsk.dumps(doc)

    def test_mapping_set_comment_before_clear(self) -> None:
        src = dedent("""\
            # above
            key: value
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("key").comment_before = None
        assert doc.node("key").comment_before is None
        assert "# above" not in yarutsk.dumps(doc)

    def test_sequence_get_comment_inline_unset(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).comment_inline is None

    def test_sequence_get_comment_inline_set(self) -> None:
        src = "- 1  # first\n- 2\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).comment_inline == "first"

    def test_sequence_set_comment_inline(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(1).comment_inline = "second item"
        assert doc.node(1).comment_inline == "second item"
        assert "# second item" in yarutsk.dumps(doc)

    def test_sequence_set_comment_inline_clear(self) -> None:
        src = "- 1  # first\n"
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).comment_inline = None
        assert doc.node(0).comment_inline is None

    def test_sequence_comment_inline_out_of_range(self) -> None:
        doc = yarutsk.loads("- 1\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        with pytest.raises(IndexError):
            doc.node(5)

    def test_sequence_get_comment_before_set(self) -> None:
        src = dedent("""\
            # first
            - 1
            - 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).comment_before == "first"

    def test_sequence_set_comment_before(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(0).comment_before = "intro"
        assert doc.node(0).comment_before == "intro"
        assert "# intro" in yarutsk.dumps(doc)

    def test_sequence_negative_index(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.node(-1).comment_inline = "last"
        assert doc.node(-1).comment_inline == "last"
        assert doc.node(1).comment_inline == "last"


class TestScalarComments:
    """Per-scalar comments on YamlScalar nodes."""

    def test_bare_scalar_doc_before_and_inline(self) -> None:
        doc = yarutsk.loads("# hello\n42  # answer\n")
        assert doc is not None
        assert doc.comment_before == "hello"
        assert doc.comment_inline == "answer"

    def test_bare_scalar_doc_roundtrip(self) -> None:
        src = "# hello\n42  # answer\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_scalar_in_mapping_via_node(self) -> None:
        doc = yarutsk.loads("foo: bar  # note\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("foo").comment_inline == "note"

    def test_scalar_in_mapping_roundtrip(self) -> None:
        src = "foo: bar  # note\n"
        doc = yarutsk.loads(src)
        assert doc is not None
        assert yarutsk.dumps(doc) == src

    def test_scalar_mutation_via_mapping_api(self) -> None:
        doc = yarutsk.loads("foo: bar  # old\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.node("foo").comment_inline = "new"
        out = yarutsk.dumps(doc)
        assert "# new" in out
        assert "# old" not in out

    def test_container_value_entry_comment_unchanged(self) -> None:
        doc = yarutsk.loads("foo:  # note\n  a: 1\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc.node("foo").comment_inline == "note"
        # Replacing the container preserves the entry-level comment.
        doc["foo"] = {"b": 2}
        assert doc.node("foo").comment_inline == "note"

    def test_scalar_value_swap_preserves_comment(self) -> None:
        doc = yarutsk.loads("port: 5432  # db port")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_constructor_scalar_get_set(self) -> None:
        s = yarutsk.YamlScalar("x")
        assert s.comment_inline is None
        assert s.comment_before is None
        s.comment_inline = "inline"
        s.comment_before = "before"
        assert s.comment_inline == "inline"
        assert s.comment_before == "before"
        s.comment_inline = None
        assert s.comment_inline is None

    def test_format_clears_scalar_comments(self) -> None:
        doc = yarutsk.loads("# before\n42  # inline\n")
        assert doc is not None
        doc.format()
        assert doc.comment_before is None
        assert doc.comment_inline is None

    def test_format_styles_only_keeps_comments(self) -> None:
        doc = yarutsk.loads("# before\n42  # inline\n")
        assert doc is not None
        doc.format(comments=False)
        assert doc.comment_before == "before"
        assert doc.comment_inline == "inline"

    def test_sequence_scalar_item_via_node(self) -> None:
        doc = yarutsk.loads("- 1  # one\n- 2\n")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc.node(0).comment_inline == "one"

    def test_scalar_constructor_mutation_persists(self) -> None:
        s = yarutsk.YamlScalar("value")
        s.comment_inline = "note"
        doc = yarutsk.YamlMapping()
        doc["key"] = s
        out = yarutsk.dumps(doc)
        assert "# note" in out


class TestRootContainerComments:
    """comment_before / blank_lines_before on root mappings and sequences."""

    def test_mapping_root_comment_before(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert doc is not None
        doc.comment_before = "header"
        out = yarutsk.dumps(doc)
        assert out.startswith("# header\n")
        assert "a: 1" in out

    def test_sequence_root_comment_before(self) -> None:
        doc = yarutsk.loads("- 1\n- 2\n")
        assert doc is not None
        doc.comment_before = "items"
        out = yarutsk.dumps(doc)
        assert out.startswith("# items\n")

    def test_root_blank_lines_before(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert doc is not None
        doc.blank_lines_before = 2
        doc.comment_before = "with blanks"
        out = yarutsk.dumps(doc)
        assert out.startswith("\n\n# with blanks\n")

    def test_multiline_root_comment_before(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert doc is not None
        doc.comment_before = "line one\nline two"
        out = yarutsk.dumps(doc)
        assert out.startswith("# line one\n# line two\n")
