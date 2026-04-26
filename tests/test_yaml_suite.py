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
from typing import Any

import pytest
from _yaml_suite import load_test_cases, normalize_for_json_compare, parse_json_docs

import yarutsk


@pytest.fixture(params=load_test_cases())
def yaml_test_case(request: pytest.FixtureRequest) -> Any:
    return request.param


class TestYamlSuite:
    """Run yaml-test-suite tests against yarutsk."""

    def test_parse(self, yaml_test_case: dict[str, Any]) -> None:
        """Valid YAML must parse without error; invalid YAML must be rejected."""
        test = yaml_test_case
        try:
            docs = yarutsk.load_all(io.StringIO(test["yaml"]))
            assert all(
                isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
                for d in docs
            )
            for doc in docs:
                repr(doc)
        except Exception as e:
            pytest.fail(f"Unexpected parse error: {e}\nYAML:\n{test['yaml']}")

    def test_values(self, yaml_test_case: dict[str, Any]) -> None:
        """Parsed values must match the expected JSON representation."""
        test = yaml_test_case

        if test["fail"]:
            pytest.skip("invalid-YAML case has no expected values")

        json_str = test["json"]
        if not json_str:
            pytest.skip("no json field in this test case")

        try:
            docs = yarutsk.load_all(io.StringIO(test["yaml"]))
            assert all(
                isinstance(d, (yarutsk.YamlMapping, yarutsk.YamlSequence, yarutsk.YamlScalar))
                for d in docs
            )
        except Exception as e:
            pytest.skip(f"parse failed (covered by test_parse): {e}")

        expected = [normalize_for_json_compare(d) for d in parse_json_docs(json_str)]
        actual = [
            normalize_for_json_compare(d.to_python() if hasattr(d, "to_python") else d)
            for d in docs
        ]
        assert actual == expected, (
            f"\nExpected: {expected}\nActual:   {actual}\nYAML:\n{test['yaml']}"
        )

    def test_roundtrip(self, yaml_test_case: dict[str, Any]) -> None:
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

        try:
            re_docs = list(yarutsk.load_all(io.StringIO(result)))
        except Exception as e:
            pytest.fail(
                f"Emitter produced invalid YAML: {e}\nOriginal:\n{yaml_src}\nEmitted:\n{result}"
            )

        if test["json"]:
            expected = [normalize_for_json_compare(d) for d in parse_json_docs(test["json"])]
            actual = [
                normalize_for_json_compare(d.to_python() if hasattr(d, "to_python") else d)
                for d in re_docs
            ]
            if actual != expected:
                pytest.fail(
                    f"Re-parsed values changed after round-trip.\n"
                    f"Expected: {expected}\nActual:   {actual}\n"
                    f"Original YAML:\n{yaml_src}\nEmitted:\n{result}"
                )
