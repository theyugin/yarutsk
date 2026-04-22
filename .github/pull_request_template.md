### Summary

<!-- What does this change and why? -->

### Verification

- [ ] `cargo fmt && cargo clippy -- -D warnings`
- [ ] `.venv/bin/maturin develop`
- [ ] `.venv/bin/pytest tests/ --ignore=tests/test_yaml_suite.py`
- [ ] `.venv/bin/pytest tests/test_yaml_suite.py` (if touching scanner/parser/emitter)
- [ ] `.venv/bin/mypy`
- [ ] `.venv/bin/ruff check . && .venv/bin/ruff format --check .`
- [ ] README / `python/yarutsk/__init__.pyi` updated if public API changed

### Notes

<!-- Anything reviewers should know: breaking changes, benchmark deltas, follow-ups. -->
