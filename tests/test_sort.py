"""Tests for key sorting functionality in yarutsk."""

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


class TestKeySorting:
    """Test key sorting functionality."""

    def test_sort_keys_default(self):
        """Default alphabetical sort."""
        content = io.StringIO("z: 1\na: 2\nm: 3")
        doc = yarutsk.load(content)

        assert list(doc.keys()) == ["z", "a", "m"]

        doc.sort_keys()
        assert list(doc.keys()) == ["a", "m", "z"]

    def test_sort_keys_custom_function(self):
        """Sort with custom key function."""
        content = io.StringIO("banana: 1\napple: 2\ncherry: 3")
        doc = yarutsk.load(content)

        doc.sort_keys(key=lambda k: len(k))
        assert list(doc.keys()) == ["apple", "banana", "cherry"]

    def test_sort_keys_reverse(self):
        """Reverse alphabetical sort."""
        content = io.StringIO("a: 1\nb: 2\nc: 3")
        doc = yarutsk.load(content)

        doc.sort_keys(reverse=True)
        assert list(doc.keys()) == ["c", "b", "a"]

    def test_sort_keys_recursive(self):
        """Recursive sort of nested mappings."""
        content = io.StringIO("""
z: 1
a:
  m: 1
  b: 2
m: 3
""")
        doc = yarutsk.load(content)

        doc.sort_keys(recursive=True)
        assert list(doc.keys()) == ["a", "m", "z"]
        assert list(doc["a"].keys()) == ["b", "m"]

    def test_sort_sequence(self):
        """Sort a sequence."""
        content = io.StringIO("""
items:
  - zebra
  - apple
  - mango
""")
        doc = yarutsk.load(content)
        items = doc["items"]

        items.sort()
        assert items[0] == "apple"
        assert items[1] == "mango"
        assert items[2] == "zebra"

    def test_sort_sequence_with_key(self):
        """Sort a sequence with custom key function."""
        content = io.StringIO("""
items:
  - banana
  - apple
  - cherry
""")
        doc = yarutsk.load(content)
        items = doc["items"]

        items.sort(key=lambda x: len(x))
        assert items[0] == "apple"
        assert items[1] == "banana"
        assert items[2] == "cherry"


class TestSortingEdgeCases:
    """Test edge cases in sorting."""

    def test_sort_empty_mapping(self):
        """Sort an empty mapping."""
        content = io.StringIO("{}")
        doc = yarutsk.load(content)
        doc.sort_keys()
        assert list(doc.keys()) == []

    def test_sort_single_key(self):
        """Sort a mapping with single key."""
        content = io.StringIO("a: 1")
        doc = yarutsk.load(content)
        doc.sort_keys()
        assert list(doc.keys()) == ["a"]

    def test_sort_preserves_comments(self):
        """Sorting preserves inline and before-key comments."""
        content = io.StringIO("""
z: 1  # z comment
a: 2  # a comment
m: 3  # m comment
""")
        doc = yarutsk.load(content)

        doc.sort_keys()
        assert list(doc.keys()) == ["a", "m", "z"]
        assert doc.get_comment_inline("a") == "a comment"
        assert doc.get_comment_inline("m") == "m comment"
        assert doc.get_comment_inline("z") == "z comment"

    def test_sort_keys_reverse_custom(self):
        """Reverse sort with custom key function."""
        content = io.StringIO("banana: 1\napple: 2\ncherry: 3")
        doc = yarutsk.load(content)
        doc.sort_keys(key=lambda k: len(k), reverse=True)
        assert list(doc.keys()) == ["cherry", "banana", "apple"]

    def test_sort_sequence_reverse(self):
        """Reverse-sort a sequence."""
        content = io.StringIO("- a\n- c\n- b")
        doc = yarutsk.load(content)
        doc.sort(reverse=True)
        assert doc[0] == "c"
        assert doc[1] == "b"
        assert doc[2] == "a"

    def test_sort_sequence_empty(self):
        """Sort an empty sequence does not error."""
        content = io.StringIO("[]")
        doc = yarutsk.load(content)
        doc.sort()
        assert len(doc) == 0

    def test_sort_not_recursive_by_default(self):
        """sort_keys does not recurse unless asked."""
        content = io.StringIO("z: 1\na:\n  m: 1\n  b: 2")
        doc = yarutsk.load(content)
        doc.sort_keys()
        assert list(doc.keys()) == ["a", "z"]
        assert list(doc["a"].keys()) == ["m", "b"]  # inner unchanged


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
