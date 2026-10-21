"""The source manifests of the interface stability check (#3030 P-STAB-1).

These manifests are read from the sources themselves in both trees, not from
a committed inventory. `compare` diffs the working tree's with the baseline
release tag's (`tools/validation/check_interface_stability.py`):

- `rust/crates/yamaa-core/src/diagnostic/mod.rs`, the core registry: the
  (phase, condition, rule) triple of each cause, read with the parser of
  `tools/validation/check_diagnostics.py`. A removed triple breaks: an
  existing failure would report another condition or rule. Cause names are
  not compared. A rule is the ID the cause cites, whichever rule files define
  it, so that 0.5.0's requirement IDs compare too. The
  registry must be in both trees, so when it moves the check fails naming its
  path, and the pull request that moves it adds the new path to REGISTRIES.
- `rules`: the ID of each rule card in `rules/**/*.md`, read with
  `tools/rules/model.py`. A removed card breaks, because numbers are never
  reused. A card may move to another rule file.
- `benchmarks`: each benchmark directory of the baseline,
  `benchmarks/<name>`, keeps its specification, project, input and expected
  files, byte for byte; a symbolic link's bytes are its target, as Git stores
  them. A file removed, changed or added there breaks, each named by its path,
  so an exception entry skips one file. A removed directory breaks as one
  element. Its README files and its `benchmark.yaml`, the review lifecycle
  and runner metadata, may change. A new directory is an addition.

A tag's benchmark files are read in one batch, with one `git ls-tree` and one
`git cat-file --batch`, rather than two git calls a file.
"""

import os
import re
import subprocess
from pathlib import Path, PurePosixPath

from tools.rules.model import load, load_ref
from tools.validation.check_diagnostics import cause_pattern, registry_lines
from tools.validation.interface_manifests import Change, ManifestError, git

# The core registry's path, newest first: a pull request that moves the
# registry adds its new path, and the older tags keep the old one.
REGISTRIES = ("rust/crates/yamaa-core/src/diagnostic/mod.rs",)
# A cause's rule as any release spells it. Whether a rule file defines the ID
# is tools/validation/check_diagnostics.py's concern.
CAUSE = cause_pattern(re.compile(r"[A-Z]+-[0-9]+"))
RULES = "rules"
BENCHMARKS = "benchmarks"
# The files of a benchmark directory that may change: README prose at any
# depth, and the metadata file at its top.
README = "README.md"
METADATA = "benchmark.yaml"


def compare(baseline, current):
    """Every change of the source manifests from `baseline` to `current`."""
    return [
        *_compare_identities(baseline, current),
        *_compare_cards(baseline, current),
        *_compare_benchmarks(baseline, current),
    ]


def identities(tree):
    """The core registry's path in `tree` and the distinct (phase, condition,
    rule) triples of its causes, in order. A cause without a rule has None."""
    for path in REGISTRIES:
        text = tree.text(path)
        if text is not None:
            break
    else:
        raise ManifestError(
            f"{' or '.join(REGISTRIES)}: the core diagnostic registry is "
            f"missing from {tree}; the pull request that moves it adds its new "
            "path to REGISTRIES in tools/validation/interface_sources.py"
        )
    lines = registry_lines(text)
    if not lines:
        raise ManifestError(f"{path} at {tree}: no cause in a `conditions!` block")
    triples = {}
    for line in lines:
        cause = CAUSE.fullmatch(line)
        if cause is None:
            raise ManifestError(f"{path} at {tree}: unrecognized cause: {line.strip()}")
        _, phase, condition, rule = cause.groups()
        cited = None if rule == "None" else rule[len('Some("') : -len('")')]
        triples.setdefault((phase, condition, cited))
    return path, list(triples)


def _triple(phase, condition, rule):
    return f"({phase}, {condition}, {'null' if rule is None else rule})"


def _compare_identities(baseline, current):
    _, before = identities(baseline)
    path, after = identities(current)
    kept, known = set(after), set(before)
    for triple in before:
        if triple not in kept:
            yield Change(path, _triple(*triple), "removed", breaking=True)
    for triple in after:
        if triple not in known:
            yield Change(path, _triple(*triple), "added", breaking=False)


def rule_cards(tree):
    """The rule cards of `tree` by ID, read with tools/rules/model.py."""
    try:
        rules = load(tree.root) if tree.ref is None else load_ref(tree.root, tree.ref)
    except (OSError, ValueError) as error:
        raise ManifestError(f"{RULES} at {tree}: {error}") from None
    return rules.cards


def _compare_cards(baseline, current):
    before, after = rule_cards(baseline), rule_cards(current)
    if not after:
        raise ManifestError(f"{RULES}: no rule card in {current}")
    for identifier in before:
        if identifier not in after:
            yield Change(RULES, identifier, "removed", breaking=True)
    for identifier in after:
        if identifier not in before:
            yield Change(RULES, identifier, "added", breaking=False)


def benchmark_files(tree):
    """Map each file in a benchmark directory of `tree` to its bytes."""
    if tree.ref is None:
        files = _working_files(tree, BENCHMARKS)
    else:
        files = _tag_files(tree, BENCHMARKS)
    # A benchmark directory's file is benchmarks/<name>/<path>; the files
    # beside the directories, such as benchmarks/README.md, are not compared.
    return {path: data for path, data in files.items() if path.count("/") > 1}


def _tag_files(tree, directory):
    """Map each file under `directory` at the tag to its bytes, read with one
    `git ls-tree` and one `git cat-file --batch`."""
    listed = git(
        tree.root, "ls-tree", "--full-tree", "-r", "-z", tree.ref, "--", directory
    )
    paths, objects = [], []
    # Each entry is `<mode> <type> <object>\t<path>`. A symbolic link is a
    # blob that holds its target; a submodule, a commit, holds no file.
    for entry in filter(None, listed.split(b"\0")):
        fields, _, path = entry.partition(b"\t")
        _, kind, name = fields.split(b" ")
        if kind == b"blob":
            paths.append(path.decode("utf-8"))
            objects.append(name)
    if not objects:
        return {}
    result = subprocess.run(
        ["git", "-C", str(tree.root), "cat-file", "--batch"],
        input=b"".join(name + b"\n" for name in objects),
        capture_output=True,
        check=False,
    )
    if result.returncode:
        message = result.stderr.decode("utf-8", "replace").strip()
        raise ManifestError(f"git cat-file --batch: {message}")
    # Each object is `<object> blob <size>\n`, its bytes, then `\n`.
    output, offset, files = result.stdout, 0, {}
    for path, name in zip(paths, objects, strict=True):
        end = output.find(b"\n", offset)
        header = output[offset:end].split(b" ") if end >= 0 else []
        if len(header) != 3 or header[:2] != [name, b"blob"]:
            raise ManifestError(f"{path} at {tree}: git cat-file --batch read no blob")
        start = end + 1
        offset = start + int(header[2])
        files[path] = output[start:offset]
        offset += 1
    return files


def _working_files(tree, directory):
    """Map each file under `directory` in the working tree to its bytes; a
    symbolic link's bytes are its target, as at a tag."""
    files = {}

    def visit(folder):
        with os.scandir(folder) as entries:
            for entry in entries:
                if entry.is_symlink():
                    data = os.fsencode(os.readlink(entry.path))
                elif entry.is_dir():
                    visit(entry.path)
                    continue
                elif entry.is_file():
                    data = Path(entry.path).read_bytes()
                else:
                    continue
                files[Path(entry.path).relative_to(tree.root).as_posix()] = data

    if (tree.root / directory).is_dir():
        try:
            visit(tree.root / directory)
        except OSError as error:
            raise ManifestError(f"{directory} in {tree}: {error}") from None
    return files


def _directory(path):
    """The benchmark directory, `benchmarks/<name>`, of one of its files."""
    return path[: path.index("/", len(BENCHMARKS) + 1)]


def _frozen(path):
    """Whether a file of a benchmark directory keeps its bytes: every file but
    the README files and the directory's metadata."""
    inside = path[len(_directory(path)) + 1 :]
    return PurePosixPath(inside).name != README and inside != METADATA


def _compare_benchmarks(baseline, current):
    before, after = benchmark_files(baseline), benchmark_files(current)
    known = {_directory(path) for path in before}
    present = {_directory(path) for path in after}
    breaks = [
        Change(BENCHMARKS, directory, "removed", breaking=True)
        for directory in known - present
    ]
    for path, data in before.items():
        if _frozen(path) and _directory(path) in present:
            if path not in after:
                breaks.append(Change(BENCHMARKS, path, "removed", breaking=True))
            elif after[path] != data:
                breaks.append(Change(BENCHMARKS, path, "changed", breaking=True))
    breaks += [
        Change(BENCHMARKS, path, "added to a baseline benchmark", breaking=True)
        for path in after
        if _frozen(path) and _directory(path) in known and path not in before
    ]
    yield from sorted(breaks, key=lambda change: change.element)
    for directory in sorted(present - known):
        yield Change(BENCHMARKS, directory, "added", breaking=False)
