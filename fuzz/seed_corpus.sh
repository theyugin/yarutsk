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

# Requires PyYAML (included in the project's dev dependencies).
exec "${PYTHON:-python3}" fuzz/seed_corpus.py
