"""Tests for YAML serialization: dump, dump_all, multi-document, empty docs."""

import io
from textwrap import dedent

import yarutsk


class TestSerialization:
    def test_dump_to_stringio(self) -> None:
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc is not None
        output = io.StringIO()
        yarutsk.dump(doc, output)
        result = output.getvalue()
        assert "name: John" in result
        assert "age: 30" in result

    def test_dump_to_bytesio(self) -> None:
        content = io.StringIO("name: John\nage: 30")
        doc = yarutsk.load(content)
        assert doc is not None
        output = io.BytesIO()
        yarutsk.dump(doc, output)
        result = output.getvalue().decode("utf-8")
        assert "name: John" in result
        assert "age: 30" in result

    def test_round_trip_preserves_data(self) -> None:
        content = io.StringIO("""
name: John
age: 30
items:
  - first
  - second
""")
        doc = yarutsk.load(content)
        assert doc is not None
        output = io.StringIO()
        yarutsk.dump(doc, output)
        doc2 = yarutsk.load(io.StringIO(output.getvalue()))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["name"] == "John"
        assert doc2["age"] == 30
        assert doc2["items"][0] == "first"
        assert doc2["items"][1] == "second"


class TestDumpDumpAll:
    """Test module-level dump and dump_all functions."""

    def test_dump_single_doc(self) -> None:
        doc = yarutsk.load(io.StringIO("name: John\nage: 30"))
        assert doc is not None
        out = io.StringIO()
        yarutsk.dump(doc, out)
        result = out.getvalue()
        assert "name: John" in result
        assert "age: 30" in result

    def test_dump_round_trip(self) -> None:
        doc = yarutsk.load(io.StringIO("a: 1\nb: 2"))
        assert doc is not None
        out = io.StringIO()
        yarutsk.dump(doc, out)
        doc2 = yarutsk.load(io.StringIO(out.getvalue()))
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["a"] == 1
        assert doc2["b"] == 2

    def test_dump_all_multiple_docs(self) -> None:
        docs = yarutsk.load_all(io.StringIO("---\na: 1\n---\nb: 2\n---\nc: 3"))
        assert len(docs) == 3
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert "---" in result
        docs2 = yarutsk.load_all(io.StringIO(result))
        assert len(docs2) == 3
        d0, d1, d2 = docs2
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert isinstance(d2, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2
        assert d2["c"] == 3

    def test_dump_all_single_doc(self) -> None:
        docs = yarutsk.load_all(io.StringIO("a: 1"))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert "---" not in result
        assert "a: 1" in result

    def test_dump_all_preserves_comments(self) -> None:
        docs = yarutsk.load_all(io.StringIO("# comment\n---\na: 1  # inline\n---\nb: 2"))
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        docs2 = yarutsk.load_all(io.StringIO(result))
        assert len(docs2) == 2
        d0, d1 = docs2
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d0.node("a").comment_inline == "inline"
        assert d1["b"] == 2


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

    def test_load_all_returns_list(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert isinstance(docs, list)
        assert len(docs) == 3

    def test_load_all_each_doc_independent(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert len(docs) == 3
        d0, d1, d2 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert isinstance(d2, yarutsk.YamlMapping)
        assert d0["name"] == "Alice"
        assert d1["name"] == "Bob"
        assert d2["name"] == "Carol"

    def test_load_all_single_doc_no_separator(self) -> None:
        docs = yarutsk.load_all(io.StringIO("a: 1\nb: 2"))
        assert len(docs) == 1
        d0 = docs[0]
        assert isinstance(d0, yarutsk.YamlMapping)
        assert d0["a"] == 1

    def test_load_all_empty_stream(self) -> None:
        docs = yarutsk.load_all(io.StringIO(""))
        assert docs == []

    def test_load_returns_first_doc_only(self) -> None:
        doc = yarutsk.load(io.StringIO(self.MULTI_DOC))
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["name"] == "Alice"

    def test_dump_all_separators(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue()
        assert result.count("---") == 3

    def test_dump_all_round_trip(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        docs2 = yarutsk.load_all(io.StringIO(out.getvalue()))
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs2
        )
        assert len(docs2) == 3
        for d1, d2 in zip(docs, docs2, strict=False):
            assert repr(d1) == repr(d2)

    def test_dump_all_single_doc_no_separator(self) -> None:
        docs = yarutsk.load_all(io.StringIO("x: 42"))
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        out = io.StringIO()
        yarutsk.dump_all(docs, out)
        assert "---" not in out.getvalue()

    def test_docs_are_independent_objects(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        d0, d1, _ = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        d0["name"] = "Modified"
        assert d1["name"] == "Bob"

    def test_mixed_types_across_docs(self) -> None:
        yaml = dedent("""\
            ---
            a: 1
            ---
            - x
            - y
        """)
        docs = yarutsk.load_all(io.StringIO(yaml))
        assert len(docs) == 2
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlSequence)
        assert d0["a"] == 1
        assert d1[0] == "x"

    def test_scalar_top_level(self) -> None:
        doc = yarutsk.loads("scalar")
        assert doc is not None
        assert type(doc).__name__ == "YamlScalar"
        assert doc.to_python() == "scalar"
        doc2 = yarutsk.loads("42")
        assert doc2 is not None
        assert doc2.to_python() == 42

    def test_comments_preserved_across_docs(self) -> None:
        yaml = dedent("""\
            ---
            key: val  # doc1 comment
            ---
            other: data  # doc2 comment
        """)
        docs = yarutsk.load_all(io.StringIO(yaml))
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0.node("key").comment_inline == "doc1 comment"
        assert d1.node("other").comment_inline == "doc2 comment"

    def test_dump_all_to_bytesio(self) -> None:
        docs = yarutsk.load_all(io.StringIO(self.MULTI_DOC))
        assert all(
            isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
            for d in docs
        )
        out = io.BytesIO()
        yarutsk.dump_all(docs, out)
        result = out.getvalue().decode("utf-8")
        assert "Alice" in result
        assert "Bob" in result


class TestEmptyDocuments:
    """Edge cases around empty / nearly-empty YAML."""

    def test_loads_empty_string(self) -> None:
        assert yarutsk.loads("") is None

    def test_loads_only_separator(self) -> None:
        """A bare --- produces a null YamlScalar, not a Python None."""
        result = yarutsk.loads("---")
        assert result is None or (
            type(result).__name__ == "YamlScalar" and result.to_python() is None
        )

    def test_loads_all_empty(self) -> None:
        assert yarutsk.loads_all("") == []

    def test_loads_empty_mapping(self) -> None:
        doc = yarutsk.loads("{}")
        assert isinstance(doc, yarutsk.YamlMapping)
        assert len(doc) == 0

    def test_loads_empty_sequence(self) -> None:
        doc = yarutsk.loads("[]")
        assert isinstance(doc, yarutsk.YamlSequence)
        assert len(doc) == 0

    def test_empty_mapping_round_trips(self) -> None:
        doc = yarutsk.loads("{}")
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert len(doc2) == 0

    def test_empty_sequence_round_trips(self) -> None:
        doc = yarutsk.loads("[]")
        assert doc is not None
        out = yarutsk.dumps(doc)
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlSequence)
        assert len(doc2) == 0


class TestPlainDictListDumping:
    """dumps/dump/dumps_all/dump_all accept plain Python dict and list."""

    def test_dumps_plain_dict(self) -> None:
        out = yarutsk.dumps({"a": 1, "b": 2})
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] == 1
        assert doc["b"] == 2

    def test_dumps_plain_list(self) -> None:
        out = yarutsk.dumps([1, "hello", None, True])
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[0] == 1
        assert doc[1] == "hello"
        assert doc[2] is None
        assert doc[3] is True

    def test_dump_plain_dict_to_stream(self) -> None:
        out = io.StringIO()
        yarutsk.dump({"key": "val"}, out)
        doc = yarutsk.loads(out.getvalue())
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["key"] == "val"

    def test_dumps_nested_plain_dict(self) -> None:
        out = yarutsk.dumps({"a": 1, "b": [1, 2, 3], "c": {"x": True}})
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["a"] == 1
        assert list(doc["b"]) == [1, 2, 3]
        assert doc["c"]["x"] is True

    def test_dumps_nested_plain_list(self) -> None:
        out = yarutsk.dumps([[1, 2], [3, 4]])
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert list(doc[0]) == [1, 2]
        assert list(doc[1]) == [3, 4]

    def test_dumps_plain_list_with_dict_items(self) -> None:
        out = yarutsk.dumps([{"a": 1}, {"b": 2}])
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlSequence)
        assert doc[0]["a"] == 1
        assert doc[1]["b"] == 2

    def test_dumps_all_plain_dicts(self) -> None:
        out = yarutsk.dumps_all([{"a": 1}, {"b": 2}])
        docs = yarutsk.loads_all(out)
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["a"] == 1
        assert d1["b"] == 2

    def test_dump_all_plain_dicts_to_stream(self) -> None:
        stream = io.StringIO()
        yarutsk.dump_all([{"x": 10}, {"y": 20}], stream)
        docs = yarutsk.loads_all(stream.getvalue())
        d0, d1 = docs
        assert isinstance(d0, yarutsk.YamlMapping)
        assert isinstance(d1, yarutsk.YamlMapping)
        assert d0["x"] == 10
        assert d1["y"] == 20

    def test_plain_dict_wrapping_yaml_mapping_preserves_metadata(self) -> None:
        """A plain dict wrapping a loaded YamlMapping keeps comments on dump."""
        loaded = yarutsk.loads("foo: bar  # inline")
        assert loaded is not None
        out = yarutsk.dumps({"outer": loaded})
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["outer"]["foo"] == "bar"
        assert doc["outer"].node("foo").comment_inline == "inline"

    def test_plain_dict_all_scalar_types(self) -> None:
        src = {"i": 42, "f": 3.14, "b": False, "s": "text", "n": None}
        out = yarutsk.dumps(src)
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["i"] == 42
        assert abs(doc["f"] - 3.14) < 1e-9
        assert doc["b"] is False
        assert doc["s"] == "text"
        assert doc["n"] is None


class TestIndent:
    """Configurable indentation via the indent= keyword argument."""

    def test_default_is_two_spaces(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert doc is not None
        assert yarutsk.dumps(doc) == dedent("""\
            a:
              b: 1
        """)

    def test_four_space_indent(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert doc is not None
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            a:
                b: 1
        """)

    def test_one_space_indent(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert doc is not None
        assert yarutsk.dumps(doc, indent=1) == dedent("""\
            a:
             b: 1
        """)

    def test_deeply_nested(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              b:
                c: 1
        """)
        )
        assert doc is not None
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            a:
                b:
                    c: 1
        """)

    def test_sequence_indent(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            items:
              - a
              - b
        """)
        )
        assert doc is not None
        assert yarutsk.dumps(doc, indent=4) == dedent("""\
            items:
                - a
                - b
        """)

    def test_comments_preserved_with_indent(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a:
              # comment
              b: 1
        """)
        )
        assert doc is not None
        out = yarutsk.dumps(doc, indent=4)
        assert "# comment" in out
        assert "    b: 1" in out

    def test_dumps_all_indent(self) -> None:
        d1 = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert d1 is not None
        d2 = yarutsk.loads(
            dedent("""\
            x:
              y: 2
        """)
        )
        assert d2 is not None
        out = yarutsk.dumps_all([d1, d2], indent=4)
        assert "    b: 1" in out
        assert "    y: 2" in out

    def test_dump_to_stream_indent(self) -> None:
        import io

        doc = yarutsk.loads(
            dedent("""\
            k:
              v: 1
        """)
        )
        assert doc is not None
        stream = io.StringIO()
        yarutsk.dump(doc, stream, indent=4)
        assert stream.getvalue() == dedent("""\
            k:
                v: 1
        """)

    def test_dump_all_to_stream_indent(self) -> None:
        import io

        d1 = yarutsk.loads(
            dedent("""\
            a:
              b: 1
        """)
        )
        assert d1 is not None
        d2 = yarutsk.loads(
            dedent("""\
            x:
              y: 2
        """)
        )
        assert d2 is not None
        stream = io.StringIO()
        yarutsk.dump_all([d1, d2], stream, indent=4)
        out = stream.getvalue()
        assert "    b: 1" in out
        assert "    y: 2" in out

    def test_round_trip_with_four_space_indent_is_stable(self) -> None:
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
        assert doc is not None
        first = yarutsk.dumps(doc, indent=4)
        doc2 = yarutsk.loads(first)
        assert doc2 is not None
        second = yarutsk.dumps(doc2, indent=4)
        assert first == second

    def test_round_trip_with_two_space_source_and_four_space_dump(self) -> None:
        """Loading 2-space-indented YAML and re-dumping with indent=4 changes
        only the whitespace, not the data.
        """
        src = dedent("""\
            a:
              b: 1
              c: 2
        """)
        doc = yarutsk.loads(src)
        assert doc is not None
        out = yarutsk.dumps(doc, indent=4)
        assert "    b: 1" in out
        assert "    c: 2" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["a"]["b"] == 1
        assert doc2["a"]["c"] == 2

    def test_round_trip_with_four_space_source_and_default_dump(self) -> None:
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
        assert doc is not None
        out = yarutsk.dumps(doc)
        assert "  b: 1" in out
        assert "    d: true" in out
        doc2 = yarutsk.loads(out)
        assert isinstance(doc2, yarutsk.YamlMapping)
        assert doc2["a"]["b"] == 1
        assert doc2["a"]["c"]["d"] is True


class TestAutoAnchor:
    """Auto-anchor emission for plain Python dicts/lists with shared identity or cycles.

    When a plain Python dict, list, or tuple appears more than once in the
    object graph (by identity), dumps() automatically assigns it an anchor
    and emits aliases for subsequent references — matching PyYAML and
    ruamel.yaml behaviour.
    """

    def test_shared_dict_gets_anchor_and_alias(self) -> None:
        shared = {"x": 1}
        out = yarutsk.dumps({"a": shared, "b": shared})
        assert "&id001" in out
        assert "*id001" in out

    def test_shared_dict_first_occurrence_is_anchor(self) -> None:
        shared = {"x": 1}
        out = yarutsk.dumps({"a": shared, "b": shared})
        # anchor must appear before alias in the output
        assert out.index("&id001") < out.index("*id001")

    def test_shared_list_gets_anchor_and_alias(self) -> None:
        shared = [1, 2, 3]
        out = yarutsk.dumps({"a": shared, "b": shared})
        assert "&id001" in out
        assert "*id001" in out

    def test_shared_tuple_gets_anchor_and_alias(self) -> None:
        shared = (10, 20)
        out = yarutsk.dumps({"a": shared, "b": shared})
        assert "&id001" in out
        assert "*id001" in out

    def test_multiple_shared_objects_numbered_sequentially(self) -> None:
        o1 = {"x": 1}
        o2 = {"y": 2}
        out = yarutsk.dumps({"a": o1, "b": o1, "c": o2, "d": o2})
        assert "&id001" in out
        assert "*id001" in out
        assert "&id002" in out
        assert "*id002" in out

    def test_unshared_object_has_no_anchor(self) -> None:
        out = yarutsk.dumps({"a": {"x": 1}, "b": {"x": 2}})
        assert "&id" not in out
        assert "*id" not in out

    def test_recursive_dict_emits_anchor_and_alias(self) -> None:
        d: dict[str, object] = {}
        d["self"] = d
        out = yarutsk.dumps(d)
        assert "&id001" in out
        assert "*id001" in out

    def test_recursive_dict_output_is_valid_yaml(self) -> None:
        d: dict[str, object] = {}
        d["self"] = d
        out = yarutsk.dumps(d)
        doc = yarutsk.loads(out)
        assert doc is not None

    def test_recursive_list_emits_anchor_and_alias(self) -> None:
        lst: list[object] = [1, 2]
        lst.append(lst)
        out = yarutsk.dumps(lst)
        assert "&id001" in out
        assert "*id001" in out

    def test_recursive_list_output_is_valid_yaml(self) -> None:
        lst: list[object] = [1, 2]
        lst.append(lst)
        out = yarutsk.dumps(lst)
        doc = yarutsk.loads(out)
        assert doc is not None

    def test_cross_cycle_emits_anchor_and_alias(self) -> None:
        a: dict[str, object] = {}
        b: dict[str, object] = {}
        a["next"] = b
        b["prev"] = a
        out = yarutsk.dumps(a)
        assert "&id001" in out
        assert "*id001" in out

    def test_dump_stream_shared_object(self) -> None:
        shared = {"x": 1}
        buf = io.StringIO()
        yarutsk.dump({"a": shared, "b": shared}, buf)
        out = buf.getvalue()
        assert "&id001" in out
        assert "*id001" in out

    def test_cross_doc_shared_object_no_cross_doc_anchor(self) -> None:
        shared = {"x": 1}
        out = yarutsk.dumps_all([{"a": shared}, {"b": shared}])
        # Each document serializes the object independently
        assert out.count("x: 1") == 2
        # No alias should appear (anchor scope is per-document)
        assert "*id" not in out

    def test_recursive_dict_detected_at_dump(self) -> None:
        d: dict[str, object] = {}
        d["self"] = d
        m = yarutsk.YamlMapping(d)
        out = yarutsk.dumps(m)
        assert "&id001" in out
        assert "*id001" in out

    def test_recursive_list_detected_at_dump(self) -> None:
        lst: list[object] = [1]
        lst.append(lst)
        s = yarutsk.YamlSequence(lst)
        out = yarutsk.dumps(s)
        assert "&id001" in out
        assert "*id001" in out

    def test_shared_tagged_mapping_serialized_twice_no_auto_anchor(self) -> None:
        # Same PyYamlMapping Python object referenced from two plain-dict keys:
        # no deduplication, both keys serialize the full node with its tag.
        tagged = yarutsk.loads("!!mytype\nx: 1\n")
        assert tagged is not None
        out = yarutsk.dumps({"a": tagged, "b": tagged})
        assert "&id" not in out
        assert "*id" not in out
        assert out.count("!!mytype") == 2
        assert out.count("x: 1") == 2

    def test_shared_tagged_sequence_serialized_twice_no_auto_anchor(self) -> None:
        tagged = yarutsk.loads("!!myseq\n- 1\n- 2\n")
        assert tagged is not None
        out = yarutsk.dumps({"a": tagged, "b": tagged})
        assert "&id" not in out
        assert "*id" not in out
        assert out.count("!!myseq") == 2

    def test_yaml_mapping_existing_anchor_emitted_per_occurrence(self) -> None:
        # A loaded mapping with its own anchor (&myanchor) is serialized as-is
        # each time it appears; no alias is injected.  Both occurrences carry
        # the anchor name — duplicate anchors are technically valid YAML
        # (the last definition wins) and our loader handles them correctly.
        anchored = yarutsk.loads("&myanchor\nx: 1\n")
        assert anchored is not None
        out = yarutsk.dumps({"a": anchored, "b": anchored})
        assert out.count("&myanchor") == 2
        assert "*myanchor" not in out
        # Round-trip must succeed
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["b"]["x"] == 1

    def test_yaml_mapping_self_assignment_uses_snapshot(self) -> None:
        # Assigning a PyYamlMapping to one of its own keys stores a *snapshot*
        # of the mapping at the moment of assignment (before the new key is
        # visible), so no true cycle is created and the output is finite.
        m = yarutsk.loads("key: value\n")
        assert isinstance(m, yarutsk.YamlMapping)
        m["self"] = m  # snapshot: captures {'key': 'value'}, not the full m
        out = yarutsk.dumps(m)
        # No anchor/alias: the self-reference was broken at assignment time
        assert "&id" not in out
        assert "*id" not in out
        # The inner snapshot has only the original key
        doc = yarutsk.loads(out)
        assert isinstance(doc, yarutsk.YamlMapping)
        assert doc["self"]["key"] == "value"
        assert "self" not in doc["self"]

    def test_plain_dict_with_tagged_value_and_self_reference(self) -> None:
        # A plain dict that is self-referential AND contains a tagged
        # PyYamlMapping as a value: the plain dict gets auto-anchored,
        # the tagged inner value is serialized normally.
        tagged = yarutsk.loads("!!mytype\nz: 9\n")
        assert tagged is not None
        d: dict[str, object] = {}
        d["inner"] = tagged
        d["self"] = d
        out = yarutsk.dumps(d)
        assert "&id001" in out  # plain dict anchored
        assert "*id001" in out  # self-ref becomes alias
        assert "!!mytype" in out  # tag preserved on inner value
        assert "z: 9" in out


class TestDumpIterableAndMappingTypes:
    """Verify that dump/dumps accept abstract iterables, mappings, bytes, etc."""

    def test_set(self) -> None:
        result = yarutsk.dumps({42})
        assert "42" in result

    def test_frozenset(self) -> None:
        result = yarutsk.dumps(frozenset([10]))
        assert "10" in result

    def test_deque(self) -> None:
        from collections import deque

        result = yarutsk.dumps(deque([1, 2, 3]))
        assert "- 1" in result
        assert "- 3" in result

    def test_generator(self) -> None:
        result = yarutsk.dumps(x * 2 for x in range(3))
        assert "- 0" in result
        assert "- 4" in result

    def test_range(self) -> None:
        result = yarutsk.dumps(range(3))
        assert "- 0" in result
        assert "- 2" in result

    def test_bytes(self) -> None:
        result = yarutsk.dumps(b"hello")
        assert "!!binary" in result

    def test_bytearray(self) -> None:
        result = yarutsk.dumps(bytearray(b"test"))
        assert "!!binary" in result

    def test_chainmap(self) -> None:
        from collections import ChainMap

        result = yarutsk.dumps(ChainMap({"a": 1}, {"b": 2}))
        assert "a: 1" in result
        assert "b: 2" in result

    def test_nested_set_in_mapping(self) -> None:
        doc = yarutsk.loads("key: placeholder\n")
        assert isinstance(doc, yarutsk.YamlMapping)
        doc["key"] = {99}
        result = yarutsk.dumps(doc)
        assert "99" in result

    def test_nested_deque_in_plain_dict(self) -> None:
        from collections import deque

        result = yarutsk.dumps({"items": deque([1, 2])})
        assert "- 1" in result
        assert "- 2" in result

    def test_bytes_in_plain_dict(self) -> None:
        result = yarutsk.dumps({"data": b"abc"})
        assert "!!binary" in result

    def test_dump_set_to_stream(self) -> None:
        buf = io.StringIO()
        yarutsk.dump({42}, buf)
        assert "42" in buf.getvalue()
