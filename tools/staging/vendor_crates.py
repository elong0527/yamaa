"""Vendor the locked crates.io crates of a Rust workspace for an offline build.

A source package that builds without the network carries the crates that its
Cargo.lock selects. CRAN's guidance for packages that use Rust asks for the
crates to be bundled with `cargo vendor` and tarred with xz compression, and
its Repository Policy prefers "a vendor.tar.xz file" to a download at
installation and admits no binary executable code in a source package.

    python3 vendor_crates.py <workspace> <archive> --target <triple> [...]

runs `cargo vendor --locked --versioned-dirs` in the workspace. A crate that a
build for one of the targets compiles (compiled) keeps its files, apart from
every Rust toolchain file, the license texts in ALTERNATIVE_LICENSES and the
development directories audited in vendor_exclusions.toml, and must hold no
binary. Production inputs and license files stay. Any other crate keeps only
its manifest, which Cargo reads to resolve the lock, with an empty src/lib.rs
where Cargo needs one to read it (stub). The script checks that Cargo still
resolves the lock offline from what
remains, and writes the crates as a reproducible xz-compressed tar whose one
top-level directory is `vendor/`. A build unpacks the archive beside a Cargo
home whose config.toml holds CONFIG, so Cargo reads every crate from it and
never from the network.

The script uses only the Python standard library, so the R source package
carries it unchanged as tools/vendor_crates.py beside vendor_exclusions.toml,
the script and policy that wrote its src/rust/vendor.tar.xz. Cargo's
`.cargo-checksum.json` files are the packaging tool's own records, like the
checksums in Cargo.lock: the archive keeps them
as Cargo wrote them, except that the entries of the left-out files are
dropped. The script computes no digest of its own.
"""

import argparse
import json
import lzma
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path

# The archive's one top-level directory, and the source replacement that a
# Cargo home's config.toml holds so Cargo reads the crates from it. Cargo
# resolves the relative directory against the parent of the Cargo home.
DIRECTORY = "vendor"
CONFIG = f"""[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "{DIRECTORY}"
"""
CHECKSUMS = ".cargo-checksum.json"
MANIFEST = "Cargo.toml"
# The library root that Cargo looks for when a manifest names no library path.
LIBRARY = "src/lib.rs"
POLICY = Path(__file__).with_name("vendor_exclusions.toml")
# The policy cannot select sources, manifests or arbitrary glob patterns.
DEVELOPMENT_ROOTS = {"tests", "benches", "examples", ".github"}
# A fixed time, owner and mode make the archive's bytes depend only on the
# crates, on any operating system.
MTIME = 946684800  # 2000-01-01T00:00:00Z
PRESET = 9 | lzma.PRESET_EXTREME

# License texts that a crate ships for an alternative to the license that
# yamaa relies on, by crate name and path in the crate. The packages carry
# only the license information that they need (#3030 D136), so these are left
# out; a listed text that the crate no longer ships stops vendoring until this
# table is reviewed.
ALTERNATIVE_LICENSES = {
    # zstd is offered under BSD-3-Clause, whose text zstd/LICENSE keeps and
    # which yamaa relies on, or under the license whose text is COPYING.
    "zstd-sys": ("zstd/COPYING",),
}
# rustup reads a toolchain file in the directory that a command runs in and
# in its parents, and Cargo runs a crate's build script in that crate's
# directory. A crate's own file there could make rustup select another
# toolchain, which an offline build cannot download, so every one is left out.
TOOLCHAIN_FILES = ("rust-toolchain", "rust-toolchain.toml")
# Leading bytes of executable or linkable code: ELF, Mach-O and universal
# binaries (the last shared with Java classes), ar archives (static libraries)
# and WebAssembly modules. PE files are recognized by their DOS and PE headers.
MAGIC = (
    b"\x7fELF",
    b"\xfe\xed\xfa\xce",
    b"\xfe\xed\xfa\xcf",
    b"\xce\xfa\xed\xfe",
    b"\xcf\xfa\xed\xfe",
    b"\xca\xfe\xba\xbe",
    b"!<arch>\n",
    b"\x00asm",
)
SUFFIXES = {
    ".a",
    ".class",
    ".dll",
    ".dylib",
    ".exe",
    ".jar",
    ".lib",
    ".o",
    ".obj",
    ".so",
    ".wasm",
}


def is_binary(path):
    """Whether a file holds executable or linkable code."""
    if path.suffix.lower() in SUFFIXES or ".so." in path.name:
        return True
    with path.open("rb") as stream:
        head = stream.read(4096)
    if head.startswith(MAGIC):
        return True
    if head.startswith(b"MZ") and len(head) >= 64:
        offset = int.from_bytes(head[0x3C:0x40], "little")
        return head[offset : offset + 4] == b"PE\x00\x00"
    return False


def manifest(crate):
    """Return a vendored crate's parsed manifest."""
    return tomllib.loads((crate / MANIFEST).read_text(encoding="utf-8"))


def package(crate):
    """Return the name and version that a vendored crate's manifest declares."""
    declared = manifest(crate)["package"]
    return declared["name"], declared["version"]


def cargo(workspace, directory, *arguments):
    """Run a Cargo command in the workspace, offline and with the locked
    versions, reading the crates.io crates from the vendor directory, and
    return its output."""
    location = json.dumps(str(Path(directory).absolute()), ensure_ascii=False)
    command = ["cargo", *arguments, "--locked", "--offline"]
    for setting in (
        'source.crates-io.replace-with="vendored-sources"',
        f"source.vendored-sources.directory={location}",
    ):
        command += ["--config", setting]
    return subprocess.run(
        command, cwd=workspace, stdout=subprocess.PIPE, text=True, check=True
    ).stdout


def compiled(workspace, directory, targets):
    """The (name, version) of each package that a build for one of `targets`
    compiles, the workspace members included.

    `cargo tree` gives the packages that each target links, as Cargo's feature
    resolver selects them; `cargo metadata` would also list the optional
    dependencies that only a weak feature or another platform's dependency
    enables, which no build compiles. A build also compiles, for the machine
    that runs it, those packages' procedural macros and their build scripts'
    dependencies. A source package is built on a machine of the target that it
    is installed for, so every package that those reach through the
    dependencies that `cargo metadata` resolves for the target counts too, and
    the result does not depend on the machine that vendors the crates.
    """
    found = set()
    for target in targets:
        output = cargo(
            workspace,
            directory,
            *("tree", "--workspace", "--all-features", "--target", target),
            *("--edges", "normal,no-proc-macro", "--prefix", "none"),
            *("--format", "{p}"),
        )
        linked = {
            (line.split()[0], line.split()[1].removeprefix("v"))
            for line in output.splitlines()
            if line.strip()
        }
        resolved = json.loads(
            cargo(
                workspace,
                directory,
                *("metadata", "--all-features", "--format-version", "1"),
                *("--filter-platform", target),
            )
        )
        packages = {package["id"]: package for package in resolved["packages"]}
        nodes = {node["id"]: node["deps"] for node in resolved["resolve"]["nodes"]}

        def key(identifier):
            return packages[identifier]["name"], packages[identifier]["version"]

        def build_time(dependency):
            """Whether a build compiles the dependency for the machine that
            runs it: a build dependency or a procedural macro."""
            kinds = {kind["kind"] for kind in dependency["dep_kinds"]}
            package = packages[dependency["pkg"]]
            macro = any("proc-macro" in item["kind"] for item in package["targets"])
            return "build" in kinds or macro

        pending = [
            dependency["pkg"]
            for identifier, dependencies in nodes.items()
            if key(identifier) in linked
            for dependency in dependencies
            if build_time(dependency)
        ]
        reached = set()
        while pending:
            identifier = pending.pop()
            if identifier not in reached:
                reached.add(identifier)
                pending.extend(dependency["pkg"] for dependency in nodes[identifier])
        found |= linked | {key(identifier) for identifier in reached}
    return found


def binaries(crate, label):
    """Refuse any binary in a crate that a target compiles: CRAN's policy
    admits no binary executable code in a source package."""
    for path in sorted(crate.rglob("*")):
        if path.is_file() and is_binary(path):
            relative = path.relative_to(crate).as_posix()
            raise ValueError(
                f"{label} ships the prebuilt binary {relative}, which a source "
                "package may not carry; review the crate"
            )


def alternative_licenses(crate, name, label):
    """Return the license texts of ALTERNATIVE_LICENSES that a crate ships;
    refuse a listed text that it no longer ships."""
    listed = sorted(ALTERNATIVE_LICENSES.get(name, ()))
    for relative in listed:
        if not (crate / relative).is_file():
            raise ValueError(
                f"{label} no longer ships the listed license text {relative}; "
                "review ALTERNATIVE_LICENSES"
            )
    return listed


def toolchain_files(crate):
    """Return the Rust toolchain files anywhere in a crate."""
    return [
        path.relative_to(crate).as_posix()
        for path in sorted(crate.rglob("*"))
        if path.is_file() and path.name in TOOLCHAIN_FILES
    ]


def load_development_policy(path=POLICY):
    """Read the version-specific allowlist of audited development directories.

    Unknown crate versions keep their development files. Limiting entries to four
    top-level directory names prevents a typo from selecting production code.
    """
    policy = tomllib.loads(Path(path).read_text(encoding="utf-8"))
    if set(policy) != {"development_directories"} or not isinstance(
        policy["development_directories"], dict
    ):
        raise ValueError(f"{path}: expected a development_directories table")
    entries = policy["development_directories"]
    for crate, directories in entries.items():
        if (
            not isinstance(directories, list)
            or not directories
            or any(
                not isinstance(name, str) or name not in DEVELOPMENT_ROOTS
                for name in directories
            )
            or len(set(directories)) != len(directories)
        ):
            raise ValueError(
                f"{path}: {crate} must name unique development directories"
            )
    return entries


def development_files(crate, policy):
    """List only audited development files, preserving production and legal inputs.

    Readme/license paths, the build script and explicit library/binary paths
    in the manifest remain even if they sit in a development directory. The
    allowlist keeps crates' embedded documentation inputs too: directory
    names alone do not prove that their contents are unused by a build.
    """
    directories = policy.get(crate.name, ())
    if not directories:
        return []
    declared = manifest(crate)
    package = declared["package"]
    protected = {
        Path(value).as_posix()
        for name in ("readme", "license-file", "build")
        if isinstance(value := package.get(name), str)
    }
    for target in (declared.get("lib", {}), *declared.get("bin", [])):
        if isinstance(target.get("path"), str):
            protected.add(Path(target["path"]).as_posix())
    found = []
    for directory in directories:
        root = crate / directory
        if not root.is_dir():
            raise ValueError(f"{crate.name}: audited directory {directory} is missing")
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            relative = path.relative_to(crate).as_posix()
            legal = (
                path.name.lstrip(".")
                .upper()
                .startswith(("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"))
            )
            if relative not in protected and not legal:
                found.append(relative)
    return sorted(found)


def leave_out(crate, paths, label):
    """Remove files from a crate, with their entries in its checksum record,
    and the directories that the removal empties.

    Cargo checks every file that the record lists when it builds the crate,
    so the remaining entries stay complete. The package checksum, which Cargo
    compares with Cargo.lock, stays as Cargo wrote it.
    """
    if not paths:
        return
    checksums = crate / CHECKSUMS
    record = json.loads(checksums.read_text(encoding="utf-8"))
    for relative in paths:
        if relative not in record["files"]:
            raise ValueError(f"{label}: {CHECKSUMS} does not list {relative}")
    for relative in paths:
        (crate / relative).unlink()
        del record["files"][relative]
    for path in sorted(crate.rglob("*"), reverse=True):
        if path.is_dir() and not any(path.iterdir()):
            path.rmdir()
    # Cargo writes this record as compact JSON with sorted keys.
    checksums.write_text(
        json.dumps(record, separators=(",", ":"), sort_keys=True),
        encoding="utf-8",
    )


def stub(crate, label):
    """Reduce a crate that no target compiles to its manifest, which Cargo
    reads to resolve the lock, and return the paths left out.

    Cargo checks the files that a checksum record lists only when it compiles
    the crate, so the record keeps the manifest's entry alone. A manifest that
    names no library path leaves the library to Cargo's discovery of LIBRARY,
    without which Cargo cannot read the manifest: an empty LIBRARY, which the
    record does not list, takes the place of the crate's, so that the package
    carries none of the crate's code.
    """
    declared = manifest(crate)
    left_out = sorted(
        relative
        for relative in (
            path.relative_to(crate).as_posix()
            for path in crate.rglob("*")
            if path.is_file()
        )
        if relative not in {MANIFEST, CHECKSUMS}
    )
    leave_out(crate, left_out, label)
    if "path" not in declared.get("lib", {}):
        (crate / LIBRARY).parent.mkdir(parents=True, exist_ok=True)
        (crate / LIBRARY).write_bytes(b"")
    return left_out


def vendor(workspace, directory, targets):
    """Vendor the workspace's locked registry crates into a new directory.

    A crate that one of `targets` compiles keeps its files but the left-out
    ones; any other keeps only its manifest (stub). Returns (name, version,
    left-out paths, whether the crate is kept whole) for each crate, in name
    order.
    """
    policy = load_development_policy()
    directory = Path(directory)
    if directory.exists():
        raise FileExistsError(f"{directory} already exists")
    subprocess.run(
        ["cargo", "vendor", "--locked", "--versioned-dirs", "--quiet", str(directory)],
        cwd=workspace,
        stdout=subprocess.DEVNULL,
        check=True,
    )
    selected = compiled(workspace, directory, targets)
    crates = []
    for crate in sorted(directory.iterdir()):
        if crate.is_symlink() or not crate.is_dir():
            raise ValueError(f"unexpected vendored entry {crate.name}")
        if not (crate / CHECKSUMS).is_file():
            raise ValueError(f"vendored crate {crate.name} has no {CHECKSUMS}")
        if any(path.is_symlink() for path in crate.rglob("*")):
            raise ValueError(f"vendored crate {crate.name} contains a link")
        name, version = package(crate)
        if crate.name != f"{name}-{version}":
            raise ValueError(f"vendored crate {crate.name} declares {name} {version}")
        whole = (name, version) in selected
        if whole:
            left_out = sorted(
                set(
                    toolchain_files(crate)
                    + alternative_licenses(crate, name, crate.name)
                    + development_files(crate, policy)
                )
            )
            leave_out(crate, left_out, crate.name)
            binaries(crate, crate.name)
        else:
            left_out = stub(crate, crate.name)
        crates.append((name, version, left_out, whole))
    unused = set(ALTERNATIVE_LICENSES) - {name for name, *_ in crates}
    if unused:
        raise ValueError(
            f"ALTERNATIVE_LICENSES lists {sorted(unused)[0]}, which is not vendored"
        )
    # Cargo reads every vendored manifest before it builds anything, so the
    # lock must still resolve from what remains.
    cargo(workspace, directory, "metadata", "--format-version", "1")
    return sorted(crates, key=lambda crate: (crate[0], crate[1]))


def write_archive(directory, archive):
    """Write a vendor directory as a reproducible xz-compressed tar.

    Members sit below DIRECTORY in path order, with a fixed time, no owner and
    mode 0644 or 0755, so the bytes depend only on the files.
    """
    directory, archive = Path(directory), Path(archive)
    entries = sorted(
        directory.rglob("*"), key=lambda path: path.relative_to(directory).as_posix()
    )
    with tarfile.open(
        archive, "x:xz", preset=PRESET, format=tarfile.GNU_FORMAT
    ) as output:
        for path in [directory, *entries]:
            if path.is_symlink() or not (path.is_dir() or path.is_file()):
                raise ValueError(f"{path}: only files and directories are archived")
            relative = path.relative_to(directory).as_posix()
            member = tarfile.TarInfo(
                DIRECTORY if relative == "." else f"{DIRECTORY}/{relative}"
            )
            member.mtime = MTIME
            if path.is_dir():
                member.type, member.mode = tarfile.DIRTYPE, 0o755
                output.addfile(member)
            else:
                member.size, member.mode = path.stat().st_size, 0o644
                with path.open("rb") as stream:
                    output.addfile(member, stream)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("workspace", type=Path, help="directory of Cargo.lock")
    parser.add_argument("archive", type=Path, help="the new .tar.xz archive")
    parser.add_argument(
        "--target",
        action="append",
        required=True,
        dest="targets",
        help="a target that the package builds for; repeat it for each target",
    )
    arguments = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="vendor-crates-") as scratch:
        directory = Path(scratch) / DIRECTORY
        crates = vendor(arguments.workspace.resolve(), directory, arguments.targets)
        write_archive(directory, arguments.archive)
    stubs = sum(not whole for *_, whole in crates)
    print(f"{arguments.archive}: {len(crates)} crates, {stubs} of them manifests alone")


if __name__ == "__main__":
    main()
