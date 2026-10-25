"""Resolve and test minimum/newest dependencies in an external repository copy."""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

from packaging.requirements import Requirement

from tools.packaging.check_minimum_rust import needs, release
from tools.paths import ROOT

LOCKS = ("rust/Cargo.lock", "python/uv.lock")
# The package notices that tools/packaging/generate_third_party_notices.py
# writes from the canonical Cargo lock. A lane regenerates its copy's notices
# for the graph it resolved and never writes the checkout's.
NOTICES = (
    "python/LICENSE-THIRD-PARTY.txt",
    "R/yamaa/inst/LICENSE-THIRD-PARTY.txt",
    "rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt",
)
IGNORED = {
    ".git",
    ".venv",
    ".venv-docs",
    "target",
    ".cache",
    ".build",
    "build",
    "site",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
}


def minimum_cargo(requirement):
    """Accept the repository's explicit lower bounds; fail on unknown syntax."""
    if not isinstance(requirement, str):
        raise ValueError(f"Cargo requirement needs an explicit minimum: {requirement}")
    match = re.fullmatch(
        r"(?:\^|=|>=)?\s*(\d+\.\d+\.\d+)(?:\s*,\s*<\s*[\d.]+)?", requirement
    )
    if not match:
        raise ValueError(f"Cargo requirement needs an explicit minimum: {requirement}")
    return "=" + match[1]


def pin_manifest(path):
    """Pin registry dependencies, retaining local and vendored path dependencies."""
    document = tomllib.loads(path.read_text())
    replacements = {}
    target = document.get("target", {})
    if not isinstance(target, dict):
        raise ValueError(f"Cargo target in {path} must be a table")
    sections = [document, *target.values()]
    for section in sections:
        if not isinstance(section, dict):
            raise ValueError(f"Cargo target section in {path} must be a table")
        for key in ("dependencies", "build-dependencies", "dev-dependencies"):
            dependencies = section.get(key, {})
            if not isinstance(dependencies, dict):
                raise ValueError(f"Cargo {key} in {path} must be a table")
            for name, dependency in dependencies.items():
                if isinstance(dependency, dict) and "path" in dependency:
                    continue
                if isinstance(dependency, str):
                    version = dependency
                elif isinstance(dependency, dict):
                    version = dependency.get("version")
                    if not isinstance(version, str):
                        raise ValueError(
                            f"Cargo dependency {name} in {path} has no version string"
                        )
                else:
                    raise ValueError(
                        f"Cargo dependency {name} in {path} must be a string or table"
                    )
                replacements[(name, version)] = minimum_cargo(version)
    text = path.read_text()
    for (name, version), minimum in replacements.items():
        # Current dependency entries are single-line strings or inline tables.
        pattern = rf'(?m)^(\s*{re.escape(name)}\s*=\s*(?:\{{[^\n]*?version\s*=\s*)?")({re.escape(version)})(")'
        text, count = re.subn(
            pattern, lambda match: match[1] + minimum + match[3], text
        )
        if not count:
            raise ValueError(f"Cannot pin {name} in {path}")
    path.write_text(text)


def minimum_python(requirement):
    """Pin one requirement to its floor, keeping its extras and markers.

    A package with marker-split floors keeps one pin per entry, so the resolver
    gives each interpreter its own floor.
    """
    if not isinstance(requirement, str):
        raise ValueError(
            f"Python requirement needs one explicit minimum: {requirement}"
        )
    parsed = Requirement(requirement)
    bounds = [
        item.version for item in parsed.specifier if item.operator in (">=", "==")
    ]
    if len(bounds) != 1 or "*" in bounds[0]:
        raise ValueError(
            f"Python requirement needs one explicit minimum: {requirement}"
        )
    parsed.specifier = type(parsed.specifier)("==" + bounds[0])
    return str(parsed)


def prepare(source, destination, graph):
    source, destination = source.resolve(), destination.resolve()
    if destination == source or source in destination.parents:
        raise ValueError("Dependency graphs must be outside the source checkout")
    if destination.exists():
        raise ValueError(f"Graph destination already exists: {destination}")
    # Conformance tooling records the source revision. Give the copy its own Git
    # refs/index; sharing read-only objects does not share checkout mutations.
    subprocess.run(
        [
            "git",
            "clone",
            "--shared",
            "--no-checkout",
            "--quiet",
            str(source),
            str(destination),
        ],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(destination), "reset", "--mixed", "--quiet", "HEAD"],
        check=True,
    )
    # Symbolic links are copied as links, as the checkout holds them: a
    # negative benchmark that FILE-8 refuses for its link must keep it.
    shutil.copytree(
        source,
        destination,
        symlinks=True,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(*IGNORED, "*.egg-info"),
    )
    workspace = tomllib.loads((destination / "rust/Cargo.toml").read_text())
    if graph == "minimum":
        for member in workspace["workspace"]["members"]:
            pin_manifest(destination / "rust" / member / "Cargo.toml")
    (destination / "rust/Cargo.lock").unlink()
    python = tomllib.loads((destination / "python/pyproject.toml").read_text())
    requirements = (
        python["project"]["dependencies"]
        + python["project"]["optional-dependencies"]["test"]
    )
    if graph == "minimum":
        requirements = list(map(minimum_python, requirements))
    # Documentation tools are development-only, already exact in the manifest.
    requirements += [
        item for item in python["dependency-groups"]["tooling"] if item != "yamaa-tools"
    ]
    requirements += python["build-system"]["requires"]
    (destination / "requirements.in").write_text("\n".join(requirements) + "\n")
    return workspace["workspace"]["package"]["rust-version"]


def assert_msrv(metadata, msrv):
    """Reject a resolved package that needs a newer Rust than the workspace.

    A package needs its declared rust-version, or its edition's first Rust when
    that is newer, on any target: a source package vendors every locked crate.
    """
    incompatible = [
        f"{package['name']} {package['version']} (Rust {needs(package)})"
        for package in metadata["packages"]
        if release(needs(package)) > release(msrv)
    ]
    if incompatible:
        raise ValueError(f"Graph exceeds Rust {msrv}: {', '.join(incompatible)}")


def run_graph(source, destination, graph, python, prepare_only=False):
    if not prepare_only:
        missing = [
            command
            for command in ("git", "cargo", "uv", "R", "Rscript")
            if not shutil.which(command)
        ]
        if missing:
            raise RuntimeError(
                "Install the development prerequisites before testing a graph: "
                + ", ".join(missing)
            )
    originals = {path: (source / path).read_bytes() for path in LOCKS + NOTICES}
    try:
        msrv = prepare(source, destination, graph)
        if prepare_only:
            return
        environment = os.environ | {
            "CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS": "fallback"
        }

        def run(*arguments, cwd=destination):
            subprocess.run(arguments, cwd=cwd, env=environment, check=True)

        rust = destination / "rust"
        run("cargo", "generate-lockfile", cwd=rust)
        metadata = json.loads(
            subprocess.check_output(
                [
                    "cargo",
                    "metadata",
                    "--locked",
                    "--all-features",
                    "--format-version",
                    "1",
                ],
                cwd=rust,
                env=environment,
            )
        )
        assert_msrv(metadata, msrv)
        (destination / "cargo-metadata.json").write_text(
            json.dumps(metadata, indent=2) + "\n"
        )
        run("uv", "venv", "--python", python, str(destination / "python/.venv"))
        interpreter = str(
            destination
            / "python/.venv"
            / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        )
        run(
            "uv",
            "pip",
            "compile",
            "requirements.in",
            "--python",
            interpreter,
            "--resolution",
            "lowest-direct" if graph == "minimum" else "highest",
            "--output-file",
            "requirements.txt",
        )
        run("uv", "pip", "install", "--python", interpreter, "-r", "requirements.txt")
        run(
            "uv",
            "pip",
            "install",
            "--python",
            interpreter,
            "--no-deps",
            "--editable",
            str(destination),
            str(destination / "rust/tests/project-programs/python"),
        )
        # The committed notices describe the canonical Cargo lock, which this
        # copy replaced. Regenerate them from the graph that the lane resolved,
        # with the copy's own tools, so that the extension, the staged R package
        # and every notice check below cover the crates that the lane builds.
        # The repository check still compares the committed notices with the
        # canonical lock.
        run(
            interpreter, "-m", "tools.packaging.generate_third_party_notices", "--write"
        )
        run(
            "uv",
            "pip",
            "install",
            "--python",
            interpreter,
            "--no-deps",
            "--no-build-isolation",
            "--config-setting",
            "build-args=--profile=dev",
            str(destination / "python"),
        )
        (destination / "python-installed.txt").write_text(
            subprocess.check_output(
                ["uv", "pip", "freeze", "--python", interpreter],
                cwd=destination,
                env=environment,
                text=True,
            )
        )
        (destination / "r-session.txt").write_text(
            subprocess.check_output(
                [
                    "Rscript",
                    "--vanilla",
                    "-e",
                    'packages <- trimws(sub("[[:space:]]*[(][^)]*[)]", "", strsplit(read.dcf("R/yamaa/DESCRIPTION")[1,"Suggests"], ",", fixed=TRUE)[[1]])); for (package in packages) cat(package, as.character(packageVersion(package)), "\\n"); sessionInfo()',
                ],
                cwd=destination,
                env=environment,
                text=True,
            )
        )
        run(interpreter, "-m", "tools.commands.rust_tests")
        run(
            "cargo",
            "test",
            "--locked",
            "-p",
            "yamaa-adapters",
            "-p",
            "yamaa-application",
            "--no-default-features",
            cwd=rust,
        )
        run(interpreter, "-m", "tools.commands.python_tests")
        run(interpreter, "-m", "tools.commands.r_tests")
    finally:
        for path, original in originals.items():
            if (source / path).read_bytes() != original:
                raise RuntimeError(
                    f"Dependency graph changed the source checkout: {path}"
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph", choices=("minimum", "newest"), required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--prepare-only", action="store_true")
    arguments = parser.parse_args()
    run_graph(
        ROOT,
        arguments.destination,
        arguments.graph,
        arguments.python,
        arguments.prepare_only,
    )


if __name__ == "__main__":
    main()
