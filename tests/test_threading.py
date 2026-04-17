"""Multithreaded stress tests for yarutsk.

These tests exercise the library from multiple threads simultaneously to detect
data races, GIL-release issues, or state corruption. They cover:

- Concurrent loads/dumps on independent documents
- Shared read-only document access from many threads
- Concurrent streaming loads (iter_load_all) across threads
- Shared Schema objects used from multiple threads
- Concurrent dump-to-stream with independent streams
- Interleaved mutations and reads on separate documents
"""

import io
import threading
from textwrap import dedent

import yarutsk

# ─── Helpers ─────────────────────────────────────────────────────────────────

N_THREADS = 16
N_ITERS = 50  # iterations per thread

SIMPLE_YAML = dedent("""\
    name: Alice
    age: 30
    scores:
      - 10
      - 20
      - 30
    address:
      city: Berlin
      zip: '10115'
""")

MULTI_DOC_YAML = "".join(f"---\nid: {i}\nvalue: item_{i}\n" for i in range(20))

COMMENT_YAML = dedent("""\
    # header comment
    host: localhost  # inline
    port: 5432

    debug: true
""")


def collect_errors(fn, n_threads=N_THREADS, n_iters=N_ITERS):
    """Run *fn(thread_index, iter_index)* from n_threads threads, each calling
    it n_iters times. Returns a list of (thread, iter, exception) triples."""
    errors: list[tuple[int, int, Exception]] = []
    lock = threading.Lock()

    def worker(tid):
        for i in range(n_iters):
            try:
                fn(tid, i)
            except Exception as exc:
                with lock:
                    errors.append((tid, i, exc))

    threads = [threading.Thread(target=worker, args=(t,)) for t in range(n_threads)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    return errors


# ─── Independent loads ────────────────────────────────────────────────────────


class TestConcurrentLoads:
    def test_loads_independent(self):
        """Many threads parsing independent YAML strings simultaneously."""

        def work(tid, i):
            doc = yarutsk.loads(SIMPLE_YAML)
            assert doc["name"] == "Alice"
            assert doc["age"] == 30
            assert doc["scores"] == [10, 20, 30]

        errors = collect_errors(work)
        assert errors == [], errors

    def test_load_stream_independent(self):
        """Many threads each loading from their own StringIO."""

        def work(tid, i):
            stream = io.StringIO(SIMPLE_YAML)
            doc = yarutsk.load(stream)
            assert doc["name"] == "Alice"

        errors = collect_errors(work)
        assert errors == [], errors

    def test_load_bytesio_independent(self):
        """Many threads each loading from their own BytesIO."""

        def work(tid, i):
            stream = io.BytesIO(SIMPLE_YAML.encode())
            doc = yarutsk.load(stream)
            assert doc["name"] == "Alice"

        errors = collect_errors(work)
        assert errors == [], errors

    def test_loads_all_independent(self):
        """Many threads parsing multi-document YAML simultaneously."""

        def work(tid, i):
            docs = yarutsk.loads_all(MULTI_DOC_YAML)
            assert len(docs) == 20
            for j, doc in enumerate(docs):
                assert doc["id"] == j

        errors = collect_errors(work)
        assert errors == [], errors

    def test_load_all_stream_independent(self):
        """Many threads loading multi-doc streams simultaneously."""

        def work(tid, i):
            stream = io.StringIO(MULTI_DOC_YAML)
            docs = yarutsk.load_all(stream)
            assert len(docs) == 20

        errors = collect_errors(work)
        assert errors == [], errors


# ─── Shared read-only document ────────────────────────────────────────────────


class TestSharedReadOnlyDocument:
    def test_concurrent_reads_from_shared_doc(self):
        """A single pre-loaded document read by many threads at once."""
        shared = yarutsk.loads(SIMPLE_YAML)

        def work(tid, i):
            assert shared["name"] == "Alice"
            assert shared["age"] == 30
            assert list(shared["scores"]) == [10, 20, 30]
            assert shared["address"]["city"] == "Berlin"

        errors = collect_errors(work)
        assert errors == [], errors

    def test_concurrent_dumps_from_shared_doc(self):
        """Many threads calling dumps() on the same document object."""
        shared = yarutsk.loads(SIMPLE_YAML)
        expected = yarutsk.dumps(shared)

        def work(tid, i):
            result = yarutsk.dumps(shared)
            assert result == expected

        errors = collect_errors(work)
        assert errors == [], errors

    def test_concurrent_dump_to_streams_from_shared_doc(self):
        """Many threads dump()ing the same document to independent streams."""
        shared = yarutsk.loads(SIMPLE_YAML)
        expected = yarutsk.dumps(shared)

        def work(tid, i):
            out = io.StringIO()
            yarutsk.dump(shared, out)
            assert out.getvalue() == expected

        errors = collect_errors(work)
        assert errors == [], errors


# ─── Concurrent dumps ─────────────────────────────────────────────────────────


class TestConcurrentDumps:
    def test_dumps_independent(self):
        """Many threads serialising independent documents simultaneously."""

        def work(tid, i):
            doc = yarutsk.loads(f"key: value_{tid}_{i}\nnum: {tid * N_ITERS + i}\n")
            result = yarutsk.dumps(doc)
            assert f"value_{tid}_{i}" in result
            assert str(tid * N_ITERS + i) in result

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_to_stream_independent(self):
        """Many threads calling dump() to their own stream simultaneously."""

        def work(tid, i):
            doc = yarutsk.loads(SIMPLE_YAML)
            out = io.StringIO()
            yarutsk.dump(doc, out)
            assert "Alice" in out.getvalue()

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_all_to_stream_independent(self):
        """Many threads calling dump_all() to their own stream simultaneously."""
        docs_src = [yarutsk.loads(f"id: {i}\n") for i in range(5)]

        def work(tid, i):
            out = io.StringIO()
            yarutsk.dump_all(docs_src, out)
            text = out.getvalue()
            for j in range(5):
                assert f"id: {j}" in text

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dumps_all_independent(self):
        """Many threads calling dumps_all() simultaneously."""
        docs_src = [yarutsk.loads(f"id: {i}\n") for i in range(5)]
        expected = yarutsk.dumps_all(docs_src)

        def work(tid, i):
            result = yarutsk.dumps_all(docs_src)
            assert result == expected

        errors = collect_errors(work)
        assert errors == [], errors


# ─── Lazy iterator ────────────────────────────────────────────────────────────


class TestConcurrentIterator:
    def test_iter_loads_all_independent(self):
        """Many threads using iter_loads_all() on independent strings."""

        def work(tid, i):
            collected = list(yarutsk.iter_loads_all(MULTI_DOC_YAML))
            assert len(collected) == 20
            for j, doc in enumerate(collected):
                assert doc["id"] == j

        errors = collect_errors(work)
        assert errors == [], errors

    def test_iter_load_all_stream_independent(self):
        """Many threads using iter_load_all() on independent streams."""

        def work(tid, i):
            stream = io.StringIO(MULTI_DOC_YAML)
            collected = list(yarutsk.iter_load_all(stream))
            assert len(collected) == 20

        errors = collect_errors(work)
        assert errors == [], errors

    def test_iter_load_all_partial_consumption(self):
        """Partially consuming an iterator from many threads is safe."""

        def work(tid, i):
            stream = io.StringIO(MULTI_DOC_YAML)
            it = yarutsk.iter_load_all(stream)
            first = next(it)
            assert first["id"] == 0
            # Iterator is dropped here without exhausting the stream — no crash.

        errors = collect_errors(work)
        assert errors == [], errors

    def test_iter_loads_all_interleaved_with_loads(self):
        """iter_loads_all and loads running simultaneously."""
        barrier = threading.Barrier(N_THREADS)

        def work(tid, i):
            barrier.wait()
            if tid % 2 == 0:
                collected = list(yarutsk.iter_loads_all(MULTI_DOC_YAML))
                assert len(collected) == 20
            else:
                docs = yarutsk.loads_all(MULTI_DOC_YAML)
                assert len(docs) == 20

        errors = collect_errors(work, n_iters=5)
        assert errors == [], errors


# ─── Shared Schema ────────────────────────────────────────────────────────────


class TestConcurrentSchema:
    def test_shared_schema_concurrent_loads(self):
        """A single Schema object used from many threads during load."""

        class Color:
            def __init__(self, r, g, b):
                self.r, self.g, self.b = r, g, b

        schema = yarutsk.Schema()
        schema.add_loader("!color", lambda s: Color(*[int(x) for x in s.split(",")]))
        schema.add_dumper(Color, lambda c: ("!color", f"{c.r},{c.g},{c.b}"))

        yaml_text = "bg: !color 255,0,128\n"

        def work(tid, i):
            doc = yarutsk.loads(yaml_text, schema=schema)
            c = doc["bg"]
            assert isinstance(c, Color)
            assert c.r == 255 and c.g == 0 and c.b == 128

        errors = collect_errors(work)
        assert errors == [], errors

    def test_shared_schema_concurrent_dumps(self):
        """A single Schema object used from many threads during dump."""

        class Tag:
            def __init__(self, v):
                self.v = v

        schema = yarutsk.Schema()
        schema.add_loader("!tag", lambda s: Tag(s))
        schema.add_dumper(Tag, lambda t: ("!tag", t.v))

        def work(tid, i):
            doc = yarutsk.loads("x: placeholder\n")
            doc["x"] = Tag(f"t_{tid}_{i}")
            result = yarutsk.dumps(doc, schema=schema)
            assert f"t_{tid}_{i}" in result

        errors = collect_errors(work)
        assert errors == [], errors

    def test_shared_schema_load_and_dump_interleaved(self):
        """Loads and dumps sharing a schema run simultaneously without corruption.

        Note: the dumper returns str(x.n) which yarutsk may single-quote (e.g.
        '10') to prevent ambiguity with integer 10.  We verify the round-trip
        value rather than the exact serialised form.
        """

        class Num:
            def __init__(self, n):
                self.n = n

        schema = yarutsk.Schema()
        schema.add_loader("!num", lambda s: Num(int(s)))
        schema.add_dumper(Num, lambda x: ("!num", str(x.n)))

        barrier = threading.Barrier(N_THREADS)

        def work(tid, i):
            barrier.wait()
            text = f"val: !num {tid}\n"
            doc = yarutsk.loads(text, schema=schema)
            assert doc["val"].n == tid
            doc["val"] = Num(tid * 2)
            result = yarutsk.dumps(doc, schema=schema)
            # Verify round-trip correctness, not the exact quoting style.
            doc2 = yarutsk.loads(result, schema=schema)
            assert doc2["val"].n == tid * 2

        errors = collect_errors(work, n_iters=10)
        assert errors == [], errors


# ─── Custom type dumping to IO ───────────────────────────────────────────────


class Color:
    """Shared fixture type used across IO dump tests."""

    def __init__(self, r, g, b):
        self.r, self.g, self.b = r, g, b

    def __eq__(self, other):
        return isinstance(other, Color) and (self.r, self.g, self.b) == (
            other.r,
            other.g,
            other.b,
        )


class Point:
    def __init__(self, x, y):
        self.x, self.y = x, y

    def __eq__(self, other):
        return isinstance(other, Point) and (self.x, self.y) == (other.x, other.y)


_color_schema = yarutsk.Schema()
_color_schema.add_loader("!color", lambda s: Color(*[int(x) for x in s.split(",")]))
_color_schema.add_dumper(Color, lambda c: ("!color", f"{c.r},{c.g},{c.b}"))
_color_schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
_color_schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))


class TestCustomTypeDumpIO:
    def test_dump_scalar_custom_type_to_stringio(self):
        """Many threads each dump a doc with a custom scalar type to their own StringIO."""

        def work(tid, i):
            doc = yarutsk.loads("bg: placeholder\n")
            doc["bg"] = Color(tid, i, 0)
            out = io.StringIO()
            yarutsk.dump(doc, out, schema=_color_schema)
            text = out.getvalue()
            assert f"!color {tid},{i},0" in text
            # Round-trip: load it back and verify
            doc2 = yarutsk.load(io.StringIO(text), schema=_color_schema)
            assert doc2["bg"] == Color(tid, i, 0)

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_scalar_custom_type_to_bytesio(self):
        """Many threads each dump a doc with a custom scalar type to their own BytesIO."""

        def work(tid, i):
            doc = yarutsk.loads("bg: placeholder\n")
            doc["bg"] = Color(tid, i, 128)
            out = io.BytesIO()
            yarutsk.dump(doc, out, schema=_color_schema)
            text = out.getvalue().decode()
            assert f"!color {tid},{i},128" in text
            doc2 = yarutsk.load(io.BytesIO(out.getvalue()), schema=_color_schema)
            assert doc2["bg"] == Color(tid, i, 128)

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_mapping_custom_type_to_stringio(self):
        """Many threads each dump a doc with a custom mapping type to their own StringIO."""

        def work(tid, i):
            doc = yarutsk.loads("origin: placeholder\n")
            doc["origin"] = Point(tid, i)
            out = io.StringIO()
            yarutsk.dump(doc, out, schema=_color_schema)
            text = out.getvalue()
            assert "!point" in text
            doc2 = yarutsk.load(io.StringIO(text), schema=_color_schema)
            assert doc2["origin"] == Point(tid, i)

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_all_custom_types_to_stringio(self):
        """Many threads each dump_all a list of docs with custom types to their own StringIO."""

        def work(tid, i):
            docs = [yarutsk.loads("x: placeholder\n") for _ in range(3)]
            for k, doc in enumerate(docs):
                doc["x"] = Color(tid, i, k)
            out = io.StringIO()
            yarutsk.dump_all(docs, out, schema=_color_schema)
            text = out.getvalue()
            for k in range(3):
                assert f"!color {tid},{i},{k}" in text
            loaded = yarutsk.load_all(io.StringIO(text), schema=_color_schema)
            for k, doc in enumerate(loaded):
                assert doc["x"] == Color(tid, i, k)

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_all_custom_types_to_bytesio(self):
        """Many threads each dump_all with custom types to their own BytesIO."""

        def work(tid, i):
            docs = [yarutsk.loads("pt: placeholder\n") for _ in range(4)]
            for k, doc in enumerate(docs):
                doc["pt"] = Point(tid + k, i + k)
            out = io.BytesIO()
            yarutsk.dump_all(docs, out, schema=_color_schema)
            loaded = yarutsk.load_all(io.BytesIO(out.getvalue()), schema=_color_schema)
            for k, doc in enumerate(loaded):
                assert doc["pt"] == Point(tid + k, i + k)

        errors = collect_errors(work)
        assert errors == [], errors

    def test_shared_doc_custom_dump_to_independent_streams(self):
        """A single pre-built doc with custom objects dumped by many threads to independent streams."""
        doc = yarutsk.loads("color: placeholder\npoint: placeholder\n")
        doc["color"] = Color(255, 128, 0)
        doc["point"] = Point(3, 4)
        expected_text = yarutsk.dumps(doc, schema=_color_schema)

        def work(tid, i):
            out = io.StringIO()
            yarutsk.dump(doc, out, schema=_color_schema)
            assert out.getvalue() == expected_text

        errors = collect_errors(work)
        assert errors == [], errors

    def test_dump_custom_type_barrier_burst(self):
        """All threads start dumping custom types to IO exactly simultaneously."""
        barrier = threading.Barrier(N_THREADS)

        def work(tid, i):
            doc = yarutsk.loads("bg: placeholder\npt: placeholder\n")
            doc["bg"] = Color(tid, i, tid + i)
            doc["pt"] = Point(tid * 2, i * 2)
            barrier.wait()
            out_str = io.StringIO()
            out_bin = io.BytesIO()
            yarutsk.dump(doc, out_str, schema=_color_schema)
            yarutsk.dump(doc, out_bin, schema=_color_schema)
            assert out_str.getvalue() == out_bin.getvalue().decode()
            doc2 = yarutsk.load(io.StringIO(out_str.getvalue()), schema=_color_schema)
            assert doc2["bg"] == Color(tid, i, tid + i)
            assert doc2["pt"] == Point(tid * 2, i * 2)

        errors = collect_errors(work, n_iters=20)
        assert errors == [], errors


# ─── Round-trip under concurrency ─────────────────────────────────────────────


class TestConcurrentRoundTrip:
    def test_roundtrip_comments_concurrent(self):
        """Comment-preserving round-trips from many threads are correct."""
        expected = yarutsk.dumps(yarutsk.loads(COMMENT_YAML))

        def work(tid, i):
            result = yarutsk.dumps(yarutsk.loads(COMMENT_YAML))
            assert result == expected

        errors = collect_errors(work)
        assert errors == [], errors

    def test_roundtrip_anchors_concurrent(self):
        """Anchor/alias round-trips from many threads are correct."""
        src = "base: &anchor hello\nref: *anchor\n"
        expected = yarutsk.dumps(yarutsk.loads(src))

        def work(tid, i):
            result = yarutsk.dumps(yarutsk.loads(src))
            assert result == expected

        errors = collect_errors(work)
        assert errors == [], errors

    def test_many_threads_load_mutate_dump(self):
        """Each thread loads a fresh document, mutates it, dumps it — no sharing."""

        def work(tid, i):
            doc = yarutsk.loads(SIMPLE_YAML)
            doc["age"] = tid * 100 + i
            doc["name"] = f"thread_{tid}_iter_{i}"
            result = yarutsk.dumps(doc)
            assert f"thread_{tid}_iter_{i}" in result
            assert str(tid * 100 + i) in result

        errors = collect_errors(work)
        assert errors == [], errors

    def test_binary_stream_dump_concurrent(self):
        """dump() to BytesIO from many threads produces valid output."""
        doc = yarutsk.loads(SIMPLE_YAML)

        def work(tid, i):
            out = io.BytesIO()
            yarutsk.dump(doc, out)
            text = out.getvalue().decode()
            assert "Alice" in text

        errors = collect_errors(work)
        assert errors == [], errors
