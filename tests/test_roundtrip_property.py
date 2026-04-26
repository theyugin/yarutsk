"""Property-based round-trip tests for the Python↔YAML boundary.

Complements the Rust-side ``idempotent_emit`` fuzz target, which feeds raw bytes
to the parser/emitter. These tests start from hypothesis-generated Python trees
(``dict``/``list``/primitives) and exercise ``dumps`` → ``loads`` → ``to_python``
and emit idempotence, catching drift in ``py_to_node`` / ``extract_yaml_node`` /
``node_to_py`` that the byte-level fuzzer would not see.
"""

from typing import Any

import pytest

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import HealthCheck, given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402

import yarutsk  # noqa: E402

# Alphabet avoids control chars and surrogates but includes space/tab so the
# emitter's leading/trailing-whitespace quoting is exercised. Strings matching
# YAML keywords (true/null/…) or numeric forms are fine: the emitter quotes
# them on dump.
_SAFE_CHARS = st.characters(
    whitelist_categories=("Ll", "Lu", "Nd"),
    whitelist_characters="_-. \t",
)

_KEYS = st.text(alphabet=_SAFE_CHARS, min_size=1, max_size=12)

_SCALARS = (
    st.integers(min_value=-(2**63), max_value=2**63 - 1)
    | st.floats(allow_nan=False, allow_infinity=False, width=64)
    | st.booleans()
    | st.none()
    | st.text(alphabet=_SAFE_CHARS, max_size=20)
)


def _trees() -> st.SearchStrategy[Any]:
    return st.recursive(
        _SCALARS,
        lambda inner: st.lists(inner, max_size=5) | st.dictionaries(_KEYS, inner, max_size=5),
        max_leaves=10,
    )


def _to_python(doc: Any) -> Any:
    """Collapse a yarutsk return value to a plain Python tree."""
    if hasattr(doc, "to_python"):
        return doc.to_python()
    return doc


@given(tree=_trees())
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_tree_roundtrip(tree: Any) -> None:
    """dumps → loads → to_python preserves the tree."""
    out = yarutsk.dumps(tree)
    doc = yarutsk.loads(out)
    assert doc is not None
    assert _to_python(doc) == tree


@given(tree=_trees())
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_dump_idempotent(tree: Any) -> None:
    """The first dump is already a fixed point: dumps(t) == dumps(loads(dumps(t)))."""
    first = yarutsk.dumps(tree)
    second = yarutsk.dumps(yarutsk.loads(first))
    assert first == second


@given(tree=_trees())
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_format_then_roundtrip(tree: Any) -> None:
    doc = yarutsk.loads(yarutsk.dumps(tree))
    assert doc is not None
    if hasattr(doc, "format"):
        doc.format()
    out = yarutsk.dumps(doc)
    again = yarutsk.loads(out)
    assert again is not None
    assert _to_python(again) == tree


_FLOW_VALUE_CHARS = st.characters(
    whitelist_categories=("Ll", "Lu", "Nd"),
    whitelist_characters="_-. \t,[]{}",
)
_FLOW_STRINGS = st.text(alphabet=_FLOW_VALUE_CHARS, min_size=1, max_size=12)
_FLOW_LEAVES = (
    st.integers(min_value=-(2**31), max_value=2**31 - 1) | st.booleans() | st.none() | _FLOW_STRINGS
)


def _flow_seq(values: list[Any]) -> yarutsk.YamlSequence:
    seq = yarutsk.YamlSequence(style="flow")
    for v in values:
        seq.append(v)
    return seq


def _flow_map(items: dict[str, Any]) -> yarutsk.YamlMapping:
    m = yarutsk.YamlMapping(style="flow")
    for k, v in items.items():
        m[k] = v
    return m


@given(values=st.lists(_FLOW_LEAVES, min_size=1, max_size=5))
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_flow_sequence_roundtrip(values: list[Any]) -> None:
    seq = _flow_seq(values)
    out = yarutsk.dumps({"k": seq})
    parsed = yarutsk.loads(out)
    assert parsed is not None
    assert _to_python(parsed) == {"k": values}


@given(items=st.dictionaries(_KEYS, _FLOW_LEAVES, min_size=1, max_size=5))
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_flow_mapping_roundtrip(items: dict[str, Any]) -> None:
    m = _flow_map(items)
    out = yarutsk.dumps({"k": m})
    parsed = yarutsk.loads(out)
    assert parsed is not None
    assert _to_python(parsed) == {"k": items}


_EDGE_KEYS = st.one_of(
    _KEYS,
    st.sampled_from(
        [
            "null",
            "Null",
            "NULL",
            "~",
            "true",
            "false",
            "yes",
            "no",
            "on",
            "off",
            "-dash",
            "-1",
            " leading",
            "trailing ",
        ]
    ),
)


@given(keys=st.lists(_EDGE_KEYS, min_size=1, max_size=5, unique=True))
@settings(
    max_examples=100,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
)
def test_edge_keys_preserved(keys: list[str]) -> None:
    tree = {k: i for i, k in enumerate(keys)}
    out = yarutsk.dumps(tree)
    parsed = yarutsk.loads(out)
    assert parsed is not None
    assert _to_python(parsed) == tree
