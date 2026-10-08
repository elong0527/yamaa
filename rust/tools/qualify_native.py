#!/usr/bin/env python3
"""Qualify installed Python native routes against independent fixtures and reference.

Run outside the checkout. Both installed package paths are checked against their
package metadata; the execution APIs cannot read expected artifacts. Comparison
uses the existing conformance protocol after each run has finished.
"""

from __future__ import annotations

import argparse
import importlib.metadata
import json
from pathlib import Path

import yamaa
from yamaa import _native as yamaa_native
from pydantic import TypeAdapter
from yamaa.adapters.conformance import execute_example, write_report
from yamaa.adapters.qualification import (
    Backend,
    Batch,
    Host,
    KnownGap,
    Level,
    MissingRoute,
    load_inventory,
    qualify,
)


def installed_artifact(module, distribution_name):
    """Refuse editable/source imports masquerading as installed package evidence."""
    distribution = importlib.metadata.distribution(distribution_name)
    path = Path(module.__file__).resolve()
    files = distribution.files
    if files is None or not any(
        distribution.locate_file(item).resolve() == path for item in files
    ):
        raise RuntimeError(
            f"{distribution_name} was not imported from its installed artifact"
        )
    return distribution.version


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--schema", type=Path, required=True)
    parser.add_argument("--gates", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--reference-artifacts", type=Path, required=True)
    parser.add_argument("--native-artifacts", type=Path, required=True)
    parser.add_argument("--reference-artifact-prefix", required=True)
    parser.add_argument("--native-artifact-prefix", required=True)
    parser.add_argument("--package-form", choices=("wheel", "source"), required=True)
    parser.add_argument("--evidence", required=True)
    args = parser.parse_args(argv)
    if args.output.resolve().is_relative_to(args.fixtures.resolve()):
        parser.error("qualification output must be outside the fixture tree")
    reference_files = tuple(args.reference_artifacts.glob("*.whl"))
    native_files = tuple(
        args.native_artifacts.glob(
            "*.whl" if args.package_form == "wheel" else "*.tar.gz"
        )
    )
    if len(reference_files) != 1 or len(native_files) != 1:
        raise ValueError(
            "qualification requires exactly one artifact of each selected package form"
        )
    reference_artifact = (
        args.reference_artifact_prefix.rstrip("/") + "/" + reference_files[0].name
    )
    native_artifact = (
        args.native_artifact_prefix.rstrip("/") + "/" + native_files[0].name
    )
    host_version = installed_artifact(yamaa, "yamaa")
    native_version = installed_artifact(yamaa_native, "yamaa")
    info = yamaa_native.engine_info()
    required = TypeAdapter(tuple[tuple[str, Host, Backend, Level], ...]).validate_json(
        (args.gates / "required.json").read_text()
    )
    gaps = TypeAdapter(tuple[KnownGap, ...]).validate_json(
        (args.gates / "known-gaps.json").read_text()
    )
    missing = TypeAdapter(tuple[MissingRoute, ...]).validate_json(
        (args.gates / "missing-routes.json").read_text()
    )
    manifest = load_inventory(args.fixtures)
    for name in sorted(manifest.examples):
        for backend in ("python", "rust"):
            report = execute_example(
                args.fixtures / name,
                schema_root=args.schema,
                output_dir=args.output / "artifacts" / backend / name,
                backend=backend,
            )
            write_report(report, args.output / "reports" / backend)
    batches = tuple(
        Batch(
            runtime="python",
            backend=backend,
            level=level,
            source_revision=args.source_revision,
            host_package_version=host_version,
            core_version=core_version,
            binding_package_version=native_version if backend == "rust" else None,
            artifact_reference=artifact,
            evidence=args.evidence,
            reports_dir=str((args.output / "reports" / backend).resolve()),
        )
        for backend, level, core_version, artifact in (
            ("python", "reference_run", None, reference_artifact),
            (
                "rust",
                "reference_assisted_run",
                info["core_version"],
                native_artifact,
            ),
        )
    )
    inventory = qualify(
        args.fixtures,
        args.source_revision,
        batches,
        required=required,
        known_gaps=gaps,
        missing_routes=missing,
    )
    (args.output / "coverage.json").write_text(
        inventory.model_dump_json(indent=2) + "\n", encoding="utf-8"
    )
    for batch in batches:
        (args.output / f"{batch.backend}-batch.json").write_text(
            batch.model_dump_json(indent=2) + "\n", encoding="utf-8"
        )
    print(json.dumps(inventory.summary, indent=2))
    for error in inventory.errors:
        print(error)
    return int(bool(inventory.errors))


if __name__ == "__main__":
    raise SystemExit(main())
