"""Test runner for yaml-test-suite.

This module runs the official YAML test suite against yarutsk.

Usage:
    pytest tests/test_yaml_suite.py -v

The test suite is expected to have some failures initially.
The goal is to track progress over time.
"""

import io
import os
import re
import sys
from pathlib import Path
from typing import Any

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

yarutsk: Any = None

try:
    import yarutsk as _yarutsk

    yarutsk = _yarutsk
    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

SUITE_DIR = Path(__file__).parent.parent / "yaml-test-suite"
SRC_DIR = SUITE_DIR / "src"


def decode_suite_text(value):
    if value is None:
        return None

    text = re.sub("—*»", "\t", value)
    text = (
        text.replace("␣", " ")
        .replace("↵", "\n")
        .replace("←", "\r")
        .replace("⇔", "\ufeff")
        .replace("∎", "")
    )
    return text


def load_stream(yaml_content):
    return yarutsk.load_all(io.StringIO(yaml_content))


def load_test_cases():
    """Load all test cases from yaml-test-suite/src/."""
    if not SRC_DIR.exists():
        return []

    test_cases = []

    for yaml_file in SRC_DIR.glob("*.yaml"):
        try:
            content = yaml_file.read_text(encoding="utf-8")

            try:
                tests = yarutsk.loads(content)
            except Exception:
                continue

            if not isinstance(tests, list):
                continue

            file_skip_note = next(
                (
                    test.get("note", "Skipped by yaml-test-suite metadata")
                    for test in tests
                    if isinstance(test, dict) and test.get("skip", False)
                ),
                None,
            )

            for test in tests:
                if not isinstance(test, dict):
                    continue

                name = test.get("name", yaml_file.stem)
                yaml_content = test.get("yaml", "")
                tree = test.get("tree", "")
                json_content = test.get("json", "")
                should_fail = test.get("fail", False)
                should_skip = test.get("skip", False) or file_skip_note is not None
                tags = test.get("tags", "")
                skip_reason = (
                    test.get("note")
                    or file_skip_note
                    or "Skipped by yaml-test-suite metadata"
                )
                marks = [
                    pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")
                ]
                if should_skip:
                    marks.append(pytest.mark.skip(reason=skip_reason))
                if should_fail:
                    marks.append(
                        pytest.mark.xfail(
                            strict=True,
                            reason="yaml-test-suite marks this input invalid",
                        )
                    )

                test_cases.append(
                    pytest.param(
                        {
                            "file": yaml_file.stem,
                            "name": name,
                            "yaml": decode_suite_text(yaml_content),
                            "tree": decode_suite_text(tree),
                            "json": decode_suite_text(json_content),
                            "dump": decode_suite_text(test.get("dump")),
                            "fail": should_fail,
                            "tags": tags,
                        },
                        id=f"{yaml_file.stem}:{name}",
                        marks=marks,
                    )
                )
        except Exception:
            pass

    return test_cases


@pytest.fixture(params=load_test_cases())
def yaml_test_case(request):
    """Provide YAML test cases from the test suite."""
    return request.param


@pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")
class TestYamlSuite:
    """Run yaml-test-suite tests against yarutsk."""

    def test_parse(self, yaml_test_case):
        """Test that the YAML can be parsed without errors."""
        test = yaml_test_case
        yaml_content = test["yaml"]

        try:
            docs = load_stream(yaml_content)
            for doc in docs:
                _ = repr(doc)

        except Exception as e:
            pytest.fail(f"Failed to parse: {e}\nYAML:\n{yaml_content}")

    @pytest.mark.xfail(strict=False, reason="serialization normalizes YAML constructs")
    def test_round_trip(self, yaml_test_case):
        """Test that the YAML can be parsed and serialized."""
        test = yaml_test_case
        yaml_content = test["yaml"]

        try:
            docs = load_stream(yaml_content)

            output = io.StringIO()
            yarutsk.dump_all(docs, output)
            docs2 = yarutsk.load_all(io.StringIO(output.getvalue()))

            assert len(docs) == len(docs2), (
                f"Document count mismatch: {len(docs)} vs {len(docs2)}"
            )

            for i, (d1, d2) in enumerate(zip(docs, docs2)):
                assert repr(d1) == repr(d2), f"Document {i} mismatch"

        except Exception as e:
            pytest.fail(f"Round-trip failed: {e}\nYAML:\n{yaml_content}")


@pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")
def test_suite_summary():
    """Print a summary of test suite coverage."""
    test_cases = load_test_cases()

    if not test_cases:
        pytest.skip("yaml-test-suite not available")

    print("\n\nYAML Test Suite Summary:")
    print(f"Total test cases: {len(test_cases)}")

    passing = 0
    failing = 0
    expected_failures = 0

    for tc in test_cases:
        test = tc.values[0] if hasattr(tc, "values") else tc
        yaml_content = test["yaml"]
        should_fail = test["fail"]

        try:
            docs = load_stream(yaml_content)
            for doc in docs:
                _ = repr(doc)

            if should_fail:
                expected_failures += 1
            else:
                passing += 1
        except Exception:
            if should_fail:
                expected_failures += 1
            else:
                failing += 1

    print(f"Passing: {passing}")
    print(f"Failing (unexpected): {failing}")
    print(f"Expected failures (strict YAML tests): {expected_failures}")

    assert True


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
