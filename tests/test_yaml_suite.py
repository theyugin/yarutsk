"""YAML spec conformance tests using the official yaml-test-suite.

Each test file in yaml-test-suite/src/ contains one or more test cases with:
  yaml    — input to parse
  json    — expected values as JSON (absent for some; always absent when fail=true)
  tree    — expected event tree (not checked here)
  dump    — canonical dump (not checked here)
  fail    — true when the input is invalid YAML and a conformant parser must reject it

Usage:
    pytest tests/test_yaml_suite.py -v
    pytest tests/test_yaml_suite.py -v -k "test_values"   # value conformance only
    pytest tests/test_yaml_suite.py -v -k "test_parse"    # parse/reject only
"""

import io
import json as json_mod
import re
from pathlib import Path

import pytest

try:
    import yarutsk

    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

SUITE_DIR = Path(__file__).parent.parent / "yaml-test-suite"
SRC_DIR = SUITE_DIR / "src"


# ── Text decoding ─────────────────────────────────────────────────────────────


def _decode(value: str | None) -> str | None:
    """Translate yaml-test-suite visual encodings back to real characters."""
    if value is None:
        return None
    text = re.sub(r"—*»", "\t", value)
    return (
        text.replace("␣", " ")
        .replace("↵", "")
        .replace("←", "\r")
        .replace("⇔", "\ufeff")
        .replace("∎", "")
    )


# ── JSON field parsing ────────────────────────────────────────────────────────


def _parse_json_docs(json_str: str) -> list:
    """Parse one or more JSON values from the json field (multi-doc YAML may
    produce multiple top-level JSON values separated by whitespace)."""
    dec = json_mod.JSONDecoder()
    pos = 0
    results = []
    s = json_str.strip()
    while pos < len(s):
        val, pos = dec.raw_decode(s, pos)
        results.append(val)
        while pos < len(s) and s[pos].isspace():
            pos += 1
    return results


# ── Test case loading ─────────────────────────────────────────────────────────


def _load_test_cases() -> list:
    if not SRC_DIR.exists():
        return []

    cases = []
    for yaml_file in sorted(SRC_DIR.glob("*.yaml")):
        try:
            raw = yaml_file.read_text(encoding="utf-8")
            tests = yarutsk.loads(raw)
        except Exception:
            continue

        if not isinstance(tests, list):
            continue

        # A file-level skip note propagates to every case in that file.
        file_skip = next(
            (
                t.get("note", "skipped by test-suite metadata")
                for t in tests
                if isinstance(t, dict) and t.get("skip")
            ),
            None,
        )

        for test in tests:
            if not isinstance(test, dict):
                continue

            should_fail = bool(test.get("fail"))
            should_skip = bool(test.get("skip")) or file_skip is not None
            skip_reason = (
                test.get("note") or file_skip or "skipped by test-suite metadata"
            )

            marks = [pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")]
            if should_skip:
                marks.append(pytest.mark.skip(reason=skip_reason))
            if should_fail:
                # The parser must reject this input; test_parse is expected to
                # call pytest.fail() (via the except branch), satisfying xfail.
                # strict=True turns an unexpected pass into a test error.
                marks.append(
                    pytest.mark.xfail(
                        strict=True,
                        reason="invalid YAML — parser must reject",
                    )
                )

            name = test.get("name", yaml_file.stem)
            cases.append(
                pytest.param(
                    {
                        "file": yaml_file.stem,
                        "name": name,
                        "yaml": _decode(test.get("yaml", "")),
                        "json": _decode(test.get("json")),
                        "fail": should_fail,
                    },
                    id=f"{yaml_file.stem}:{name}",
                    marks=marks,
                )
            )

    return cases


# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture(params=_load_test_cases())
def yaml_test_case(request):
    return request.param


# ── Test classes ──────────────────────────────────────────────────────────────


@pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")
class TestYamlSuite:
    """Run yaml-test-suite tests against yarutsk."""

    def test_parse(self, yaml_test_case):
        """Valid YAML must parse without error; invalid YAML must be rejected."""
        test = yaml_test_case
        try:
            docs = yarutsk.load_all(io.StringIO(test["yaml"]))
            for doc in docs:
                repr(doc)
        except Exception as e:
            pytest.fail(f"Unexpected parse error: {e}\nYAML:\n{test['yaml']}")

    def test_values(self, yaml_test_case):
        """Parsed values must match the expected JSON representation."""
        test = yaml_test_case

        if test["fail"]:
            pytest.skip("invalid-YAML case has no expected values")

        json_str = test["json"]
        if not json_str:
            pytest.skip("no json field in this test case")

        try:
            docs = yarutsk.load_all(io.StringIO(test["yaml"]))
        except Exception as e:
            pytest.skip(f"parse failed (covered by test_parse): {e}")

        expected = _parse_json_docs(json_str)
        actual = [d.to_dict() if hasattr(d, "to_dict") else d for d in docs]
        assert actual == expected, (
            f"\nExpected: {expected}\nActual:   {actual}\nYAML:\n{test['yaml']}"
        )

    def test_roundtrip(self, yaml_test_case):
        """Round-trip value preservation: re-parsed values must match after emit.

        The test verifies two hard properties:
          1. The emitted YAML is re-parseable without errors.
          2. Re-parsed values match the expected JSON representation.

        Structural limitations that the emitter cannot faithfully reproduce are
        classified as pytest.skip() so they are clearly excluded rather than
        expected to fail.  Source-fidelity (byte-for-byte identity) is not
        checked — only value preservation matters.
        """
        test = yaml_test_case

        if test["fail"]:
            pytest.skip("invalid YAML — no round-trip expected")

        yaml_src = test["yaml"]
        try:
            docs = list(yarutsk.load_all(io.StringIO(yaml_src)))
        except Exception as e:
            pytest.skip(f"parse failed (covered by test_parse): {e}")

        result = yarutsk.dumps_all(docs)

        # ── Hard check: emitted YAML must be re-parseable ────────────────────
        try:
            re_docs = list(yarutsk.load_all(io.StringIO(result)))
        except Exception as e:
            pytest.fail(
                f"Emitter produced invalid YAML: {e}\n"
                f"Original:\n{yaml_src}\nEmitted:\n{result}"
            )

        # ── Hard check: re-parsed values must match original ─────────────────
        if test["json"]:
            expected = _parse_json_docs(test["json"])
            actual = [d.to_dict() if hasattr(d, "to_dict") else d for d in re_docs]
            if actual != expected:
                pytest.fail(
                    f"Re-parsed values changed after round-trip.\n"
                    f"Expected: {expected}\nActual:   {actual}\n"
                    f"Original YAML:\n{yaml_src}\nEmitted:\n{result}"
                )
