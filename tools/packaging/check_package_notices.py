"""Require every built package to carry the project license and package notices.

The python/pyproject.toml license files are the package notices. A wheel holds
them in its .dist-info/licenses directory, the Python source archive at its
root, the R source archive in inst/, from which R installs them, and an R
binary package at the root of the installed package. Each copy must equal its
host's copy in the checkout byte for byte: python/ for the Python packages and
R/yamaa/inst/ for the R packages. The two hosts' copies are the same, apart
from LICENSE-THIRD-PARTY.txt and COMPONENTS.tsv, which list each host's own
crates and components: tools/packaging/generate_third_party_notices.py and
tools/packaging/generate_component_inventory.py write them. The Python source
archive's COMPONENTS.tsv is the wheels' without its rust-std row, the Rust
standard library that the wheels link (source_inventory in the latter). An R
binary package's is the R source package's followed by the one rust-std row
that the package's configure step appends when R installs it, for the pinned
toolchain that builds the release binaries (installed_inventory).

No text in a package names a GNU license, apart from the clause of the LLVM
exception that names GPLv2 (generate_third_party_notices.gnu_licenses). That
covers every text member of the archive, and of the xz-compressed tar archive
of vendored crates inside the R source package. The one exception is the
upstream files of the vendored crates in VENDORED_GNU, which the packages
carry unchanged, as `cargo vendor` writes them.
"""

import argparse
import io
import re
import tarfile
import tomllib
import zipfile
from pathlib import Path

from tools.ci import toolchain
from tools.packaging import generate_component_inventory as inventory
from tools.packaging import generate_third_party_notices as notices
from tools.packaging import release_identity
from tools.paths import ROOT

# Vendored crates whose upstream files name a GNU license, by crate name, with
# the reason that the source packages may carry those files. yamaa relies on
# each crate under the license that the notices and inventory name.
VENDORED_GNU = {
    "zstd-sys": (
        "the bundled zstd C sources say that zstd is offered under BSD-3-Clause "
        "or GPL-2.0-only, and zstd/COPYING holds the GPL-2.0 text; yamaa relies "
        "on BSD-3-Clause"
    ),
    "zstd": "its Readme says that the zstd C library is dual BSD/GPLv2 licensed",
    "r-efi": (
        "offered under MIT, Apache-2.0 or LGPL-2.1-or-later; yamaa relies on MIT, "
        "and no supported target compiles it"
    ),
    "pyo3": "its changelog says that the MIT OR Apache-2.0 license is GPLv2-compatible",
    "num-bigint": "its README names the license of rug, another crate",
}
# A vendored crate's directory, as `cargo vendor --versioned-dirs` names it,
# at the root of an archive inside the package (after its name and !) or below.
VENDORED_CRATE = re.compile(
    r"(?:^|[/!])vendor/([A-Za-z0-9_-]+?)-[0-9]+\.[0-9]+\.[0-9]+[^/]*/"
)


def package_notices(root=ROOT):
    """Return the release identity and the package notice file names."""
    project = tomllib.loads(
        (root / "python/pyproject.toml").read_text(encoding="utf-8")
    )["project"]
    return release_identity.read(root), tuple(project["license-files"])


def notice_members(name, identity, notices):
    """Map each notice to its member in a wheel, Python sdist or R package.

    Also returns the package's form, "wheel", "sdist", "r-source" or
    "r-binary", and the checkout directory that holds the host's copies. The
    Python packages carry the Python version and the R packages the R
    version, which differ for a release candidate. An R binary package's name
    adds the R series that it was built for, as in yamaa_1.0.0-4.6.zip.
    """
    python, r = identity.python, identity.r
    if name.startswith(f"yamaa-{python}-") and name.endswith(".whl"):
        form, directory = "wheel", f"yamaa-{python}.dist-info/licenses"
    elif name == f"yamaa-{python}.tar.gz":
        form, directory = "sdist", f"yamaa-{python}"
    elif name == f"yamaa_{r}.tar.gz":
        form, directory = "r-source", "yamaa/inst"
    elif re.fullmatch(rf"yamaa_{re.escape(r)}-[0-9]+\.[0-9]+\.(zip|tgz)", name):
        form, directory = "r-binary", "yamaa"
    else:
        raise ValueError(
            f"{name}: not a yamaa {identity} wheel, source archive or R binary package"
        )
    copies = "python" if form in {"wheel", "sdist"} else "R/yamaa/inst"
    return form, copies, {notice: f"{directory}/{notice}" for notice in notices}


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
    # A wheel and a Windows R binary package are ZIP archives; the source
    # archives and a macOS R binary package are gzipped tar archives.
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


def archive_files(name, data):
    """Yield (name, bytes) for every regular member of a package archive, and
    for every member of an xz-compressed tar archive inside it, named after
    the archive that holds it, as in `inner.tar.xz!vendor/crate/file`."""
    if name.endswith((".whl", ".zip")):
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            members = (
                (member.filename, lambda member=member: archive.read(member))
                for member in archive.infolist()
                if not member.is_dir()
            )
            yield from expanded(members)
    else:
        mode = "r:xz" if name.endswith(".tar.xz") else "r:gz"
        with tarfile.open(fileobj=io.BytesIO(data), mode=mode) as archive:
            members = (
                (member.name, lambda member=member: archive.extractfile(member).read())
                for member in archive
                if member.isfile()
            )
            yield from expanded(members)


def expanded(members):
    """Yield each (name, bytes) member, opening the xz-compressed tar archives
    among them; `members` pairs each name with a reader of its bytes."""
    for member, read in members:
        if member.endswith(".tar.xz"):
            for inner, contents in archive_files(member, read()):
                yield f"{member}!{inner}", contents
        else:
            yield member, read()


def gnu_findings(name, data):
    """Return a message for each text member of a package archive that names
    a GNU license, outside the vendored crates of VENDORED_GNU."""
    findings = []
    for member, payload in archive_files(name, data):
        try:
            text = payload.decode("utf-8")
        except UnicodeDecodeError:
            continue  # Compiled code and images hold no license text to read.
        found = notices.gnu_licenses(text)
        crate = VENDORED_CRATE.search(member)
        if found and (crate is None or crate[1] not in VENDORED_GNU):
            findings.append(f"{name}: {member} names a GNU license ({found[0]})")
    return findings


def check_archive(path, root=ROOT):
    """Raise ValueError unless the package carries every notice unchanged and
    no text in it names a GNU license outside VENDORED_GNU."""
    identity, names = package_notices(root)
    form, copies, members = notice_members(path.name, identity, names)
    payloads = read_members(path, set(members.values()))
    for notice, member in members.items():
        expected = (root / copies / notice).read_bytes()
        if notice == inventory.NAME:
            expected = packaged_inventory(form, expected, root)
        if payloads.get(member) != expected:
            raise ValueError(
                f"{path.name}: required notice differs or is missing: {member}"
            )
    findings = gnu_findings(path.name, path.read_bytes())
    if findings:
        raise ValueError("\n".join(findings))
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
