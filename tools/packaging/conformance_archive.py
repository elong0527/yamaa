"""Write and check the conformance archive that a release publishes.

The conformance archive (#3690) is yamaa-<version>-conformance.zip, where the
version is the release's in its Cargo spelling, as its tag and title give it,
so a candidate's reads 1.0.0-rc.1. It holds one directory of the same name,
and in it the closed grammars and the evaluation set of the expression
language, each at its path in the repository, beside the project license:
FILES below. Each member is the checkout's file, byte for byte, with Unix
mode 0644, so another implementation can check itself against the grammars'
vectors and the evaluation cases of the release that it names.

    python -m tools.packaging.conformance_archive write <directory>

writes the checkout's archive into <directory> and prints its path, and

    python -m tools.packaging.conformance_archive check <archive>...

checks archives against the checkout as the release selector does.
"""

import argparse
import stat
import time
import zipfile
from pathlib import Path

from tools.packaging import release_identity
from tools.packaging.cli_archive import UNIX, add
from tools.paths import ROOT

# The files of the archive, at their paths in the repository: the two closed
# grammars (yaml/grammar/README.md), the evaluation set of the expression
# language, and the project license that covers them.
FILES = (
    "yaml/grammar/expression.yaml",
    "yaml/grammar/string-template.yaml",
    "yaml/conformance/expressions.yaml",
    "LICENSE",
)
# Every member is a regular file that anyone may read.
MODE = stat.S_IFREG | 0o644


def name(identity):
    """The archive's file name; without .zip, the name of its directory."""
    return f"yamaa-{identity.cargo}-conformance.zip"


def layout(identity):
    """Map each member of the archive to the file it copies."""
    directory = name(identity).removesuffix(".zip")
    return {f"{directory}/{file}": file for file in FILES}


def write(directory, root=ROOT):
    """Write the archive of the checkout at `root` into `directory`, then
    check it; return its path. An existing archive is never overwritten."""
    identity = release_identity.read(root)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name(identity)
    with zipfile.ZipFile(path, "x") as archive:
        for member, file in layout(identity).items():
            source = root / file
            date_time = time.localtime(source.stat().st_mtime)[:6]
            add(archive, member, source.read_bytes(), MODE, date_time)
    check(path, root)
    return path


def check(path, root=ROOT):
    """Raise ValueError unless the archive is the release's and holds exactly
    FILES, each a regular file with Unix mode 0644 and the checkout's bytes."""
    identity = release_identity.read(root)
    if path.name != name(identity):
        raise ValueError(f"{path.name}: not the yamaa {identity} conformance archive")
    members = layout(identity)
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or set(names) != set(members):
            raise ValueError(
                f"{path.name}: must hold exactly " + ", ".join(sorted(members))
            )
        for entry in entries:
            if entry.create_system != UNIX or entry.external_attr >> 16 != MODE:
                raise ValueError(
                    f"{path.name}: {entry.filename} must be a regular file with "
                    f"Unix mode {stat.S_IMODE(MODE):o}"
                )
            file = members[entry.filename]
            if archive.read(entry) != (root / file).read_bytes():
                raise ValueError(
                    f"{path.name}: {entry.filename} differs from {file} in the checkout"
                )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    writing = commands.add_parser("write", help="archive the checkout's files")
    writing.add_argument("directory", type=Path)
    checking = commands.add_parser("check", help="check archives")
    checking.add_argument("archives", nargs="+", type=Path)
    arguments = parser.parse_args(argv)
    try:
        if arguments.command == "write":
            print(write(arguments.directory, ROOT))
        else:
            for archive in arguments.archives:
                check(archive, ROOT)
                print(f"{archive.name}: {', '.join(FILES)}, as committed")
    except (ValueError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
