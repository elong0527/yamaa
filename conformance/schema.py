"""Render or check the committed JSON Schema for adapter implementers."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from pydantic.json_schema import models_json_schema

from conformance.models import ComparisonSummary, Invocation, Report

SCHEMA_PATH = Path(__file__).with_name("protocol.schema.json")


def protocol_schema() -> dict[str, object]:
    """Return one schema bundle with named invocation and report definitions."""

    _, schema = models_json_schema(
        [
            (Invocation, "validation"),
            (Report, "validation"),
            (ComparisonSummary, "validation"),
        ],
        title="YAMAA conformance protocol 1.0",
    )
    schema["$schema"] = "https://json-schema.org/draft/2020-12/schema"
    schema["$id"] = "urn:yamaa:conformance:protocol:1.0"
    schema["oneOf"] = [
        {"$ref": "#/$defs/Invocation"},
        {"$ref": "#/$defs/Report"},
        {"$ref": "#/$defs/ComparisonSummary"},
    ]
    return schema


def rendered_schema() -> str:
    return json.dumps(protocol_schema(), indent=2, sort_keys=True) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true")
    action.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    rendered = rendered_schema()
    if args.write:
        SCHEMA_PATH.write_text(rendered, encoding="utf-8")
        return 0
    if not SCHEMA_PATH.is_file() or SCHEMA_PATH.read_text(encoding="utf-8") != rendered:
        print("conformance/protocol.schema.json is stale; run schema --write")
        return 1
    print("conformance protocol schema is current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
