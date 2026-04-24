"""Shared fixture and helpers for yaml-test-suite-driven tests.

Loads each ``*.yaml`` file under ``yaml-test-suite/src/`` once, exposes the
parsed cases as ``pytest.param`` entries (with skip / xfail marks already
attached), and provides the JSON normalisation used to compare yarutsk output
against the suite's ``json`` field.

The leading underscore in the filename keeps pytest from collecting this as a
test module; both ``test_yaml_suite.py`` and ``test_yaml_suite_format.py``
import from it.
"""

import base64
import datetime
import json as json_mod
import re
from pathlib import Path

import pytest

import yarutsk

SUITE_DIR = Path(__file__).parent.parent / "yaml-test-suite"
SRC_DIR = SUITE_DIR / "src"


def _decode(value: str | None) -> str | None:
    """Translate yaml-test-suite visual encodings back to real characters."""
    if value is None:
        return None
    text = re.sub(r"—*»", "\t", value)
    return (
        text.replace("␣", " ").replace("↵", "").replace("←", "\r").replace("⇔", "﻿").replace("∎", "")
    )


_BASE64_RE = re.compile(r"^[A-Za-z0-9+/\s]*={0,2}\s*$")


def normalize_for_json_compare(value):
    """Coerce yarutsk's tag-aware scalars (``bytes``, ``datetime``, ``date``) into
    a canonical form for comparison with the yaml-test-suite's ``json`` field.

    ``datetime``/``date`` → ISO string. For ``bytes`` and base64-looking strings
    (as the suite's JSON encodes ``!!binary`` payloads), both sides decode to
    ``bytes`` so embedded base64 whitespace from literal-block YAML blocks is
    normalised away.
    """
    if isinstance(value, bytes):
        return value
    if isinstance(value, (datetime.datetime, datetime.date)):
        return value.isoformat()
    if isinstance(value, str) and _BASE64_RE.fullmatch(value) and ("=" in value or "\n" in value):
        try:
            return base64.b64decode(value, validate=False)
        except Exception:
            return value
    if isinstance(value, dict):
        return {k: normalize_for_json_compare(v) for k, v in value.items()}
    if isinstance(value, list):
        return [normalize_for_json_compare(v) for v in value]
    return value


def parse_json_docs(json_str: str) -> list:
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


def load_test_cases() -> list:
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
            skip_reason = test.get("note") or file_skip or "skipped by test-suite metadata"

            marks = []
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
