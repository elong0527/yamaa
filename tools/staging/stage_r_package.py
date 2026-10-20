"""Stage a standalone R source package without maintaining duplicate Rust code.

src/rust holds the R engine bundle of tools/staging/engine_bundle.py (#3030
D55): the engine crates and the R binding crate, with a lock trimmed from the
canonical one and verified against it, and no tests or toolchain file. The
package builds offline: src/rust/vendor.tar.xz carries the crates.io crates
that the trimmed lock selects, written by tools/vendor_crates.py, which the
package carries too, and the Makevars that configure writes point Cargo at
them through src/rust/vendor-config.toml, the bundle's Cargo configuration.
configure reads the minimum Cargo and rustc from the rust-version of the
bundle's workspace manifest. inst/AUTHORS names every bundled crate's authors
and license, as CRAN's Repository Policy asks.
"""

import argparse
import fnmatch
import json
import shutil
import tempfile
from pathlib import Path

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
    "`cargo vendor` writes them, apart from the prebuilt binaries and Rust "
    "toolchain files that an entry names as left out; "
    f"{VENDOR_SCRIPT} wrote the archive. The other crates "
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


def bundle_rust(destination: Path):
    """Stage the R engine bundle in src/rust, its crates.io crates compressed.

    The bundle's workspace becomes src/rust, its vendored crates
    src/rust/vendor.tar.xz and its Cargo configuration
    src/rust/vendor-config.toml, which the Makevars copy into the Cargo home
    of the build. The package also carries the script that wrote the archive.
    Returns (name, version, left-out paths) for each vendored crate.
    """
    with tempfile.TemporaryDirectory(prefix="yamaa-r-bundle-") as scratch:
        bundle = Path(scratch) / "bundle"
        crates = engine_bundle.stage("r", bundle)
        shutil.copytree(bundle / "rust", destination / "src/rust")
        vendor_crates.write_archive(
            bundle / vendor_crates.DIRECTORY, destination / VENDOR_ARCHIVE
        )
        shutil.copyfile(bundle / ".cargo/config.toml", destination / VENDOR_CONFIG)
    script = destination / VENDOR_SCRIPT
    script.parent.mkdir(exist_ok=True)
    shutil.copyfile(vendor_crates.__file__, script)
    return crates


def authors_text(metadata, crates, listed, locked):
    """Name the authors and license of every Rust crate the package bundles.

    `crates` are the vendored crates, `locked` the (name, version, source)
    packages of the bundle's trimmed lock and `listed` the (name, version)
    pairs that LICENSE-THIRD-PARTY.txt covers. The vendored crates must be
    exactly the crates.io crates of that lock, and every bundled crate needs a
    notice entry.
    """
    members = set(metadata["workspace_members"])
    bundled = [
        package
        for package in metadata["packages"]
        if package["id"] not in members
        and (package["name"], package["version"], package["source"]) in locked
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
    crates = bundle_rust(destination)
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
    _, locked = engine_bundle.lock_packages(
        (destination / "src/rust/Cargo.lock").read_text(encoding="utf-8"),
        "src/rust/Cargo.lock",
    )
    (destination / AUTHORS).write_text(
        authors_text(notices.locked_metadata(), crates, listed, set(locked)),
        encoding="utf-8",
    )
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
