"""Compare the frozen interface manifests with the latest release (#3030 D20).

From 1.0.0, every 1.x release keeps the 1.0 interface (#3030 section 2.11,
P-STAB-1; docs/design/interface-1.0/stability.md at tag yamaa-v0.6.2). This
check, which `make check-repository`, `make quick` and CI's repository job
run, compares each interface manifest of the working tree with the same
manifest at a baseline release tag, and refuses a breaking change.

Baseline. The release identity comes from tools/packaging/release.toml,
through tools/packaging/release_identity.py, and its major version M decides:

- major 0 is never enforced: the interface freezes at 1.0.0;
- otherwise the baseline is the newest tag named exactly yamaa-v<M>.<m>.<p>.
  Candidate tags, yamaa-vX.Y.Z-rc.N, never count (D1);
- at M.0.0, a candidate included, with no tag of major M, the check compares
  with the newest final tag of an earlier major, prints every break for the
  release notes, and passes;
- at any other version with no tag of major M, it fails and asks for the
  tags to be fetched, so a shallow clone cannot pass by accident.

Comparers. COMPARERS is the one registry of manifest comparers. It names
modules, as tools/commands/repository.py names checks, and each module
defines

    compare(baseline, current) -> iterable of Change

`baseline` is the tag's `Tree` and `current` the working tree's (both from
tools/validation/interface_manifests.py). A Tree reads repository-relative
POSIX paths: `read(path)` gives the bytes and `text(path)` the UTF-8 text,
each None for a file the tree lacks, and `files(directory)` lists the files
under a directory. Its `root` and `ref`, None for the working tree, let a
comparer run git or a loader of its own, such as `load_ref` of
tools/rules/model.py. Each `Change` names its `manifest`, one `element` of it
and `what` changed, and says whether it is `breaking`; an addition is not. A
comparer raises `ManifestError` when it cannot compare, for instance when a
manifest is missing from the working tree, and the check fails. A new
manifest needs only its module and one line in COMPARERS.

Exceptions. tools/validation/interface_exceptions.yaml lists the reviewed
defect fixes that must change a frozen element. Each entry names one
`element` of one `manifest`, exactly as the check prints them, with the
`rule` the fix restores and the `issue` that records the owner's decision.
The check skips exactly the breaks that the entries name. An entry that
matches no break is reported but does not fail: the next release tag carries
the fix, so the entry is then unused and can be removed.

Output. One line per break, `<manifest>: <element>: <what changed>`, then the
excepted breaks and the additions in the same form. The check exits 1 while a
break remains against a baseline of the release's own major.
"""

import argparse
import importlib
import re
import sys
from pathlib import Path

import yaml
from tools.packaging import release_identity
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.interface_manifests import ManifestError, Tree, git

COMPARERS = (
    "tools.validation.interface_lines",
    "tools.validation.interface_schemas",
    "tools.validation.interface_sources",
)
EXCEPTIONS = "tools/validation/interface_exceptions.yaml"
EXCEPTION_FIELDS = ("manifest", "element", "rule", "issue")
RULE = re.compile(r"[A-Z]+-[1-9][0-9]*")
TAGS = "'+refs/tags/yamaa-v*:refs/tags/yamaa-v*'"


def count(items, noun):
    return f"{len(items)} {noun}{'' if len(items) == 1 else 's'}"


def load_exceptions(root):
    """Map each (manifest, element) that the exception list names to its entry.

    Raise ManifestError for a missing or unreadable list, or any other shape.
    """
    try:
        document = yaml.load(
            (Path(root) / EXCEPTIONS).read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )
    except (OSError, UnicodeError, ValueError, yaml.YAMLError) as error:
        raise ManifestError(f"{EXCEPTIONS}: {error}") from None
    if not (
        isinstance(document, dict)
        and set(document) == {"exceptions"}
        and isinstance(document["exceptions"], list)
    ):
        raise ManifestError(
            f"{EXCEPTIONS}: expected `exceptions`, a list of entries, and nothing else"
        )
    entries = {}
    for index, entry in enumerate(document["exceptions"]):
        where = f"{EXCEPTIONS}: exceptions[{index}]"
        if not isinstance(entry, dict) or set(entry) != set(EXCEPTION_FIELDS):
            raise ManifestError(
                f"{where}: expected exactly {', '.join(EXCEPTION_FIELDS)}"
            )
        for field in ("manifest", "element"):
            value = entry[field]
            if not (
                isinstance(value, str)
                and value
                and value == value.strip()
                and "\n" not in value
            ):
                raise ManifestError(f"{where}.{field}: expected one line of text")
        if not (isinstance(entry["rule"], str) and RULE.fullmatch(entry["rule"])):
            raise ManifestError(
                f"{where}.rule: expected the ID of the rule card the fix restores"
            )
        if type(entry["issue"]) is not int or entry["issue"] < 1:
            raise ManifestError(
                f"{where}.issue: expected the number of the issue that records "
                "the owner's decision"
            )
        key = (entry["manifest"], entry["element"])
        if key in entries:
            raise ManifestError(f"{where}: repeats the entry for {key[0]}: {key[1]}")
        entries[key] = entry
    return entries


def release_tags(root):
    """Map the (major, minor, patch) of each final release tag to its name."""
    tags = {}
    for name in git(root, "tag", "--list", "yamaa-v*").decode("utf-8").split():
        match = release_identity.TAG.fullmatch(name)
        if match and match[2] is None:
            tags[tuple(int(part) for part in match[1].split("."))] = name
    return tags


def fetch_advice(root):
    """How to fetch the release tags into this clone."""
    try:
        shallow = git(root, "rev-parse", "--is-shallow-repository").strip() == b"true"
    except ManifestError:
        shallow = False
    if shallow:
        return (
            "fetch the release tags into this shallow clone with "
            f"git fetch --no-tags --depth=1 origin {TAGS}"
        )
    return f"fetch the release tags with git fetch --no-tags origin {TAGS}"


def compare(baseline, current):
    """Every change that the registered comparers find."""
    return [
        change
        for name in COMPARERS
        for change in importlib.import_module(name).compare(baseline, current)
    ]


def check(root):
    """The check's output lines and exit status for the repository at `root`."""
    try:
        identity = release_identity.read(root)
        exceptions = load_exceptions(root)
    except (ValueError, ManifestError) as error:
        return [f"ERROR: {error}"], 1
    major, minor, patch = (int(part) for part in identity.version.split("."))
    if major == 0:
        return [
            f"PASS: {identity} is a major 0 release, which the interface "
            "stability check never enforces; the interface freezes at 1.0.0"
        ], 0
    try:
        tags = release_tags(root)
    except ManifestError as error:
        return [f"ERROR: cannot list the release tags: {error}"], 1
    own = [version for version in tags if version[0] == major]
    earlier = [version for version in tags if version[0] < major]
    starts_major = minor == 0 and patch == 0
    if own:
        tag, enforced = tags[max(own)], True
    elif starts_major and earlier:
        tag, enforced = tags[max(earlier)], False
    elif starts_major:
        return [
            f"PASS: {identity} starts major {major}, and no final release "
            "tag of an earlier major is present to list its breaking changes "
            f"against; to list them, {fetch_advice(root)}"
        ], 0
    else:
        return [
            f"ERROR: {identity} is compared with the newest "
            f"yamaa-v{major}.<minor>.<patch> tag, and none is present: "
            f"{fetch_advice(root)}"
        ], 1
    try:
        changes = compare(Tree(root, tag), Tree(root))
    except ManifestError as error:
        return [f"ERROR: {error}"], 1
    breaks, excepted, additions = [], [], []
    for change in changes:
        if not change.breaking:
            additions.append(change)
        elif (change.manifest, change.element) in exceptions:
            excepted.append(change)
        else:
            breaks.append(change)
    lines = []
    if breaks:
        purpose = "" if enforced else f", for the release notes of {identity.version}"
        lines += [f"Breaking changes against {tag}{purpose}:", *map(str, breaks)]
    if excepted:
        lines.append(f"Breaking changes against {tag} that {EXCEPTIONS} excepts:")
        for change in excepted:
            entry = exceptions[change.manifest, change.element]
            lines.append(f"{change} ({entry['rule']}, #{entry['issue']})")
    if additions:
        lines += [f"Additions against {tag}:", *map(str, additions)]
    used = {(change.manifest, change.element) for change in excepted}
    lines += [
        f"NOTE: {EXCEPTIONS}: {manifest}: {element} matches no breaking change "
        f"against {tag}; remove the entry"
        for manifest, element in exceptions
        if (manifest, element) not in used
    ]
    if enforced and breaks:
        lines.append(
            f"ERROR: {count(breaks, 'breaking change')} against {tag}, the newest "
            f"release of major {major}. Only a new major release may break the "
            "frozen interface; a reviewed defect fix lists the element it "
            f"changes in {EXCEPTIONS}"
        )
        return lines, 1
    if enforced:
        lines.append(
            f"PASS: no breaking change against {tag}, the newest release of major "
            f"{major}; {count(additions, 'addition')}"
        )
    else:
        lines.append(
            f"PASS: {identity} starts major {major}, so the check lists its "
            f"{count(breaks, 'breaking change')} against {tag} for the release "
            f"notes and refuses none; {count(additions, 'addition')}"
        )
    return lines, 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=ROOT, help="Repository root directory"
    )
    args = parser.parse_args(argv)
    lines, status = check(args.root)
    for line in lines:
        print(line)
    return status


if __name__ == "__main__":
    sys.exit(main())
