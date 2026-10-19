"""Require every built package to carry the project license and package notices.

The python/pyproject.toml license files are the package notices. A wheel holds
them in its .dist-info/licenses directory, the Python source archive at its
root and the R source archive in inst/, from which R installs them. Each copy
must equal the checkout's python/ file byte for byte.
"""

import argparse
import tarfile
import tomllib
import zipfile
from pathlib import Path

from tools.packaging import release_identity
from tools.paths import ROOT


def package_notices(root=ROOT):
    """Return the release identity and the package notice file names."""
    project = tomllib.loads(
        (root / "python/pyproject.toml").read_text(encoding="utf-8")
    )["project"]
    return release_identity.read(root), tuple(project["license-files"])


def notice_members(name, identity, notices):
    """Map each notice to its member in a wheel, Python sdist or R source archive.

    The Python packages carry the Python version and the R source archive the
    R version, which differ for a release candidate.
    """
    python, r = identity.python, identity.r
    if name.startswith(f"yamaa-{python}-") and name.endswith(".whl"):
        directory = f"yamaa-{python}.dist-info/licenses"
    elif name == f"yamaa-{python}.tar.gz":
        directory = f"yamaa-{python}"
    elif name == f"yamaa_{r}.tar.gz":
        directory = "yamaa/inst"
    else:
        raise ValueError(f"{name}: not a yamaa {identity} wheel or source archive")
    return {notice: f"{directory}/{notice}" for notice in notices}


def read_members(path, wanted):
    """Read the wanted regular members of an archive; refuse duplicate names."""
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(set(names)):
                raise ValueError(f"{path.name}: duplicate wheel member")
            return {name: archive.read(name) for name in names if name in wanted}
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        if len({entry.name for entry in entries}) != len(entries):
            raise ValueError(f"{path.name}: duplicate source archive member")
        return {
            entry.name: archive.extractfile(entry).read()
            for entry in entries
            if entry.isfile() and entry.name in wanted
        }


def check_archive(path, root=ROOT):
    """Raise ValueError unless the package carries every notice unchanged."""
    identity, notices = package_notices(root)
    members = notice_members(path.name, identity, notices)
    payloads = read_members(path, set(members.values()))
    for notice, member in members.items():
        if payloads.get(member) != (root / "python" / notice).read_bytes():
            raise ValueError(
                f"{path.name}: required notice differs or is missing: {member}"
            )
    return members


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="+", type=Path)
    args = parser.parse_args()
    try:
        for archive in args.archives:
            members = check_archive(archive)
            print(f"{archive.name}: carries its {len(members)} package notices")
    except (ValueError, OSError, zipfile.BadZipFile, tarfile.TarError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
