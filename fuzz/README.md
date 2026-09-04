# Fuzzing

Install `cargo-fuzz` and a nightly Rust toolchain, then seed from the pinned
`yaml-test-suite` submodule (PyYAML is included in the dev environment):

```sh
PYTHON=.venv/bin/python ./fuzz/seed_corpus.sh
cargo +nightly fuzz run idempotent_emit --target x86_64-unknown-linux-gnu -- -max_total_time=60
```

The four targets are `scanner`, `parser`, `roundtrip`, and `idempotent_emit`.
All receive the suite's decoded input YAML, including invalid cases. Parser
errors on original inputs are expected; errors parsing emitted YAML are failures.
The round-trip target checks document counts, and the idempotence target also
requires byte-identical output after another parse/emit cycle. These properties
do not establish full semantic equality. Parser loops have a 10,000-event limit.

Crashes are written under `fuzz/artifacts/<target>/`. Replay a saved input with:

```sh
cargo +nightly fuzz run idempotent_emit fuzz/artifacts/idempotent_emit/<filename> --target x86_64-unknown-linux-gnu
```

Use `cargo +nightly fuzz tmin <target> <filename>` to reduce a failing input,
then add a regression test before fixing it. Corpus and artifact directories
are ignored by git; regression tests preserve the cases in normal CI.
