# Installation

## From PyPI

```bash
pip install yarutsk
```

Requires **Python 3.12+**. `YamlMapping` and `YamlSequence` are standalone
wrappers that implement the usual mapping and sequence protocols; use
`.to_python()` when a plain `dict` or `list` is required.

Pre-built wheels are published for:

| OS | Architectures | Python |
|---|---|---|
| Linux | `x86_64`, `aarch64` | 3.12, 3.13, 3.14, 3.14t |
| macOS (universal2) | `x86_64`, `arm64` | 3.12, 3.13, 3.14, 3.14t |
| Windows | `x86_64`, `arm64` | 3.12, 3.13, 3.14, 3.14t |

The `3.14t` artifacts target free-threaded CPython. They keep the GIL disabled
when yarutsk is imported and follow the sharing rules in [Thread safety](threading.md).

## From source

Requires the **current stable Rust toolchain** and [uv](https://github.com/astral-sh/uv):

```bash
git clone --recurse-submodules https://github.com/theyugin/yarutsk
cd yarutsk
make setup
```

`make setup` installs all dependency groups and does an initial debug build via
maturin. The Rust and Python dependency locks are committed for reproducible
builds. For a release build, use `make build-release`. See
[Contributing](contributing.md) for the full development workflow.
