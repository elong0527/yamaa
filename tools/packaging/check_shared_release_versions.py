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


def _split_r_arguments(text):
    """Split an R call's argument text on top-level commas.

    Returns the raw argument strings, or None when quotes or parentheses
    are unbalanced."""
    arguments = []
    depth = 0
    quoted = False
    escaped = False
    start = 0
    for index, char in enumerate(text):
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth < 0:
                return None
        elif char == "," and depth == 0:
            arguments.append(text[start:index])
            start = index + 1
    if quoted or depth != 0:
        return None
    arguments.append(text[start:])
    return arguments


def _r_string(value):
    """Return the contents of a double-quoted R string literal, else None."""
    match = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', value.strip())
    if match is None:
        return None
    return re.sub(r"\\(.)", r"\1", match.group(1))


def _r_person(arguments):
    """Build one (name, email-or-None, roles) triple from person() arguments.

    Positional arguments map to given, family, middle in order; email and
    role must be named. Any argument the parser cannot account for returns
    None."""
    fields = {}
    positional = ("given", "family", "middle")
    for index, argument in enumerate(arguments):
        named = re.match(r"^([A-Za-z]+)\s*=\s*(.+)$", argument.strip(), re.S)
        if named:
            key, raw = named.group(1), named.group(2)
        elif index < len(positional):
            key, raw = positional[index], argument
        else:
            return None
        if key in fields or key not in ("given", "family", "middle", "email", "role"):
            return None
        if key == "role":
            raw = raw.strip()
            if raw.startswith("c(") and raw.endswith(")"):
                parts = _split_r_arguments(raw[2:-1])
                if parts is None:
                    return None
                values = [_r_string(part) for part in parts if part.strip()]
                if any(value is None for value in values):
                    return None
                fields[key] = set(values)
            else:
                value = _r_string(raw)
                if value is None:
                    return None
                fields[key] = {value}
        else:
            value = _r_string(raw)
            if value is None:
                return None
            fields[key] = value
    given, family = fields.get("given"), fields.get("family")
    if given is None or family is None:
        return None
    name = " ".join(part for part in (given, fields.get("middle"), family) if part)
    return (name, fields.get("email"), fields.get("role", set()))


def _r_people(description):
    """Parse the Authors@R persons as (name, email-or-None, roles) triples.

    Every entry must be accounted for: a person() call the parser cannot
    fully read, or any other content in the field, fails the parse (None)
    so the caller reports a violation instead of comparing a truncated
    list against the selected people."""
    field = re.search(r"(?ms)^Authors@R: (.*?)(?=^\S)", description)
    if field is None:
        return None
    text = field.group(1).strip()
    if text.startswith("c(") and text.endswith(")"):
        text = text[2:-1].strip()
    people = []
    position = 0
    while position < len(text):
        match = re.match(r"person\s*\(", text[position:])
        if match is None:
            return None
        start = position + match.end()
        depth = 1
        quoted = False
        escaped = False
        index = start
        while index < len(text) and depth:
            char = text[index]
            if quoted:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    quoted = False
            elif char == '"':
                quoted = True
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            index += 1
        if depth or quoted:
            return None
        arguments = _split_r_arguments(text[start : index - 1])
        if arguments is None:
            return None
        person = _r_person(arguments)
        if person is None:
            return None
        people.append(person)
        rest = re.match(r"\s*(,)?\s*", text[index:])
        position = index + rest.end()
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
        ("LICENSE", r"(?m)Copyright \(c\) \d{4} " + COLLECTIVE_HOLDER + r"$"),
        ("mkdocs.yml", r"(?m)^copyright:.*" + COLLECTIVE_HOLDER + r"$"),
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
