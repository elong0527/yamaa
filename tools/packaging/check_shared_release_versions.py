"""Require the Python/R/Cargo spellings of one release identity, Python README
links that name its tag, one coherent author list across the package metadata,
and unchanged lock ownership."""

import argparse
import re
import tomllib
from pathlib import Path
from urllib.parse import urlsplit

from tools.packaging import release_identity
from tools.packaging.check_benchmark_sdk_versions import violations as sdk_violations
from tools.paths import ROOT

# The selected author list: the author first, then the contributors in the
# order the R package declares them. Python and Cargo metadata cannot
# express contributor roles, so they carry the same people in the same
# order; the R DESCRIPTION alone assigns the roles. There the author is also
# a copyright holder, as CRAN asks of a package's author.
SELECTED_PEOPLE = [
    ("Yilong Zhang", "elong0527@gmail.com"),
    ("Ming-Chun Chen", None),
    ("Peng Zhang", None),
    ("Bingjun Wang", None),
    ("Jingwei Xiong", None),
]
COLLECTIVE_HOLDER = "yamaa contributors"
# The R DESCRIPTION alone also credits the authors of the third-party code that
# the R source package bundles, as contributors and copyright holders, as
# CRAN's Repository Policy asks: the Rust crates, which the staged package's
# inst/AUTHORS lists, and the ReadStat C sources.
BUNDLED_AUTHORS = [
    ("The authors of the bundled Rust crates", None, {"ctb", "cph"}),
    ("Evan Miller and the ReadStat authors", None, {"ctb", "cph"}),
]
# The Python README is the wheels' long description, which PyPI keeps as each
# version's page. Its links into the repository that name a release tag must
# name the tag of the release the wheels ship, which never moves under that
# page (#3057). A link to `main` is deliberate and left alone.
PYTHON_README = "python/README.md"
REPOSITORY_LINK = re.compile(
    r"https://(?:github\.com|raw\.githubusercontent\.com)/elong0527/yamaa/"
    r"[^\s\"'()<>]*"
)


def _people(entries):
    """Normalize author entries to (name, email-or-None) pairs.

    Returns None when an entry cannot be accounted for (a non-list
    collection, a non-mapping/non-string entry, or a string the name
    pattern cannot read) so the caller reports a violation instead of
    crashing on malformed metadata."""
    if not isinstance(entries, (list, tuple)):
        return None
    people = []
    for entry in entries:
        if isinstance(entry, dict):
            people.append((entry.get("name"), entry.get("email")))
        elif isinstance(entry, str):
            match = re.fullmatch(r"(.+?)(?: <([^>]+)>)?", entry.strip())
            if match is None:
                return None
            people.append((match.group(1), match.group(2)))
        else:
            return None
    return people


def _load_toml(root, relative, errors, label):
    """Load one TOML file, recording a violation instead of raising."""
    try:
        return tomllib.loads((root / relative).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"{label}: unreadable or invalid metadata: {error}")
        return None


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

    Positional arguments map to given, family, middle in order; email, role
    and comment must be named, and a comment must be one string. A person
    with a given name alone is a collective entry. Any argument the parser
    cannot account for returns None."""
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
        if key in fields or key not in (
            "given",
            "family",
            "middle",
            "email",
            "role",
            "comment",
        ):
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
    if given is None or (family is None and "middle" in fields):
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
        try:
            python_document = tomllib.loads(python_text)
        except tomllib.TOMLDecodeError:
            python_document = None
        project = (
            python_document.get("project")
            if isinstance(python_document, dict)
            else None
        )
        authors = project.get("authors", []) if isinstance(project, dict) else None
        if _people(authors) != SELECTED_PEOPLE:
            errors.append(
                "python/pyproject.toml: author list differs from the selected"
                " author list"
            )
    description = read("R/yamaa/DESCRIPTION")
    if description is not None:
        expected = [
            (name, email, {"aut", "cre", "cph"} if index == 0 else {"ctb"})
            for index, (name, email) in enumerate(SELECTED_PEOPLE)
        ] + BUNDLED_AUTHORS
        if _r_people(description) != expected:
            errors.append(
                "R/yamaa/DESCRIPTION: author list differs from the selected author list"
            )
    cargo_text = read("rust/Cargo.toml")
    if cargo_text is not None:
        try:
            cargo_document = tomllib.loads(cargo_text)
        except tomllib.TOMLDecodeError:
            cargo_document = None
        workspace = (
            cargo_document.get("workspace")
            if isinstance(cargo_document, dict)
            else None
        )
        workspace = workspace if isinstance(workspace, dict) else {}
        package_table = workspace.get("package")
        workspace_authors = (
            package_table.get("authors", [])
            if isinstance(package_table, dict)
            else None
        )
        if _people(workspace_authors) != SELECTED_PEOPLE:
            errors.append(
                "rust/Cargo.toml: author list differs from the selected author list"
            )
        members = workspace.get("members")
        if not isinstance(members, list):
            members = []
        for member in members:
            if not isinstance(member, str):
                continue
            member_text = read(f"rust/{member}/Cargo.toml")
            if member_text is None:
                continue
            try:
                member_document = tomllib.loads(member_text)
            except tomllib.TOMLDecodeError:
                member_document = None
            package = (
                member_document.get("package")
                if isinstance(member_document, dict)
                else None
            )
            if not isinstance(package, dict) or package.get("authors") != {
                "workspace": True
            }:
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


def python_readme_violations(root, identity):
    """List the repository links of the Python README that name another tag.

    A path segment that starts with `yamaa-v` names a release tag, and it must
    be the release's own tag, in the candidate form for a candidate; a link to
    `main` or any other ref is left alone."""
    try:
        text = (root / PYTHON_README).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        return [f"{PYTHON_README}: unreadable: {error}"]
    errors = []
    for number, line in enumerate(text.splitlines(), 1):
        for link in REPOSITORY_LINK.findall(line):
            for segment in urlsplit(link).path.split("/"):
                if segment.startswith("yamaa-v") and segment != identity.tag:
                    errors.append(
                        f"{PYTHON_README}:{number}: link names tag {segment!r}, "
                        f"not {identity.tag!r}, the tag of release {identity}"
                    )
    return errors


def violations(root):
    """List every way the tree's versions, locks, links and authors disagree.

    Each manifest must carry its own spelling of the one release identity:
    Python X.Y.Z or X.Y.ZrcN, Cargo X.Y.Z or X.Y.Z-rc.N, and R X.Y.Z. The
    Python README's links that name a release tag name its tag, yamaa-vX.Y.Z
    or yamaa-vX.Y.Z-rc.N.
    """
    try:
        identity = release_identity.read(root)
    except ValueError as error:
        return [str(error), *author_violations(root)]
    errors = sdk_violations(root, identity)
    cargo = _load_toml(root, "rust/Cargo.toml", errors, "rust/Cargo.toml")
    workspace = cargo.get("workspace") if isinstance(cargo, dict) else None
    workspace = workspace if isinstance(workspace, dict) else {}
    workspace_package = workspace.get("package")
    workspace_version = (
        workspace_package.get("version")
        if isinstance(workspace_package, dict)
        else None
    )
    expected = identity.cargo
    if workspace_version != expected:
        errors.append(
            f"rust/Cargo.toml: workspace version {workspace_version!r} is not "
            f"{expected!r}, the Cargo version of release {identity}"
        )
    member_names = []
    members = workspace.get("members")
    if not isinstance(members, list) or not all(
        isinstance(member, str) for member in members
    ):
        errors.append("rust/Cargo.toml: workspace members are missing or malformed")
        members = []
    for member in members:
        relative = f"rust/{member}/Cargo.toml"
        document = _load_toml(root, relative, errors, relative)
        package = document.get("package") if isinstance(document, dict) else None
        if not isinstance(package, dict) or not isinstance(package.get("name"), str):
            errors.append(f"{member}: package metadata is missing or malformed")
            continue
        member_names.append(package["name"])
        version = package.get("version")
        actual = workspace_version if version == {"workspace": True} else version
        if actual != expected:
            errors.append(
                f"{member}: package version differs from the shared Cargo version "
                f"{expected!r}"
            )
    locked_document = _load_toml(root, "rust/Cargo.lock", errors, "rust/Cargo.lock")
    locked = (
        locked_document.get("package") if isinstance(locked_document, dict) else None
    )
    if not isinstance(locked, list):
        locked = []
    for name in member_names:
        versions = [
            package.get("version")
            for package in locked
            if isinstance(package, dict) and package.get("name") == name
        ]
        if versions != [expected]:
            errors.append(f"Cargo.lock: {name} version identity is stale or ambiguous")
    python_locked_document = _load_toml(
        root, "python/uv.lock", errors, "python/uv.lock"
    )
    python_locked = (
        python_locked_document.get("package")
        if isinstance(python_locked_document, dict)
        else None
    )
    if not isinstance(python_locked, list):
        python_locked = []
    owners = [
        package
        for package in python_locked
        if isinstance(package, dict) and package.get("name") == "yamaa"
    ]
    if (
        len(owners) != 1
        or owners[0].get("version") != identity.python
        or owners[0].get("source") != {"editable": "."}
    ):
        errors.append("uv.lock: selected yamaa project/version ownership differs")
    errors += python_readme_violations(root, identity)
    errors += author_violations(root)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    errors = violations(args.root)
    if errors:
        raise SystemExit("\n".join(errors))
    identity = release_identity.read(args.root)
    print(
        f"PASS: release {identity} (Python {identity.python}, Cargo "
        f"{identity.cargo}, R {identity.r}, tag {identity.tag}), Python README "
        "links, author lists and package lock identities agree"
    )


if __name__ == "__main__":
    main()
