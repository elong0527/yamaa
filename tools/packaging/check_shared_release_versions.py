"""Require one selected Python/R/Cargo version, one coherent author list
across the package metadata, and unchanged lock ownership."""

import argparse
import re
import tomllib
from pathlib import Path

from tools.packaging.check_benchmark_sdk_versions import violations as sdk_violations
from tools.paths import ROOT

# The selected author list: the author first, then the contributors in the
# order the R package declares them. Python and Cargo metadata cannot
# express contributor roles, so they carry the same people in the same
# order; the R DESCRIPTION alone assigns the roles.
SELECTED_PEOPLE = [
    ("Yilong Zhang", "elong0527@gmail.com"),
    ("Ming-Chun Chen", "hellomingchun@gmail.com"),
    ("Peng Zhang", None),
    ("Bingjun Wang", None),
    ("Jingwei Xiong", None),
]
COLLECTIVE_HOLDER = "yamaa contributors"


def _people(entries):
    """Normalize author entries to (name, email-or-None) pairs."""
    people = []
    for entry in entries:
        if isinstance(entry, dict):
            people.append((entry.get("name"), entry.get("email")))
        else:
            match = re.fullmatch(r"(.+?)(?: <([^>]+)>)?", entry.strip())
            people.append((match.group(1), match.group(2)))
    return people


def _r_people(description):
    """Parse the Authors@R persons as (name, email-or-None, roles) triples."""
    field = re.search(r"(?ms)^Authors@R: (.*?)(?=^\S)", description)
    if field is None:
        return None
    people = []
    pattern = r'person\("([^"]+)",\s*"([^"]+)"([^)]*)\)'
    for match in re.finditer(pattern, field.group(1)):
        rest = match.group(3)
        email = re.search(r'email\s*=\s*"([^"]+)"', rest)
        role = re.search(r"role\s*=\s*(.+)$", rest)
        roles = set(re.findall(r'"([^"]+)"', role.group(1))) if role else set()
        people.append(
            (
                f"{match.group(1)} {match.group(2)}",
                email.group(1) if email else None,
                roles,
            )
        )
    return people


def author_violations(root):
    errors = []

    def read(relative):
        try:
            return (root / relative).read_text()
        except OSError:
            errors.append(f"{relative}: author metadata is unreadable")
            return None

    python_text = read("python/pyproject.toml")
    if python_text is not None:
        authors = tomllib.loads(python_text)["project"].get("authors", [])
        if _people(authors) != SELECTED_PEOPLE:
            errors.append(
                "python/pyproject.toml: author list differs from the selected"
                " author list"
            )
    description = read("R/yamaa/DESCRIPTION")
    if description is not None:
        expected = [
            (name, email, {"aut", "cre"} if index == 0 else {"ctb"})
            for index, (name, email) in enumerate(SELECTED_PEOPLE)
        ]
        if _r_people(description) != expected:
            errors.append(
                "R/yamaa/DESCRIPTION: author list differs from the selected author list"
            )
    cargo_text = read("rust/Cargo.toml")
    if cargo_text is not None:
        workspace = tomllib.loads(cargo_text)["workspace"]
        if _people(workspace["package"].get("authors", [])) != SELECTED_PEOPLE:
            errors.append(
                "rust/Cargo.toml: author list differs from the selected author list"
            )
        for member in workspace["members"]:
            member_text = read(f"rust/{member}/Cargo.toml")
            if member_text is None:
                continue
            package = tomllib.loads(member_text)["package"]
            if package.get("authors") != {"workspace": True}:
                errors.append(
                    f"{member}: authors do not inherit the workspace author list"
                )
    holders = (
        ("LICENSE", r"Copyright \(c\) \d{4} " + COLLECTIVE_HOLDER + r"\n"),
        ("mkdocs.yml", r"(?m)^copyright:.*" + COLLECTIVE_HOLDER),
        ("R/yamaa/LICENSE", r"(?m)^COPYRIGHT HOLDER: " + COLLECTIVE_HOLDER + r"$"),
    )
    for relative, pattern in holders:
        text = read(relative)
        if text is not None and not re.search(pattern, text):
            errors.append(f"{relative}: copyright holder is not '{COLLECTIVE_HOLDER}'")
    return errors


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
    errors += author_violations(root)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    errors = violations(args.root)
    if errors:
        raise SystemExit("\n".join(errors))
    print(
        "PASS: selected Python/R/Cargo versions, author lists and package"
        " lock identities agree"
    )


if __name__ == "__main__":
    main()
