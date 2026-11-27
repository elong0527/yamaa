"""Refuse an unnumbered decision in the notes of a released version.

A pull request that makes a routine user-visible decision cites it as
"D-next" in the notes of the release in development, and the maintenance
session numbers it on the decision log, #4064 (AGENTS.md, Decisions). A
reader of the notes follows a citation to one decision only once it has its
number, so the notes of every version up to the one that
`tools/packaging/release.toml` names, the notes a release publishes, cite no
"D-next". The notes of a later version, such as `docs/releases/1.0.0.md`
while 0.9.0 is cut, are in development and may.

Notes that still cite "D-next" pass only while ALLOWLIST names the open issue
whose release numbers them. That entry fails once its notes cite no "D-next",
once its file is gone, and once its issue is closed. An issue's state is
GitHub's, and a repository check reads no network: the caller states the
issues it knows to be closed with `--closed`, as for
`tools.validation.check_source_size`.
"""

import argparse
import re
import sys
from pathlib import Path

from tools.packaging import release_identity
from tools.paths import ROOT

NOTES = Path("docs/releases")
PLACEHOLDER = "D-next"
# Each released notes file that still cites PLACEHOLDER, and the open issue
# whose release numbers those citations first.
ALLOWLIST = {}
HINT = (
    'Replace each "D-next" with the number the decision log, #4064, gives it; '
    "until the maintenance session numbers it there, an ALLOWLIST entry in "
    "tools/validation/check_release_notes.py names the open release issue "
    "that will."
)


def version(text):
    """An X.Y.Z version as a comparable tuple, or None for any other text."""
    if re.fullmatch(release_identity.VERSION, text) is None:
        return None
    return tuple(int(part) for part in text.split("."))


def citations(text):
    """The numbers of the lines of `text` that cite PLACEHOLDER."""
    return [
        number
        for number, line in enumerate(text.split("\n"), start=1)
        if PLACEHOLDER in line
    ]


def check(root, allowlist, closed=frozenset()):
    """Every citation of PLACEHOLDER in the notes of a version up to the one
    `root` declares that `allowlist` does not hold, and every entry of
    `allowlist` that no longer holds. `closed` holds the numbers of the
    issues known to be closed."""
    root = Path(root)
    declared = version(release_identity.read(root).version)
    errors = []
    cited = set()
    for path in sorted((root / NOTES).glob("*.md")):
        relative = path.relative_to(root)
        released = version(path.stem)
        if released is None or released > declared:
            continue
        lines = citations(path.read_text(encoding="utf-8"))
        if lines:
            cited.add(relative)
        if lines and relative not in allowlist:
            errors.extend(
                f"{relative.as_posix()}:{line}: cites {PLACEHOLDER}, "
                "a decision the log has not numbered"
                for line in lines
            )
    for relative, issue in sorted(allowlist.items()):
        entry = f"{relative.as_posix()}: allowed to cite {PLACEHOLDER} until #{issue}"
        if not (root / relative).is_file():
            errors.append(f"{entry}, but the file is gone; remove its entry")
        elif relative not in cited:
            errors.append(f"{entry}, but it cites none; remove its entry")
        elif issue in closed:
            errors.append(
                f"{entry}, and #{issue} is closed; number its decisions, "
                "or name the open issue that will"
            )
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument(
        "--closed",
        type=int,
        nargs="*",
        default=[],
        metavar="ISSUE",
        help="the issues known to be closed; an ALLOWLIST entry naming one fails",
    )
    args = parser.parse_args(argv)
    errors = check(args.root, ALLOWLIST, frozenset(args.closed))
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        print(HINT)
        return 1
    allowed = ", ".join(
        f"{path.as_posix()} until #{issue}" for path, issue in sorted(ALLOWLIST.items())
    )
    print(
        f"PASS: no released notes under {NOTES.as_posix()}/ cite {PLACEHOLDER}"
        + (f", but the allowlisted {allowed}." if allowed else ".")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
