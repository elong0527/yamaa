"""Keep the host package versions and benchmark R SDK pins on the release identity.

The Python and R packages each carry their own spelling of the release that
tools/packaging/release.toml names, and every benchmark R environment pins the
R spelling. For a release candidate that is the final version, since an R
version cannot mark a candidate (#3030 D53).
"""

import argparse
import json
import tomllib
from pathlib import Path

from tools.packaging import release_identity
from tools.paths import ROOT


def violations(root: Path, identity=None) -> list[str]:
    """List the version disagreements; `identity` defaults to the tree's own."""
    if identity is None:
        try:
            identity = release_identity.read(root)
        except ValueError as error:
            return [str(error)]
    errors = []
    try:
        description = (root / "R/yamaa/DESCRIPTION").read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        errors.append(f"R/yamaa/DESCRIPTION: unreadable metadata: {error}")
    else:
        versions = [
            line.removeprefix("Version:").strip()
            for line in description.splitlines()
            if line.startswith("Version:")
        ]
        if len(versions) != 1 or not versions[0]:
            errors.append(
                "R/yamaa/DESCRIPTION must declare exactly one nonempty Version"
            )
        elif versions[0] != identity.r:
            errors.append(
                f"R/yamaa/DESCRIPTION: Version {versions[0]!r} is not "
                f"{identity.r!r}, the R version of release {identity}"
            )
    try:
        python = tomllib.loads(
            (root / "python/pyproject.toml").read_text(encoding="utf-8")
        )
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"python/pyproject.toml: unreadable or invalid metadata: {error}")
    else:
        project = python.get("project")
        version = project.get("version") if isinstance(project, dict) else None
        if version != identity.python:
            errors.append(
                f"python/pyproject.toml: version {version!r} is not "
                f"{identity.python!r}, the Python version of release {identity}"
            )
    locks = sorted((root / "benchmarks").glob("*/r/renv.lock"))
    if not locks:
        errors.append("no benchmark R SDK locks found")
    for lock in locks:
        name = lock.relative_to(root).as_posix()
        try:
            document = json.loads(lock.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError, OSError) as error:
            errors.append(f"{name}: unreadable or invalid JSON: {error}")
            continue
        packages = document.get("Packages") if isinstance(document, dict) else None
        sdk = packages.get("yamaa") if isinstance(packages, dict) else None
        if not isinstance(sdk, dict) or sdk.get("Package") != "yamaa":
            errors.append(f"{name}: SDK entry missing or malformed")
        elif not isinstance(sdk.get("Version"), str) or sdk["Version"] != identity.r:
            errors.append(
                f"{name}: SDK version {sdk.get('Version')!r} differs from prepared "
                f"{identity.r!r}"
            )
    return errors


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    errors = violations(args.root)
    if errors:
        raise SystemExit("\n".join(errors))
    print("PASS: host package versions and benchmark R SDK locks match the release")


if __name__ == "__main__":
    main()
