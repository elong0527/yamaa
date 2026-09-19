"""Dump inferred cross-dataset join keys with the OLD yamaa engine.

Run BEFORE any #568 code change. Output: join_keys.json with, per benchmark
spec, the ResolvedJoins the old planner inferred plus the declared
record_lookups and mapping_from usages. The migration script consumes this.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, "/home/hatch/workspace/wt-lookup/python/src")
import yamaa  # noqa: E402

assert "wt-lookup" in yamaa.__file__, yamaa.__file__

from yamaa.expressions.dispatch import DEFAULT_EXPRESSION_OPERATIONS  # noqa: E402
from yamaa.io.project import ProjectResources  # noqa: E402
from yamaa.io.source import load_source_tables  # noqa: E402
from yamaa.planning import plan_execution  # noqa: E402
from yamaa.specification import load_specification  # noqa: E402

REPO = Path("/home/hatch/workspace/wt-lookup")
SCHEMA_ROOT = REPO / "yaml"


def main() -> None:
    out: dict[str, object] = {}
    failures: list[str] = []
    for spec_path in sorted((REPO / "benchmark").glob("*/spec.yaml")):
        name = spec_path.parent.name
        if name.startswith("negative-"):
            continue
        try:
            specification = load_specification(spec_path, SCHEMA_ROOT).specification
            resources = ProjectResources(spec_path.parent)
            sources = load_source_tables(specification.input, resources)
            plan = plan_execution(
                specification,
                sources,
                supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
            )
        except Exception as error:  # noqa: BLE001
            failures.append(f"{name}: {type(error).__name__}: {error}")
            continue
        joins = [
            {
                "spec_path": join.spec_path,
                "dataset": join.dataset,
                "keys": list(join.keys),
                "declared_grain": join.declared_grain,
            }
            for join in plan.resolved_joins
        ]
        lookups = [
            {
                "id": lookup.id,
                "dataset": lookup.dataset,
                "source": lookup.source,
                "key": lookup.key,
                "unmatched": lookup.unmatched,
                "incomplete": lookup.incomplete,
                "between": lookup.between is not None,
                "order_by": lookup.order_by is not None,
                "keep": lookup.keep,
                "filter": lookup.filter,
            }
            for lookup in specification.record_lookups or ()
        ]
        out[name] = {
            "keys": list(specification.keys),
            "joins": joins,
            "record_lookups": lookups,
        }
    (REPO / ".migration-568").mkdir(exist_ok=True)
    with open(REPO / ".migration-568" / "join_keys.json", "w") as handle:
        json.dump(out, handle, indent=1, sort_keys=True)
    print(f"specs: {len(out)}, failures: {len(failures)}")
    for failure in failures:
        print("FAIL:", failure)


if __name__ == "__main__":
    main()
