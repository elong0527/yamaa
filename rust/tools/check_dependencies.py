"""Reject dependencies that cross the workspace's architecture boundaries."""

import json
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
ALLOWED = {
    "yamaa-core": {"ryu", "libm", "num-bigint"},
    "yamaa-engine": {"yamaa-core"},
    "yamaa-adapters": {"yamaa-core", "yamaa-engine", "serde", "serde_json"},
    "yamaa-python": {"yamaa-engine", "yamaa-adapters", "pyo3", "pyo3-build-config"},
    "yamaa-r": {"yamaa-engine", "yamaa-adapters", "extendr-api"},
}


def violations(metadata):
    packages = {p["name"]: p for p in metadata["packages"]}
    errors = []
    if set(packages) != set(ALLOWED):
        errors.append("workspace members differ from the five reviewed crates")
    for name, package in packages.items():
        for dependency in package["dependencies"]:
            if dependency["name"] not in ALLOWED.get(name, set()):
                errors.append(f"{name} must not depend on {dependency['name']}")
    return errors


def main():
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=WORKSPACE,
            text=True,
        )
    )
    errors = violations(metadata)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Rust dependency boundaries passed")


if __name__ == "__main__":
    main()
