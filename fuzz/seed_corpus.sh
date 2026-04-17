#!/usr/bin/env bash
# Seed fuzz corpora from the yaml-test-suite submodule.
# Run from repo root: ./fuzz/seed_corpus.sh
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ ! -d yaml-test-suite/src ]]; then
    echo "yaml-test-suite submodule not initialised."
    echo "Run: git submodule update --init --recursive"
    exit 1
fi

for target in scanner parser roundtrip; do
    dest="fuzz/corpus/$target"
    mkdir -p "$dest"
    # Every test case has an `in.yaml` file; copy them with a unique name.
    find yaml-test-suite/src -name 'in.yaml' | while read -r f; do
        id=$(dirname "$f" | tr / _)
        cp "$f" "$dest/$id.yaml"
    done
    count=$(find "$dest" -type f | wc -l)
    echo "seeded $count files into $dest"
done
