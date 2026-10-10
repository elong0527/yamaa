"""Keep benchmark R SDK pins coherent with the prepared Python/R release."""

import argparse
import json
import tomllib
from pathlib import Path


def violations(root: Path) -> list[str]:
    errors = []
    description = root / "R/yamaa/DESCRIPTION"
    try:
        description_text = description.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        return [f"R/yamaa/DESCRIPTION: unreadable metadata: {error}"]
    versions = [
        line.removeprefix("Version:").strip()
        for line in description_text.splitlines()
        if line.startswith("Version:")
    ]
    if len(versions) != 1 or not versions[0]:
        return ["R/yamaa/DESCRIPTION must declare exactly one nonempty Version"]
    expected = versions[0]
    try:
        python = tomllib.loads((root / "python/pyproject.toml").read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"python/pyproject.toml: unreadable or invalid metadata: {error}")
    else:
        project = python.get("project")
        if not isinstance(project, dict) or project.get("version") != expected:
            errors.append("prepared Python/R SDK versions differ")
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
        elif not isinstance(sdk.get("Version"), str) or sdk["Version"] != expected:
            errors.append(
                f"{name}: SDK version {sdk.get('Version')!r} differs from prepared {expected!r}"
            )
    return errors


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    errors = violations(args.root)
    if errors:
        raise SystemExit("\n".join(errors))
    print("PASS: benchmark R SDK locks match the prepared Python/R release")


if __name__ == "__main__":
    main()
