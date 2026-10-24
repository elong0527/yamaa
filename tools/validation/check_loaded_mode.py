"""Check that no shipped build enables the test-only loaded mode.

A binding built with its `dynamic-engine` feature links no engine and loads
the engine library from beside its own module at runtime, and the engine
crate built with the same feature exports the entry that the loader looks
up, which the boundary's `loader` feature compiles (rust/interface/README.md).
That mode serves tests alone, and YAMAA_ENGINE selects it for the
repository's host checks (tools/commands/loaded.py). Every shipped package
links its engine, so neither the wheel nor the R library loads a library at
runtime or exports a new symbol. This check refuses either name in the files
that build or publish a shipped package: the Python package's pyproject.toml,
the R package's build files, the release workflows and the staging and
build tools that they run. The native workflow builds every package that a
release publishes, so it refuses either name there too, except in the job
that checks the loaded mode. Cargo builds a crate with its default features
unless told otherwise, so it also refuses a default feature of the four
crates of the boundary that enables the mode, and requires both bindings to
link their engine by default.
"""

import argparse
import json
import sys
import tomllib
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

NAMES = ("dynamic-engine", "YAMAA_ENGINE")
SHIPPED = (
    "python/pyproject.toml",
    "R/yamaa/DESCRIPTION",
    "R/yamaa/configure",
    "R/yamaa/configure.win",
    "R/yamaa/cleanup",
    "R/yamaa/cleanup.win",
    "R/yamaa/src/Makevars.in",
    "R/yamaa/src/Makevars.win.in",
    "R/yamaa/tools/configure.R",
    ".github/workflows/release.yml",
    ".github/workflows/release-rehearsal.yml",
    ".github/workflows/release-compatibility.yml",
    ".github/workflows/pypi-install.yml",
    "tools/ci/manylinux.py",
    "tools/staging/engine_bundle.py",
    "tools/staging/stage_r_package.py",
    "tools/staging/vendor_crates.py",
)
# The workflow that builds every package a release publishes, and its one job
# that checks the loaded mode.
NATIVE = ".github/workflows/native.yml"
LOADED_JOB = "loaded"
# The features of the loaded mode: the bindings' and the engine's
# `dynamic-engine`, and the boundary's `loader`.
FEATURES = ("dynamic-engine", "loader")
# The four crates of the boundary, each with the default features it must
# have, or None where only the loaded mode's features are refused.
MANIFESTS = {
    "rust/crates/yamaa-boundary/Cargo.toml": None,
    "rust/crates/yamaa-engine/Cargo.toml": None,
    "rust/crates/yamaa-python/Cargo.toml": ["linked-engine"],
    "rust/crates/yamaa-r/Cargo.toml": ["linked-engine"],
}
REFUSED = "of the test-only loaded mode, which no shipped build enables"


def strings(value):
    """Every text in a parsed YAML value, its mapping keys too."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for key, item in value.items():
            yield from strings(key)
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


def named(value):
    """The names of the loaded mode that a parsed YAML value holds."""
    return sorted({name for text in strings(value) for name in NAMES if name in text})


def check_files(root):
    """Each line of a shipped build's files that names the loaded mode, and
    each of those files that is missing."""
    errors = []
    for relative in SHIPPED:
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative} is missing")
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, start=1):
            for name in NAMES:
                if name in line:
                    errors.append(f"{relative}:{number}: names {name}, {REFUSED}")
    return errors


def check_native(root):
    """Each part of the native workflow, but its loaded job, that names the
    loaded mode: a key of the workflow, such as its env, or a job."""
    path = root / NATIVE
    if not path.is_file():
        return [f"{NATIVE} is missing"]
    workflow = yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)
    parts = [(f"its {key}", value) for key, value in workflow.items() if key != "jobs"]
    parts += [
        (f"its {job} job", definition)
        for job, definition in workflow.get("jobs", {}).items()
        if job != LOADED_JOB
    ]
    return [
        f"{NATIVE}: {part} names {name}, {REFUSED}"
        for part, value in parts
        for name in named(value)
    ]


def enabled(features):
    """Every feature that `default` enables, through the manifest's own
    features."""
    found, pending = set(), list(features.get("default", []))
    while pending:
        feature = pending.pop()
        if feature not in found:
            found.add(feature)
            pending.extend(features.get(feature, []))
    return found


def check_manifests(root):
    """Each default feature of the boundary's crates that enables the loaded
    mode, and each binding that does not link its engine by default."""
    errors = []
    for relative, default in MANIFESTS.items():
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative} is missing")
            continue
        features = tomllib.loads(path.read_text(encoding="utf-8")).get("features", {})
        for feature in sorted(enabled(features)):
            # A feature of a dependency, such as yamaa-boundary/loader,
            # counts as its last part.
            if feature.rsplit("/", 1)[-1] in FEATURES:
                errors.append(
                    f"{relative}: the default features enable {feature}, {REFUSED}"
                )
        found = features.get("default", [])
        if default is not None and found != default:
            errors.append(
                f"{relative}: the default features are {json.dumps(found)}, not "
                f"{json.dumps(default)}: every shipped package links its engine"
            )
    return errors


def check(root):
    """Every error of the three routes by which a shipped build could enable
    the loaded mode."""
    return check_files(root) + check_native(root) + check_manifests(root)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args(argv).root)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        return 1
    print(
        f"PASS: none of the {len(SHIPPED)} files of a shipped build, no part of "
        f"{NATIVE} but its {LOADED_JOB} job and no default feature of the "
        f"{len(MANIFESTS)} crates of the boundary enables the loaded mode."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
