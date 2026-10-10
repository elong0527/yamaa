"""Reject generated Python caches in the wheel and source distribution."""

import argparse
import tarfile
import zipfile
from pathlib import Path, PurePosixPath


def check_archive(path: Path) -> int:
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as archive:
            names = archive.getnames()
    else:
        raise ValueError(f"unsupported Python distribution: {path}")

    generated = []
    for name in names:
        member = PurePosixPath(name.replace("\\", "/"))
        if "__pycache__" in member.parts or member.suffix.lower() in {".pyc", ".pyo"}:
            generated.append(name)
    if generated:
        raise ValueError(
            f"{path.name} contains generated Python caches:\n" + "\n".join(generated)
        )
    return len(names)


def check_distributions(directory: Path) -> None:
    wheels = sorted(directory.glob("*.whl"))
    sources = sorted(directory.glob("*.tar.gz"))
    if len(wheels) != 1 or len(sources) != 1:
        raise ValueError("expected exactly one wheel and one source archive")
    for path in wheels + sources:
        count = check_archive(path)
        print(f"{path.name}: {count} members, no generated Python caches")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        check_distributions(args.directory)
    except ValueError as error:
        parser.exit(1, f"{error}\n")
