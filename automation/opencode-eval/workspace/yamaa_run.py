"""Run this workspace's yamaa specification and publish what it produces.

The entry is `spec.yaml`, or the one `spec_*.yaml` no other specification
names as a parent or producer. Every run first clears `output/` (keeping
`NOTES.md`), then publishes the artifacts a successful run produces there.
The outcome is printed as one YAML document: the published files on
success, or the engine's diagnostics on failure.
"""

from __future__ import annotations

import shutil
import sys
import tempfile
from pathlib import Path

import yaml

from yamaa.adapters.conformance import ConformanceError, execute_example

HERE = Path(__file__).resolve().parent
SCHEMA_ROOT = HERE / "reference" / "yaml"
OUTPUT = HERE / "output"
KEEP = {"NOTES.md"}


def _clear_output() -> None:
    OUTPUT.mkdir(exist_ok=True)
    for path in OUTPUT.iterdir():
        if path.name in KEEP:
            continue
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()


def main() -> int:
    _clear_output()
    # The engine refuses to publish inside the example it runs, so the run
    # lands in a scratch directory and its artifacts move into `output/`.
    with tempfile.TemporaryDirectory() as scratch:
        try:
            report = execute_example(
                HERE, schema_root=SCHEMA_ROOT, output_dir=Path(scratch)
            )
        except ConformanceError as error:
            print(yaml.safe_dump({"outcome": "error", "error": str(error)}))
            return 2
        published = []
        for artifact in report.artifacts:
            source = next(Path(scratch).glob(f"{artifact.name}.*"))
            shutil.move(source, OUTPUT / source.name)
            published.append(
                {
                    "file": f"output/{source.name}",
                    "records": artifact.row_count,
                    "columns": list(artifact.columns),
                }
            )

    document: dict[str, object] = {"outcome": report.outcome}
    if published:
        document["artifacts"] = published
    if report.diagnostics:
        document["diagnostics"] = [
            {
                "phase": item.phase,
                "condition": item.condition,
                "spec_paths": list(item.spec_paths),
                "requirement": item.requirement,
                "context": item.context,
            }
            for item in report.diagnostics
        ]
    if report.unsupported:
        document["unsupported"] = [item.model_dump() for item in report.unsupported]
    if report.error:
        document["error"] = report.error
    print(yaml.safe_dump(document, sort_keys=False, allow_unicode=True))
    return 0 if report.outcome == "success" else 1


if __name__ == "__main__":
    sys.exit(main())
