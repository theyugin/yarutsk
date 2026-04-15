"""YAML library benchmarks: yarutsk vs PyYAML vs ruamel.yaml.

Run with:
    pytest benchmarks/ -v --benchmark-sort=name

Or for per-group histograms:
    make bench-compare

Groups (one histogram each):
    load / small     load / medium     load / large
    dump / small     dump / medium     dump / large
    roundtrip / small  roundtrip / medium  roundtrip / large

Each group contains one entry per library:
    yarutsk, pyyaml, ruamel_safe, ruamel_rt

Libraries that are not installed are skipped automatically.
"""

import io

import pytest

# ── yarutsk (always present — the project under test) ─────────────────────────
import yarutsk

# ── Optional competitors ───────────────────────────────────────────────────────
yaml = pytest.importorskip("yaml", reason="pyyaml not installed")
ruamel_yaml_mod = pytest.importorskip("ruamel.yaml", reason="ruamel.yaml not installed")
YAML = ruamel_yaml_mod.YAML

# Build ruamel instances once; construction cost is not part of the benchmark.
_ruamel_safe = YAML(typ="safe")
_ruamel_rt = YAML(typ="rt")

# ── YAML fixtures ──────────────────────────────────────────────────────────────

SMALL = """\
name: Alice
age: 30
active: true
score: 9.8
ratio: 0.5
level: 3
city: Springfield
tags:
  - python
  - rust
  - yaml
"""

MEDIUM = """\
# CI/CD pipeline configuration
project: my-service
version: "1.4.2"

stages:
  - build
  - test
  - deploy

variables:
  PYTHON_VERSION: "3.12"
  CACHE_DIR: .cache/pip
  COVERAGE_THRESHOLD: 85

build:
  image: python:3.12-slim
  stage: build
  script:
    - pip install --upgrade pip
    - pip install -e .[dev]
    - maturin develop
  cache:
    key: build-${CI_COMMIT_REF_SLUG}
    paths:
      - .cache/pip
      - target/
  artifacts:
    paths:
      - dist/
    expire_in: 1 hour

test:
  image: python:3.12-slim
  stage: test
  needs: [build]
  script:
    - pytest tests/ -v --tb=short
    - pytest tests/test_yaml_suite.py -q
  coverage: '/TOTAL.*\\s+(\\d+%)$/'
  artifacts:
    reports:
      coverage: coverage.xml
    when: always
    expire_in: 7 days

deploy:
  image: python:3.12-slim
  stage: deploy
  needs: [test]
  only:
    - main
    - tags
  script:
    - pip install twine
    - twine upload dist/*
  environment:
    name: production
    url: https://pypi.org/project/my-service/
"""

# 200 person records — ~16 KB
_LARGE_LINES = ["people:"]
_NAMES = [
    "Alice",
    "Bob",
    "Carol",
    "Dave",
    "Eve",
    "Frank",
    "Grace",
    "Hank",
    "Iris",
    "Jack",
    "Karen",
    "Leo",
    "Mia",
    "Ned",
    "Olivia",
    "Pat",
    "Quinn",
    "Rita",
    "Sam",
    "Tara",
]
_CITIES = [
    "New York",
    "Los Angeles",
    "Chicago",
    "Houston",
    "Phoenix",
    "Philadelphia",
    "San Antonio",
    "San Diego",
    "Dallas",
    "Austin",
]
for _i in range(200):
    _n = _NAMES[_i % len(_NAMES)]
    _suffix = str(_i // len(_NAMES)) if _i >= len(_NAMES) else ""
    _age = 20 + (_i * 7 + 13) % 50
    _city = _CITIES[_i % len(_CITIES)]
    _active = "true" if _i % 3 != 0 else "false"
    _score = round(5.0 + (_i * 0.37) % 5.0, 2)
    _LARGE_LINES += [
        f"  - name: {_n}{_suffix}",
        f"    age: {_age}",
        f"    city: {_city}",
        f"    active: {_active}",
        f"    score: {_score}",
    ]
LARGE = "\n".join(_LARGE_LINES) + "\n"
del _LARGE_LINES, _NAMES, _CITIES, _i, _n, _suffix, _age, _city, _active, _score


# ── Helpers ────────────────────────────────────────────────────────────────────


def _ruamel_load(instance, text: str):
    return instance.load(io.StringIO(text))


def _ruamel_dump(instance, obj) -> str:
    buf = io.StringIO()
    instance.dump(obj, buf)
    return buf.getvalue()


# ── Pre-parsed objects for dump benchmarks ────────────────────────────────────

_obj_small_yarutsk = yarutsk.loads(SMALL)
_obj_medium_yarutsk = yarutsk.loads(MEDIUM)
_obj_large_yarutsk = yarutsk.loads(LARGE)

_obj_small_pyyaml = yaml.safe_load(SMALL)
_obj_medium_pyyaml = yaml.safe_load(MEDIUM)
_obj_large_pyyaml = yaml.safe_load(LARGE)

_obj_small_ruamel_safe = _ruamel_safe.load(io.StringIO(SMALL))
_obj_medium_ruamel_safe = _ruamel_safe.load(io.StringIO(MEDIUM))
_obj_large_ruamel_safe = _ruamel_safe.load(io.StringIO(LARGE))

_obj_small_ruamel_rt = _ruamel_rt.load(io.StringIO(SMALL))
_obj_medium_ruamel_rt = _ruamel_rt.load(io.StringIO(MEDIUM))
_obj_large_ruamel_rt = _ruamel_rt.load(io.StringIO(LARGE))

# ── Round-trip helpers ────────────────────────────────────────────────────────


def _rt_yarutsk(text: str) -> str:
    return yarutsk.dumps(yarutsk.loads(text))


def _rt_pyyaml(text: str) -> str:
    return yaml.dump(yaml.safe_load(text))


def _rt_ruamel_safe(text: str) -> str:
    return _ruamel_dump(_ruamel_safe, _ruamel_load(_ruamel_safe, text))


def _rt_ruamel_rt(text: str) -> str:
    return _ruamel_dump(_ruamel_rt, _ruamel_load(_ruamel_rt, text))


# ══════════════════════════════════════════════════════════════════════════════
# LOAD — small
# ══════════════════════════════════════════════════════════════════════════════


def test_load_small_yarutsk(benchmark):
    benchmark.group = "load / small"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.loads, SMALL)


def test_load_small_pyyaml(benchmark):
    benchmark.group = "load / small"
    benchmark.name = "pyyaml"
    benchmark(yaml.safe_load, SMALL)


def test_load_small_ruamel_safe(benchmark):
    benchmark.group = "load / small"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_load, _ruamel_safe, SMALL)


def test_load_small_ruamel_rt(benchmark):
    benchmark.group = "load / small"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_load, _ruamel_rt, SMALL)


# ══════════════════════════════════════════════════════════════════════════════
# LOAD — medium
# ══════════════════════════════════════════════════════════════════════════════


def test_load_medium_yarutsk(benchmark):
    benchmark.group = "load / medium"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.loads, MEDIUM)


def test_load_medium_pyyaml(benchmark):
    benchmark.group = "load / medium"
    benchmark.name = "pyyaml"
    benchmark(yaml.safe_load, MEDIUM)


def test_load_medium_ruamel_safe(benchmark):
    benchmark.group = "load / medium"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_load, _ruamel_safe, MEDIUM)


def test_load_medium_ruamel_rt(benchmark):
    benchmark.group = "load / medium"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_load, _ruamel_rt, MEDIUM)


# ══════════════════════════════════════════════════════════════════════════════
# LOAD — large
# ══════════════════════════════════════════════════════════════════════════════


def test_load_large_yarutsk(benchmark):
    benchmark.group = "load / large"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.loads, LARGE)


def test_load_large_pyyaml(benchmark):
    benchmark.group = "load / large"
    benchmark.name = "pyyaml"
    benchmark(yaml.safe_load, LARGE)


def test_load_large_ruamel_safe(benchmark):
    benchmark.group = "load / large"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_load, _ruamel_safe, LARGE)


def test_load_large_ruamel_rt(benchmark):
    benchmark.group = "load / large"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_load, _ruamel_rt, LARGE)


# ══════════════════════════════════════════════════════════════════════════════
# DUMP — small
# ══════════════════════════════════════════════════════════════════════════════


def test_dump_small_yarutsk(benchmark):
    benchmark.group = "dump / small"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.dumps, _obj_small_yarutsk)


def test_dump_small_pyyaml(benchmark):
    benchmark.group = "dump / small"
    benchmark.name = "pyyaml"
    benchmark(yaml.dump, _obj_small_pyyaml)


def test_dump_small_ruamel_safe(benchmark):
    benchmark.group = "dump / small"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_dump, _ruamel_safe, _obj_small_ruamel_safe)


def test_dump_small_ruamel_rt(benchmark):
    benchmark.group = "dump / small"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_dump, _ruamel_rt, _obj_small_ruamel_rt)


# ══════════════════════════════════════════════════════════════════════════════
# DUMP — medium
# ══════════════════════════════════════════════════════════════════════════════


def test_dump_medium_yarutsk(benchmark):
    benchmark.group = "dump / medium"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.dumps, _obj_medium_yarutsk)


def test_dump_medium_pyyaml(benchmark):
    benchmark.group = "dump / medium"
    benchmark.name = "pyyaml"
    benchmark(yaml.dump, _obj_medium_pyyaml)


def test_dump_medium_ruamel_safe(benchmark):
    benchmark.group = "dump / medium"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_dump, _ruamel_safe, _obj_medium_ruamel_safe)


def test_dump_medium_ruamel_rt(benchmark):
    benchmark.group = "dump / medium"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_dump, _ruamel_rt, _obj_medium_ruamel_rt)


# ══════════════════════════════════════════════════════════════════════════════
# DUMP — large
# ══════════════════════════════════════════════════════════════════════════════


def test_dump_large_yarutsk(benchmark):
    benchmark.group = "dump / large"
    benchmark.name = "yarutsk"
    benchmark(yarutsk.dumps, _obj_large_yarutsk)


def test_dump_large_pyyaml(benchmark):
    benchmark.group = "dump / large"
    benchmark.name = "pyyaml"
    benchmark(yaml.dump, _obj_large_pyyaml)


def test_dump_large_ruamel_safe(benchmark):
    benchmark.group = "dump / large"
    benchmark.name = "ruamel_safe"
    benchmark(_ruamel_dump, _ruamel_safe, _obj_large_ruamel_safe)


def test_dump_large_ruamel_rt(benchmark):
    benchmark.group = "dump / large"
    benchmark.name = "ruamel_rt"
    benchmark(_ruamel_dump, _ruamel_rt, _obj_large_ruamel_rt)


# ══════════════════════════════════════════════════════════════════════════════
# ROUND-TRIP — small
# ══════════════════════════════════════════════════════════════════════════════


def test_roundtrip_small_yarutsk(benchmark):
    benchmark.group = "roundtrip / small"
    benchmark.name = "yarutsk"
    benchmark(_rt_yarutsk, SMALL)


def test_roundtrip_small_pyyaml(benchmark):
    benchmark.group = "roundtrip / small"
    benchmark.name = "pyyaml"
    benchmark(_rt_pyyaml, SMALL)


def test_roundtrip_small_ruamel_safe(benchmark):
    benchmark.group = "roundtrip / small"
    benchmark.name = "ruamel_safe"
    benchmark(_rt_ruamel_safe, SMALL)


def test_roundtrip_small_ruamel_rt(benchmark):
    benchmark.group = "roundtrip / small"
    benchmark.name = "ruamel_rt"
    benchmark(_rt_ruamel_rt, SMALL)


# ══════════════════════════════════════════════════════════════════════════════
# ROUND-TRIP — medium
# ══════════════════════════════════════════════════════════════════════════════


def test_roundtrip_medium_yarutsk(benchmark):
    benchmark.group = "roundtrip / medium"
    benchmark.name = "yarutsk"
    benchmark(_rt_yarutsk, MEDIUM)


def test_roundtrip_medium_pyyaml(benchmark):
    benchmark.group = "roundtrip / medium"
    benchmark.name = "pyyaml"
    benchmark(_rt_pyyaml, MEDIUM)


def test_roundtrip_medium_ruamel_safe(benchmark):
    benchmark.group = "roundtrip / medium"
    benchmark.name = "ruamel_safe"
    benchmark(_rt_ruamel_safe, MEDIUM)


def test_roundtrip_medium_ruamel_rt(benchmark):
    benchmark.group = "roundtrip / medium"
    benchmark.name = "ruamel_rt"
    benchmark(_rt_ruamel_rt, MEDIUM)


# ══════════════════════════════════════════════════════════════════════════════
# ROUND-TRIP — large
# ══════════════════════════════════════════════════════════════════════════════


def test_roundtrip_large_yarutsk(benchmark):
    benchmark.group = "roundtrip / large"
    benchmark.name = "yarutsk"
    benchmark(_rt_yarutsk, LARGE)


def test_roundtrip_large_pyyaml(benchmark):
    benchmark.group = "roundtrip / large"
    benchmark.name = "pyyaml"
    benchmark(_rt_pyyaml, LARGE)


def test_roundtrip_large_ruamel_safe(benchmark):
    benchmark.group = "roundtrip / large"
    benchmark.name = "ruamel_safe"
    benchmark(_rt_ruamel_safe, LARGE)


def test_roundtrip_large_ruamel_rt(benchmark):
    benchmark.group = "roundtrip / large"
    benchmark.name = "ruamel_rt"
    benchmark(_rt_ruamel_rt, LARGE)
