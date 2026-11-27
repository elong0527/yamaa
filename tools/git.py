"""The git commands tools run on a checkout, and a revision's files.

This module imports only the standard library and `tools.paths`, so the
workflow steps that run a tool without the tooling environment can use it.
"""

import subprocess
from pathlib import Path

from tools.paths import ROOT


class GitError(RuntimeError):
    """A git command failed, or a file of a tree is not UTF-8 text."""


def git(root, *arguments, input=None):
    """The standard output, as bytes, of a git command in the repository at
    `root`; `input` is its standard input. A failing command raises GitError
    with git's own message."""
    result = subprocess.run(
        ["git", "-C", str(root), *arguments],
        input=input,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        message = result.stderr.decode("utf-8", "replace").strip()
        raise GitError(f"git {' '.join(arguments)}: {message}")
    return result.stdout


def revision(root=ROOT):
    """The full commit name of the checkout at `root`."""
    return git(root, "rev-parse", "HEAD").decode("ascii").strip()


class Tree:
    """The files of the repository at `root`: at the revision `ref`, or, when
    `ref` is None, in the working tree.

    Paths are repository-relative and POSIX. A revision's file is read with
    `git cat-file blob <ref>:<path>`.
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
            raise GitError(f"{path} at {self}: not UTF-8: {error}") from None
