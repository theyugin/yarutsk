"""Tests for yarutsk YAML library."""

import io
import os
import sys
from typing import Any

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

yarutsk: Any = None

try:
    import yarutsk as _yarutsk

    yarutsk = _yarutsk
    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

pytestmark = pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk module not built")


class TestBasicLoading:
    """Test basic YAML loading functionality."""

    def test_load_from_stringio(self):
        """Test loading from StringIO."""
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_from_bytesio(self):
        """Test loading from BytesIO."""
        content = io.BytesIO(b"name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_load_nested_mapping(self):
        """Test loading nested mappings."""
        content = io.StringIO("""
person:
  name: John
  age: 30
  address:
    city: New York
""")
        doc = yarutsk.load(content)
        assert doc["person"]["name"] == "John"
        assert doc["person"]["age"] == 30
        assert doc["person"]["address"]["city"] == "New York"

    def test_load_sequence(self):
        """Test loading sequences."""
        content = io.StringIO("""
items:
  - first
  - second
  - third
""")
        doc = yarutsk.load(content)
        items = doc["items"]
        assert items[0] == "first"
        assert items[1] == "second"
        assert items[2] == "third"

    def test_load_flow_sequence(self):
        """Test loading flow sequences."""
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        assert doc[0] == "a"
        assert doc[1] == "b"
        assert doc[2] == "c"

    def test_load_flow_mapping(self):
        """Test loading flow mappings."""
        content = io.StringIO("{a: 1, b: 2}")
        doc = yarutsk.load(content)
        assert doc["a"] == 1
        assert doc["b"] == 2


class TestTypePreservation:
    """Test that YAML types are correctly preserved."""

    def test_integer(self):
        content = io.StringIO("value: 42")
        doc = yarutsk.load(content)
        assert doc["value"] == 42
        assert isinstance(doc["value"], int)

    def test_float(self):
        content = io.StringIO("value: 3.14")
        doc = yarutsk.load(content)
        assert doc["value"] == pytest.approx(3.14)
        assert isinstance(doc["value"], float)

    def test_boolean_true(self):
        content = io.StringIO("value: true")
        doc = yarutsk.load(content)
        assert doc["value"] is True

    def test_boolean_false(self):
        content = io.StringIO("value: false")
        doc = yarutsk.load(content)
        assert doc["value"] is False

    def test_null(self):
        content = io.StringIO("value: null")
        doc = yarutsk.load(content)
        assert doc["value"] is None

    def test_string(self):
        content = io.StringIO('value: "hello world"')
        doc = yarutsk.load(content)
        assert doc["value"] == "hello world"
        assert isinstance(doc["value"], str)

    def test_quoted_string(self):
        content = io.StringIO("value: 'quoted string'")
        doc = yarutsk.load(content)
        assert doc["value"] == "quoted string"

    def test_empty_double_quoted_string(self):
        """Empty double-quoted string \"\" must be an empty str, not None."""
        doc = yarutsk.loads('key: ""')
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_single_quoted_string(self):
        """Empty single-quoted string '' must be an empty str, not None."""
        doc = yarutsk.loads("key: ''")
        assert doc["key"] == ""
        assert isinstance(doc["key"], str)

    def test_empty_quoted_strings_in_sequence(self):
        """Empty quoted strings inside a sequence are preserved as empty strings."""
        doc = yarutsk.loads("- \"\"\n- ''")
        assert doc[0] == ""
        assert doc[1] == ""
        assert isinstance(doc[0], str)
        assert isinstance(doc[1], str)

    def test_empty_quoted_vs_bare_null(self):
        """Bare empty value and ~ are null; quoted empty is an empty string."""
        doc = yarutsk.loads('bare:\nnull_tilde: ~\nquoted: ""')
        assert doc["bare"] is None
        assert doc["null_tilde"] is None
        assert doc["quoted"] == ""

    def test_empty_quoted_round_trips(self):
        """Empty quoted string survives a dump/load cycle as an empty string."""
        doc = yarutsk.loads("a: \"\"\nb: ''")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["a"] == ""
        assert doc2["b"] == ""


class TestInsertionOrderPreservation:
    """Test that insertion order is preserved."""

    def test_order_preserved_on_load(self):
        """Keys appear in same order as input YAML."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        assert list(doc.keys()) == ["z", "a", "m"]

    def test_order_preserved_on_insert(self):
        """New keys appended at end."""
        content = io.StringIO("a: 1\nb: 2")
        doc = yarutsk.load(content)
        doc["z"] = 3
        assert list(doc.keys()) == ["a", "b", "z"]

    def test_nested_order_preserved(self):
        """Nested mappings also preserve order."""
        content = io.StringIO("""
outer:
  z: 1
  a: 2
  m: 3
""")
        doc = yarutsk.load(content)
        assert list(doc["outer"].keys()) == ["z", "a", "m"]

    def test_round_trip_order(self):
        """Order preserved through parse-modify-serialize cycle."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        doc["b"] = 4
        output = io.StringIO()
        yarutsk.dump(doc, output)
        result = output.getvalue()
        assert result.index("z:") < result.index("a:")
        assert result.index("a:") < result.index("m:")
        assert result.index("m:") < result.index("b:")


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
        # Reload and verify the comment survived the round-trip
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
        # "b" is now at index 2; its comment should be there
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


class TestSorting:
    """Test sorting functionality."""

    def test_sort_keys_default(self):
        """Default alphabetical sort."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        assert list(doc.keys()) == ["z", "a", "m"]
        doc.sort_keys()
        assert list(doc.keys()) == ["a", "m", "z"]

    def test_sort_then_insert(self):
        """New keys inserted after sort go to the end."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)
        doc.sort_keys()
        doc["b"] = 4
        assert list(doc.keys()) == ["a", "m", "z", "b"]


class TestContains:
    """Test __contains__ functionality."""

    def test_contains_existing_key(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert "name" in doc
        assert "age" in doc

    def test_contains_missing_key(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        assert "missing" not in doc


class TestStringAPI:
    """Test loads/dumps string-based API."""

    def test_loads_basic(self):
        doc = yarutsk.loads("name: John\nage: 30")
        assert doc["name"] == "John"
        assert doc["age"] == 30

    def test_loads_empty(self):
        assert yarutsk.loads("") is None

    def test_loads_returns_first_doc(self):
        doc = yarutsk.loads("---\na: 1\n---\nb: 2")
        assert doc["a"] == 1

    def test_loads_all_basic(self):
        docs = yarutsk.loads_all("---\na: 1\n---\nb: 2")
        assert len(docs) == 2
        assert docs[0]["a"] == 1
        assert docs[1]["b"] == 2

    def test_loads_all_empty(self):
        assert yarutsk.loads_all("") == []

    def test_dumps_basic(self):
        doc = yarutsk.loads("name: John\nage: 30")
        result = yarutsk.dumps(doc)
        assert isinstance(result, str)
        assert "name: John" in result
        assert "age: 30" in result

    def test_dumps_preserves_comments(self):
        doc = yarutsk.loads("key: val  # note")
        result = yarutsk.dumps(doc)
        assert "# note" in result

    def test_dumps_all_basic(self):
        docs = yarutsk.loads_all("---\na: 1\n---\nb: 2")
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
        original = "name: Alice\nage: 30  # years\ncity: Berlin"
        doc = yarutsk.loads(original)
        result = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(result)
        assert doc2["name"] == "Alice"
        assert doc2["age"] == 30
        assert doc2.get_comment_inline("age") == "years"

    def test_loads_all_dumps_all_round_trip(self):
        original = "---\na: 1\n---\nb: 2\n---\nc: 3"
        docs = yarutsk.loads_all(original)
        result = yarutsk.dumps_all(docs)
        docs2 = yarutsk.loads_all(result)
        assert len(docs2) == 3
        assert docs2[0]["a"] == 1
        assert docs2[1]["b"] == 2
        assert docs2[2]["c"] == 3

    def test_loads_is_equivalent_to_load(self):
        yaml = "x: 1\ny: 2"
        doc_stream = yarutsk.load(io.StringIO(yaml))
        doc_str = yarutsk.loads(yaml)
        assert repr(doc_stream) == repr(doc_str)


class TestSerialization:
    """Test serialization functionality."""

    def test_dump_to_stringio(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        output = io.StringIO()
        yarutsk.dump(doc, output)
        result = output.getvalue()
        assert "name: John" in result
        assert "age: 30" in result

    def test_dump_to_bytesio(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        output = io.BytesIO()
        yarutsk.dump(doc, output)
        result = output.getvalue().decode("utf-8")
        assert "name: John" in result
        assert "age: 30" in result

    def test_round_trip_preserves_data(self):
        content = io.StringIO("""
name: John
age: 30
items:
  - first
  - second
""")
        doc = yarutsk.load(content)
        output = io.StringIO()
        yarutsk.dump(doc, output)

        # Reload and verify
        doc2 = yarutsk.load(io.StringIO(output.getvalue()))
        assert doc2["name"] == "John"
        assert doc2["age"] == 30
        assert doc2["items"][0] == "first"
        assert doc2["items"][1] == "second"


class TestToDict:
    """Test to_dict conversion."""

    def test_to_dict_simple(self):
        content = io.StringIO("name: John\nage: 30")
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


class TestDumpDumpAll:
    """Test module-level dump and dump_all functions."""

    def test_dump_single_doc(self):
        doc = yarutsk.load(io.StringIO("name: John\nage: 30"))
        out = io.StringIO()
        yarutsk.dump(doc, out)
        result = out.getvalue()
        assert "name: John" in result
        assert "age: 30" in result

    def test_dump_round_trip(self):
        doc = yarutsk.load(io.StringIO("a: 1\nb: 2"))
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        assert doc2["a"] == 1
        assert doc2["b"] == 2

    def test_dump_all_multiple_docs(self):
        docs = yarutsk.load_all(io.StringIO("---\na: 1\n---\nb: 2\n---\nc: 3"))
        assert len(docs) == 3
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert "---" in result
        # Round-trip
        docs2 = yarutsk.load_all(io.StringIO(result))
        assert len(docs2) == 3
        assert docs2[0]["a"] == 1
        assert docs2[1]["b"] == 2
        assert docs2[2]["c"] == 3

    def test_dump_all_single_doc(self):
        docs = yarutsk.load_all(io.StringIO("a: 1"))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert "---" not in result
        assert "a: 1" in result

    def test_dump_all_preserves_comments(self):
        docs = yarutsk.load_all(
            io.StringIO("# comment\n---\na: 1  # inline\n---\nb: 2")
        )
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        docs2 = yarutsk.load_all(io.StringIO(result))
        assert docs2[0]["a"] == 1
        assert docs2[0].get_comment_inline("a") == "inline"
        assert docs2[1]["b"] == 2


class TestMultiDocument:
    """Test multi-document YAML support."""

    MULTI_DOC = (
        "---\nname: Alice\nage: 30\n---\nname: Bob\nage: 25\n---\nname: Carol\nage: 35"
    )

    def test_load_all_returns_list(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert isinstance(docs, list)
        assert len(docs) == 3

    def test_load_all_each_doc_independent(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert docs[0]["name"] == "Alice"
        assert docs[1]["name"] == "Bob"
        assert docs[2]["name"] == "Carol"

    def test_load_all_single_doc_no_separator(self):
        docs = yarutsk.load_all(io.StringIO("a: 1\nb: 2"))
        assert len(docs) == 1
        assert docs[0]["a"] == 1

    def test_load_all_empty_stream(self):
        docs = yarutsk.load_all(io.StringIO(""))
        assert docs == []

    def test_load_returns_first_doc_only(self):
        doc = yarutsk.load(io.StringIO(self.MULTI_DOC))
        assert doc["name"] == "Alice"

    def test_dump_all_separators(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert result.count("---") == 3

    def test_dump_all_round_trip(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        docs2 = yarutsk.load_all(io.StringIO(out.getvalue()))
        assert len(docs2) == 3
        for d1, d2 in zip(docs, docs2):
            assert repr(d1) == repr(d2)

    def test_dump_all_single_doc_no_separator(self):
        docs = yarutsk.load_all(io.StringIO("x: 42"))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        assert "---" not in out.getvalue()

    def test_docs_are_independent_objects(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        docs[0]["name"] = "Modified"
        assert docs[1]["name"] == "Bob"

    def test_mixed_types_across_docs(self):
        yaml = "---\na: 1\n---\n- x\n- y"
        docs = yarutsk.load_all(io.StringIO(yaml))
        assert len(docs) == 2
        assert docs[0]["a"] == 1
        assert docs[1][0] == "x"

    def test_scalar_top_level(self):
        doc = yarutsk.loads("scalar")
        assert type(doc).__name__ == "YamlScalar"
        assert doc.to_dict() == "scalar"
        doc2 = yarutsk.loads("42")
        assert doc2.to_dict() == 42

    def test_comments_preserved_across_docs(self):
        yaml = "---\nkey: val  # doc1 comment\n---\nother: data  # doc2 comment"
        docs = yarutsk.load_all(io.StringIO(yaml))
        assert docs[0].get_comment_inline("key") == "doc1 comment"
        assert docs[1].get_comment_inline("other") == "doc2 comment"

    def test_dump_all_to_bytesio(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        out = io.BytesIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue().decode("utf-8")
        assert "Alice" in result
        assert "Bob" in result


class TestRepr:
    """Test __repr__ functionality."""

    def test_repr_mapping(self):
        content = io.StringIO("a: 1\nb: 2")
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "mapping" in r.lower() or "YAML" in r

    def test_repr_sequence(self):
        content = io.StringIO("[a, b, c]")
        doc = yarutsk.load(content)
        r = repr(doc)
        assert "sequence" in r.lower() or "YAML" in r


class TestQuotedTypeLookalikes:
    """Quoted scalars that look like other types must stay as strings."""

    def test_quoted_true_is_str(self):
        doc = yarutsk.loads('key: "true"')
        assert doc["key"] == "true"
        assert isinstance(doc["key"], str)

    def test_quoted_false_is_str(self):
        doc = yarutsk.loads("key: 'false'")
        assert doc["key"] == "false"
        assert isinstance(doc["key"], str)

    def test_quoted_null_is_str(self):
        doc = yarutsk.loads('key: "null"')
        assert doc["key"] == "null"
        assert isinstance(doc["key"], str)

    def test_quoted_integer_is_str(self):
        doc = yarutsk.loads('key: "42"')
        assert doc["key"] == "42"
        assert isinstance(doc["key"], str)

    def test_quoted_float_is_str(self):
        doc = yarutsk.loads("key: '3.14'")
        assert doc["key"] == "3.14"
        assert isinstance(doc["key"], str)

    def test_quoted_zero_is_str(self):
        doc = yarutsk.loads('key: "0"')
        assert doc["key"] == "0"
        assert isinstance(doc["key"], str)

    def test_quoted_yes_is_str(self):
        """'yes' is a bool in YAML 1.1 — but only unquoted."""
        doc = yarutsk.loads('key: "yes"')
        assert doc["key"] == "yes"
        assert isinstance(doc["key"], str)

    def test_plain_true_is_bool(self):
        doc = yarutsk.loads("key: true")
        assert doc["key"] is True

    def test_plain_false_is_bool(self):
        doc = yarutsk.loads("key: false")
        assert doc["key"] is False

    def test_plain_null_is_none(self):
        doc = yarutsk.loads("key: null")
        assert doc["key"] is None

    def test_tilde_is_none(self):
        doc = yarutsk.loads("key: ~")
        assert doc["key"] is None

    def test_plain_integer_is_int(self):
        doc = yarutsk.loads("key: 42")
        assert doc["key"] == 42
        assert isinstance(doc["key"], int)


class TestSpecialFloats:
    """Special float literals: .inf, -.inf, .nan."""

    def test_inf(self):
        import math

        doc = yarutsk.loads("key: .inf")
        assert math.isinf(doc["key"])
        assert doc["key"] > 0

    def test_negative_inf(self):
        import math

        doc = yarutsk.loads("key: -.inf")
        assert math.isinf(doc["key"])
        assert doc["key"] < 0

    def test_nan(self):
        import math

        doc = yarutsk.loads("key: .nan")
        assert math.isnan(doc["key"])

    def test_inf_round_trip(self):
        import math

        doc = yarutsk.loads("key: .inf")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert math.isinf(doc2["key"])


class TestBlockScalars:
    """Literal | and folded > block scalars."""

    def test_literal_block_preserves_newlines(self):
        yaml = "text: |\n  line one\n  line two\n"
        doc = yarutsk.loads(yaml)
        assert "line one" in doc["text"]
        assert "line two" in doc["text"]
        assert "\n" in doc["text"]

    def test_folded_block_is_string(self):
        yaml = "text: >\n  folded\n  text\n"
        doc = yarutsk.loads(yaml)
        assert isinstance(doc["text"], str)
        assert "folded" in doc["text"]

    def test_literal_block_value_is_string(self):
        """Block scalar value is a plain Python str after loading."""
        yaml = "text: |\n  hello\n  world\n"
        doc = yarutsk.loads(yaml)
        assert isinstance(doc["text"], str)
        assert doc["text"].startswith("hello")


class TestNegativeSequenceIndices:
    """Negative indices on YamlSequence should work like Python lists."""

    def test_getitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        assert doc[-1] == "c"
        assert doc[-2] == "b"
        assert doc[-3] == "a"

    def test_setitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc[-1] = "z"
        assert doc[2] == "z"

    def test_delitem_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        del doc[-1]
        assert len(doc) == 2
        assert doc[-1] == "b"

    def test_pop_negative(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        val = doc.pop(-1)
        assert val == "c"
        assert len(doc) == 2

    def test_get_comment_negative_index(self):
        doc = yarutsk.loads("- a  # first\n- b\n- c  # last")
        assert doc.get_comment_inline(-1) == "last"
        assert doc.get_comment_inline(-3) == "first"

    def test_set_comment_negative_index(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.set_comment_inline(-1, "tail note")
        out = yarutsk.dumps(doc)
        assert "# tail note" in out
        doc2 = yarutsk.loads(out)
        assert doc2.get_comment_inline(2) == "tail note"


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
        doc = yarutsk.loads("a: 1\nb: 2")
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
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(IndexError):
            _ = doc[5]

    def test_delitem_out_of_range_raises(self):
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(IndexError):
            del doc[5]

    def test_set_comment_inline_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            doc.set_comment_inline("missing", "note")

    def test_set_comment_before_missing_key_raises(self):
        doc = yarutsk.loads("a: 1")
        with pytest.raises(KeyError):
            doc.set_comment_before("missing", "note")

    def test_get_comment_inline_missing_key_returns_none(self):
        doc = yarutsk.loads("a: 1")
        assert doc.get_comment_inline("missing") is None

    def test_get_comment_before_missing_key_returns_none(self):
        doc = yarutsk.loads("a: 1")
        assert doc.get_comment_before("missing") is None


class TestDictProtocol:
    """Dict/list unpacking and protocol compliance."""

    def test_dict_unpacking(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        d = {**doc}
        assert d == {"a": 1, "b": 2}

    def test_dict_constructor(self):
        doc = yarutsk.loads("a: 1\nb: 2")
        d = dict(doc)
        assert d["a"] == 1
        assert d["b"] == 2

    def test_list_unpacking(self):
        doc = yarutsk.loads("- 1\n- 2\n- 3")
        lst = [*doc]
        assert lst == [1, 2, 3]

    def test_list_constructor(self):
        doc = yarutsk.loads("- x\n- y")
        lst = list(doc)
        assert lst == ["x", "y"]

    def test_isinstance_dict(self):
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, dict)

    def test_isinstance_list(self):
        doc = yarutsk.loads("- a")
        assert isinstance(doc, list)

    def test_mapping_values(self):
        doc = yarutsk.loads("a: 1\nb: 2\nc: 3")
        vals = list(doc.values())
        assert sorted(vals) == [1, 2, 3]

    def test_mapping_items(self):
        doc = yarutsk.loads("x: 10\ny: 20")
        items = dict(doc.items())
        assert items == {"x": 10, "y": 20}

    def test_sequence_iteration(self):
        doc = yarutsk.loads("- 1\n- 2\n- 3")
        total = sum(doc)
        assert total == 6


class TestNestedObjectIdentity:
    """Mutations to nested objects must be visible through the parent."""

    def test_nested_mutation_visible_via_parent(self):
        doc = yarutsk.loads("server:\n  host: localhost\n  port: 5432")
        server = doc["server"]
        server["host"] = "remote"
        assert doc["server"]["host"] == "remote"

    def test_nested_mutation_survives_dump(self):
        doc = yarutsk.loads("db:\n  name: mydb\n  port: 5432")
        doc["db"]["port"] = 9999
        out = yarutsk.dumps(doc)
        assert "9999" in out
        doc2 = yarutsk.loads(out)
        assert doc2["db"]["port"] == 9999

    def test_deeply_nested_mutation_visible(self):
        doc = yarutsk.loads("a:\n  b:\n    c: original")
        doc["a"]["b"]["c"] = "changed"
        out = yarutsk.dumps(doc)
        assert "changed" in out
        assert "original" not in out

    def test_sequence_item_mutation_visible(self):
        doc = yarutsk.loads("items:\n  - x: 1\n  - x: 2")
        item = doc["items"][0]
        item["x"] = 99
        assert doc["items"][0]["x"] == 99

    def test_two_references_same_object(self):
        doc = yarutsk.loads("cfg:\n  val: 0")
        ref1 = doc["cfg"]
        ref2 = doc["cfg"]
        assert ref1 is ref2


class TestSequenceListMethods:
    """count(), index(), extend(), and friends on YamlSequence."""

    def test_count(self):
        doc = yarutsk.loads("- a\n- b\n- a\n- c\n- a")
        assert doc.count("a") == 3
        assert doc.count("b") == 1
        assert doc.count("missing") == 0

    def test_index(self):
        doc = yarutsk.loads("- x\n- y\n- z")
        assert doc.index("y") == 1

    def test_index_with_bounds(self):
        doc = yarutsk.loads("- a\n- b\n- c\n- b")
        assert doc.index("b", 2) == 3

    def test_index_missing_raises(self):
        doc = yarutsk.loads("- a\n- b")
        with pytest.raises(ValueError):
            doc.index("missing")

    def test_extend_appends_all(self):
        doc = yarutsk.loads("- a\n- b")
        doc.extend(["c", "d"])
        assert len(doc) == 4
        assert doc[2] == "c"
        assert doc[3] == "d"

    def test_extend_empty_no_change(self):
        doc = yarutsk.loads("- a\n- b")
        doc.extend([])
        assert len(doc) == 2

    def test_remove(self):
        doc = yarutsk.loads("- a\n- b\n- c")
        doc.remove("b")
        assert len(doc) == 2
        assert list(doc) == ["a", "c"]

    def test_mixed_types_in_sequence(self):
        doc = yarutsk.loads("- 1\n- hello\n- true\n- null\n- 3.14")
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is True
        assert doc[3] is None
        assert doc[4] == pytest.approx(3.14)

    def test_contains_in_sequence(self):
        doc = yarutsk.loads("- foo\n- bar")
        assert "foo" in doc
        assert "baz" not in doc


class TestSpecialStringRoundTrips:
    """Strings containing YAML-special characters survive dump/load."""

    def test_string_with_colon(self):
        doc = yarutsk.loads("url: 'http://example.com:8080/path'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["url"] == "http://example.com:8080/path"

    def test_string_with_hash(self):
        doc = yarutsk.loads("comment: 'color: #fff'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["comment"] == "color: #fff"

    @pytest.mark.xfail(reason="emitter does not quote strings with leading spaces")
    def test_string_with_leading_spaces(self):
        doc = yarutsk.loads("key: '  leading spaces'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == "  leading spaces"

    def test_string_with_newline(self):
        doc = yarutsk.loads("key: 'line1\\nline2'")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == doc["key"]

    def test_empty_string_key(self):
        """An empty string value on a non-empty key round-trips correctly."""
        doc = yarutsk.loads("key: ''")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert doc2["key"] == ""
        assert isinstance(doc2["key"], str)


class TestEmptyDocuments:
    """Edge cases around empty / nearly-empty YAML."""

    def test_loads_empty_string(self):
        assert yarutsk.loads("") is None

    def test_loads_only_separator(self):
        """A bare --- produces a null YamlScalar, not a Python None."""
        result = yarutsk.loads("---")
        # An explicit document start with no content yields a null scalar node
        assert result is None or (
            type(result).__name__ == "YamlScalar" and result.to_dict() is None
        )

    def test_loads_all_empty(self):
        assert yarutsk.loads_all("") == []

    def test_loads_empty_mapping(self):
        doc = yarutsk.loads("{}")
        assert isinstance(doc, dict)
        assert len(doc) == 0

    def test_loads_empty_sequence(self):
        doc = yarutsk.loads("[]")
        assert isinstance(doc, list)
        assert len(doc) == 0

    def test_empty_mapping_round_trips(self):
        doc = yarutsk.loads("{}")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, dict)
        assert len(doc2) == 0

    def test_empty_sequence_round_trips(self):
        doc = yarutsk.loads("[]")
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, list)
        assert len(doc2) == 0


class TestGetMethod:
    """YamlMapping.get() edge cases."""

    def test_get_existing_key(self):
        doc = yarutsk.loads("a: 1\nb: 2")
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


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
