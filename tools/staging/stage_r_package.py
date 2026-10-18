"""Stage a standalone R source package without maintaining duplicate Rust code."""

import argparse
import shutil
from pathlib import Path

from tools.conformance import supplemental_inventory as inventory
from tools.paths import ROOT, WORKSPACE

REPOSITORY = ROOT


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
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
