"""Check the vendored ReadStat C: its delta against upstream and its warnings.

`rust/crates/yamaa-adapters/vendor/readstat/` is upstream ReadStat, at the
commit its README names, plus `local-changes.patch` (#4871).

- `patch` fetches that upstream commit, applies the patch to the upstream
  files the tree vendors, and compares the result with the vendored tree byte
  for byte. `README.md`, which describes the changes, and the patch itself are
  the two files outside the comparison. `--write` regenerates the patch from
  upstream and the tree, after a deliberate change to the vendored C.
- `warnings` builds the adapters' C with `YAMAA_READSTAT_WARNINGS=deny`, so
  that `build.rs` compiles it with `-Wall -Wextra -Wformat-security` as
  errors, once with the default C compiler and once with Clang. The release
  profile optimizes the C, as the hosts ship it, so the warnings that only
  optimization finds and the fortified libc calls are checked too. It builds
  in a target directory of its own, so the ordinary build keeps its cache.

`patch` needs network access to the upstream repository unless `--upstream`
names a checkout of the commit.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from tools import git
from tools.packaging.generate_component_inventory import READSTAT_RELEASE, flat_text
from tools.paths import WORKSPACE

VENDORED = WORKSPACE / "crates/yamaa-adapters/vendor/readstat"
PATCH = "local-changes.patch"
# The vendored files that are not upstream's source: the description of the
# changes and the changes themselves.
DESCRIPTION = frozenset({"README.md", PATCH})
# The text before the first `diff --git` line, which `git apply` skips.
PATCH_HEADER = (
    "The local changes to the vendored ReadStat, against the upstream commit\n"
    "that README.md names. Regenerate with\n"
    "`python -m tools.ci.readstat patch --write`; README.md says what each\n"
    "change does and why.\n\n"
)
COMPILERS = (None, "clang")


def upstream_release(vendored=VENDORED):
    """The upstream repository and commit that the README names."""
    match = READSTAT_RELEASE.search(flat_text(vendored / "README.md"))
    if match is None:
        raise ValueError(f"{vendored}/README.md names no upstream ReadStat commit")
    _, commit, repository = match.groups()
    return repository, commit


def upstream_path(relative):
    """Where a vendored file sits in the upstream repository."""
    return relative if relative == "LICENSE" else f"src/{relative}"


def vendored_files(vendored):
    """Each vendored source file, relative to the tree, but the description."""
    return sorted(
        path.relative_to(vendored).as_posix()
        for path in vendored.rglob("*")
        if path.is_file() and path.relative_to(vendored).as_posix() not in DESCRIPTION
    )


def lay_out_upstream(upstream, vendored, destination):
    """Copy the upstream counterpart of each vendored file to its vendored path."""
    for relative in vendored_files(vendored):
        source = upstream / upstream_path(relative)
        if source.is_file():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)


def fetch_upstream(repository, commit, destination):
    """Check out one upstream commit, and nothing more, into `destination`.

    A failed fetch, offline most often, stops with one error line that names
    the commit and the `--upstream` remedy, after git's own message.
    """
    destination.mkdir(parents=True)
    try:
        git.git(destination, "init", "--quiet")
        git.git(destination, "fetch", "--quiet", "--depth", "1", repository, commit)
        git.git(destination, "checkout", "--quiet", "FETCH_HEAD")
    except git.GitError as error:
        print(error, file=sys.stderr)
        raise SystemExit(
            f"::error::vendored ReadStat: cannot fetch upstream {repository} at "
            f"{commit}; without network access, check that commit out and pass "
            "--upstream <checkout>"
        ) from None


def generate(upstream, vendored):
    """The patch that turns the laid-out upstream files into the vendored tree.

    Git's blob names are left out: the repository names nothing by a digest
    of its content, and `git apply` does not need them.
    """
    with tempfile.TemporaryDirectory() as scratch:
        tree = Path(scratch)
        lay_out_upstream(upstream, vendored, tree)
        config = (
            *("-c", "user.name=yamaa", "-c", "user.email=yamaa@invalid"),
            *("-c", "commit.gpgSign=false"),
        )
        git.git(tree, "init", "--quiet")
        git.git(tree, "add", "--all")
        git.git(tree, *config, "commit", "--quiet", "--allow-empty", "-m", "upstream")
        for relative in vendored_files(vendored):
            target = tree / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(vendored / relative, target)
        git.git(tree, "add", "--all")
        diff = git.git(
            tree,
            "-c",
            "core.quotePath=false",
            "diff",
            "--cached",
            "--no-renames",
            "--no-color",
            "--no-ext-diff",
            "--diff-algorithm=myers",
            "--unified=3",
        )
    lines = diff.decode("utf-8").splitlines(keepends=True)
    return PATCH_HEADER + "".join(
        line for line in lines if not line.startswith("index ")
    )


def differences(upstream, vendored):
    """The vendored files that upstream plus the patch does not reproduce."""
    with tempfile.TemporaryDirectory() as scratch:
        tree = Path(scratch)
        lay_out_upstream(upstream, vendored, tree)
        # A repository of its own, so `git apply` patches paths from here even
        # when the scratch directory sits inside another repository.
        git.git(tree, "init", "--quiet")
        applied = subprocess.run(
            ("git", "apply", "--whitespace=nowarn", str(vendored / PATCH)),
            cwd=tree,
            stderr=subprocess.PIPE,
            text=True,
        )
        if applied.returncode:
            return [f"{PATCH} does not apply to upstream: {applied.stderr.strip()}"]
        produced = sorted(
            path.relative_to(tree).as_posix()
            for path in tree.rglob("*")
            if path.is_file() and ".git" not in path.relative_to(tree).parts
        )
        expected = vendored_files(vendored)
        found = [
            f"{relative}: in the vendored tree, not in upstream plus the patch"
            for relative in sorted(set(expected) - set(produced))
        ]
        found += [
            f"{relative}: in upstream plus the patch, not in the vendored tree"
            for relative in sorted(set(produced) - set(expected))
        ]
        found += [
            f"{relative}: upstream plus the patch differs from the vendored file"
            for relative in sorted(set(expected) & set(produced))
            if (tree / relative).read_bytes() != (vendored / relative).read_bytes()
        ]
        return found


def check_patch(upstream=None, write=False, vendored=VENDORED):
    """Compare (or with `write`, regenerate) the patch; return the findings."""
    with tempfile.TemporaryDirectory() as scratch:
        if upstream is None:
            upstream = Path(scratch) / "upstream"
            fetch_upstream(*upstream_release(vendored), upstream)
        if write:
            (vendored / PATCH).write_text(
                generate(upstream, vendored), encoding="utf-8"
            )
        return differences(upstream, vendored)


def build_with_warnings_denied(target_dir):
    """Build the adapters' C with every warning an error, under each compiler."""
    for compiler in COMPILERS:
        environment = os.environ | {
            "YAMAA_READSTAT_WARNINGS": "deny",
            "CARGO_TARGET_DIR": str(target_dir),
            "CARGO_INCREMENTAL": "0",
        }
        if compiler is not None:
            environment["CC"] = compiler
        print(f"ReadStat with warnings denied, CC={compiler or 'default'}", flush=True)
        subprocess.run(
            (
                "cargo",
                "check",
                "--locked",
                "--release",
                "-p",
                "yamaa-adapters",
                "--lib",
                "--no-default-features",
                "--features",
                "readstat",
            ),
            cwd=WORKSPACE,
            env=environment,
            check=True,
        )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    patch = commands.add_parser("patch", help="check upstream plus the patch")
    patch.add_argument(
        "--upstream", type=Path, help="a checkout of the upstream commit; else fetched"
    )
    patch.add_argument("--write", action="store_true", help="regenerate the patch")
    warnings = commands.add_parser("warnings", help="build with warnings as errors")
    warnings.add_argument(
        "--target-dir",
        type=Path,
        default=Path(os.environ.get("CARGO_TARGET_DIR", WORKSPACE / "target"))
        / "readstat-warnings",
    )
    args = parser.parse_args(argv)
    if args.command == "warnings":
        build_with_warnings_denied(args.target_dir)
        return
    found = check_patch(args.upstream, args.write)
    for finding in found:
        print(f"::error::vendored ReadStat: {finding}", file=sys.stderr)
    if found:
        sys.exit(1)


if __name__ == "__main__":
    main()
