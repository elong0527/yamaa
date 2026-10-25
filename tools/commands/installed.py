"""Stage and run cataloged supplemental suites through an installed host.

Installation and public benchmark qualification remain separate steps. The owned
supplemental runner verifies the installed distribution, exact source identity
and complete suite denominator; these commands do not qualify a release matrix.
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from tools.conformance import supplemental_inventory as inventory
from tools.paths import ROOT
from tools.staging import stage_r_package

CATALOG = ROOT / "rust/qualification/supplemental.json"


def outside_checkout(path):
    path = path.resolve()
    if path.is_relative_to(ROOT):
        raise ValueError("installed checks must run outside the checkout")
    return path


def stage(runtime, destination):
    destination = outside_checkout(destination)
    catalog = inventory.load_catalog(ROOT, CATALOG)
    copies = [
        (ROOT / suite["path"], destination / suite["staged_path"])
        for suite in catalog["suites"]
        if suite["runtime"] == runtime
    ]
    copies += [
        (ROOT / name, destination / staged_name)
        for name, staged_name in inventory.host_fixture_files(catalog)
    ]
    directories = [destination / "host-fixtures"]
    if runtime == "python":
        directories += [
            destination / name
            for name in (
                "specification-original",
                "project-original",
                "define-fixtures",
                "grammar-original",
            )
        ]
    else:
        # Full R supplemental inputs stay outside the small CRAN archive.
        # The suites read these trees from their working directory.
        directories += [destination / name for name in stage_r_package.TEST_INPUTS]
    for target in [*(target for _, target in copies), *directories]:
        if target.exists():
            raise FileExistsError(f"refusing to overwrite staged checks: {target}")
    destination.mkdir(parents=True, exist_ok=True)
    for source, target in copies:
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    if runtime == "python":
        for tool, name in (
            ("stage_specification_inputs.py", "specification-original"),
            ("stage_project_inputs.py", "project-original"),
            ("stage_define_inputs.py", "define-fixtures"),
            ("stage_grammar_inputs.py", "grammar-original"),
        ):
            subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "tools.staging." + tool.removesuffix(".py"),
                    str(destination / name),
                ],
                check=True,
            )
    else:
        stage_r_package.stage_test_inputs(destination)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    staging = subparsers.add_parser(
        "stage", help="copy cataloged suites into an external directory"
    )
    staging.add_argument("--runtime", choices=("python", "r"), required=True)
    staging.add_argument("--staged-root", type=Path, required=True)
    running = subparsers.add_parser(
        "run", help="run the canonical supplemental inventory tool"
    )
    running.add_argument("--runtime", choices=("python", "r"), required=True)
    running.add_argument("--staged-root", type=Path, required=True)
    running.add_argument("--output", type=Path, required=True)
    running.add_argument("--source-revision", required=True)
    running.add_argument(
        "--package-form", choices=("wheel", "source", "binary"), required=True
    )
    running.add_argument("--artifact-reference", required=True)
    running.add_argument("--host-artifact-reference")
    running.add_argument("--evidence", required=True)
    args = parser.parse_args(argv)
    staged = outside_checkout(args.staged_root)
    if args.command == "stage":
        stage(args.runtime, staged)
        return
    if not args.artifact_reference or not args.evidence:
        parser.error(
            "artifact reference and evidence must identify the actual installed bytes/run"
        )
    if args.runtime == "python" and not args.host_artifact_reference:
        parser.error("Python checks require --host-artifact-reference")
    if args.runtime == "r" and not os.environ.get("R_LIBS"):
        parser.error(
            "R_LIBS must select the installed R package and dependency library"
        )
    output = outside_checkout(args.output)
    if output.exists():
        parser.error("--output must be a fresh directory to preserve earlier evidence")
    command = [
        sys.executable,
        "-I",
        "-m",
        "tools.conformance.supplemental_inventory",
        "--root",
        str(ROOT),
        "--catalog",
        str(CATALOG),
        "--runtime",
        args.runtime,
        "--staged-root",
        str(staged),
        "--output",
        str(output),
        "--source-revision",
        args.source_revision,
        "--package-form",
        args.package_form,
        "--artifact-reference",
        args.artifact_reference,
        "--evidence",
        args.evidence,
    ]
    if args.host_artifact_reference:
        command += ["--host-artifact-reference", args.host_artifact_reference]
    subprocess.run(command, cwd=staged, check=True)


if __name__ == "__main__":
    main()
