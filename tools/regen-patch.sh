#!/usr/bin/env bash
# Regenerate vendor/yarutsk.patch from the diff between the in-tree
# src/core/{scanner,parser,char_traits,debug}.rs and the upstream submodule.
#
# Run this after intentionally modifying any of those four files.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

submodule="vendor/yaml-rust2"
patch_file="vendor/yarutsk.patch"
files=(scanner parser char_traits debug)

if [[ ! -d "$submodule/.git" && ! -f "$submodule/.git" ]]; then
    echo "submodule $submodule is not initialised — run:" >&2
    echo "  git submodule update --init $submodule" >&2
    exit 1
fi

{
    for f in "${files[@]}"; do
        diff -u "$submodule/src/$f.rs" "src/core/$f.rs" \
            --label "a/src/$f.rs" --label "b/src/$f.rs" || true
    done
} > "$patch_file"

echo "wrote $patch_file ($(wc -l < "$patch_file") lines)"
