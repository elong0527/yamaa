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
packages' copies of the others. It also keeps LICENSE-MUSL.txt there, musl's
notice, which only the archives for the musl targets carry, because only
their executables link musl. The Python source archive's COMPONENTS.tsv is
the wheels' without its rust-std row, the Rust standard library that the
wheels link (source_inventory in the latter). An R binary package's is the R
source package's followed by the one rust-std row that the package's
configure step appends when R installs it, for the pinned toolchain that
builds the release binaries (installed_inventory). A command-line archive's
is the command line's committed inventory, which names that toolchain's
standard library, as the wheels' does, and the musl C library that the
archives for the musl targets alone link (#3030 D168).

No text in a package names a GNU license, apart from the clause of the LLVM
exception that names GPLv2 (generate_third_party_notices.gnu_licenses). That
covers every text member of the archive, and of the xz-compressed tar archive
of vendored crates inside the R source package. The exceptions are the
upstream files of the vendored crates in VENDORED_GNU, which the packages
carry unchanged, as `cargo vendor` writes them, and an R binary package's
namespace loader, which R writes itself (R_LOADER).
"""

import argparse
import io
import itertools
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
# Vendored crates whose upstream files name a GNU license, by crate name, with
# the reason that the source packages may carry those files. yamaa relies on
# each crate under the license that the notices and inventory name.
VENDORED_GNU = {
    "zstd-sys": (
        "the headers of the bundled zstd sources say that zstd is offered under "
        "BSD-3-Clause or GPL-2.0-only; yamaa relies on BSD-3-Clause, and "
        "tools/staging/vendor_crates.py leaves out the GPL-2.0 text"
    ),
    "zstd": "its Readme says that the zstd C library is dual BSD/GPLv2 licensed",
    "r-efi": (
        "no supported target compiles it, so the packages keep only its "
        "manifest, which Cargo reads to resolve the lock and which offers "
        "LGPL-2.1-or-later beside MIT and Apache-2.0; yamaa relies on MIT"
    ),
    "pyo3": "its changelog says that the MIT OR Apache-2.0 license is GPLv2-compatible",
    "num-bigint": "its README names the license of rug, another crate",
}
# A vendored crate's directory, as `cargo vendor --versioned-dirs` names it,
# at the root of an archive inside the package (after its name and !) or below.
VENDORED_CRATE = re.compile(
    r"(?:^|[/!])vendor/([A-Za-z0-9_-]+?)-[0-9]+\.[0-9]+\.[0-9]+[^/]*/"
)
# An R binary package's namespace loader, R/yamaa in the installed package
# (#3393, D136). R CMD INSTALL writes it itself, a copy of R's own
# share/R/nspackloader.R, whose header names the GNU General Public License
# that R is under, so it is R's build output, not text of yamaa's. The loader
# is a header of comment lines, the first naming R_LOADER_FILE, followed by
# R_LOADER_BODY and nothing else, whitespace aside; any other text at that
# path, and the same text anywhere else, is read as usual.
R_LOADER = "yamaa/R/yamaa"
R_LOADER_FILE = "File share/R/nspackloader.R"
R_LOADER_BODY = """
local({
    info <- loadingNamespaceInfo()
    pkg <- info$pkgname
    ns <- .getNamespace(as.name(pkg))
    if (is.null(ns))
        stop("cannot find namespace environment for ", pkg, domain = NA);
    dbbase <- file.path(info$libname, pkg, "R", pkg)
    lazyLoad(dbbase, ns, filter = function(n) n != ".__NAMESPACE__.")
})
"""


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
    The archive for a musl target also carries musl's notice.
    """
    python, r, cargo = identity.python, identity.r, identity.cargo
    archives = {
        f"yamaa-{cargo}-{target}.zip": target for target in native.CLI_TARGETS.values()
    }
    if archives.get(name) in third_party.MUSL_TARGETS:
        notices = (*notices, third_party.MUSL_NOTICE)
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
    if host == "cli" and notice not in third_party.CLI_NOTICES:
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


def r_loader(text):
    """Whether a text is R's namespace loader: comment lines, the first naming
    R_LOADER_FILE, then R_LOADER_BODY alone, whitespace aside."""
    lines = text.splitlines()
    header = len(list(itertools.takewhile(lambda line: line.startswith("#"), lines)))
    return (
        header > 0
        and lines[0].lstrip("#").split() == R_LOADER_FILE.split()
        and " ".join(lines[header:]).split() == R_LOADER_BODY.split()
    )


def gnu_findings(name, data, form):
    """Return a message for each text member of a package archive of `form`
    that names a GNU license, outside the vendored crates of VENDORED_GNU and
    an R binary package's namespace loader."""
    findings = []
    for member, payload in archive_files(name, data):
        try:
            text = payload.decode("utf-8")
        except UnicodeDecodeError:
            continue  # Compiled code and images hold no license text to read.
        found = third_party.gnu_licenses(text)
        crate = VENDORED_CRATE.search(member)
        loader = form == "r-binary" and member == R_LOADER and r_loader(text)
        if found and not loader and (crate is None or crate[1] not in VENDORED_GNU):
            findings.append(f"{name}: {member} names a GNU license ({found[0]})")
    return findings


def check_archive(path, root=ROOT):
    """Raise ValueError unless the package carries every notice unchanged and
    no text in it names a GNU license outside VENDORED_GNU and R_LOADER."""
    identity, names = package_notices(root)
    form, members = notice_members(path.name, identity, names)
    payloads = read_members(path, set(members.values()))
    for notice, member in members.items():
        expected = (root / checkout_copy(form, notice)).read_bytes()
        if notice == inventory.NAME:
            expected = packaged_inventory(form, expected, root)
        if payloads.get(member) != expected:
            raise ValueError(
                f"{path.name}: required notice differs or is missing: {member}"
            )
    findings = gnu_findings(path.name, path.read_bytes(), form)
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
