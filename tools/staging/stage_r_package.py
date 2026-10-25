"""Stage a standalone R source package without maintaining duplicate Rust code.

src/rust holds the R engine bundle of tools/staging/engine_bundle.py (#3030
D55): the engine crates and the R binding crate, with a lock trimmed from the
canonical one and verified against it, the selftest cases that the facade
embeds (D108), the copies of the schema files that the adapters embed, and no
tests or toolchain file. The package builds offline:
src/rust/vendor.tar.xz carries the crates.io crates that the trimmed lock
selects, written by tools/vendor_crates.py, which the package carries too,
and the Makevars that configure writes point Cargo at them through
src/rust/vendor-config.toml, the bundle's Cargo configuration.
configure reads the minimum Cargo and rustc from the rust-version of the
bundle's workspace manifest. inst/AUTHORS names every bundled crate's authors
and license, as CRAN's Repository Policy asks.

inst/ holds only AUTHORS, the license notices and the component inventory of
the R bundle (tools/packaging/generate_component_inventory.py), which R
installs with the package. tests/ carries the small CRAN smoke suite and its
project study. The full supplemental suites and their inputs stay in the
checkout: tools.commands.installed stages them outside it for CI to run
against the installed package, independently of R CMD check.
"""

import argparse
import fnmatch
import json
import shutil
import tempfile
from pathlib import Path

import yaml
from tools.packaging import generate_third_party_notices as notices
from tools.paths import ROOT, WORKSPACE
from tools.staging import engine_bundle, vendor_crates

REPOSITORY = ROOT
VENDOR_ARCHIVE = "src/rust/vendor.tar.xz"
VENDOR_CONFIG = "src/rust/vendor-config.toml"
VENDOR_SCRIPT = "tools/vendor_crates.py"
AUTHORS = "inst/AUTHORS"
# Only these suites ship in the source archive. Discovery still includes
# every R/yamaa/tests/*.R in installed-package qualification.
CRAN_TESTS = ("smoke.R",)
AUTHORS_PREAMBLE = (
    "The yamaa R source package bundles the Rust crates below. The crates.io "
    f"crates that src/rust/Cargo.lock selects are in {VENDOR_ARCHIVE}, as "
    "`cargo vendor` writes them, apart from the files that an entry names as "
    "left out: Rust toolchain files, the license texts of alternatives to the "
    "license that yamaa relies on, and, of a crate that no supported target "
    "compiles, every file but its manifest, which Cargo reads to resolve the "
    "lock (with an empty src/lib.rs where Cargo needs one to read the "
    f"manifest); {VENDOR_SCRIPT} wrote the archive. The other crates "
    "are in the directory their entry names. Every crate that a supported "
    "target compiles keeps its own license and copyright files beside its "
    "sources, apart from those that its entry names as left out, and "
    "LICENSE-THIRD-PARTY.txt reproduces those texts for every crate that the "
    "installed package links.",
    "Each entry gives the authors and repository that the crate's manifest "
    "declares, and the one license that yamaa relies on of those that it "
    "offers. The yamaa authors are in DESCRIPTION.",
)
# The trees of inputs that the R suites read from their working directory:
# R CMD check runs each suite in a copy of the package's tests directory, and
# native qualification in the directory where it stages the installed suites.
# The shared host fixtures are staged beside them separately.
TEST_INPUTS = (
    "specification-original",
    "grammar-original",
    "project-base",
    "define-fixtures",
)
# The authored project studies; stage_projects adds a benchmark study to them.
PROJECTS = REPOSITORY / "R/yamaa/tests/project-base"
# Staged test inputs that no R suite reads, left out wherever the R suites'
# inputs are staged, as paths from the directory that holds the TEST_INPUTS
# trees, in .gitignore form: "*" stays within one name, a final "/**" covers a
# whole directory, and a later "!" pattern keeps a file an earlier one left
# out. The R suites (R/yamaa/tests) read every other staged file; the Python
# suites stage the complete trees.
UNREAD_BY_R_SUITES = (
    # The Python suites' XML and JSON schema validation.
    "define-fixtures/schemas/**",
    # JSON truth; the R suites read the generated R truth instead.
    "specification-original/*.json",
    "specification-original/row-cases/*/findings.json",
    # R strings cannot hold NUL, so original_specifications_text.R skips this case.
    "specification-original/text-template-cases/nul-value/**",
    # Staged for no installed suite.
    "specification-original/binding-diagnostics.tsv",
    "specification-original/grammar-diagnostics.tsv",
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
    """Remove the staged inputs no R suite reads.

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

    `crates` are the vendored crates, as tools/staging/vendor_crates.py
    returns them, `locked` the (name, version, source) packages of the
    bundle's trimmed lock and `listed` the (name, version) pairs that the
    package's LICENSE-THIRD-PARTY.txt covers. The vendored crates must be
    exactly the crates.io crates of that lock, and the notices may name only
    bundled crates: those that the R binding links.
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
    vendored = {
        (name, version): (left_out, whole) for name, version, left_out, whole in crates
    }
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
    unbundled = listed - {(package["name"], package["version"]) for package in bundled}
    if unbundled:
        name, version = sorted(unbundled)[0]
        raise ValueError(
            f"{name} {version}: LICENSE-THIRD-PARTY.txt lists it, but the package "
            "does not bundle it"
        )
    title = "Rust crates in the yamaa R source package"
    lines = [title, "=" * len(title), ""]
    for paragraph in AUTHORS_PREAMBLE:
        lines += notices.paragraph(paragraph)
    for package in bundled:
        label = (package["name"], package["version"])
        lines.append(f"Crate: {' '.join(label)}")
        if package["source"] is None:
            lines.append(f"    Directory: src/{notices.source_label(package)}")
        if package["authors"]:
            lines += notices.wrapped("Authors:", package["authors"], indent="    ")
        else:
            lines.append("    Authors: none declared")
        lines.append(f"    License: {notices.relied_on(package, metadata)}")
        lines.append(f"    Repository: {package['repository'] or 'none declared'}")
        left_out, whole = vendored.get(label, ([], True))
        if not whole:
            lines.append("    Left out: every file but its manifest")
        elif left_out:
            lines += notices.wrapped("Left out:", left_out, indent="    ")
        lines.append("")
    return notices.refuse_gnu("\n".join(lines), AUTHORS)


def r_callables(case: Path) -> list[Path]:
    """The files that name a study's R callables: the function files its
    project lists (#3030 D8)."""
    project = yaml.safe_load((case / "project.yaml").read_text(encoding="utf-8"))
    return [case / path for path in project.get("functions") or []]


def stage_projects(projects: Path) -> None:
    """Stage the project studies that the R suites read into a new directory.

    Package checks use base R functions: the original project studies and
    their separately installed helper belong to public benchmark
    qualification. Beside the authored base-round study,
    negative-function-contract calls base::identity instead of the helper, and
    both get the benchmark's R lock without the helper. The rewritten files end
    their lines with LF on every platform, as the copied ones do.
    """
    from tools.staging import stage_project_inputs as project_helper

    shutil.copytree(PROJECTS, projects)
    negative = projects / "negative-function-contract"
    # Retain the foreign Python lock for the R lock-kind refusal test.
    project_helper.stage_case(
        REPOSITORY / "benchmarks/negative-function-contract", negative
    )
    for definitions in r_callables(negative):
        definitions.write_text(
            definitions.read_text(encoding="utf-8").replace(
                "yamaabenchmarks::project_value", "base::identity"
            ),
            encoding="utf-8",
            newline="\n",
        )
    lock = json.loads((negative / "r/renv.lock").read_text(encoding="utf-8"))
    del lock["Packages"]["yamaabenchmarks"]
    for case in (negative, projects / "base-round"):
        # The authored study commits no file under r/ (D8).
        (case / "r").mkdir(exist_ok=True)
        (case / "r/renv.lock").write_text(
            json.dumps(lock, indent=2) + "\n", encoding="utf-8", newline="\n"
        )


def stage_test_inputs(tests: Path) -> None:
    """Stage the TEST_INPUTS trees beside the R suites, in tests.

    The trees are staged apart first and left without the inputs that no R
    suite reads, so that nothing else in tests is touched. A tree that tests
    already holds is refused before anything is staged.
    """
    from tools.staging import stage_define_inputs as define_helper
    from tools.staging import stage_grammar_inputs as grammar_helper
    from tools.staging import stage_specification_inputs as helper

    for name in TEST_INPUTS:
        if (tests / name).exists():
            raise FileExistsError(f"refusing to overwrite test inputs: {tests / name}")
    with tempfile.TemporaryDirectory(prefix="yamaa-r-test-inputs-") as scratch:
        staged = Path(scratch)
        helper.stage(staged / "specification-original")
        grammar_helper.stage(staged / "grammar-original")
        stage_projects(staged / "project-base")
        define_helper.stage(staged / "define-fixtures")
        leave_out_unread_inputs(staged)
        for name in TEST_INPUTS:
            shutil.copytree(staged / name, tests / name)


def stage_cran_tests(tests: Path) -> None:
    """Stage the smoke suite and only the project study that it reads.

    Reuse the installed suites' base-R project and lock staging; the larger
    regression inputs are never copied into the source package.
    """
    tests.mkdir()
    for name in CRAN_TESTS:
        shutil.copyfile(REPOSITORY / "R/yamaa/tests" / name, tests / name)
    with tempfile.TemporaryDirectory(prefix="yamaa-r-smoke-") as scratch:
        projects = Path(scratch) / "projects"
        stage_projects(projects)
        shutil.copytree(projects / "base-round", tests / "project-base/base-round")


def stage(destination: Path):
    """Copy shared sources and a small smoke suite into a new archive tree."""
    # A source package must never generate its own dependency resolution.
    if not (WORKSPACE / "Cargo.lock").is_file():
        raise FileNotFoundError("canonical Cargo.lock is missing")
    # copytree rejects an existing destination, protecting previous builds.
    # Full supplemental suites are CI inputs, not CRAN's test payload.
    shutil.copytree(
        REPOSITORY / "R/yamaa",
        destination,
        ignore=lambda directory, names: [
            name
            for name in names
            if Path(directory, name) == REPOSITORY / "R/yamaa/tests"
        ],
    )
    crates = bundle_rust(destination)
    stage_cran_tests(destination / "tests")
    listed = notices.committed_entries(
        (destination / "inst" / notices.NOTICES).read_text(encoding="utf-8")
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
