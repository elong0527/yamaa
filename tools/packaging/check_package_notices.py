"""Require every built package to carry the project license and package notices.

The python/pyproject.toml license files are the package notices. A wheel holds
them in its .dist-info/licenses directory, the Python source archive at its
root, the R source archive in inst/, from which R installs them, an R binary
package at the root of the installed package, and a command-line archive in
its one directory, beside the executable. Each copy must equal its host's copy
in the checkout byte for byte: python/ for the Python packages, R/yamaa/inst/
for the R packages and rust/crates/yamaa-cli/ for the command-line archives.
The hosts' copies are the same, apart from LICENSE-THIRD-PARTY.txt and
COMPONENTS.tsv, which list each host's own crates and components:
tools/packaging/generate_third_party_notices.py and
tools/packaging/generate_component_inventory.py write them. The command line
keeps only those two beside its crate, and its archives carry the Python
packages' copies of the others. The Python source archive's COMPONENTS.tsv is
the wheels' without its rust-std row, the Rust standard library that the
wheels link (source_inventory in the latter). An R binary package's is the R
source package's followed by the one rust-std row that the package's
configure step appends when R installs it, for the pinned toolchain that
builds the release binaries (installed_inventory). A command-line archive's
is the command line's committed inventory, which names that toolchain's
standard library, as the wheels' does.
"""

import argparse
import re
import tarfile
import tomllib
import zipfile
from pathlib import Path

from tools.ci import native, toolchain
from tools.packaging import generate_component_inventory as inventory
from tools.packaging import generate_third_party_notices as third_party
from tools.packaging import release_identity
from tools.paths import ROOT

# The host whose checkout copies of the package notices each form carries.
HOSTS = {
    "wheel": "python",
    "sdist": "python",
    "r-source": "r",
    "r-binary": "r",
    "cli": "cli",
}


def package_notices(root=ROOT):
    """Return the release identity and the package notice file names."""
    project = tomllib.loads(
        (root / "python/pyproject.toml").read_text(encoding="utf-8")
    )["project"]
    return release_identity.read(root), tuple(project["license-files"])


def notice_members(name, identity, notices):
    """Map each notice to its member in a wheel, Python sdist, R package or
    command-line archive.

    Also returns the package's form, "wheel", "sdist", "r-source", "r-binary"
    or "cli". The Python packages carry the Python version, the R packages the
    R version and the command-line archives the Cargo version, which differ
    for a release candidate. An R binary package's name adds the R series that
    it was built for, as in yamaa_1.0.0-4.6.zip, and a command-line archive's
    the target of its executable, as in yamaa-1.0.0-aarch64-apple-darwin.zip.
    """
    python, r, cargo = identity.python, identity.r, identity.cargo
    archives = {f"yamaa-{cargo}-{target}.zip" for target in native.CLI_TARGETS.values()}
    if name.startswith(f"yamaa-{python}-") and name.endswith(".whl"):
        form, directory = "wheel", f"yamaa-{python}.dist-info/licenses"
    elif name == f"yamaa-{python}.tar.gz":
        form, directory = "sdist", f"yamaa-{python}"
    elif name == f"yamaa_{r}.tar.gz":
        form, directory = "r-source", "yamaa/inst"
    elif re.fullmatch(rf"yamaa_{re.escape(r)}-[0-9]+\.[0-9]+\.(zip|tgz)", name):
        form, directory = "r-binary", "yamaa"
    elif name in archives:
        form, directory = "cli", name.removesuffix(".zip")
    else:
        raise ValueError(
            f"{name}: not a yamaa {identity} wheel, source archive, R binary "
            "package or command-line archive"
        )
    return form, {notice: f"{directory}/{notice}" for notice in notices}


def checkout_copy(form, notice):
    """The checkout file, below the root, that a package of `form` carries as
    `notice`: its host's copy, or for a command-line archive the Python
    packages' copy of a notice that the hosts share."""
    host = HOSTS[form]
    if host == "cli" and notice not in third_party.HOST_NOTICES:
        host = "python"
    directory = third_party.copies()[host].relative_to(third_party.ROOT)
    return f"{directory.as_posix()}/{notice}"


def packaged_inventory(form, committed, root=ROOT):
    """The COMPONENTS.tsv that a package of `form` carries, from its host's
    committed copy: the Python source archive's leaves out the wheels'
    rust-std row, and an R binary package's adds the one that configure
    appends, for the toolchain that rust/rust-toolchain.toml pins."""
    if form == "sdist":
        text = inventory.source_inventory(committed.decode("utf-8"))
    elif form == "r-binary":
        release = toolchain.channel(root / "rust")
        text = inventory.installed_inventory(committed.decode("utf-8"), release)
    else:
        return committed
    return text.encode("utf-8")


def read_members(path, wanted):
    """Read the wanted regular members of an archive; refuse duplicate names."""
    # A wheel, a Windows R binary package and a command-line archive are ZIP
    # archives; the source archives and a macOS R binary package are gzipped
    # tar archives.
    if path.suffix in {".whl", ".zip"}:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(set(names)):
                raise ValueError(f"{path.name}: duplicate archive member")
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
    form, members = notice_members(path.name, identity, notices)
    payloads = read_members(path, set(members.values()))
    for notice, member in members.items():
        expected = (root / checkout_copy(form, notice)).read_bytes()
        if notice == inventory.NAME:
            expected = packaged_inventory(form, expected, root)
        if payloads.get(member) != expected:
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
