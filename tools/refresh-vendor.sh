#!/usr/bin/env bash
# Refresh src/core/{scanner,parser,char_traits,debug}.rs from the upstream
# yaml-rust2 submodule + vendor/yarutsk.patch.
#
# Usage:
#   tools/refresh-vendor.sh              # apply current submodule HEAD
#   tools/refresh-vendor.sh <commit>     # check out <commit> in the submodule first

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

if [[ ! -f "$patch_file" ]]; then
    echo "missing $patch_file" >&2
    exit 1
fi

if [[ $# -ge 1 ]]; then
    pin="$1"
    git -C "$submodule" fetch --tags origin
    git -C "$submodule" checkout "$pin"
fi

upstream_sha="$(git -C "$submodule" rev-parse HEAD)"
echo "upstream pinned at $upstream_sha"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/src"
for f in "${files[@]}"; do
    cp "$submodule/src/$f.rs" "$tmp/src/$f.rs"
done

if ! patch --quiet -d "$tmp" -p1 < "$patch_file"; then
    echo "" >&2
    echo "patch failed — leaving rejects under $tmp for inspection:" >&2
    find "$tmp" -name '*.rej' >&2
    trap - EXIT
    exit 1
fi

for f in "${files[@]}"; do
    cp "$tmp/src/$f.rs" "src/core/$f.rs"
done

echo ""
echo "src/core/ updated. Review the diff:"
git --no-pager diff --stat src/core/
