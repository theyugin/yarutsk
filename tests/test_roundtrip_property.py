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
