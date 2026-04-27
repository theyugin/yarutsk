# Vendoring yaml-rust2

`src/core/{scanner,parser,char_traits,debug}.rs` are derived from
[yaml-rust2](https://github.com/Ethiraric/yaml-rust2). The upstream source
lives in this directory as a git submodule (`vendor/yaml-rust2`) pinned to a
specific commit. Our modifications are kept as a unified diff in
[`yarutsk.patch`](yarutsk.patch).

The build does **not** apply the patch at compile time. The four files in
`src/core/` are checked in and built directly. The submodule + patch exist
so we can mechanically refresh from upstream and so it's always clear what
we changed.

## Initial checkout

```bash
git submodule update --init vendor/yaml-rust2
```

## Refreshing from upstream

```bash
# stay on the current pin (re-apply the patch verbatim)
tools/refresh-vendor.sh

# move to a new upstream commit/tag
tools/refresh-vendor.sh v0.12.0
```

The script copies upstream `src/{scanner,parser,char_traits,debug}.rs`,
applies `vendor/yarutsk.patch`, and writes the result back to `src/core/`.
On a patch reject it leaves `.rej` files in a tempdir and exits non-zero —
resolve manually, then run `tools/regen-patch.sh` to capture the new diff.

If the move succeeded, commit the new submodule SHA together with any
resulting changes to `src/core/`.

## Regenerating the patch

After **intentionally** modifying any of the four vendored files, regenerate
the patch so the submodule + patch combination still produces what's on
disk:

```bash
tools/regen-patch.sh
```

Commit the `src/core/` change and the `vendor/yarutsk.patch` update
together.

## Sending fixes upstream

If a fix in our copy is also a bug in upstream:

1. Open a PR against `Ethiraric/yaml-rust2`.
2. When it merges and a release lands, run `tools/refresh-vendor.sh <tag>`.
3. Run `tools/regen-patch.sh` — the patch should shrink (the fix is now
   upstream).
4. Commit the new submodule SHA, the refreshed `src/core/` files, and the
   smaller patch in one PR.
