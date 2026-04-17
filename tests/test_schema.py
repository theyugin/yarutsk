"""Tests for the Schema custom loader/dumper registry."""

import datetime
from textwrap import dedent

import pytest
import yarutsk


class Point:
    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Point) and self.x == other.x and self.y == other.y


class Color:
    def __init__(self, r: int, g: int, b: int) -> None:
        self.r, self.g, self.b = r, g, b

    def __eq__(self, other: object) -> bool:
        return (
            isinstance(other, Color)
            and self.r == other.r
            and self.g == other.g
            and self.b == other.b
        )


class TestCustomMappingType:
    def setup_method(self) -> None:
        self.schema = yarutsk.Schema()
        self.schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
        self.schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

    def test_load_mapping_tag(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                origin: !point
                  x: 0
                  y: 0
            """),
            schema=self.schema,
        )
        assert isinstance(doc["origin"], Point)
        assert doc["origin"] == Point(0, 0)

    def test_dump_custom_object(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                origin: !point
                  x: 1
                  y: 2
            """),
            schema=self.schema,
        )
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 1" in out
        assert "y: 2" in out

    def test_set_and_dump(self) -> None:
        doc = yarutsk.loads("name: test\n", schema=self.schema)
        doc["pos"] = Point(3, 4)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 3" in out
        assert "y: 4" in out

    def test_roundtrip(self) -> None:
        src = dedent("""\
            origin: !point
              x: 0
              y: 0
        """)
        doc = yarutsk.loads(src, schema=self.schema)
        out = yarutsk.dumps(doc, schema=self.schema)
        doc2 = yarutsk.loads(out, schema=self.schema)
        assert isinstance(doc2["origin"], Point)
        assert doc2["origin"] == Point(0, 0)

    def test_dump_items_in_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                points:
                  - !point
                    x: 1
                    y: 2
            """),
            schema=self.schema,
        )
        doc["points"].append(Point(3, 4))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2

    def test_first_registered_dumper_wins(self) -> None:
        schema = yarutsk.Schema()
        schema.add_dumper(Point, lambda p: ("!point-a", f"{p.x},{p.y}"))
        schema.add_dumper(Point, lambda p: ("!point-b", f"{p.x},{p.y}"))

        doc = yarutsk.loads("x: placeholder\n")
        doc["x"] = Point(1, 2)
        out = yarutsk.dumps(doc, schema=schema)
        assert "!point-a" in out
        assert "!point-b" not in out


class TestCustomScalarType:
    def setup_method(self) -> None:
        self.schema = yarutsk.Schema()
        self.schema.add_loader("!color", lambda s: Color(*[int(x) for x in s.split(",")]))
        self.schema.add_dumper(Color, lambda c: ("!color", f"{c.r},{c.g},{c.b}"))

    def test_load_scalar_tag(self) -> None:
        doc = yarutsk.loads("bg: !color 255,0,128\n", schema=self.schema)
        assert isinstance(doc["bg"], Color)
        assert doc["bg"] == Color(255, 0, 128)

    def test_dump_scalar_custom(self) -> None:
        doc = yarutsk.loads("x: placeholder\n")
        doc["x"] = Color(10, 20, 30)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!color" in out
        assert "10,20,30" in out

    def test_roundtrip(self) -> None:
        src = "bg: !color 255,0,128\n"
        doc = yarutsk.loads(src, schema=self.schema)
        out = yarutsk.dumps(doc, schema=self.schema)
        doc2 = yarutsk.loads(out, schema=self.schema)
        assert doc2["bg"] == Color(255, 0, 128)

    def test_setitem_mapping(self) -> None:
        doc = yarutsk.loads("x: placeholder\n")
        doc["x"] = Color(255, 128, 0)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!color" in out
        assert "255,128,0" in out

    def test_append_sequence(self) -> None:
        doc = yarutsk.loads("colors: []\n")
        doc["colors"].append(Color(0, 128, 255))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!color" in out
        assert "0,128,255" in out


class TestOverrideBuiltinTags:
    def test_override_int_receives_raw_string(self) -> None:
        received: list[str] = []
        schema = yarutsk.Schema()
        schema.add_loader("!!int", lambda raw: received.append(raw) or int(raw, 0))

        doc = yarutsk.loads("x: !!int 0xFF\n", schema=schema)
        assert doc["x"] == 255
        assert received == ["0xFF"]

    def test_override_float_receives_raw_string(self) -> None:
        received: list[str] = []
        schema = yarutsk.Schema()
        schema.add_loader("!!float", lambda raw: received.append(raw) or float(raw))

        doc = yarutsk.loads("x: !!float 1.5\n", schema=schema)
        assert doc["x"] == 1.5
        assert received == ["1.5"]

    def test_override_bool_receives_raw_string(self) -> None:
        received: list[str] = []
        schema = yarutsk.Schema()
        schema.add_loader("!!bool", lambda raw: received.append(raw) or (raw.lower() == "true"))

        doc = yarutsk.loads("x: !!bool true\n", schema=schema)
        assert doc["x"] is True
        assert received == ["true"]

    def test_override_null_receives_raw_string(self) -> None:
        received: list[str] = []
        schema = yarutsk.Schema()
        schema.add_loader("!!null", lambda raw: received.append(raw))

        doc = yarutsk.loads("x: !!null ~\n", schema=schema)
        assert doc["x"] is None
        assert received == ["~"]

    def test_override_str_receives_raw_string(self) -> None:
        received: list[str] = []
        schema = yarutsk.Schema()
        schema.add_loader("!!str", lambda raw: received.append(raw) or raw.upper())

        doc = yarutsk.loads("x: !!str hello\n", schema=schema)
        assert doc["x"] == "HELLO"
        assert received == ["hello"]


class TestDefaultTypeConversions:
    def test_implicit_int(self) -> None:
        doc = yarutsk.loads("x: 42\n")
        assert doc["x"] == 42
        assert isinstance(doc["x"], int)

    def test_implicit_negative_int(self) -> None:
        doc = yarutsk.loads("x: -7\n")
        assert doc["x"] == -7

    def test_implicit_float(self) -> None:
        doc = yarutsk.loads("x: 3.14\n")
        assert doc["x"] == 3.14
        assert isinstance(doc["x"], float)

    def test_implicit_float_exponent(self) -> None:
        doc = yarutsk.loads("x: 1.5e2\n")
        assert doc["x"] == 150.0

    def test_implicit_float_inf(self) -> None:
        doc = yarutsk.loads("x: .inf\n")
        import math

        assert math.isinf(doc["x"]) and doc["x"] > 0

    def test_implicit_float_neg_inf(self) -> None:
        doc = yarutsk.loads("x: -.inf\n")
        import math

        assert math.isinf(doc["x"]) and doc["x"] < 0

    def test_implicit_float_nan(self) -> None:
        doc = yarutsk.loads("x: .nan\n")
        import math

        assert math.isnan(doc["x"])

    def test_implicit_bool_true(self) -> None:
        doc = yarutsk.loads("x: true\n")
        assert doc["x"] is True

    def test_implicit_bool_false(self) -> None:
        doc = yarutsk.loads("x: false\n")
        assert doc["x"] is False

    def test_implicit_bool_yaml11_yes(self) -> None:
        doc = yarutsk.loads("x: yes\n")
        assert doc["x"] is True

    def test_implicit_bool_yaml11_no(self) -> None:
        doc = yarutsk.loads("x: no\n")
        assert doc["x"] is False

    def test_implicit_bool_yaml11_on(self) -> None:
        doc = yarutsk.loads("x: on\n")
        assert doc["x"] is True

    def test_implicit_bool_yaml11_off(self) -> None:
        doc = yarutsk.loads("x: off\n")
        assert doc["x"] is False

    def test_implicit_null_tilde(self) -> None:
        doc = yarutsk.loads("x: ~\n")
        assert doc["x"] is None

    def test_implicit_null_keyword(self) -> None:
        doc = yarutsk.loads("x: null\n")
        assert doc["x"] is None

    def test_implicit_null_empty(self) -> None:
        doc = yarutsk.loads("x:\n")
        assert doc["x"] is None

    def test_implicit_string(self) -> None:
        doc = yarutsk.loads("x: hello\n")
        assert doc["x"] == "hello"
        assert isinstance(doc["x"], str)

    def test_tag_int_decimal(self) -> None:
        doc = yarutsk.loads("x: !!int 42\n")
        assert doc["x"] == 42
        assert isinstance(doc["x"], int)

    def test_tag_int_hex(self) -> None:
        doc = yarutsk.loads("x: !!int 0xFF\n")
        assert doc["x"] == 255

    def test_tag_int_octal(self) -> None:
        doc = yarutsk.loads("x: !!int 0o17\n")
        assert doc["x"] == 15

    def test_tag_float_on_integer(self) -> None:
        doc = yarutsk.loads("x: !!float 1\n")
        assert doc["x"] == 1.0
        assert isinstance(doc["x"], float)

    def test_tag_float(self) -> None:
        doc = yarutsk.loads("x: !!float 1.5\n")
        assert doc["x"] == 1.5

    def test_tag_bool_true(self) -> None:
        doc = yarutsk.loads("x: !!bool true\n")
        assert doc["x"] is True

    def test_tag_bool_false(self) -> None:
        doc = yarutsk.loads("x: !!bool false\n")
        assert doc["x"] is False

    def test_tag_null(self) -> None:
        doc = yarutsk.loads("x: !!null ~\n")
        assert doc["x"] is None

    def test_tag_str_on_int(self) -> None:
        doc = yarutsk.loads("x: !!str 42\n")
        assert doc["x"] == "42"
        assert isinstance(doc["x"], str)

    def test_tag_str_on_bool(self) -> None:
        doc = yarutsk.loads("x: !!str true\n")
        assert doc["x"] == "true"
        assert isinstance(doc["x"], str)

    def test_tag_binary(self) -> None:
        doc = yarutsk.loads("x: !!binary aGVsbG8=\n")
        assert doc["x"] == b"hello"

    def test_tag_timestamp_datetime(self) -> None:
        doc = yarutsk.loads("x: !!timestamp 2024-01-15T12:30:00\n")
        assert isinstance(doc["x"], datetime.datetime)
        assert doc["x"].year == 2024
        assert doc["x"].month == 1
        assert doc["x"].day == 15

    def test_tag_timestamp_date_only(self) -> None:
        doc = yarutsk.loads("x: !!timestamp 2024-06-01\n")
        assert isinstance(doc["x"], datetime.date)
        assert doc["x"].year == 2024
        assert doc["x"].month == 6

    def test_tag_timestamp_space_separator(self) -> None:
        doc = yarutsk.loads("x: !!timestamp 2024-03-10 08:00:00\n")
        assert isinstance(doc["x"], datetime.datetime)
        assert doc["x"].hour == 8


class TestMutableMappingCustomTypes:
    def setup_method(self) -> None:
        self.schema = yarutsk.Schema()
        self.schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
        self.schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

    def test_setitem_new_key(self) -> None:
        doc = yarutsk.loads("name: test\n")
        doc["origin"] = Point(0, 0)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 0" in out
        assert "y: 0" in out

    def test_setitem_overwrite_existing_key(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                origin: !point
                  x: 1
                  y: 2
            """),
            schema=self.schema,
        )
        doc["origin"] = Point(9, 9)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "x: 9" in out
        assert "y: 9" in out
        assert "x: 1" not in out

    def test_setitem_multiple_custom_values(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            a: 1
            b: 2
        """)
        )
        doc["a"] = Point(1, 2)
        doc["b"] = Point(3, 4)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2

    def test_update_with_custom_values(self) -> None:
        doc = yarutsk.loads("name: test\n")
        doc.update({"p1": Point(1, 2), "p2": Point(3, 4)})
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2
        assert "x: 1" in out
        assert "x: 3" in out

    def test_setdefault_inserts_custom(self) -> None:
        doc = yarutsk.loads("name: test\n")
        result = doc.setdefault("pos", Point(5, 6))
        assert result == Point(5, 6)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 5" in out

    def test_setdefault_does_not_overwrite(self) -> None:
        doc = yarutsk.loads("name: test\n", schema=self.schema)
        doc["pos"] = Point(1, 2)
        result = doc.setdefault("pos", Point(99, 99))
        assert result == Point(1, 2)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "x: 1" in out
        assert "x: 99" not in out

    def test_nested_mapping_custom_value(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            config:
              debug: true
        """)
        )
        doc["config"]["pos"] = Point(7, 8)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 7" in out

    def test_roundtrip_after_setitem(self) -> None:
        doc = yarutsk.loads("name: test\n")
        doc["pos"] = Point(3, 4)
        out = yarutsk.dumps(doc, schema=self.schema)
        doc2 = yarutsk.loads(out, schema=self.schema)
        assert isinstance(doc2["pos"], Point)
        assert doc2["pos"] == Point(3, 4)


class TestMutableSequenceCustomTypes:
    def setup_method(self) -> None:
        self.schema = yarutsk.Schema()
        self.schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
        self.schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

    def test_append_custom_to_empty_sequence(self) -> None:
        doc = yarutsk.loads("points: []\n")
        doc["points"].append(Point(1, 2))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "!point" in out
        assert "x: 1" in out

    def test_append_multiple_custom(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                points:
                  - !point
                    x: 0
                    y: 0
            """),
            schema=self.schema,
        )
        doc["points"].append(Point(1, 2))
        doc["points"].append(Point(3, 4))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 3

    def test_setitem_replaces_custom(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                points:
                  - !point
                    x: 1
                    y: 2
            """),
            schema=self.schema,
        )
        doc["points"][0] = Point(9, 9)
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "x: 9" in out
        assert "x: 1" not in out

    def test_insert_custom(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                points:
                  - !point
                    x: 1
                    y: 2
            """),
            schema=self.schema,
        )
        doc["points"].insert(0, Point(0, 0))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2

    def test_extend_with_custom(self) -> None:
        doc = yarutsk.loads("points: []\n")
        doc["points"].extend([Point(1, 2), Point(3, 4)])
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2

    def test_top_level_sequence_append(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                - !point
                  x: 0
                  y: 0
            """),
            schema=self.schema,
        )
        doc.append(Point(5, 6))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert out.count("!point") == 2
        assert "x: 5" in out

    def test_roundtrip_after_append(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
                points:
                  - !point
                    x: 0
                    y: 0
            """),
            schema=self.schema,
        )
        doc["points"].append(Point(7, 8))
        out = yarutsk.dumps(doc, schema=self.schema)
        doc2 = yarutsk.loads(out, schema=self.schema)
        assert len(doc2["points"]) == 2
        assert doc2["points"][1] == Point(7, 8)

    def test_mixed_native_and_custom_in_sequence(self) -> None:
        doc = yarutsk.loads(
            dedent("""\
            items:
              - 1
              - 2
        """)
        )
        doc["items"].append(Point(3, 4))
        out = yarutsk.dumps(doc, schema=self.schema)
        assert "- 1" in out
        assert "- 2" in out
        assert "!point" in out


class TestSchemaErrors:
    def test_loader_receives_mapping_raises_loader_error(self) -> None:
        schema = yarutsk.Schema()
        schema.add_loader("!color", lambda s: s.split(","))  # expects str, gets mapping

        with pytest.raises(yarutsk.LoaderError) as exc_info:
            yarutsk.loads(
                "bg: !color\n  r: 255\n  g: 0\n  b: 128\n",
                schema=schema,
            )
        assert "!color" in str(exc_info.value)

    def test_loader_receives_sequence_raises_loader_error(self) -> None:
        schema = yarutsk.Schema()
        schema.add_loader("!color", lambda s: s.split(","))  # expects str, gets sequence

        with pytest.raises(yarutsk.LoaderError) as exc_info:
            yarutsk.loads(
                "bg: !color\n  - 255\n  - 0\n  - 128\n",
                schema=schema,
            )
        assert "!color" in str(exc_info.value)

    def test_loader_raises_on_valid_input(self) -> None:
        schema = yarutsk.Schema()
        schema.add_loader("!boom", lambda s: (_ for _ in ()).throw(ValueError("intentional")))

        with pytest.raises(yarutsk.LoaderError) as exc_info:
            yarutsk.loads("x: !boom value\n", schema=schema)
        assert "!boom" in str(exc_info.value)

    def test_dumper_raises_loader_error(self) -> None:
        schema = yarutsk.Schema()
        schema.add_dumper(Point, lambda p: (_ for _ in ()).throw(RuntimeError("dump fail")))

        doc = yarutsk.loads("x: placeholder\n")
        doc["x"] = Point(1, 2)
        with pytest.raises(yarutsk.DumperError) as exc_info:
            yarutsk.dumps(doc, schema=schema)
        assert "Point" in str(exc_info.value)

    def test_dumper_returns_wrong_type_raises_dumper_error(self) -> None:
        schema = yarutsk.Schema()
        schema.add_dumper(Point, lambda p: "not-a-tuple")  # must return (tag, data)

        doc = yarutsk.loads("x: placeholder\n")
        doc["x"] = Point(1, 2)
        with pytest.raises(yarutsk.DumperError) as exc_info:
            yarutsk.dumps(doc, schema=schema)
        assert "Point" in str(exc_info.value)
        assert "tuple" in str(exc_info.value)

    def test_parse_error_raises_parse_error(self) -> None:
        with pytest.raises(yarutsk.ParseError):
            yarutsk.loads("{bad yaml: [}")

    def test_hierarchy_loader_error_is_yarutsk_error(self) -> None:
        assert issubclass(yarutsk.LoaderError, yarutsk.YarutskError)

    def test_hierarchy_dumper_error_is_yarutsk_error(self) -> None:
        assert issubclass(yarutsk.DumperError, yarutsk.YarutskError)

    def test_hierarchy_parse_error_is_yarutsk_error(self) -> None:
        assert issubclass(yarutsk.ParseError, yarutsk.YarutskError)

    def test_catch_with_base_exception(self) -> None:
        with pytest.raises(yarutsk.YarutskError):
            yarutsk.loads("{bad yaml: [}")
