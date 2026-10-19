"""The one identity of the release a tree prepares, and its spellings.

`tools/packaging/release.toml` writes the identity once: the final release
version X.Y.Z and, for a release candidate, its number N. Each package format
spells that identity in its own syntax (#3030 D53):

| Release | Python | Cargo | R | Tag |
| --- | --- | --- | --- | --- |
| Final | X.Y.Z | X.Y.Z | X.Y.Z | yamaa-vX.Y.Z |
| Candidate N | X.Y.ZrcN | X.Y.Z-rc.N | X.Y.Z | yamaa-vX.Y.Z-rc.N |

An R version holds digits and separators only, so the R package keeps the
final version through the candidates. A candidate lives in the tag, the Python
and Cargo versions and the release body.
"""

import re
import tomllib
from dataclasses import dataclass

from tools.paths import ROOT

DECLARATION = "tools/packaging/release.toml"
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
TAG = re.compile(rf"yamaa-v({VERSION})(?:-rc\.([1-9][0-9]*))?")
TAG_FORMS = "yamaa-vX.Y.Z or yamaa-vX.Y.Z-rc.N"


@dataclass(frozen=True)
class Identity:
    """A final release X.Y.Z, or its release candidate N."""

    version: str
    candidate: int | None = None

    def __post_init__(self):
        if not isinstance(self.version, str) or not re.fullmatch(VERSION, self.version):
            raise ValueError(f"the release version must be X.Y.Z, not {self.version!r}")
        if self.candidate is not None and (
            type(self.candidate) is not int or self.candidate < 1
        ):
            raise ValueError(
                "the release candidate must be a positive integer, not "
                f"{self.candidate!r}"
            )

    @property
    def python(self):
        """The PEP 440 version of the Python packages."""
        if self.candidate is None:
            return self.version
        return f"{self.version}rc{self.candidate}"

    @property
    def cargo(self):
        """The SemVer version of the Rust workspace."""
        if self.candidate is None:
            return self.version
        return f"{self.version}-rc.{self.candidate}"

    @property
    def r(self):
        """The R package version: the final version, also for a candidate."""
        return self.version

    @property
    def tag(self):
        return f"yamaa-v{self.cargo}"

    def previous(self):
        """The candidate before this one; None for candidate 1 or a final release."""
        if self.candidate is None or self.candidate == 1:
            return None
        return Identity(self.version, self.candidate - 1)

    def __str__(self):
        if self.candidate is None:
            return self.version
        return f"{self.version} release candidate {self.candidate}"


def from_tag(tag):
    """Return the identity a release tag names; raise ValueError for any other name."""
    match = TAG.fullmatch(tag) if isinstance(tag, str) else None
    if match is None:
        raise ValueError(f"{tag!r} is not a release tag: {TAG_FORMS}")
    return Identity(match[1], None if match[2] is None else int(match[2]))


def read(root=ROOT):
    """Return the identity that the tree's declaration names.

    Raise ValueError when the declaration is unreadable or has any other shape.
    """
    try:
        document = tomllib.loads((root / DECLARATION).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError(
            f"{DECLARATION}: unreadable release identity: {error}"
        ) from None
    if "version" not in document or set(document) - {"version", "candidate"}:
        raise ValueError(
            f"{DECLARATION}: declare `version` and an optional `candidate`, "
            "and nothing else"
        )
    try:
        return Identity(document["version"], document.get("candidate"))
    except ValueError as error:
        raise ValueError(f"{DECLARATION}: {error}") from None
