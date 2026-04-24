"""Format-then-roundtrip tests against the official yaml-test-suite.

Pipeline: ``loads`` → ``.format()`` → ``dumps`` → ``loads`` again.

``.format()`` strips every cosmetic field (quoting, comments, blank lines,
flow vs. block) and rewrites scalars to canonical plain/literal form. The
emitter then has to re-quote anything risky so the second ``loads`` sees the
same Python values as the first.

For each suite case with a ``json`` field this test asserts:
  1. The emitted (post-format) YAML is re-parseable.
  2. Re-parsed values match the suite's expected JSON representation.

Cases without a ``json`` field, or marked ``fail``/``skip``, are skipped —
data preservation can't be verified without an oracle.
"""

import io

import pytest
from _yaml_suite import load_test_cases, normalize_for_json_compare, parse_json_docs

import yarutsk


@pytest.fixture(params=load_test_cases())
def yaml_test_case(request):
    return request.param


class TestYamlSuiteFormat:
    def test_format_roundtrip(self, yaml_test_case):
        test = yaml_test_case

        if test["fail"]:
            pytest.skip("invalid YAML — no round-trip expected")

        if not test["json"]:
            pytest.skip("no json field — can't verify data preservation")

        yaml_src = test["yaml"]
        try:
            docs = list(yarutsk.load_all(io.StringIO(yaml_src)))
        except Exception as e:
            pytest.skip(f"parse failed (covered by test_yaml_suite.test_parse): {e}")

        for doc in docs:
            if hasattr(doc, "format"):
                doc.format()

        result = yarutsk.dumps_all(docs)

        try:
            re_docs = list(yarutsk.load_all(io.StringIO(result)))
        except Exception as e:
            pytest.fail(
                f"Emitter produced invalid YAML after format(): {e}\n"
                f"Original:\n{yaml_src}\nEmitted:\n{result}"
            )

        expected = [normalize_for_json_compare(d) for d in parse_json_docs(test["json"])]
        actual = [
            normalize_for_json_compare(d.to_python() if hasattr(d, "to_python") else d)
            for d in re_docs
        ]
        if actual != expected:
            pytest.fail(
                f"Re-parsed values changed after format() round-trip.\n"
                f"Expected: {expected}\nActual:   {actual}\n"
                f"Original YAML:\n{yaml_src}\nEmitted:\n{result}"
            )
