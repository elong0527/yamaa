"""Generate packaged host styling order from the authoritative schema fields."""

import argparse
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "python/src/yamaa/style/_fields.json"
CLASSES = (
    "root_class",
    "dataset_class",
    "output_class",
    "intermediate_class",
    "column_class",
    "row_class",
)


def render():
    classes = {}
    for source in sorted((ROOT / "yaml").glob("schema*.yaml")):
        if source.name in {"schema_define.yaml", "schema_environment.yaml"}:
            continue
        classes.update(
            (name, value)
            for name, value in yaml.safe_load(source.read_text()).items()
            if isinstance(value, list) and name.endswith(("_class", "_fields"))
        )

    def fields(name, stack=()):
        if name in stack:
            raise ValueError("cyclic fields_from in style order")
        result = []
        for entry in classes[name]:
            if "fields_from" in entry:
                result.extend(fields(entry["fields_from"], (*stack, name)))
            else:
                result.extend(entry)
        return tuple(result)

    return json.dumps({name: fields(name) for name in CLASSES}, indent=2) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    expected = render()
    if args.write:
        TARGET.write_text(expected)
    else:
        assert TARGET.read_text() == expected, (
            "packaged styling order differs from schema"
        )
        print("PASS: packaged styling order preserves schema field order")


if __name__ == "__main__":
    main()
