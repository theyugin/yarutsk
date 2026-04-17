# Installation

## From PyPI

```bash
pip install yarutsk
```

Requires **Python 3.12+**. `YamlSequence` subclasses `list`, which requires PyO3's `extends = PyList` support introduced in Python 3.12.

Pre-built wheels are published for:

| OS | Architectures | Python |
|---|---|---|
| Linux | `x86_64`, `aarch64` | 3.12, 3.13, 3.14 |
| macOS | `x86_64`, `arm64` | 3.12, 3.13, 3.14 |
| Windows | `x86_64` | 3.12, 3.13, 3.14 |

## From source

Requires **Rust 1.85+** and [uv](https://github.com/astral-sh/uv):

```bash
git clone --recurse-submodules https://github.com/theyugin/yarutsk
cd yarutsk
make setup
```

`make setup` installs all dependency groups and does an initial debug build via maturin. For a release build, use `make build-release`. See [Contributing](contributing.md) for the full development workflow.
