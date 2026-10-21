"""What a comparer of the interface stability check reads and reports.

`tools/validation/check_interface_stability.py` documents the comparers and
their registry. Each comparer reads two `Tree`s, the baseline release tag's
and the working tree's, reports each difference as a `Change`, and raises
`ManifestError` when it cannot compare. These types live apart from the check
so that a comparer imports the same classes the check uses, also when the
check runs as `python -m`.
"""

import subprocess
from dataclasses import dataclass
from pathlib import Path


class ManifestError(Exception):
    """A manifest, the baseline or the exception list cannot be read."""


@dataclass(frozen=True)
class Change:
    """One element of a manifest that differs from the baseline.

    `manifest` names the manifest, such as its repository path, and `element`
    one element of it; `what` says what changed. An addition is not
    `breaking`.
    """

    manifest: str
    element: str
    what: str
    breaking: bool

    def __str__(self):
        return f"{self.manifest}: {self.element}: {self.what}"


def git(root, *arguments):
    """The standard output of a git command in the repository at `root`."""
    result = subprocess.run(
        ["git", "-C", str(root), *arguments], capture_output=True, check=False
    )
    if result.returncode:
        message = result.stderr.decode("utf-8", "replace").strip()
        raise ManifestError(f"git {' '.join(arguments)}: {message}")
    return result.stdout


class Tree:
    """The files of the repository at `root`: at the tag `ref`, or, when `ref`
    is None, in the working tree.

    Paths are repository-relative and POSIX. A tag's file is read with
    `git cat-file blob <tag>:<path>`, as `load_ref` in tools/rules/model.py
    reads a revision's rules.
    """

    def __init__(self, root, ref=None):
        self.root = Path(root)
        self.ref = ref

    def __str__(self):
        return self.ref or "the working tree"

    def files(self, directory):
        """The paths of the files under `directory`, sorted."""
        if self.ref is None:
            base = self.root / directory
            if not base.is_dir():
                return []
            return sorted(
                path.relative_to(self.root).as_posix()
                for path in base.rglob("*")
                if path.is_file()
            )
        listed = git(
            self.root,
            *("ls-tree", "--full-tree", "-r", "-z", "--name-only"),
            *(self.ref, "--", directory),
        )
        return sorted(filter(None, listed.decode("utf-8").split("\0")))

    def read(self, path):
        """The bytes of the file at `path`, or None when the tree has none."""
        if self.ref is None:
            file = self.root / path
            return file.read_bytes() if file.is_file() else None
        # One entry, `<mode> <type> <object>\t<path>`, when the path exists.
        listed = git(self.root, "ls-tree", "--full-tree", "-z", self.ref, "--", path)
        entry, _, name = listed.decode("utf-8").removesuffix("\0").partition("\t")
        if name != path or entry.split()[1:2] != ["blob"]:
            return None
        return git(self.root, "cat-file", "blob", f"{self.ref}:{path}")

    def text(self, path):
        """The file at `path` as UTF-8 text, or None when the tree has none."""
        data = self.read(path)
        if data is None:
            return None
        try:
            return data.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ManifestError(f"{path} at {self}: not UTF-8: {error}") from None
