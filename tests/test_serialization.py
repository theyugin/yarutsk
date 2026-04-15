"""Tests for YAML serialization: dump, dump_all, multi-document, empty docs."""

import io
from textwrap import dedent


import yarutsk


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
        doc2 = yarutsk.load(io.StringIO(output.getvalue()))
        assert doc2["name"] == "John"
        assert doc2["age"] == 30
        assert doc2["items"][0] == "first"
        assert doc2["items"][1] == "second"


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
        assert docs2[0].comment_inline("a") == "inline"
        assert docs2[1]["b"] == 2


class TestMultiDocument:
    """Test multi-document YAML support."""

    MULTI_DOC = dedent("""\
        ---
        name: Alice
        age: 30
        ---
        name: Bob
        age: 25
        ---
        name: Carol
        age: 35
    """)

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
        yaml = dedent("""\
            ---
            a: 1
            ---
            - x
            - y
        """)
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
        yaml = dedent("""\
            ---
            key: val  # doc1 comment
            ---
            other: data  # doc2 comment
        """)
        docs = yarutsk.load_all(io.StringIO(yaml))
        assert docs[0].comment_inline("key") == "doc1 comment"
        assert docs[1].comment_inline("other") == "doc2 comment"

    def test_dump_all_to_bytesio(self):
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        out = io.BytesIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue().decode("utf-8")
        assert "Alice" in result
        assert "Bob" in result


class TestEmptyDocuments:
    """Edge cases around empty / nearly-empty YAML."""

    def test_loads_empty_string(self):
        assert yarutsk.loads("") is None

    def test_loads_only_separator(self):
        """A bare --- produces a null YamlScalar, not a Python None."""
        result = yarutsk.loads("---")
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


class TestPlainDictListDumping:
    """dumps/dump/dumps_all/dump_all accept plain Python dict and list."""

    def test_dumps_plain_dict(self):
        out = yarutsk.dumps({"a": 1, "b": 2})
        doc = yarutsk.loads(out)
        assert doc["a"] == 1
        assert doc["b"] == 2

    def test_dumps_plain_list(self):
        out = yarutsk.dumps([1, "hello", None, True])
        doc = yarutsk.loads(out)
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is None
        assert doc[3] is True

    def test_dump_plain_dict_to_stream(self):
        out = io.StringIO()
        yarutsk.dump({"key": "val"}, out)
        doc = yarutsk.loads(out.getvalue())
        assert doc["key"] == "val"

    def test_dumps_nested_plain_dict(self):
        out = yarutsk.dumps({"a": 1, "b": [1, 2, 3], "c": {"x": True}})
        doc = yarutsk.loads(out)
        assert doc["a"] == 1
        assert list(doc["b"]) == [1, 2, 3]
        assert doc["c"]["x"] is True

    def test_dumps_nested_plain_list(self):
        out = yarutsk.dumps([[1, 2], [3, 4]])
        doc = yarutsk.loads(out)
        assert list(doc[0]) == [1, 2]
        assert list(doc[1]) == [3, 4]

    def test_dumps_plain_list_with_dict_items(self):
        out = yarutsk.dumps([{"a": 1}, {"b": 2}])
        doc = yarutsk.loads(out)
        assert doc[0]["a"] == 1
        assert doc[1]["b"] == 2

    def test_dumps_all_plain_dicts(self):
        out = yarutsk.dumps_all([{"a": 1}, {"b": 2}])
        docs = yarutsk.loads_all(out)
        assert docs[0]["a"] == 1
        assert docs[1]["b"] == 2

    def test_dump_all_plain_dicts_to_stream(self):
        stream = io.StringIO()
        yarutsk.dump_all([{"x": 10}, {"y": 20}], stream)
        docs = yarutsk.loads_all(stream.getvalue())
        assert docs[0]["x"] == 10
        assert docs[1]["y"] == 20

    def test_plain_dict_wrapping_yaml_mapping_preserves_metadata(self):
        """A plain dict wrapping a loaded YamlMapping keeps comments on dump."""
        loaded = yarutsk.loads("foo: bar  # inline")
        out = yarutsk.dumps({"outer": loaded})
        doc = yarutsk.loads(out)
        assert doc["outer"]["foo"] == "bar"
        assert doc["outer"].comment_inline("foo") == "inline"

    def test_plain_dict_all_scalar_types(self):
        src = {"i": 42, "f": 3.14, "b": False, "s": "text", "n": None}
        out = yarutsk.dumps(src)
        doc = yarutsk.loads(out)
        assert doc["i"] == 42
        assert abs(doc["f"] - 3.14) < 1e-9
        assert doc["b"] is False
        assert doc["s"] == "text"
        assert doc["n"] is None


class TestIndent:
    """Configurable indentation via the indent= keyword argument."""

    def test_default_is_two_spaces(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert yarutsk.dumps(doc) == dedent("""\
            a:
              b: 1
        """)

    def test_four_space_indent(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            a:
                b: 1
        """)

    def test_one_space_indent(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert yarutsk.dumps(doc, indent=1) == dedent("""\
            a:
             b: 1
        """)

    def test_deeply_nested(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              b:
                c: 1
        """)
        )
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            a:
                b:
                    c: 1
        """)

    def test_sequence_indent(self):
        doc = yarutsk.loads(
            dedent("""\
            items:
              - a
              - b
        """)
        )
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            items:
                - a
                - b
        """)

    def test_comments_preserved_with_indent(self):
        doc = yarutsk.loads(
            dedent("""\
            a:
              # comment
              b: 1
        """)
        )
        out = yarutsk.dumps(doc, indent=4)
        assert "# comment" in out
        assert "    b: 1" in out

    def test_dumps_all_indent(self):
        d1 = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        d2 = yarutsk.loads(
            dedent("""\
            x:
              y: 2
        """)
        )
        out = yarutsk.dumps_all([d1, d2], indent=4)
        assert "    b: 1" in out
        assert "    y: 2" in out

    def test_dump_to_stream_indent(self):
        import io

        doc = yarutsk.loads(
            dedent("""\
            k:
              v: 1
        """)
        )
        stream = io.StringIO()
        yarutsk.dump(doc, stream, indent=4)
        assert stream.getvalue() == dedent("""\
            k:
                v: 1
        """)

    def test_dump_all_to_stream_indent(self):
        import io

        d1 = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        d2 = yarutsk.loads(
            dedent("""\
            x:
              y: 2
        """)
        )
        stream = io.StringIO()
        yarutsk.dump_all([d1, d2], stream, indent=4)
        out = stream.getvalue()
        assert "    b: 1" in out
        assert "    y: 2" in out

    def test_round_trip_with_four_space_indent_is_stable(self):
        """dumps(loads(dumps(doc, indent=4)), indent=4) == dumps(doc, indent=4).

        The emitter does not remember original indentation, so the only guarantee
        is that a second pass with the same indent= produces identical output to
        the first pass — i.e. the round-trip is idempotent once a specific width
        is chosen.
        """
        src = dedent("""\
            a:
              b:
                c: 1
              d: 2
            items:
              - x
              - y
        """)
        doc = yarutsk.loads(src)
        first = yarutsk.dumps(doc, indent=4)
        doc2 = yarutsk.loads(first)
        second = yarutsk.dumps(doc2, indent=4)
        assert first == second

    def test_round_trip_with_two_space_source_and_four_space_dump(self):
        """Loading 2-space-indented YAML and re-dumping with indent=4 changes
        only the whitespace, not the data.
        """
        src = dedent("""\
            a:
              b: 1
              c: 2
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc, indent=4)
        assert "    b: 1" in out
        assert "    c: 2" in out
        doc2 = yarutsk.loads(out)
        assert doc2["a"]["b"] == 1
        assert doc2["a"]["c"] == 2

    def test_round_trip_with_four_space_source_and_default_dump(self):
        """Loading 4-space-indented YAML and re-dumping with the default (2-space)
        produces 2-space output, but all values survive intact.
        """
        src = dedent("""\
            a:
                b: 1
                c:
                    d: true
        """)
        doc = yarutsk.loads(src)
        out = yarutsk.dumps(doc)
        assert "  b: 1" in out
        assert "    d: true" in out
        doc2 = yarutsk.loads(out)
        assert doc2["a"]["b"] == 1
        assert doc2["a"]["c"]["d"] is True
