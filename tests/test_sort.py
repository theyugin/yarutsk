"""Tests for key sorting functionality in yarutsk."""

from textwrap import dedent

import pytest

import yarutsk


class TestKeySorting:
    def test_sort_keys_default(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)

        assert list(doc.keys()) == ["z", "a", "m"]

        doc.sort_keys()
        assert list(doc.keys()) == ["a", "m", "z"]

    def test_sort_keys_custom_function(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            banana: 1
            apple: 2
            cherry: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)

        doc.sort_keys(key=lambda k: len(k))
        assert list(doc.keys()) == ["apple", "banana", "cherry"]

    def test_sort_keys_reverse(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
            c: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)

        doc.sort_keys(reverse=True)
        assert list(doc.keys()) == ["c", "b", "a"]

    def test_sort_keys_recursive(self) -> None:
        doc = yarutsk.loads("""
z: 1
a:
  m: 1
  b: 2
m: 3
""")
        assert isinstance(doc, yarutsk.YamlMapping)

        doc.sort_keys(recursive=True)
        assert list(doc.keys()) == ["a", "m", "z"]
        assert list(doc["a"].keys()) == ["b", "m"]

    def test_sort_sequence(self) -> None:
        doc = yarutsk.loads("""
items:
  - zebra
  - apple
  - mango
""")
        assert isinstance(doc, yarutsk.YamlMapping)
        items = doc["items"]

        items.sort()
        assert items[0] == "apple"
        assert items[1] == "mango"
        assert items[2] == "zebra"

    def test_sort_sequence_with_key(self) -> None:
        doc = yarutsk.loads("""
items:
  - banana
  - apple
  - cherry
""")
        assert isinstance(doc, yarutsk.YamlMapping)
        items = doc["items"]

        items.sort(key=lambda x: len(x))
        assert items[0] == "apple"
        assert items[1] == "banana"
        assert items[2] == "cherry"


class TestSortingEdgeCases:
    def test_sort_empty_mapping(self) -> None:
        doc = yarutsk.loads("{}")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys()
        assert list(doc.keys()) == []

    def test_sort_single_key(self) -> None:
        doc = yarutsk.loads("a: 1")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys()
        assert list(doc.keys()) == ["a"]

    def test_sort_preserves_comments(self) -> None:
        doc = yarutsk.loads("""
z: 1  # z comment
a: 2  # a comment
m: 3  # m comment
""")
        assert isinstance(doc, yarutsk.YamlMapping)

        doc.sort_keys()
        assert list(doc.keys()) == ["a", "m", "z"]
        assert doc.node("a").comment_inline == "a comment"
        assert doc.node("m").comment_inline == "m comment"
        assert doc.node("z").comment_inline == "z comment"

    def test_sort_keys_reverse_custom(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            banana: 1
            apple: 2
            cherry: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys(key=lambda k: len(k), reverse=True)
        assert list(doc.keys()) == ["cherry", "banana", "apple"]

    def test_sort_sequence_reverse(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - a
            - c
            - b
        """)
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(reverse=True)
        assert doc[0] == "c"
        assert doc[1] == "b"
        assert doc[2] == "a"

    def test_sort_sequence_empty(self) -> None:
        doc = yarutsk.loads("[]")
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort()
        assert len(doc) == 0

    def test_sort_not_recursive_by_default(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            z: 1
            a:
              m: 1
              b: 2
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys()
        assert list(doc.keys()) == ["a", "z"]
        assert list(doc["a"].keys()) == ["m", "b"]

    def test_sort_then_insert(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            z: 1
            a: 2
            m: 3
        """)
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys()
        doc["b"] = 4
        assert list(doc.keys()) == ["a", "m", "z", "b"]


class TestSortWithCustomTypes:
    """Sort behaviour when mappings/sequences contain custom-loaded Python objects."""

    def setup_method(self) -> None:
        self.schema = yarutsk.Schema()
        self.schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
        self.schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

    def test_sort_keys_preserves_custom_value_nonrecursive(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            z: !point
              x: 1
              y: 2
            a: plain
        """),
            schema=self.schema,
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["z"], Point)
        doc.sort_keys()
        assert list(doc.keys()) == ["a", "z"]
        assert isinstance(doc["z"], Point)
        assert doc["z"] == Point(1, 2)

    def test_sort_keys_preserves_custom_value_recursive(self) -> None:
        """Regression: node_to_py on the empty placeholder must not replace a custom object with {}."""
        doc = yarutsk.loads(
            dedent("""\
            z: !point
              x: 3
              y: 4
            a:
              m: 1
              b: 2
        """),
            schema=self.schema,
        )
        assert isinstance(doc, yarutsk.YamlMapping)
        assert isinstance(doc["z"], Point)
        doc.sort_keys(recursive=True)
        assert list(doc.keys()) == ["a", "z"]
        assert list(doc["a"].keys()) == ["b", "m"]
        assert isinstance(doc["z"], Point)
        assert doc["z"] == Point(3, 4)

    def test_sort_sequence_custom_type_no_key_raises(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - !point
              x: 2
              y: 0
            - !point
              x: 1
              y: 0
        """),
            schema=self.schema,
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        assert all(isinstance(v, Point) for v in doc)
        with pytest.raises(TypeError):
            doc.sort()

    def test_sort_sequence_custom_type_with_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            - !point
              x: 3
              y: 0
            - !point
              x: 1
              y: 0
            - !point
              x: 2
              y: 0
        """),
            schema=self.schema,
        )
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(key=lambda p: p.x)
        assert all(isinstance(v, Point) for v in doc)
        assert [v.x for v in doc] == [1, 2, 3]


class Point:
    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Point) and self.x == other.x and self.y == other.y


class TestSequenceRecursiveSort:
    """Test YamlSequence.sort(recursive=True)."""

    def test_recursive_sorts_nested_mapping_keys(self) -> None:
        src = dedent("""\
            a_entry:
              z: 1
              a: 2
            b_entry:
              m: 3
              b: 4
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlMapping)
        doc.sort_keys(recursive=True)
        assert list(doc["a_entry"].keys()) == ["a", "z"]
        assert list(doc["b_entry"].keys()) == ["b", "m"]

    def test_recursive_sorts_nested_mapping_keys_via_sequence(self) -> None:
        inner_src = dedent("""\
            - - 3
              - 1
              - 2
        """)
        inner_doc = yarutsk.loads(inner_src)
        assert isinstance(inner_doc, yarutsk.YamlSequence)
        inner_doc.sort(recursive=True)
        assert list(inner_doc[0]) == [1, 2, 3]

    def test_recursive_sorts_nested_sequence(self) -> None:
        src = dedent("""\
            - - 3
              - 1
              - 2
            - - 9
              - 7
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(recursive=True)
        assert list(doc[0]) == [1, 2, 3]
        assert list(doc[1]) == [7, 9]

    def test_recursive_false_does_not_sort_nested(self) -> None:
        src = dedent("""\
            - z: 1
              a: 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(recursive=False)
        assert list(doc[0].keys()) == ["z", "a"]

    def test_recursive_with_reverse(self) -> None:
        src = dedent("""\
            - - 1
              - 3
              - 2
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(recursive=True, reverse=True)
        assert list(doc[0]) == [3, 2, 1]

    def test_recursive_preserves_comments(self) -> None:
        src = dedent("""\
            - - 2  # second
              - 1  # first
        """)
        doc = yarutsk.loads(src)
        assert isinstance(doc, yarutsk.YamlSequence)
        doc.sort(recursive=True)
        assert doc[0].node(0).comment_inline == "first"
        assert doc[0].node(1).comment_inline == "second"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
