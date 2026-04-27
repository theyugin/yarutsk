PYTHON  := .venv/bin/python
MATURIN := .venv/bin/maturin
PYTEST  := .venv/bin/pytest
RUFF    := .venv/bin/ruff
MYPY    := .venv/bin/mypy
MKDOCS  := .venv/bin/mkdocs

.PHONY: help setup build build-release \
        test test-all test-suite test-invalid test-roundtrip \
        bench bench-compare \
        lint lint-fix fmt fmt-check typecheck check \
        audit \
        docs docs-serve docs-build \
        fuzz-seed fuzz-scanner fuzz-parser fuzz-roundtrip \
        vendor-refresh vendor-regen-patch \
        clean clean-all

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Setup"
	@echo "  setup           Install all dependency groups and do an initial debug build"
	@echo ""
	@echo "Build"
	@echo "  build           Debug build via maturin"
	@echo "  build-release   Release build via maturin"
	@echo ""
	@echo "Test"
	@echo "  test            Core Python tests (excludes yaml-test-suite)"
	@echo "  test-all        All Python tests including yaml-test-suite"
	@echo "  test-suite      yaml-test-suite compliance only"
	@echo "  test-invalid    Invalid/malformed input tests only"
	@echo "  test-roundtrip  Round-trip fidelity tests only"
	@echo ""
	@echo "Benchmark"
	@echo "  bench           Run benchmarks"
	@echo "  bench-compare   Benchmarks with histogram output"
	@echo ""
	@echo "Code quality"
	@echo "  lint            ruff check + cargo clippy -D warnings"
	@echo "  lint-fix        ruff --fix (safe autofixes only)"
	@echo "  fmt             Auto-format Python (ruff) and Rust (cargo fmt)"
	@echo "  fmt-check       Check formatting without modifying files"
	@echo "  typecheck       mypy strict type-check on Python stubs"
	@echo "  check           Full local gate: fmt-check + lint + typecheck + test + test-suite"
	@echo ""
	@echo "Audit"
	@echo "  audit           cargo deny check (licences, advisories, bans, sources)"
	@echo ""
	@echo "Docs"
	@echo "  docs            Build mkdocs site into ./site"
	@echo "  docs-serve      Live-preview mkdocs on :8000"
	@echo "  docs-build      Alias for docs"
	@echo ""
	@echo "Fuzz (nightly toolchain)"
	@echo "  fuzz-seed       Populate fuzz/corpus/* from yaml-test-suite"
	@echo "  fuzz-scanner    Fuzz the scanner for 30s"
	@echo "  fuzz-parser     Fuzz the parser for 30s"
	@echo "  fuzz-roundtrip  Fuzz the parse-emit-parse path for 30s"
	@echo ""
	@echo "Vendor (yaml-rust2)"
	@echo "  vendor-refresh        Re-apply vendor/yarutsk.patch onto the submodule and copy into src/core/"
	@echo "  vendor-regen-patch    Regenerate vendor/yarutsk.patch from current src/core/ vs the submodule"
	@echo ""
	@echo "Clean"
	@echo "  clean           Remove Python caches and mkdocs build output"
	@echo "  clean-all       Also remove cargo and fuzz target directories"

setup:
	uv sync --group dev --group benchmark --group docs
	$(MATURIN) develop

build:
	$(MATURIN) develop

build-release:
	$(MATURIN) develop --release

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

bench:
	uv sync --group benchmark
	$(MATURIN) develop --release
	$(PYTEST) benchmarks/ --benchmark-only -q \
	    --benchmark-group-by=group \
	    --benchmark-sort=name \
	    --override-ini="python_files=bench_*.py"

bench-compare:
	uv sync --group benchmark
	$(MATURIN) develop --release
	$(PYTEST) benchmarks/ --benchmark-only \
	    --benchmark-group-by=group \
	    --benchmark-histogram=histograms/bench \
	    --benchmark-sort=name \
	    --override-ini="python_files=bench_*.py"

lint:
	$(RUFF) check .
	cargo clippy --all-targets -- -D warnings

lint-fix:
	$(RUFF) check --fix .

fmt:
	$(RUFF) format .
	cargo fmt

fmt-check:
	$(RUFF) format --check .
	cargo fmt --check

typecheck:
	$(MYPY)

check: fmt-check lint typecheck test test-suite

audit:
	cargo deny check

docs: docs-build

docs-build:
	$(MKDOCS) build --strict

docs-serve:
	$(MKDOCS) serve

fuzz-seed:
	./fuzz/seed_corpus.sh

fuzz-scanner:
	cargo +nightly fuzz run scanner -- -max_total_time=30

fuzz-parser:
	cargo +nightly fuzz run parser -- -max_total_time=30

fuzz-roundtrip:
	cargo +nightly fuzz run roundtrip -- -max_total_time=30

vendor-refresh:
	./tools/refresh-vendor.sh

vendor-regen-patch:
	./tools/regen-patch.sh

clean:
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -type d -name .pytest_cache -exec rm -rf {} +
	find . -type d -name .ruff_cache -exec rm -rf {} +
	find . -name "*.pyc" -delete
	rm -rf site

clean-all: clean
	cargo clean
	rm -rf fuzz/target fuzz/corpus fuzz/artifacts
