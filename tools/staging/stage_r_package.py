"""Stage a standalone R source package without maintaining duplicate Rust code.

The package builds offline: src/rust/vendor.tar.xz carries the crates.io crates
that the unchanged Cargo.lock selects, written by tools/vendor_crates.py, which
the package carries too, and the Makevars that configure writes point Cargo at
them through src/rust/vendor-config.toml. configure reads the minimum Cargo and
rustc from the rust-version of the copied workspace manifest. inst/AUTHORS
names every bundled crate's authors and license, as CRAN's Repository Policy
asks.
"""

import argparse
import fnmatch
import json
import re
import shutil
import tempfile
from pathlib import Path, PurePosixPath

from tools.conformance import supplemental_inventory as inventory
from tools.packaging import generate_third_party_notices as notices
from tools.paths import ROOT, WORKSPACE
from tools.staging import engine_bundle, vendor_crates

REPOSITORY = ROOT
VENDOR_ARCHIVE = "src/rust/vendor.tar.xz"
VENDOR_CONFIG = "src/rust/vendor-config.toml"
VENDOR_SCRIPT = "tools/vendor_crates.py"
AUTHORS = "inst/AUTHORS"
AUTHORS_PREAMBLE = (
    "The yamaa R source package bundles the Rust crates below. The crates.io "
    f"crates that src/rust/Cargo.lock selects are in {VENDOR_ARCHIVE}, as "
    "`cargo vendor` writes them, apart from the prebuilt binaries an entry "
    f"names as left out; {VENDOR_SCRIPT} wrote the archive. The other crates "
    "are in the directory their entry names. Each crate keeps its own license "
    "and copyright files beside its sources, and LICENSE-THIRD-PARTY.txt "
    "reproduces those texts for every crate.",
    "Each entry gives the authors, license and repository that the crate's "
    "manifest declares. The yamaa authors are in DESCRIPTION.",
)
# Staged test inputs the installed R package leaves out, as paths under inst/
# in .gitignore form: "*" stays within one name, a final "/**" covers a whole
# directory, and a later "!" pattern keeps a file an earlier one left out. The
# installed R suites (R/yamaa/tests) read every other staged file; the Python
# suites stage the complete trees outside any package.
UNREAD_BY_R_SUITES = (
    # The Python suites' truth codec and their XML and JSON schema validation.
    "host_fixtures.py",
    "define-fixtures/schemas/**",
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


def native_test_fixtures(workspace: Path):
    """Select the original test inputs and truth that the crate tests read.

    The R package carries the crates with their tests, so it carries the
    explicitly listed benchmark files that they compile in as well.
    """
    repository = workspace.parent
    relative_manifest = "rust/qualification/native-source-fixtures.json"
    manifest = repository / relative_manifest
    declaration = json.loads(manifest.read_bytes())
    if (
        not isinstance(declaration, dict)
        or set(declaration) != {"format", "files"}
        or declaration["format"] != "yamaa-native-source-fixtures/1"
        or not isinstance(declaration["files"], list)
    ):
        raise ValueError("invalid native source fixture manifest")
    files = declaration["files"]
    if len(files) > 4096:
        raise ValueError("native source fixture manifest is too large")
    selected = [(relative_manifest, manifest)]
    seen = set()
    for name in files:
        if (
            not isinstance(name, str)
            or len(name) > 65_536
            or not name.startswith("benchmarks/")
            or "\\" in name
            or ":" in name
            or any(part in {"", ".", ".."} for part in name.split("/"))
            or name in seen
        ):
            raise ValueError("invalid or duplicate native source fixture path")
        seen.add(name)
        selected.append((name, repository / name))
    for name, path in selected:
        if not path.is_file():
            raise ValueError(f"native source fixture is missing: {name}")
        parent = repository
        for part in name.split("/"):
            parent = parent / part
            if parent.is_symlink():
                raise ValueError("native source fixtures must not be symlinks")
    # This conservative source-text scan admits direct include_str!/include_bytes!
    # calls only with ordinary quoted literals and matching (), [] or {}.
    # Raw/dynamic arguments and comment-separated tokens are refused. Inputs
    # outside Cargo's workspace need an explicit declaration before an archive.
    include = re.compile(r"\binclude_(?:str|bytes)\b")
    invocation = re.compile(r"\s*!\s*([\(\[\{])\s*")
    argument = re.compile(r'"((?:[^"\\]|\\.)*)"\s*,?\s*([\)\]\}])')
    closing = {"(": ")", "[": "]", "{": "}"}
    crates = workspace / "crates"
    for source in sorted(crates.rglob("*.rs")):
        if any(
            part in {"target", ".git", "__pycache__"}
            for part in source.relative_to(crates).parts
        ):
            continue
        text = source.read_text(encoding="utf-8")
        for directive in include.finditer(text):
            call = invocation.match(text, directive.end())
            literal = argument.match(text, call.end()) if call is not None else None
            if literal is None or literal.group(2) != closing[call.group(1)]:
                raise ValueError(
                    "native compile fixture arguments must be ordinary string literals"
                )
            written = literal.group(1)
            if (
                "\\" in written
                or ":" in written
                or PurePosixPath(written).is_absolute()
            ):
                raise ValueError(
                    "native compile fixture paths must be unescaped relative paths"
                )
            path = source.parent.resolve()
            for part in PurePosixPath(written).parts:
                path = path / part
                if path.is_symlink():
                    raise ValueError("native compile fixtures must not be symlinks")
            path = path.resolve()
            if path.is_relative_to(workspace.resolve()):
                continue
            if not path.is_relative_to(repository.resolve()):
                raise ValueError("native compile fixture escapes the repository")
            relative = path.relative_to(repository.resolve()).as_posix()
            if relative not in seen:
                raise ValueError(f"native compile fixture is not declared: {relative}")
    return selected


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


def vendor_rust(destination: Path):
    """Bundle the locked crates.io crates and the script that bundles them.

    Returns (name, version, left-out paths) for each vendored crate.
    """
    rust = destination / "src/rust"
    with tempfile.TemporaryDirectory(prefix="yamaa-r-vendor-") as scratch:
        directory = Path(scratch) / vendor_crates.DIRECTORY
        crates = vendor_crates.vendor(rust, directory)
        vendor_crates.write_archive(directory, destination / VENDOR_ARCHIVE)
    (destination / VENDOR_CONFIG).write_text(vendor_crates.CONFIG, encoding="utf-8")
    script = destination / VENDOR_SCRIPT
    script.parent.mkdir(exist_ok=True)
    shutil.copyfile(vendor_crates.__file__, script)
    return crates


def authors_text(metadata, crates, listed):
    """Name the authors and license of every Rust crate the package bundles.

    `crates` are the vendored crates and `listed` the (name, version) pairs
    that LICENSE-THIRD-PARTY.txt covers. The vendored crates must be exactly
    the locked crates.io crates, and every bundled crate needs a notice entry.
    """
    members = set(metadata["workspace_members"])
    bundled = [
        package
        for package in metadata["packages"]
        if package["id"] not in members
        and package["source"] in (notices.CRATES_IO, None)
    ]
    bundled.sort(
        key=lambda package: (package["name"], notices.version_key(package["version"]))
    )
    vendored = {(name, version): left_out for name, version, left_out in crates}
    registry = {
        (package["name"], package["version"])
        for package in bundled
        if package["source"] == notices.CRATES_IO
    }
    if set(vendored) != registry:
        name, version = sorted(set(vendored) ^ registry)[0]
        raise ValueError(
            f"{name} {version}: the vendored crates differ from the locked "
            "crates.io crates"
        )
    title = "Rust crates in the yamaa R source package"
    lines = [title, "=" * len(title), ""]
    for paragraph in AUTHORS_PREAMBLE:
        lines += notices.paragraph(paragraph)
    for package in bundled:
        label = (package["name"], package["version"])
        if label not in listed:
            raise ValueError(f"{' '.join(label)}: LICENSE-THIRD-PARTY.txt has no entry")
        lines.append(f"Crate: {' '.join(label)}")
        if package["source"] is None:
            lines.append(f"    Directory: src/{notices.source_label(package)}")
        if package["authors"]:
            lines += notices.wrapped("Authors:", package["authors"], indent="    ")
        else:
            lines.append("    Authors: none declared")
        lines.append(f"    License: {package['license'] or 'none declared'}")
        lines.append(f"    Repository: {package['repository'] or 'none declared'}")
        if vendored.get(label):
            lines += notices.wrapped("Left out:", vendored[label], indent="    ")
        lines.append("")
    return "\n".join(lines)


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
    for original in engine_bundle.vendored_sources(WORKSPACE):
        target = rust / original.relative_to(WORKSPACE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
    for relative, original in native_test_fixtures(WORKSPACE):
        target = destination / "src" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
    crates = vendor_rust(destination)
    resources = destination / "inst"
    resources.mkdir(exist_ok=True)
    from tools.staging import stage_specification_inputs as helper

    helper.stage(resources / "specification-original")
    from tools.staging import stage_grammar_inputs as grammar_helper

    grammar_helper.stage(resources / "grammar-original")
    # Package checks use base R functions. The original project studies and
    # their separately installed helper belong to public benchmark qualification.
    projects = resources / "project-base"
    negative = projects / "negative-function-contract"
    from tools.staging import stage_project_inputs as project_helper

    # Retain the foreign Python lock for the R lock-kind refusal test.
    project_helper.stage_case(
        REPOSITORY / "benchmarks/negative-function-contract", negative
    )
    environment = negative / "r/environment.yaml"
    environment.write_text(
        environment.read_text().replace(
            "yamaabenchmarks::project_value", "base::identity"
        )
    )
    lock = json.loads((negative / "r/renv.lock").read_text())
    del lock["Packages"]["yamaabenchmarks"]
    for case in (negative, projects / "base-round"):
        (case / "r/renv.lock").write_text(json.dumps(lock, indent=2) + "\n")
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
    listed = notices.committed_entries(
        (resources / notices.NOTICES).read_text(encoding="utf-8")
    )
    (destination / AUTHORS).write_text(
        authors_text(notices.locked_metadata(), crates, listed), encoding="utf-8"
    )
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
