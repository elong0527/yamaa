"""Stage a standalone R source package without maintaining duplicate Rust code."""

import argparse
import fnmatch
import shutil
from pathlib import Path

from tools.conformance import supplemental_inventory as inventory
from tools.paths import ROOT, WORKSPACE

REPOSITORY = ROOT
# Staged test inputs the installed R package leaves out, as paths under inst/
# in .gitignore form: "*" stays within one name, a final "/**" covers a whole
# directory, and a later "!" pattern keeps a file an earlier one left out. The
# installed R suites (R/yamaa/tests) read every other staged file; the Python
# suites stage the complete trees outside any package.
UNREAD_BY_R_SUITES = (
    # The Python suites' truth codec and their XML and JSON schema validation.
    "host_fixtures.py",
    "define-fixtures/schemas/**",
    # R runs no Python entries. public_environment.R reads one case's Python
    # lock, as a packaging lock in a foreign syntax.
    "project-original/*/python/uv.lock",
    "!project-original/negative-function-contract/python/uv.lock",
    # Top-level JSON truth; the R suites read the generated R truth instead.
    "specification-original/*.json",
    # R strings cannot hold NUL, so original_specifications.R skips this case.
    "specification-original/text-template-cases/nul-value/**",
    # Staged for no installed suite.
    "specification-original/binding-diagnostics.tsv",
    "specification-original/grammar-diagnostics.tsv",
    "specification-original/inheritance-preparation.tsv",
    "specification-original/inheritance-replay.tsv",
    "specification-original/normative-inheritance-preparation.tsv",
    "specification-original/output-declarations.tsv",
    "specification-original/predicate-diagnostics.tsv",
    "specification-original/preflight.tsv",
    "specification-original/window-diagnostics.tsv",
    "specification-original/mapping-cut-cases/**",
    "specification-original/source-binding-cases/**",
    "specification-original/cases/adam-adfa-fever/**",
    "specification-original/cases/adam-adtte-pfs/**",
    "specification-original/expected/README.md",
    "specification-original/expected/*-failures.json",
    "specification-original/flags/implicit_raw_ambiguous/**",
    "specification-original/flags/nested_case_mixed_declarations/**",
    "specification-original/case-predicate-history/**",
    "!specification-original/case-predicate-history/*/expected.csv",
    "specification-original/pq/*",
    "!specification-original/pq/duplicate.parquet",
    "!specification-original/pq/empty-name.parquet",
    "!specification-original/pq/mixed-*.parquet",
    "!specification-original/pq/ordered-sum.parquet",
    "!specification-original/pq/text.parquet",
    "!specification-original/pq/unsupported-*.parquet",
    "!specification-original/pq/utf8*.parquet",
)


def matches(relative: str, pattern: str) -> bool:
    """Match a staged path against one UNREAD_BY_R_SUITES pattern."""
    names, parts = relative.split("/"), pattern.split("/")
    if parts[-1] == "**":
        parts = parts[:-1]
        if len(names) <= len(parts):
            return False
        names = names[: len(parts)]
    return len(names) == len(parts) and all(
        fnmatch.fnmatchcase(name, part) for name, part in zip(names, parts)
    )


def leave_out_unread_inputs(resources: Path, patterns=UNREAD_BY_R_SUITES) -> None:
    """Remove the staged inputs no installed R suite reads.

    A pattern that matches no staged file is refused before anything is removed.
    """
    matched, unread = set(), []
    for path in sorted(resources.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(resources).as_posix()
        left_out = False
        for pattern in patterns:
            if matches(relative, pattern.removeprefix("!")):
                matched.add(pattern)
                left_out = not pattern.startswith("!")
        if left_out:
            unread.append(path)
    for pattern in patterns:
        if pattern not in matched:
            raise ValueError(f"R package exclusion matches no staged file: {pattern}")
    for path in unread:
        path.unlink()
    for directory in sorted(
        (path for path in resources.rglob("*") if path.is_dir()), reverse=True
    ):
        if not any(directory.iterdir()):
            directory.rmdir()


def stage(destination: Path):
    """Copy shared sources and independent installed-test truth into a new archive tree."""
    # A source package must never generate its own dependency resolution.
    if not (WORKSPACE / "Cargo.lock").is_file():
        raise FileNotFoundError("canonical Cargo.lock is missing")
    # copytree rejects an existing destination, protecting previous builds.
    shutil.copytree(REPOSITORY / "R/yamaa", destination)
    rust = destination / "src/rust"
    rust.mkdir()
    for name in ("Cargo.toml", "rust-toolchain.toml", "Cargo.lock"):
        shutil.copy2(WORKSPACE / name, rust / name)
    shutil.copytree(
        WORKSPACE / "crates",
        rust / "crates",
        ignore=shutil.ignore_patterns("target", "__pycache__", "*.pyc", "Cargo.lock"),
    )
    from python import yamaa_build_backend as backend

    for original in backend.vendored_sources(WORKSPACE):
        target = rust / original.relative_to(WORKSPACE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
    for relative, original in backend.native_test_fixtures(WORKSPACE):
        target = destination / "src" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
    resources = destination / "inst"
    resources.mkdir(exist_ok=True)
    from tools.staging import stage_specification_inputs as helper

    helper.stage(resources / "specification-original")
    from tools.staging import stage_grammar_inputs as grammar_helper

    grammar_helper.stage(resources / "grammar-original")
    from tools.staging import stage_project_inputs as project_helper

    project_helper.stage(resources / "project-original")
    from tools.staging import stage_define_inputs as define_helper

    define_helper.stage(resources / "define-fixtures")
    catalog = inventory.load_catalog(
        REPOSITORY, WORKSPACE / "qualification/supplemental.json"
    )
    for name, relative in inventory.host_fixture_files(catalog):
        target = resources / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPOSITORY / name, target)
    leave_out_unread_inputs(resources)
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
