#!/usr/bin/env python3
"""Enforce R022 schema-bundle release invariants."""

import argparse
import re
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
YAML_DIR = ROOT / "yaml"
SEMVER = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
SENSITIVE_PREFIXES = (
    "yaml/schema", "yaml/rules/", "yaml/examples/", "yaml/migrations/",
    ".github/workflows/validate_repository.py",
)


def load(path):
    with path.open(encoding="ascii") as stream:
        return yaml.safe_load(stream)


def schema_version_at(revision=None):
    if revision:
        result = subprocess.run(
            ["git", "show", f"{revision}:yaml/schema.yaml"], cwd=ROOT,
            check=True, text=True, capture_output=True,
        )
        return str(yaml.safe_load(result.stdout)["version"])
    return str(load(YAML_DIR / "schema.yaml")["version"])


def changed_paths(base):
    result = subprocess.run(
        ["git", "diff", "--name-only", f"{base}...HEAD"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def validate(base=None):
    errors = []
    version = schema_version_at()
    if not SEMVER.fullmatch(version):
        errors.append(f"bundle version {version!r} is not Semantic Versioning")

    modules = sorted(YAML_DIR.glob("schema*.yaml"))
    for module in modules:
        actual = str(load(module).get("version"))
        if actual != version:
            errors.append(f"{module.relative_to(ROOT)} has version {actual!r}, expected {version!r}")

    manifest_path = YAML_DIR / "releases" / f"{version}.yaml"
    if not manifest_path.is_file():
        errors.append(f"missing release manifest {manifest_path.relative_to(ROOT)}")
    else:
        manifest = load(manifest_path)
        if str(manifest.get("version")) != version:
            errors.append("release manifest version does not match bundle")
        status = manifest.get("status")
        if status not in ("prerelease", "released"):
            errors.append("release manifest status must be prerelease or released")
        expected_tag = f"schema-v{version}" if status == "released" else None
        if manifest.get("tag") != expected_tag:
            errors.append(f"release manifest tag must be {expected_tag!r}")
        for prior in manifest.get("migration_from", []):
            migration = YAML_DIR / "migrations" / f"{prior}-to-{version}"
            required = ("README.md", "source.resolved.yaml", "target.resolved.yaml")
            for name in required:
                if not (migration / name).is_file():
                    errors.append(f"missing migration fixture {(migration / name).relative_to(ROOT)}")

    for implementation in ("r", "python"):
        path = YAML_DIR / "implementations" / f"{implementation}.yaml"
        if not path.is_file():
            errors.append(f"missing capability report {path.relative_to(ROOT)}")
            continue
        report = load(path)
        supported = report.get("supported_bundle_versions", [])
        if version not in supported:
            errors.append(f"{path.relative_to(ROOT)} does not report support for {version}")
        if report.get("default_bundle_version") not in supported:
            errors.append(f"{path.relative_to(ROOT)} default is not supported")

    if base:
        try:
            base_version = schema_version_at(base)
            paths = changed_paths(base)
        except subprocess.CalledProcessError as exc:
            errors.append(f"cannot compare release base {base!r}: {exc.stderr.strip()}")
        else:
            sensitive = [p for p in paths if p.startswith(SENSITIVE_PREFIXES)]
            if sensitive and base_version == version:
                errors.append(
                    "release-sensitive paths changed without a bundle version "
                    f"change from {base_version}: {', '.join(sensitive)}"
                )
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", help="Git revision used to enforce version action")
    args = parser.parse_args()
    errors = validate(args.base)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        return 1
    print(f"Schema release {schema_version_at()} satisfies R022")
    return 0


if __name__ == "__main__":
    sys.exit(main())
