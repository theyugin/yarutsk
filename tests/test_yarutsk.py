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
        doc.dump(output)
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
        doc.dump(output)
        assert "# inline comment" in output.getvalue()

    def test_set_comment_inline(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.set_comment_inline("name", "new comment")
        output = io.StringIO()
        doc.dump(output)
        assert "# new comment" in output.getvalue()

    def test_set_comment_before(self):
        content = io.StringIO("name: John")
        doc = yarutsk.load(content)
        doc.set_comment_before("name", "Header comment")
        output = io.StringIO()
        doc.dump(output)
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
        doc.dump(out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        result = out.getvalue()
        assert "# first item" in result
        assert doc2["items"][0] == "foo"

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
        doc.dump(out)
        assert "# text with spaces  " in out.getvalue()

    def test_multiline_before_comment_round_trips(self):
        """Multi-line before-comment round-trips through dump/load."""
        doc = yarutsk.load(io.StringIO("# line one\n# line two\nkey: val"))
        out = io.StringIO()
        doc.dump(out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        before = doc2.get_comment_before("key")
        assert "line one" in before
        assert "line two" in before

    def test_set_multiline_before_comment(self):
        """set_comment_before with embedded newlines emits multiple # lines."""
        doc = yarutsk.load(io.StringIO("key: val"))
        doc.set_comment_before("key", "first line\nsecond line")
        out = io.StringIO()
        doc.dump(out)
        result = out.getvalue()
        assert "# first line" in result
        assert "# second line" in result


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


class TestSerialization:
    """Test serialization functionality."""

    def test_dump_to_stringio(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        output = io.StringIO()
        doc.dump(output)
        result = output.getvalue()
        assert "name: John" in result
        assert "age: 30" in result

    def test_dump_to_bytesio(self):
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        output = io.BytesIO()
        doc.dump(output)
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
        doc.dump(output)

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

    MULTI_DOC = "---\nname: Alice\nage: 30\n---\nname: Bob\nage: 25\n---\nname: Carol\nage: 35"

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
        yaml = "---\na: 1\n---\n- x\n- y\n---\nscalar"
        docs = yarutsk.load_all(io.StringIO(yaml))
        assert len(docs) == 3
        assert docs[0]["a"] == 1
        assert docs[1][0] == "x"
        assert docs[2].to_dict() == "scalar"

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


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
