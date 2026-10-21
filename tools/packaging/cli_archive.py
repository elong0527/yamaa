"""Write and check the command-line archives that a release publishes.

A command-line archive (#3030 D12) is yamaa-<version>-<target>.zip, where the
version is the engine's in its Cargo spelling, which `yamaa --version`
prints, and the target is one of the Rust targets that tools/ci/native.py
declares. It holds one directory of the same name, and in it the yamaa
executable built for that target, yamaa.exe on Windows, beside the package
notices: the license files that python/pyproject.toml lists. The command
line's LICENSE-THIRD-PARTY.txt and COMPONENTS.tsv are its own, from
rust/crates/yamaa-cli/; the project license and the notices of the vendored
sources are the Python packages' copies. tools/packaging/check_package_notices.py
checks their bytes.

Each member carries Unix permissions, 0755 for the executable and 0644 for the
notices, so that unzip on Linux and macOS gives an executable that runs as it
is. The check also reads the executable's header, which must name the
target's format and architecture: ELF on Linux, Mach-O on macOS and PE on
Windows.

    python -m tools.packaging.cli_archive write <target> <executable> <directory>

writes the archive of an executable built for <target> into <directory> and
prints its path, and

    python -m tools.packaging.cli_archive check <archive>...

checks archives as the release selector does, apart from the notices' bytes.
"""

import argparse
import stat
import struct
import time
import zipfile
from pathlib import Path

from tools.ci import native
from tools.packaging import check_package_notices as notices
from tools.paths import ROOT

EXECUTABLE_MODE = stat.S_IFREG | 0o755
NOTICE_MODE = stat.S_IFREG | 0o644
# The ZIP "version made by" system under which unzip restores the permissions
# that a member's external attributes hold.
UNIX = 3
# The machine field of each executable format, by architecture.
MACHINES = {
    "ELF": {62: "x86_64", 183: "aarch64"},
    "Mach-O": {0x01000007: "x86_64", 0x0100000C: "aarch64"},
    "PE": {0x8664: "x86_64", 0xAA64: "aarch64"},
}


def name(identity, target):
    """The archive's file name; without .zip, the name of its directory."""
    return f"yamaa-{identity.cargo}-{target}.zip"


def executable(target):
    """The executable's file name on the target's system."""
    return "yamaa.exe" if "-windows-" in target else "yamaa"


def expected_format(target):
    """The executable format and architecture of a Rust target."""
    architecture = target.partition("-")[0]
    if "-linux-" in target:
        return "ELF", architecture
    if target.endswith("-apple-darwin"):
        return "Mach-O", architecture
    if target.endswith("-windows-msvc"):
        return "PE", architecture
    raise ValueError(f"{target}: not a command-line target")


def executable_format(payload):
    """The format and architecture that an executable's header names, or None:
    a 64-bit little-endian ELF file, a 64-bit Mach-O file or a PE file."""
    if payload[:6] == b"\x7fELF\x02\x01" and len(payload) >= 20:
        kind, machine = "ELF", struct.unpack_from("<H", payload, 18)[0]
    elif payload[:4] == b"\xcf\xfa\xed\xfe" and len(payload) >= 8:
        kind, machine = "Mach-O", struct.unpack_from("<I", payload, 4)[0]
    elif payload[:2] == b"MZ" and len(payload) >= 64:
        offset = struct.unpack_from("<I", payload, 60)[0]
        if payload[offset : offset + 4] != b"PE\0\0" or len(payload) < offset + 6:
            return None
        kind, machine = "PE", struct.unpack_from("<H", payload, offset + 4)[0]
    else:
        return None
    architecture = MACHINES[kind].get(machine)
    return None if architecture is None else (kind, architecture)


def layout(identity, target, root=ROOT):
    """The executable's member of the target's archive, and each notice's."""
    _, names = notices.package_notices(root)
    archive = name(identity, target)
    _, notice_members = notices.notice_members(archive, identity, names)
    return f"{archive.removesuffix('.zip')}/{executable(target)}", notice_members


def add(archive, member, payload, mode, date_time):
    entry = zipfile.ZipInfo(member, date_time=date_time)
    entry.create_system = UNIX
    entry.external_attr = mode << 16
    entry.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(entry, payload)


def write(target, binary, directory, root=ROOT):
    """Write the archive of `binary`, the executable built for `target`, into
    `directory`, then check it; return its path."""
    if target not in native.CLI_TARGETS.values():
        raise ValueError(f"{target}: not a command-line target")
    payload = binary.read_bytes()
    if executable_format(payload) != expected_format(target):
        raise ValueError(f"{binary}: not the yamaa executable for {target}")
    identity, _ = notices.package_notices(root)
    program, notice_members = layout(identity, target, root)
    date_time = time.localtime(binary.stat().st_mtime)[:6]
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name(identity, target)
    with zipfile.ZipFile(path, "x") as archive:
        add(archive, program, payload, EXECUTABLE_MODE, date_time)
        for notice, member in notice_members.items():
            copy = (root / notices.checkout_copy("cli", notice)).read_bytes()
            add(archive, member, copy, NOTICE_MODE, date_time)
    check(path, root)
    return path


def check(path, root=ROOT):
    """Return the target of a command-line archive; raise ValueError unless it
    holds exactly the target's executable and the notices, each a regular
    file with its mode. tools/packaging/check_package_notices.py checks the
    notices' bytes."""
    identity, _ = notices.package_notices(root)
    targets = {name(identity, target): target for target in native.CLI_TARGETS.values()}
    target = targets.get(path.name)
    if target is None:
        raise ValueError(f"{path.name}: not a yamaa {identity} command-line archive")
    program, notice_members = layout(identity, target, root)
    modes = dict.fromkeys(notice_members.values(), NOTICE_MODE)
    modes[program] = EXECUTABLE_MODE
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or set(names) != set(modes):
            raise ValueError(
                f"{path.name}: must hold exactly " + ", ".join(sorted(modes))
            )
        for entry in entries:
            if (
                entry.create_system != UNIX
                or entry.external_attr >> 16 != modes[entry.filename]
            ):
                raise ValueError(
                    f"{path.name}: {entry.filename} must be a regular file with "
                    f"Unix mode {stat.S_IMODE(modes[entry.filename]):o}"
                )
        payload = archive.read(program)
    if executable_format(payload) != expected_format(target):
        raise ValueError(
            f"{path.name}: {program} is not the yamaa executable for {target}"
        )
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    writing = commands.add_parser("write", help="archive an executable")
    writing.add_argument("target", choices=sorted(native.CLI_TARGETS.values()))
    writing.add_argument("executable", type=Path)
    writing.add_argument("directory", type=Path)
    checking = commands.add_parser("check", help="check archives")
    checking.add_argument("archives", nargs="+", type=Path)
    arguments = parser.parse_args()
    try:
        if arguments.command == "write":
            print(
                write(arguments.target, arguments.executable, arguments.directory, ROOT)
            )
        else:
            for archive in arguments.archives:
                target = check(archive, ROOT)
                print(f"{archive.name}: the yamaa executable for {target}")
    except (ValueError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
