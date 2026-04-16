"""Tests for comment preservation, edge cases, and mutations."""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestCommentPreservation:
    def test_inline_comment_preserved(self):
        content = io.StringIO("name: John  # inline comment")
        doc = yarutsk.load(content)
        assert doc.comment_inline("name") == "inline comment"

    def test_leading_comment_preserved(self):
        content = io.StringIO(
            dedent("""\
            # Leading comment
            name: John
        """)
        )
        doc = yarutsk.load(content)
        assert doc.comment_before("name") == "Leading comment"

    def test_multiple_leading_comments(self):
        content = io.StringIO(
            dedent("""\
            # Line 1
            # Line 2
            name: John
        """)
        )
        doc = yarutsk.load(content)
        before = doc.comment_before("name")
        assert "Line 1" in before
        assert "Line 2" in before

    def test_comment_in_serialized_output(self):
        content = io.StringIO("name: John  # inline comment")
        doc = yarutsk.load(content)
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# inline comment" in output.getvalue()

    def test_comment_inline_set(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.comment_inline("name", "new comment")
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# new comment" in output.getvalue()

    def test_comment_before_set(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.comment_before("name", "Header comment")
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# Header comment" in output.getvalue()


class TestCommentEdgeCases:
    """Tests for unusual comment placement and whitespace in comments."""

    def test_inline_no_space_after_hash(self):
        """# with no space still captured."""
        doc = yarutsk.load(io.StringIO("key: val  #nospace"))
        assert doc.comment_inline("key") == "nospace"

    def test_inline_leading_spaces_inside_comment(self):
        """Spaces after the # are part of the comment text."""
        doc = yarutsk.load(io.StringIO("key: val  #   padded"))
        assert doc.comment_inline("key") == "  padded"

    def test_inline_on_null_value(self):
        """Comment on a key whose value is null (bare `key:`)."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            key:  # empty
            other: x
        """)
            )
        )
        assert doc.comment_inline("key") == "empty"
        assert doc.comment_before("other") is None

    def test_inline_only_on_last_key_in_block(self):
        """Inline comment on the last key of a mapping."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            a: 1
            b: 2  # last
        """)
            )
        )
        assert doc.comment_inline("a") is None
        assert doc.comment_inline("b") == "last"

    def test_multiple_keys_each_has_own_inline(self):
        """Every key in a multi-key mapping gets its own inline comment."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            a: 1  # c1
            b: 2  # c2
            c: 3  # c3
        """)
            )
        )
        assert doc.comment_inline("a") == "c1"
        assert doc.comment_inline("b") == "c2"
        assert doc.comment_inline("c") == "c3"

    def test_inline_does_not_bleed_to_next_key(self):
        """An inline comment on key N is not treated as before-comment for key N+1."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            a: 1  # only-a
            b: 2
        """)
            )
        )
        assert doc.comment_inline("a") == "only-a"
        assert doc.comment_before("b") is None

    def test_before_comment_on_second_key(self):
        """A comment between two keys is attached to the second key."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            a: 1
            # before b
            b: 2
        """)
            )
        )
        assert doc.comment_before("a") is None
        assert doc.comment_before("b") == "before b"

    def test_before_comment_on_every_key(self):
        """Each key can carry its own before-comment."""
        yaml = dedent("""\
            # c-a
            a: 1
            # c-b
            b: 2
            # c-c
            c: 3
        """)
        doc = yarutsk.load(io.StringIO(yaml))
        assert doc.comment_before("a") == "c-a"
        assert doc.comment_before("b") == "c-b"
        assert doc.comment_before("c") == "c-c"

    def test_before_comment_blank_line_between_comment_and_key(self):
        """A blank line between the comment and the key still associates them."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            # header

            key: val
        """)
            )
        )
        assert doc.comment_before("key") == "header"

    def test_multiple_blank_lines_dont_lose_comment(self):
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            # note


            key: val
        """)
            )
        )
        assert doc.comment_before("key") == "note"

    def test_multi_line_before_comment_joined(self):
        """Multiple consecutive comment lines are joined with newline."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            # L1
            # L2
            # L3
            key: val
        """)
            )
        )
        before = doc.comment_before("key")
        assert before == "L1\nL2\nL3"

    def test_inline_on_nested_key_not_outer(self):
        """An inline comment on a nested value is on the inner key, not the outer."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            outer:
              inner: val  # deep
        """)
            )
        )
        assert doc["outer"].comment_inline("inner") == "deep"
        assert doc.comment_inline("outer") is None

    def test_before_comment_on_nested_key(self):
        """A comment before an indented key belongs to that key."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            outer:
              # before inner
              inner: val
        """)
            )
        )
        assert doc["outer"].comment_before("inner") == "before inner"

    def test_inline_on_deeply_nested_key(self):
        yaml = dedent("""\
            l1:
              l2:
                l3: v  # deep inline
        """)
        doc = yarutsk.load(io.StringIO(yaml))
        assert doc["l1"]["l2"].comment_inline("l3") == "deep inline"

    def test_before_comment_on_sequence_item_round_trips(self):
        """A before-comment on a sequence item survives a dump/load cycle."""
        yaml = dedent("""\
            items:
              # first item
              - foo
              - bar
        """)
        doc = yarutsk.load(io.StringIO(yaml))
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        result = out.getvalue()
        assert "# first item" in result
        assert doc2["items"][0] == "foo"

    def test_append_then_comment_survives_dump(self):
        """Appending an item and adding a comment on the new item round-trips correctly."""
        doc = yarutsk.loads(
            dedent("""\
            items:
              - foo
              - bar
        """)
        )
        items = doc["items"]
        items.append("baz")
        items.comment_inline(2, "newly added")
        result = yarutsk.dumps(doc)
        assert "baz" in result
        assert "# newly added" in result
        doc2 = yarutsk.loads(result)
        assert doc2["items"][2] == "baz"
        assert doc2["items"].comment_inline(2) == "newly added"

    def test_nested_mapping_mutation_then_comment_survives_dump(self):
        """Mutating a nested mapping and adding a comment on it round-trips correctly."""
        doc = yarutsk.loads(
            dedent("""\
            server:
              host: localhost
              port: 5432
        """)
        )
        server = doc["server"]
        server["port"] = 5433
        server.comment_inline("port", "changed")
        result = yarutsk.dumps(doc)
        assert "5433" in result
        assert "# changed" in result
        doc2 = yarutsk.loads(result)
        assert doc2["server"]["port"] == 5433
        assert doc2["server"].comment_inline("port") == "changed"

    def test_inline_on_sequence_item_does_not_attach_to_parent_key(self):
        """Inline comment on a sequence item is NOT on the mapping key above."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            items:
              - foo  # item comment
        """)
            )
        )
        assert doc.comment_inline("items") is None

    def test_comment_text_trailing_spaces_stripped_by_emitter(self):
        """Verify the emitter writes the comment text we stored."""
        doc = yarutsk.load(io.StringIO("key: val"))
        doc.comment_inline("key", "text with spaces  ")
        out = io.StringIO()
        yarutsk.dump(doc, out)
        assert "# text with spaces  " in out.getvalue()

    def test_multiline_before_comment_round_trips(self):
        """Multi-line before-comment round-trips through dump/load."""
        doc = yarutsk.load(
            io.StringIO(
                dedent("""\
            # line one
            # line two
            key: val
        """)
            )
        )
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        before = doc2.comment_before("key")
        assert "line one" in before
        assert "line two" in before

    def test_set_multiline_before_comment(self):
        """comment_before with embedded newlines emits multiple # lines."""
        doc = yarutsk.load(io.StringIO("key: val"))
        doc.comment_before("key", "first line\nsecond line")
        out = io.StringIO()
        yarutsk.dump(doc, out)
        result = out.getvalue()
        assert "# first line" in result
        assert "# second line" in result


class TestCommentMutations:
    """Tests for comment behaviour when values or structure are mutated."""

    def test_overwrite_inline_comment(self):
        """Calling comment_inline twice keeps only the latest text."""
        doc = yarutsk.loads("key: val  # original")
        doc.comment_inline("key", "updated")
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_overwrite_before_comment(self):
        """Calling comment_before twice keeps only the latest text."""
        doc = yarutsk.loads(
            dedent("""\
            # original
            key: val
        """)
        )
        doc.comment_before("key", "updated")
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_clear_inline_comment_with_none(self):
        """comment_inline(key, None) removes the comment from output."""
        doc = yarutsk.loads("key: val  # remove me")
        doc.comment_inline("key", None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_clear_before_comment_with_none(self):
        """comment_before(key, None) removes the comment from output."""
        doc = yarutsk.loads(
            dedent("""\
            # remove me
            key: val
        """)
        )
        doc.comment_before("key", None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_inline_and_before_on_same_key(self):
        """A key can carry both an inline and a before-comment simultaneously."""
        doc = yarutsk.loads(
            dedent("""\
            # above
            key: val  # beside
        """)
        )
        assert doc.comment_before("key") == "above"
        assert doc.comment_inline("key") == "beside"
        out = yarutsk.dumps(doc)
        assert "# above" in out
        assert "# beside" in out

    def test_set_both_comment_types_then_round_trip(self):
        doc = yarutsk.loads("key: val")
        doc.comment_before("key", "header")
        doc.comment_inline("key", "side")
        doc2 = yarutsk.loads(yarutsk.dumps(doc))
        assert doc2.comment_before("key") == "header"
        assert doc2.comment_inline("key") == "side"

    def test_inline_comment_survives_value_change(self):
        """Changing a value via __setitem__ preserves the existing inline comment."""
        doc = yarutsk.loads("port: 5432  # db port")
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_before_comment_survives_value_change(self):
        """Changing a value via __setitem__ preserves the existing before-comment."""
        doc = yarutsk.loads(
            dedent("""\
            # db port
            port: 5432
        """)
        )
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_comment_gone_after_del(self):
        """After deleting a key its comment no longer appears in output."""
        doc = yarutsk.loads("a: 1  # keep\nb: 2  # gone")
        del doc["b"]
        out = yarutsk.dumps(doc)
        assert "# keep" in out
        assert "# gone" not in out

    def test_comment_gone_after_pop(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            # before b
            b: 2
        """)
        )
        doc.pop("b")
        out = yarutsk.dumps(doc)
        assert "before b" not in out

    def test_update_preserves_comments_on_untouched_keys(self):
        """update() with a key not in other leaves existing comments intact."""
        doc = yarutsk.loads("a: 1  # side\nb: 2")
        doc.update({"b": 99})
        out = yarutsk.dumps(doc)
        assert "# side" in out
        assert "99" in out

    def test_update_with_new_key_no_comment(self):
        """A key introduced via update() has no comment."""
        doc = yarutsk.loads("a: 1")
        doc.update({"b": 2})
        assert doc.comment_inline("b") is None
        assert doc.comment_before("b") is None

    def test_add_comment_to_new_key(self):
        """A key added via __setitem__ can receive a comment and round-trips."""
        doc = yarutsk.loads("a: 1")
        doc["b"] = 2
        doc.comment_inline("b", "new key")
        out = yarutsk.dumps(doc)
        assert "b: 2" in out
        assert "# new key" in out
        doc2 = yarutsk.loads(out)
        assert doc2.comment_inline("b") == "new key"


class TestCommentSequenceMutations:
    """Tests for comment behaviour on sequence items after mutation."""

    def test_set_inline_on_sequence_item(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        doc.comment_inline(1, "middle")
        out = yarutsk.dumps(doc)
        assert "# middle" in out
        doc2 = yarutsk.loads(out)
        assert doc2.comment_inline(1) == "middle"

    def test_set_before_on_sequence_item(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
            - c
        """)
        )
        doc.comment_before(2, "last item")
        out = yarutsk.dumps(doc)
        assert "# last item" in out
        doc2 = yarutsk.loads(out)
        assert doc2.comment_before(2) == "last item"

    def test_overwrite_inline_on_sequence_item(self):
        doc = yarutsk.loads(
            dedent("""\
            - a  # old
            - b
        """)
        )
        doc.comment_inline(0, "new")
        out = yarutsk.dumps(doc)
        assert "# new" in out
        assert "old" not in out

    def test_clear_inline_on_sequence_item(self):
        doc = yarutsk.loads(
            dedent("""\
            - a  # remove
            - b
        """)
        )
        doc.comment_inline(0, None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_insert_shifts_item_with_comment(self):
        """insert(0, …) shifts existing items; the comment travels with them."""
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b  # on b
        """)
        )
        doc.insert(0, "z")
        assert doc[2] == "b"
        assert doc.comment_inline(2) == "on b"

    def test_insert_new_item_has_no_comment(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        doc.insert(1, "new")
        assert doc.comment_inline(1) is None
        assert doc.comment_before(1) is None

    def test_pop_removes_comment_from_output(self):
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c
        """)
        )
        doc.pop(0)
        out = yarutsk.dumps(doc)
        assert "# first" not in out

    def test_pop_shifts_remaining_comments(self):
        """After pop(0), what was item 1 is now item 0 and keeps its comment."""
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b  # on b
            - c
        """)
        )
        doc.pop(0)
        assert doc[0] == "b"
        assert doc.comment_inline(0) == "on b"

    def test_reverse_keeps_comments_with_items(self):
        doc = yarutsk.loads(
            dedent("""\
            - a  # first
            - b
            - c  # last
        """)
        )
        doc.reverse()
        assert doc[0] == "c"
        assert doc.comment_inline(0) == "last"
        assert doc[2] == "a"
        assert doc.comment_inline(2) == "first"


class TestBlankLines:
    """blank_lines_before and trailing_blank_lines APIs on mappings and sequences."""

    def test_mapping_blank_lines_before_roundtrip(self):
        src = dedent("""\
            a: 1


            b: 2
        """)
        doc = yarutsk.loads(src)
        assert doc.blank_lines_before("a") == 0
        assert doc.blank_lines_before("b") == 2
        assert yarutsk.dumps(doc) == src

    def test_mapping_blank_lines_before_set(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc.blank_lines_before("b", 1)
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1

            b: 2
        """)

    def test_mapping_blank_lines_before_clear(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1

            b: 2
        """)
        )
        doc.blank_lines_before("b", 0)
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1
            b: 2
        """)

    def test_mapping_blank_lines_before_key_error(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
        """)
        )
        with pytest.raises(KeyError):
            doc.blank_lines_before("missing")

    def test_mapping_trailing_blank_lines_roundtrip(self):
        src = dedent("""\
            a: 1
            b: 2


        """)
        doc = yarutsk.loads(src)
        assert doc.trailing_blank_lines == 2
        assert yarutsk.dumps(doc) == src

    def test_mapping_trailing_blank_lines_set(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1
        """)
        )
        doc.trailing_blank_lines = 2
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1


        """)

    def test_mapping_trailing_blank_lines_clear(self):
        doc = yarutsk.loads(
            dedent("""\
            a: 1


        """)
        )
        doc.trailing_blank_lines = 0
        assert yarutsk.dumps(doc) == dedent("""\
            a: 1
        """)

    def test_sequence_blank_lines_before_roundtrip(self):
        src = dedent("""\
            - 1


            - 2
        """)
        doc = yarutsk.loads(src)
        assert doc.blank_lines_before(0) == 0
        assert doc.blank_lines_before(1) == 2
        assert yarutsk.dumps(doc) == src

    def test_sequence_blank_lines_before_set(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        doc.blank_lines_before(1, 1)
        assert yarutsk.dumps(doc) == dedent("""\
            - 1

            - 2
        """)

    def test_sequence_blank_lines_before_negative_index(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
            - 3
        """)
        )
        assert doc.blank_lines_before(-1) == 0
        doc.blank_lines_before(-1, 2)
        assert yarutsk.dumps(doc) == dedent("""\
            - 1
            - 2


            - 3
        """)

    def test_sequence_trailing_blank_lines_roundtrip(self):
        src = dedent("""\
            - 1
            - 2

        """)
        doc = yarutsk.loads(src)
        assert doc.trailing_blank_lines == 1
        assert yarutsk.dumps(doc) == src

    def test_sequence_trailing_blank_lines_set(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
        """)
        )
        doc.trailing_blank_lines = 3
        assert yarutsk.dumps(doc) == dedent("""\
            - 1



        """)


class TestExplicitCommentMethods:
    """Tests for get_comment_inline / set_comment_inline / get_comment_before / set_comment_before."""

    def test_mapping_get_comment_inline_unset(self):
        doc = yarutsk.loads("key: value\n")
        assert doc.get_comment_inline("key") is None

    def test_mapping_get_comment_inline_set(self):
        src = "key: value  # note\n"
        doc = yarutsk.loads(src)
        assert doc.get_comment_inline("key") == "note"

    def test_mapping_set_comment_inline(self):
        doc = yarutsk.loads("key: value\n")
        doc.set_comment_inline("key", "added comment")
        assert doc.get_comment_inline("key") == "added comment"
        assert "# added comment" in yarutsk.dumps(doc)

    def test_mapping_set_comment_inline_clear(self):
        src = "key: value  # note\n"
        doc = yarutsk.loads(src)
        doc.set_comment_inline("key", None)
        assert doc.get_comment_inline("key") is None
        assert "# note" not in yarutsk.dumps(doc)

    def test_mapping_get_comment_inline_missing_key(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(KeyError):
            doc.get_comment_inline("missing")

    def test_mapping_set_comment_inline_missing_key(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(KeyError):
            doc.set_comment_inline("missing", "text")

    def test_mapping_get_comment_before_unset(self):
        doc = yarutsk.loads("key: value\n")
        assert doc.get_comment_before("key") is None

    def test_mapping_get_comment_before_set(self):
        src = dedent("""\
            # above
            key: value
        """)
        doc = yarutsk.loads(src)
        assert doc.get_comment_before("key") == "above"

    def test_mapping_set_comment_before(self):
        doc = yarutsk.loads("key: value\n")
        doc.set_comment_before("key", "header")
        assert doc.get_comment_before("key") == "header"
        assert "# header" in yarutsk.dumps(doc)

    def test_mapping_set_comment_before_clear(self):
        src = dedent("""\
            # above
            key: value
        """)
        doc = yarutsk.loads(src)
        doc.set_comment_before("key", None)
        assert doc.get_comment_before("key") is None
        assert "# above" not in yarutsk.dumps(doc)

    def test_sequence_get_comment_inline_unset(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        assert doc.get_comment_inline(0) is None

    def test_sequence_get_comment_inline_set(self):
        src = "- 1  # first\n- 2\n"
        doc = yarutsk.loads(src)
        assert doc.get_comment_inline(0) == "first"

    def test_sequence_set_comment_inline(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        doc.set_comment_inline(1, "second item")
        assert doc.get_comment_inline(1) == "second item"
        assert "# second item" in yarutsk.dumps(doc)

    def test_sequence_set_comment_inline_clear(self):
        src = "- 1  # first\n"
        doc = yarutsk.loads(src)
        doc.set_comment_inline(0, None)
        assert doc.get_comment_inline(0) is None

    def test_sequence_get_comment_inline_out_of_range(self):
        doc = yarutsk.loads("- 1\n")
        with pytest.raises(IndexError):
            doc.get_comment_inline(5)

    def test_sequence_set_comment_inline_out_of_range(self):
        doc = yarutsk.loads("- 1\n")
        with pytest.raises(IndexError):
            doc.set_comment_inline(5, "text")

    def test_sequence_get_comment_before_set(self):
        src = dedent("""\
            # first
            - 1
            - 2
        """)
        doc = yarutsk.loads(src)
        assert doc.get_comment_before(0) == "first"

    def test_sequence_set_comment_before(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        doc.set_comment_before(0, "intro")
        assert doc.get_comment_before(0) == "intro"
        assert "# intro" in yarutsk.dumps(doc)

    def test_sequence_negative_index(self):
        doc = yarutsk.loads("- 1\n- 2\n")
        doc.set_comment_inline(-1, "last")
        assert doc.get_comment_inline(-1) == "last"
        assert doc.get_comment_inline(1) == "last"
