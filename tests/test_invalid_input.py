"""Tests that pass invalid or malformed data through the yarutsk API.

Covers:
  1. Malformed YAML syntax (parse-time errors)
  2. Invalid Python types passed to dumps/dump
  3. Schema dumper error cases
  4. Schema loader error cases
  5. Stream edge cases (load / dump)
  6. dumps_all / dump_all with invalid docs
  7. Comment / style operations with bad arguments
  8. Plain Python collections assigned as values
"""

import io
from textwrap import dedent

import pytest

import yarutsk


class TestMalformedYaml:
    def test_unclosed_single_quote(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads("x: 'unclosed")

    def test_unclosed_double_quote(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads('x: "unclosed')

    def test_tab_indentation(self):
        # YAML forbids tabs as indentation characters
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads(
                dedent("""\
                key:
                \tvalue: 1
            """)
            )

    def test_unclosed_flow_mapping(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads("{foo: bar")

    def test_unclosed_flow_sequence(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads("[1, 2, 3")

    def test_invalid_block_mapping_indentation(self):
        # Second key less-indented than first value would be a parse error
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads(
                dedent("""\
                a:
                  b: 1
                 c: 2
            """)
            )

    def test_loads_all_malformed(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads_all(
                dedent("""\
                ---
                good: 1
                ---
                bad: 'unclosed
            """)
            )

    def test_load_stream_malformed(self):
        with pytest.raises(yarutsk.ParseError):
            yarutsk.load(io.StringIO("x: 'unclosed"))


class TestInvalidDumpTypes:
    def test_object_raises(self):
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps(object())

    def test_lambda_raises(self):
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps(lambda: None)

    def test_nested_object_in_sequence_raises(self):
        doc = yarutsk.loads(
            dedent("""\
            - 1
            - 2
        """)
        )
        doc.append(object())
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps(doc)

    def test_tuple_accepted_as_sequence(self):
        """tuple should serialize as a YAML sequence (was previously rejected)."""
        result = yarutsk.dumps((1, 2, 3))
        assert "1" in result
        assert "2" in result
        assert "3" in result

    def test_nested_tuple_accepted(self):
        doc = yarutsk.loads("items: []\n")
        doc["items"] = (10, 20, 30)
        result = yarutsk.dumps(doc)
        assert "10" in result
        assert "20" in result
        assert "30" in result

    def test_empty_tuple_accepted(self):
        result = yarutsk.dumps(())
        assert isinstance(result, str)

    def test_tuple_in_plain_dict(self):
        result = yarutsk.dumps({"coords": (1.0, 2.0)})
        assert "1.0" in result
        assert "2.0" in result

    def test_dump_invalid_type_to_stream_raises(self):
        buf = io.StringIO()
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dump(object(), buf)

    def test_dumps_all_with_invalid_item(self):
        doc = yarutsk.loads("a: 1\n")
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps_all([doc, object()])


class _Opaque:
    """A custom type with no registered dumper."""


class TestSchemaDumperErrors:
    def test_dumper_returns_string_not_tuple(self):
        schema = yarutsk.Schema()
        schema.add_dumper(_Opaque, lambda x: "not-a-tuple")
        doc = yarutsk.loads("x: 1\n")
        doc["x"] = _Opaque()
        with pytest.raises(yarutsk.DumperError, match="tuple"):
            yarutsk.dumps(doc, schema=schema)

    def test_dumper_returns_1_tuple(self):
        schema = yarutsk.Schema()
        schema.add_dumper(_Opaque, lambda x: ("!tag",))
        doc = yarutsk.loads("x: 1\n")
        doc["x"] = _Opaque()
        with pytest.raises(yarutsk.DumperError, match="tuple"):
            yarutsk.dumps(doc, schema=schema)

    def test_dumper_returns_3_tuple(self):
        schema = yarutsk.Schema()
        schema.add_dumper(_Opaque, lambda x: ("!tag", "data", "extra"))
        doc = yarutsk.loads("x: 1\n")
        doc["x"] = _Opaque()
        with pytest.raises(yarutsk.DumperError, match="tuple"):
            yarutsk.dumps(doc, schema=schema)

    def test_dumper_returns_non_serializable_data(self):
        schema = yarutsk.Schema()
        # Returns a valid tag but the data (an object) cannot be serialized.
        schema.add_dumper(_Opaque, lambda x: ("!opaque", object()))
        doc = yarutsk.loads("x: 1\n")
        doc["x"] = _Opaque()
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps(doc, schema=schema)

    def test_dumper_raises_exception_propagates(self):
        schema = yarutsk.Schema()

        def bad_dumper(x):
            raise ValueError("dumper exploded")

        schema.add_dumper(_Opaque, bad_dumper)
        doc = yarutsk.loads("x: 1\n")
        doc["x"] = _Opaque()
        with pytest.raises(yarutsk.DumperError, match="dumper exploded"):
            yarutsk.dumps(doc, schema=schema)

    def test_no_dumper_registered_raises(self):
        """Dumping an unknown type without a schema raises RuntimeError."""
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps(_Opaque())


class TestSchemaLoaderErrors:
    def test_loader_exception_propagates(self):
        schema = yarutsk.Schema()

        def bad_loader(v):
            raise ValueError("loader exploded")

        schema.add_loader("!boom", bad_loader)
        with pytest.raises(yarutsk.LoaderError, match="loader exploded"):
            yarutsk.loads("x: !boom whatever\n", schema=schema)

    def test_loader_returning_unserializable_object_is_stored(self):
        """A loader may return any Python object; it is stored in the Python dict
        layer but the Rust inner model retains the original YAML node.

        As a result:
        - The Python-visible value is what the loader returned.
        - dumps() re-serializes the Rust inner node, not the Python value, so
          it succeeds without a matching dumper registered.
        """
        schema = yarutsk.Schema()
        schema.add_loader("!opaque", lambda v: {1, 2, 3})  # returns a set
        doc = yarutsk.loads("x: !opaque value\n", schema=schema)
        # The set is accessible via the Python dict layer.
        assert doc["x"] == {1, 2, 3}
        # dumps uses the Rust inner node, so it re-emits the original YAML.
        result = yarutsk.dumps(doc)
        assert "!opaque" in result


class _NoReadStream:
    """A fake stream with no read() method."""


class _BadReadStream:
    """A stream whose read() returns an int."""

    def read(self, n: int = -1) -> int:
        return 42


class _NoneReadStream:
    """A stream whose read() returns None."""

    def read(self, n: int = -1) -> None:
        return None


class _NoWriteStream:
    """A fake stream with no write() method."""


class TestStreamEdgeCases:
    def test_load_no_read_method(self):
        with pytest.raises(AttributeError):
            yarutsk.load(_NoReadStream())

    def test_load_read_returns_int(self):
        with pytest.raises(RuntimeError, match="str or bytes"):
            yarutsk.load(_BadReadStream())

    def test_load_read_returns_none(self):
        with pytest.raises(RuntimeError, match="str or bytes"):
            yarutsk.load(_NoneReadStream())

    def test_load_non_utf8_bytes(self):
        stream = io.BytesIO(b"\xff\xfe invalid utf-8")
        with pytest.raises(RuntimeError, match="[Uu][Tt][Ff]"):
            yarutsk.load(stream)

    def test_dump_no_write_method(self):
        doc = yarutsk.loads("a: 1\n")
        with pytest.raises((AttributeError, RuntimeError)):
            yarutsk.dump(doc, _NoWriteStream())

    def test_load_all_no_read_method(self):
        with pytest.raises(AttributeError):
            yarutsk.load_all(_NoReadStream())


class TestDumpsAllInvalidDocs:
    def test_docs_none(self):
        with pytest.raises((TypeError, RuntimeError)):
            yarutsk.dumps_all(None)  # type: ignore[arg-type]

    def test_docs_integer(self):
        with pytest.raises((TypeError, RuntimeError)):
            yarutsk.dumps_all(42)  # type: ignore[arg-type]

    def test_docs_with_invalid_item(self):
        doc = yarutsk.loads("a: 1\n")
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dumps_all([doc, object()])  # type: ignore[list-item]

    def test_dump_all_docs_none(self):
        buf = io.StringIO()
        with pytest.raises((TypeError, RuntimeError)):
            yarutsk.dump_all(None, buf)  # type: ignore[arg-type]

    def test_dump_all_with_invalid_item(self):
        doc = yarutsk.loads("a: 1\n")
        buf = io.StringIO()
        with pytest.raises((RuntimeError, TypeError)):
            yarutsk.dump_all([doc, object()], buf)  # type: ignore[list-item]


class TestBadCommentAndStyleArgs:
    def test_comment_inline_out_of_range_index_raises(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        with pytest.raises(IndexError):
            doc.comment_inline(99, "note")

    def test_comment_before_out_of_range_index_raises(self):
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        with pytest.raises(IndexError):
            doc.comment_before(99, "note")

    def test_scalar_style_invalid_name_raises(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(ValueError):
            doc.scalar_style("key", "invalid_style")

    def test_scalar_style_missing_key_raises(self):
        doc = yarutsk.loads("key: value\n")
        with pytest.raises(KeyError):
            doc.scalar_style("missing", "plain")

    def test_comment_inline_read_out_of_range_raises(self):
        # Reading a comment at an out-of-bounds index raises IndexError,
        # consistent with __getitem__ semantics.
        doc = yarutsk.loads(
            dedent("""\
            - a
            - b
        """)
        )
        with pytest.raises(IndexError):
            doc.comment_inline(99)


class TestPlainCollectionAssignment:
    """Assigning plain Python dicts and lists as mapping/sequence values.

    A plain Python list assigned to a key must become a YAML sequence, not
    !!binary — even when the list contains small integers that PyO3 could
    otherwise extract as Vec<u8>.
    """

    def test_list_of_integers_becomes_sequence_not_binary(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = [1, 2, 3]
        out = yarutsk.dumps(doc)
        assert "!!binary" not in out
        assert "- 1" in out

    def test_list_of_strings_becomes_sequence(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = ["a", "b"]
        out = yarutsk.dumps(doc)
        assert "- a\n" in out
        assert "- b\n" in out

    def test_bytes_still_becomes_binary(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = b"hello"
        out = yarutsk.dumps(doc)
        assert "!!binary" in out

    def test_new_list_default_style_is_block(self):
        doc = yarutsk.loads("k: placeholder\n")
        doc["k"] = ["a", "b"]
        assert doc.node("k").style == "block"

    def test_list_in_sequence_item_becomes_sequence(self):
        doc = yarutsk.loads("- placeholder\n")
        doc[0] = [10, 20]
        out = yarutsk.dumps(doc)
        assert "!!binary" not in out
        assert "- 10\n" in out
