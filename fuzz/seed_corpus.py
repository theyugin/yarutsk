"""Extract input YAML (including invalid cases) from the pinned test suite."""

import re
from pathlib import Path

import yaml


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    sources = sorted((root / "yaml-test-suite" / "src").glob("*.yaml"))
    if not sources:
        raise SystemExit("No suite fixtures found; initialise the yaml-test-suite submodule")
    seeds = {}
    for source in sources:
        for index, case in enumerate(yaml.safe_load(source.read_text(encoding="utf-8"))):
            value = case.get("yaml", "")
            # Decode the suite's visible whitespace notation, as in tests/_yaml_suite.py.
            value = re.sub(r"—*»", "\t", value)
            for visible, actual in [("␣", " "), ("↵", ""), ("←", "\r"), ("⇔", "\ufeff"), ("∎", "")]:
                value = value.replace(visible, actual)
            seeds[f"suite-{source.stem}-{index}.yaml"] = value.encode("utf-8")
    for target in ("scanner", "parser", "roundtrip", "idempotent_emit"):
        destination = root / "fuzz" / "corpus" / target
        destination.mkdir(parents=True, exist_ok=True)
        for name, data in seeds.items():
            (destination / name).write_bytes(data)
        print(f"seeded {len(seeds)} suite inputs into {destination.relative_to(root)}")


if __name__ == "__main__":
    main()
