"""Reject generated caches and retired evaluator families in Python distributions."""

import argparse
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

# #1994 retires these import roots; the retained helpers are _odm and style.
RETIRED_IMPORTS = {
    "_reference_domain",
    "application",
    "regex",
    "adapters",
    "expressions",
    "io",
    "models",
    "odm",
    "planning",
    "runtime",
    "schema",
    "specification",
    "submission",
    "verification",
}


def package_members(member: PurePosixPath, wheel: bool) -> tuple[str, ...]:
    """Locate Python package members without confusing retained Rust/test assets."""
    parts = member.parts
    folded = tuple(part.casefold() for part in parts)
    if wheel:
        if folded[:1] == ("yamaa",):
            return parts[1:]
        # A wheel's data scheme can also install files into the import namespace.
        if (
            len(parts) >= 4
            and folded[0].endswith(".data")
            and folded[1] in {"purelib", "platlib"}
            and folded[2] == "yamaa"
        ):
            return parts[3:]
    elif folded[1:4] == ("python", "src", "yamaa"):
        return parts[4:]
    return ()


def check_archive(path: Path) -> int:
    wheel = path.suffix == ".whl"
    if wheel:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            names = archive.getnames()
    else:
        raise ValueError(f"unsupported Python distribution: {path}")

    generated = []
    retired = []
    for name in names:
        member = PurePosixPath(name.replace("\\", "/"))
        if "__pycache__" in member.parts or member.suffix.lower() in {".pyc", ".pyo"}:
            generated.append(name)
        package = package_members(member, wheel)
        if package and package[0].split(".", 1)[0].casefold() in RETIRED_IMPORTS:
            retired.append(name)
    if generated:
        raise ValueError(
            f"{path.name} contains generated Python caches:\n" + "\n".join(generated)
        )
    if retired:
        raise ValueError(
            f"{path.name} contains retired Python evaluator/API members:\n"
            + "\n".join(retired)
        )
    return len(names)


def check_distributions(directory: Path) -> None:
    wheels = sorted(directory.glob("*.whl"))
    sources = sorted(directory.glob("*.tar.gz"))
    if len(wheels) != 1 or len(sources) != 1:
        raise ValueError("expected exactly one wheel and one source archive")
    for path in wheels + sources:
        count = check_archive(path)
        print(f"{path.name}: {count} members, no generated caches or retired evaluator/API members")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        check_distributions(args.directory)
    except ValueError as error:
        parser.exit(1, f"{error}\n")
