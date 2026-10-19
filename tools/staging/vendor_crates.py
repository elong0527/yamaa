"""Vendor the locked crates.io crates of a Rust workspace for an offline build.

A source package that builds without the network carries the crates that its
Cargo.lock selects. CRAN's guidance for packages that use Rust asks for the
crates to be bundled with `cargo vendor` and tarred with xz compression, and
its Repository Policy prefers "a vendor.tar.xz file" to a download at
installation and admits no binary executable code in a source package.

    python3 vendor_crates.py <workspace> <archive>

runs `cargo vendor --locked --versioned-dirs` in the workspace, leaves out the
reviewed prebuilt binaries that PREBUILT lists, refuses any other binary, and
writes the crates as a reproducible xz-compressed tar whose one top-level
directory is `vendor/`. A build unpacks the archive beside a Cargo home whose
config.toml holds CONFIG, so Cargo reads every crate from it and never from
the network.

The script uses only the Python standard library, so the R source package
carries it unchanged as tools/vendor_crates.py, the script that wrote its
src/rust/vendor.tar.xz. Cargo's `.cargo-checksum.json` files are the packaging
tool's own records, like the checksums in Cargo.lock: the archive keeps them
as Cargo wrote them, except that the entries of the left-out binaries are
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
# A fixed time, owner and mode make the archive's bytes depend only on the
# crates, on any operating system.
MTIME = 946684800  # 2000-01-01T00:00:00Z
PRESET = 9 | lzma.PRESET_EXTREME

# Prebuilt binaries that a crate ships, by crate name and path in the crate.
# CRAN's policy admits no binary executable code in a source package, so they
# are left out. Each crate listed here builds only for targets that the
# packages never build, so no build reads the files; any other binary stops
# vendoring until it is reviewed and listed.
PREBUILT = {
    # The component-model runtime for wasm32 targets, which getrandom reaches
    # through wasip2: WebAssembly objects and an archive of them.
    "wit-bindgen": (
        "src/rt/libwit_bindgen_cabi.a",
        "src/rt/wit_bindgen_cabi_realloc.o",
        "src/rt/wit_bindgen_cabi_wasip3.o",
        "wasi-cli@0.2.0.wasm",
    ),
}
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


def package(crate):
    """Return the name and version that a vendored crate's manifest declares."""
    manifest = tomllib.loads((crate / "Cargo.toml").read_text(encoding="utf-8"))
    return manifest["package"]["name"], manifest["package"]["version"]


def leave_out_prebuilt(crate, name, label):
    """Remove a crate's reviewed prebuilt binaries; refuse any other binary.

    Returns the removed paths. Their entries leave the crate's checksum file,
    which Cargo reads when it builds the crate, so the remaining records stay
    complete.
    """
    listed = set(PREBUILT.get(name, ()))
    found = []
    for path in sorted(crate.rglob("*")):
        if path.is_file() and is_binary(path):
            relative = path.relative_to(crate).as_posix()
            if relative not in listed:
                raise ValueError(
                    f"{label} ships the prebuilt binary {relative}; review it "
                    "and list it in PREBUILT"
                )
            found.append(relative)
    missing = sorted(listed - set(found))
    if missing:
        raise ValueError(
            f"{label} no longer ships the listed binary {missing[0]}; review PREBUILT"
        )
    if found:
        checksums = crate / CHECKSUMS
        record = json.loads(checksums.read_text(encoding="utf-8"))
        for relative in found:
            (crate / relative).unlink()
            del record["files"][relative]
        # Cargo writes this record as compact JSON with sorted keys.
        checksums.write_text(
            json.dumps(record, separators=(",", ":"), sort_keys=True),
            encoding="utf-8",
        )
    return found


def vendor(workspace, directory):
    """Vendor the workspace's locked registry crates into a new directory.

    Returns (name, version, left-out paths) for each crate, in name order.
    """
    directory = Path(directory)
    if directory.exists():
        raise FileExistsError(f"{directory} already exists")
    subprocess.run(
        ["cargo", "vendor", "--locked", "--versioned-dirs", "--quiet", str(directory)],
        cwd=workspace,
        stdout=subprocess.DEVNULL,
        check=True,
    )
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
        crates.append((name, version, leave_out_prebuilt(crate, name, crate.name)))
    unused = set(PREBUILT) - {name for name, _, _ in crates}
    if unused:
        raise ValueError(f"PREBUILT lists {sorted(unused)[0]}, which is not vendored")
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
    arguments = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="vendor-crates-") as scratch:
        directory = Path(scratch) / DIRECTORY
        crates = vendor(arguments.workspace.resolve(), directory)
        write_archive(directory, arguments.archive)
    print(f"{arguments.archive}: {len(crates)} crates")


if __name__ == "__main__":
    main()
