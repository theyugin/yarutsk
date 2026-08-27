"""Minimal installed-artifact smoke test used by wheel and sdist release jobs."""

from __future__ import annotations

import io
import os
import sys

import yarutsk


def main() -> None:
    if os.environ.get("YARUTSK_EXPECT_FREE_THREADED") == "1":
        is_gil_enabled = getattr(sys, "_is_gil_enabled", None)
        assert is_gil_enabled is not None, "free-threaded runtime probe is unavailable"
        assert not is_gil_enabled(), "importing yarutsk re-enabled the GIL"

    doc = yarutsk.loads("# before\na: 1\nb: &shared\n  c: 2\nalias: *shared\n")
    assert doc.to_python() == {"a": 1, "b": {"c": 2}, "alias": {"c": 2}}
    assert doc["b"] is doc["alias"]
    assert "# before" in yarutsk.dumps(doc)

    text = io.StringIO()
    yarutsk.dump(doc, text)
    assert yarutsk.loads(text.getvalue()).to_python() == doc.to_python()

    binary = io.BytesIO()
    yarutsk.dump(doc, binary)
    assert yarutsk.loads(binary.getvalue()).to_python() == doc.to_python()

    schema = yarutsk.Schema(loaders={"!upper": lambda value: value.upper()})
    assert yarutsk.loads("value: !upper hello\n", schema=schema)["value"] == "HELLO"

    try:
        yarutsk.loads("[unterminated")
    except yarutsk.ParseError:
        pass
    else:
        raise AssertionError("invalid YAML did not raise ParseError")


if __name__ == "__main__":
    main()
