"""Require one selected Python/R/Cargo version and unchanged lock ownership."""

import argparse
from pathlib import Path

import tomllib
from check_benchmark_sdk_versions import violations as sdk_violations


def violations(root):
    errors = sdk_violations(root)
    python = tomllib.loads((root / "python/pyproject.toml").read_text())
    expected = python["project"]["version"]
    workspace = tomllib.loads((root / "rust/Cargo.toml").read_text())["workspace"]
    if workspace["package"]["version"] != expected:
        errors.append("Cargo workspace version differs from selected Python/R release")
    member_names = []
    for member in workspace["members"]:
        package = tomllib.loads((root / "rust" / member / "Cargo.toml").read_text())[
            "package"
        ]
        member_names.append(package["name"])
        version = package["version"]
        actual = (
            workspace["package"]["version"]
            if version == {"workspace": True}
            else version
        )
        if actual != expected:
            errors.append(
                f"{member}: package version differs from selected shared release"
            )
    locked = tomllib.loads((root / "rust/Cargo.lock").read_text())["package"]
    for name in member_names:
        versions = [package["version"] for package in locked if package["name"] == name]
        if versions != [expected]:
            errors.append(f"Cargo.lock: {name} version identity is stale or ambiguous")
    python_locked = tomllib.loads((root / "python/uv.lock").read_text())["package"]
    owners = [package for package in python_locked if package["name"] == "yamaa"]
    if (
        len(owners) != 1
        or owners[0]["version"] != expected
        or owners[0]["source"] != {"editable": "."}
    ):
        errors.append("uv.lock: selected yamaa project/version ownership differs")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[2]
    )
    args = parser.parse_args()
    errors = violations(args.root)
    if errors:
        raise SystemExit("\n".join(errors))
    print("PASS: selected Python/R/Cargo versions and package lock identities agree")


if __name__ == "__main__":
    main()
