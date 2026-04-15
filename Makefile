PYTHON := .venv/bin/python
MATURIN := .venv/bin/maturin
PYTEST  := .venv/bin/pytest
RUFF    := .venv/bin/ruff
MYPY    := .venv/bin/mypy

.PHONY: help setup build build-release dev test test-all test-suite bench bench-compare \
        lint fmt fmt-check typecheck clean clean-all

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Setup"
	@echo "  setup          Install all dependencies and do an initial debug build"
	@echo ""
	@echo "Build"
	@echo "  build          Debug build (fast, no optimisations)"
	@echo "  build-release  Release build (optimised)"
	@echo ""
	@echo "Test"
	@echo "  test           Core tests (excludes yaml-test-suite)"
	@echo "  test-all       All tests including yaml-test-suite compliance"
	@echo "  test-suite     yaml-test-suite compliance tests only"
	@echo "  test-invalid   Invalid/malformed input tests only"
	@echo "  test-roundtrip Round-trip fidelity tests only"
	@echo ""
	@echo "Benchmark"
	@echo "  bench          Run benchmarks"
	@echo "  bench-compare  Run benchmarks with histogram output"
	@echo ""
	@echo "Code quality"
	@echo "  lint           ruff check + cargo clippy"
	@echo "  fmt            Auto-format Python (ruff) and Rust (cargo fmt)"
	@echo "  fmt-check      Check formatting without modifying files"
	@echo "  typecheck      mypy strict type-check on Python stubs"
	@echo ""
	@echo "Clean"
	@echo "  clean          Remove Python caches and build artefacts"
	@echo "  clean-all      Also remove the Rust target directory"

# ── Setup ─────────────────────────────────────────────────────────────────────

setup:
	uv sync --group dev --group benchmark
	$(MATURIN) develop

# ── Build ─────────────────────────────────────────────────────────────────────

build:
	$(MATURIN) develop

build-release:
	$(MATURIN) develop --release

# ── Test ──────────────────────────────────────────────────────────────────────

test:
	$(PYTEST) tests/ --ignore=tests/test_yaml_suite.py -v

test-all: build
	$(PYTEST) tests/ -v

test-suite:
	$(PYTEST) tests/test_yaml_suite.py -q

test-invalid:
	$(PYTEST) tests/test_invalid_input.py -v

test-roundtrip:
	$(PYTEST) tests/test_roundtrip.py -v

# ── Benchmark ─────────────────────────────────────────────────────────────────

bench: build-release
	uv sync --group benchmark
	$(PYTEST) benchmarks/ --benchmark-only -q \
	    --benchmark-group-by=group \
	    --benchmark-sort=name \
	    --override-ini="python_files=bench_*.py"

bench-compare: build-release
	uv sync --group benchmark
	$(PYTEST) benchmarks/ --benchmark-only \
	    --benchmark-group-by=group \
	    --benchmark-histogram=histograms/bench \
	    --benchmark-sort=name \
	    --override-ini="python_files=bench_*.py"

# ── Code quality ──────────────────────────────────────────────────────────────

lint:
	$(RUFF) check .
	cargo clippy

fmt:
	$(RUFF) format .
	cargo fmt

fmt-check:
	$(RUFF) format --check .
	cargo fmt --check

typecheck:
	$(MYPY)

# ── Clean ─────────────────────────────────────────────────────────────────────

clean:
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -type d -name .pytest_cache -exec rm -rf {} +
	find . -type d -name .ruff_cache -exec rm -rf {} +
	find . -name "*.pyc" -delete

clean-all: clean
	cargo clean
