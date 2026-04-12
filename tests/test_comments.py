"""Tests for comment preservation, edge cases, and mutations."""

import io

import pytest

try:
    import yarutsk

    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

pytestmark = pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk module not built")


class TestCommentPreservation:
    """Test that comments are preserved."""

    def test_inline_comment_preserved(self):
        content = io.StringIO("name: John  # inline comment")
        doc = yarutsk.load(content)
        assert doc.get_comment_inline("name") == "inline comment"

    def test_leading_comment_preserved(self):
        content = io.StringIO("# Leading comment\nname: John")
        doc = yarutsk.load(content)
        assert doc.get_comment_before("name") == "Leading comment"

    def test_multiple_leading_comments(self):
        content = io.StringIO("# Line 1\n# Line 2\nname: John")
        doc = yarutsk.load(content)
        before = doc.get_comment_before("name")
        assert "Line 1" in before
        assert "Line 2" in before

    def test_comment_in_serialized_output(self):
        content = io.StringIO("name: John  # inline comment")
        doc = yarutsk.load(content)
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# inline comment" in output.getvalue()

    def test_set_comment_inline(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.set_comment_inline("name", "new comment")
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# new comment" in output.getvalue()

    def test_set_comment_before(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.set_comment_before("name", "Header comment")
        output = io.StringIO()
        yarutsk.dump(doc, output)
        assert "# Header comment" in output.getvalue()


class TestCommentEdgeCases:
    """Tests for unusual comment placement and whitespace in comments."""

    # ── inline comment placement ─────────────────────────────────────────────

    def test_inline_no_space_after_hash(self):
        """# with no space still captured."""
        doc = yarutsk.load(io.StringIO("key: val  #nospace"))
        assert doc.get_comment_inline("key") == "nospace"

    def test_inline_leading_spaces_inside_comment(self):
        """Spaces after the # are part of the comment text."""
        doc = yarutsk.load(io.StringIO("key: val  #   padded"))
        assert doc.get_comment_inline("key") == "  padded"

    def test_inline_on_null_value(self):
        """Comment on a key whose value is null (bare `key:`)."""
        doc = yarutsk.load(io.StringIO("key:  # empty\nother: x"))
        assert doc.get_comment_inline("key") == "empty"
        assert doc.get_comment_before("other") is None

    def test_inline_only_on_last_key_in_block(self):
        """Inline comment on the last key of a mapping."""
        doc = yarutsk.load(io.StringIO("a: 1\nb: 2  # last"))
        assert doc.get_comment_inline("a") is None
        assert doc.get_comment_inline("b") == "last"

    def test_multiple_keys_each_has_own_inline(self):
        """Every key in a multi-key mapping gets its own inline comment."""
        doc = yarutsk.load(io.StringIO("a: 1  # c1\nb: 2  # c2\nc: 3  # c3"))
        assert doc.get_comment_inline("a") == "c1"
        assert doc.get_comment_inline("b") == "c2"
        assert doc.get_comment_inline("c") == "c3"

    def test_inline_does_not_bleed_to_next_key(self):
        """An inline comment on key N is not treated as before-comment for key N+1."""
        doc = yarutsk.load(io.StringIO("a: 1  # only-a\nb: 2"))
        assert doc.get_comment_inline("a") == "only-a"
        assert doc.get_comment_before("b") is None

    # ── before-key comment placement ─────────────────────────────────────────

    def test_before_comment_on_second_key(self):
        """A comment between two keys is attached to the second key."""
        doc = yarutsk.load(io.StringIO("a: 1\n# before b\nb: 2"))
        assert doc.get_comment_before("a") is None
        assert doc.get_comment_before("b") == "before b"

    def test_before_comment_on_every_key(self):
        """Each key can carry its own before-comment."""
        yaml = "# c-a\na: 1\n# c-b\nb: 2\n# c-c\nc: 3"
        doc = yarutsk.load(io.StringIO(yaml))
        assert doc.get_comment_before("a") == "c-a"
        assert doc.get_comment_before("b") == "c-b"
        assert doc.get_comment_before("c") == "c-c"

    def test_before_comment_blank_line_between_comment_and_key(self):
        """A blank line between the comment and the key still associates them."""
        doc = yarutsk.load(io.StringIO("# header\n\nkey: val"))
        assert doc.get_comment_before("key") == "header"

    def test_multiple_blank_lines_dont_lose_comment(self):
        doc = yarutsk.load(io.StringIO("# note\n\n\nkey: val"))
        assert doc.get_comment_before("key") == "note"

    def test_multi_line_before_comment_joined(self):
        """Multiple consecutive comment lines are joined with newline."""
        doc = yarutsk.load(io.StringIO("# L1\n# L2\n# L3\nkey: val"))
        before = doc.get_comment_before("key")
        assert before == "L1\nL2\nL3"

    # ── nested mapping comments ───────────────────────────────────────────────

    def test_inline_on_nested_key_not_outer(self):
        """An inline comment on a nested value is on the inner key, not the outer."""
        doc = yarutsk.load(io.StringIO("outer:\n  inner: val  # deep"))
        assert doc["outer"].get_comment_inline("inner") == "deep"
        assert doc.get_comment_inline("outer") is None

    def test_before_comment_on_nested_key(self):
        """A comment before an indented key belongs to that key."""
        doc = yarutsk.load(io.StringIO("outer:\n  # before inner\n  inner: val"))
        assert doc["outer"].get_comment_before("inner") == "before inner"

    def test_inline_on_deeply_nested_key(self):
        yaml = "l1:\n  l2:\n    l3: v  # deep inline"
        doc = yarutsk.load(io.StringIO(yaml))
        assert doc["l1"]["l2"].get_comment_inline("l3") == "deep inline"

    # ── sequence item comments ────────────────────────────────────────────────

    def test_before_comment_on_sequence_item_round_trips(self):
        """A before-comment on a sequence item survives a dump/load cycle."""
        yaml = "items:\n  # first item\n  - foo\n  - bar"
        doc = yarutsk.load(io.StringIO(yaml))
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        result = out.getvalue()
        assert "# first item" in result
        assert doc2["items"][0] == "foo"

    def test_append_then_comment_survives_dump(self):
        """Appending an item and adding a comment on the new item round-trips correctly."""
        doc = yarutsk.loads("items:\n  - foo\n  - bar")
        items = doc["items"]
        items.append("baz")
        items.set_comment_inline(2, "newly added")
        result = yarutsk.dumps(doc)
        assert "baz" in result
        assert "# newly added" in result
        doc2 = yarutsk.loads(result)
        assert doc2["items"][2] == "baz"
        assert doc2["items"].get_comment_inline(2) == "newly added"

    def test_nested_mapping_mutation_then_comment_survives_dump(self):
        """Mutating a nested mapping and adding a comment on it round-trips correctly."""
        doc = yarutsk.loads("server:\n  host: localhost\n  port: 5432")
        server = doc["server"]
        server["port"] = 5433
        server.set_comment_inline("port", "changed")
        result = yarutsk.dumps(doc)
        assert "5433" in result
        assert "# changed" in result
        doc2 = yarutsk.loads(result)
        assert doc2["server"]["port"] == 5433
        assert doc2["server"].get_comment_inline("port") == "changed"

    def test_inline_on_sequence_item_does_not_attach_to_parent_key(self):
        """Inline comment on a sequence item is NOT on the mapping key above."""
        doc = yarutsk.load(io.StringIO("items:\n  - foo  # item comment"))
        assert doc.get_comment_inline("items") is None

    # ── whitespace preservation in comment text ───────────────────────────────

    def test_comment_text_trailing_spaces_stripped_by_emitter(self):
        """Verify the emitter writes the comment text we stored."""
        doc = yarutsk.load(io.StringIO("key: val"))
        doc.set_comment_inline("key", "text with spaces  ")
        out = io.StringIO()
        yarutsk.dump(doc, out)
        assert "# text with spaces  " in out.getvalue()

    def test_multiline_before_comment_round_trips(self):
        """Multi-line before-comment round-trips through dump/load."""
        doc = yarutsk.load(io.StringIO("# line one\n# line two\nkey: val"))
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        before = doc2.get_comment_before("key")
        assert "line one" in before
        assert "line two" in before

    def test_set_multiline_before_comment(self):
        """set_comment_before with embedded newlines emits multiple # lines."""
        doc = yarutsk.load(io.StringIO("key: val"))
        doc.set_comment_before("key", "first line\nsecond line")
        out = io.StringIO()
        yarutsk.dump(doc, out)
        result = out.getvalue()
        assert "# first line" in result
        assert "# second line" in result


class TestCommentMutations:
    """Tests for comment behaviour when values or structure are mutated."""

    # ── overwrite / clear ────────────────────────────────────────────────────

    def test_overwrite_inline_comment(self):
        """Calling set_comment_inline twice keeps only the latest text."""
        doc = yarutsk.loads("key: val  # original")
        doc.set_comment_inline("key", "updated")
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_overwrite_before_comment(self):
        """Calling set_comment_before twice keeps only the latest text."""
        doc = yarutsk.loads("# original\nkey: val")
        doc.set_comment_before("key", "updated")
        out = yarutsk.dumps(doc)
        assert "# updated" in out
        assert "original" not in out

    def test_clear_inline_comment_with_none(self):
        """set_comment_inline(key, None) removes the comment from output."""
        doc = yarutsk.loads("key: val  # remove me")
        doc.set_comment_inline("key", None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    def test_clear_before_comment_with_none(self):
        """set_comment_before(key, None) removes the comment from output."""
        doc = yarutsk.loads("# remove me\nkey: val")
        doc.set_comment_before("key", None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    # ── both comment types on the same key ───────────────────────────────────

    def test_inline_and_before_on_same_key(self):
        """A key can carry both an inline and a before-comment simultaneously."""
        doc = yarutsk.loads("# above\nkey: val  # beside")
        assert doc.get_comment_before("key") == "above"
        assert doc.get_comment_inline("key") == "beside"
        out = yarutsk.dumps(doc)
        assert "# above" in out
        assert "# beside" in out

    def test_set_both_comment_types_then_round_trip(self):
        doc = yarutsk.loads("key: val")
        doc.set_comment_before("key", "header")
        doc.set_comment_inline("key", "side")
        doc2 = yarutsk.loads(yarutsk.dumps(doc))
        assert doc2.get_comment_before("key") == "header"
        assert doc2.get_comment_inline("key") == "side"

    # ── comment survives value mutation ─────────────────────────────────────

    def test_inline_comment_survives_value_change(self):
        """Changing a value via __setitem__ preserves the existing inline comment."""
        doc = yarutsk.loads("port: 5432  # db port")
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    def test_before_comment_survives_value_change(self):
        """Changing a value via __setitem__ preserves the existing before-comment."""
        doc = yarutsk.loads("# db port\nport: 5432")
        doc["port"] = 5433
        out = yarutsk.dumps(doc)
        assert "5433" in out
        assert "# db port" in out

    # ── comments gone after deletion ─────────────────────────────────────────

    def test_comment_gone_after_del(self):
        """After deleting a key its comment no longer appears in output."""
        doc = yarutsk.loads("a: 1  # keep\nb: 2  # gone")
        del doc["b"]
        out = yarutsk.dumps(doc)
        assert "# keep" in out
        assert "# gone" not in out

    def test_comment_gone_after_pop(self):
        doc = yarutsk.loads("a: 1\n# before b\nb: 2")
        doc.pop("b")
        out = yarutsk.dumps(doc)
        assert "before b" not in out

    # ── update() ─────────────────────────────────────────────────────────────

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
        assert doc.get_comment_inline("b") is None
        assert doc.get_comment_before("b") is None

    # ── comment on newly added key ───────────────────────────────────────────

    def test_add_comment_to_new_key(self):
        """A key added via __setitem__ can receive a comment and round-trips."""
        doc = yarutsk.loads("a: 1")
        doc["b"] = 2
        doc.set_comment_inline("b", "new key")
        out = yarutsk.dumps(doc)
        assert "b: 2" in out
        assert "# new key" in out
        doc2 = yarutsk.loads(out)
        assert doc2.get_comment_inline("b") == "new key"


class TestCommentSequenceMutations:
    """Tests for comment behaviour on sequence items after mutation."""

    # ── set_comment_* on sequence indices ────────────────────────────────────

    def test_set_inline_on_sequence_item(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.set_comment_inline(1, "middle")
        out = yarutsk.dumps(doc)
        assert "# middle" in out
        doc2 = yarutsk.loads(out)
        assert doc2.get_comment_inline(1) == "middle"

    def test_set_before_on_sequence_item(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.set_comment_before(2, "last item")
        out = yarutsk.dumps(doc)
        assert "# last item" in out
        doc2 = yarutsk.loads(out)
        assert doc2.get_comment_before(2) == "last item"

    def test_overwrite_inline_on_sequence_item(self):
        doc = yarutsk.loads("- a  # old\n- b")
        doc.set_comment_inline(0, "new")
        out = yarutsk.dumps(doc)
        assert "# new" in out
        assert "old" not in out

    def test_clear_inline_on_sequence_item(self):
        doc = yarutsk.loads("- a  # remove\n- b")
        doc.set_comment_inline(0, None)
        out = yarutsk.dumps(doc)
        assert "#" not in out

    # ── insert() shifts comments ─────────────────────────────────────────────

    def test_insert_shifts_item_with_comment(self):
        """insert(0, …) shifts existing items; the comment travels with them."""
        doc = yarutsk.loads("- a\n- b  # on b")
        doc.insert(0, "z")
        assert doc[2] == "b"
        assert doc.get_comment_inline(2) == "on b"

    def test_insert_new_item_has_no_comment(self):
        doc = yarutsk.loads("- a\n- b")
        doc.insert(1, "new")
        assert doc.get_comment_inline(1) is None
        assert doc.get_comment_before(1) is None

    # ── pop() removes comment ────────────────────────────────────────────────

    def test_pop_removes_comment_from_output(self):
        doc = yarutsk.loads("- a  # first\n- b\n- c")
        doc.pop(0)
        out = yarutsk.dumps(doc)
        assert "# first" not in out

    def test_pop_shifts_remaining_comments(self):
        """After pop(0), what was item 1 is now item 0 and keeps its comment."""
        doc = yarutsk.loads("- a\n- b  # on b\n- c")
        doc.pop(0)
        assert doc[0] == "b"
        assert doc.get_comment_inline(0) == "on b"

    # ── reverse() keeps comments with their items ────────────────────────────

    def test_reverse_keeps_comments_with_items(self):
        doc = yarutsk.loads("- a  # first\n- b\n- c  # last")
        doc.reverse()
        assert doc[0] == "c"
        assert doc.get_comment_inline(0) == "last"
        assert doc[2] == "a"
        assert doc.get_comment_inline(2) == "first"
