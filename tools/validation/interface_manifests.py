"""What a comparer of the interface stability check reads and reports.

`tools/validation/check_interface_stability.py` documents the comparers and
their registry. Each comparer reads two `tools.git.Tree`s, the baseline
release tag's and the working tree's, reports each difference as a `Change`,
and raises `ManifestError` when it cannot compare; a tree that git cannot read
raises `tools.git.GitError`. These types live apart from the check
so that a comparer imports the same classes the check uses, also when the
check runs as `python -m`.
"""

from dataclasses import dataclass


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
