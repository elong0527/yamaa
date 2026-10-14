"""Stage a standalone R source package without maintaining duplicate Rust code."""

import argparse
import importlib.util
import shutil
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
REPOSITORY = WORKSPACE.parent


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
    backend_spec = importlib.util.spec_from_file_location(
        "yamaa_build_backend", REPOSITORY / "python/yamaa_build_backend.py"
    )
    backend = importlib.util.module_from_spec(backend_spec)
    backend_spec.loader.exec_module(backend)
    for relative, original in backend.native_test_fixtures(WORKSPACE):
        target = destination / "src" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
    resources = destination / "inst"
    resources.mkdir(exist_ok=True)
    helper_spec = importlib.util.spec_from_file_location(
        "stage_specification_inputs",
        Path(__file__).with_name("stage_specification_inputs.py"),
    )
    helper = importlib.util.module_from_spec(helper_spec)
    helper_spec.loader.exec_module(helper)
    helper.stage(resources / "specification-original")
    grammar_spec = importlib.util.spec_from_file_location(
        "stage_grammar_inputs", Path(__file__).with_name("stage_grammar_inputs.py")
    )
    grammar_helper = importlib.util.module_from_spec(grammar_spec)
    grammar_spec.loader.exec_module(grammar_helper)
    grammar_helper.stage(resources / "grammar-original")
    shutil.copytree(
        WORKSPACE / "crates/yamaa-adapters/tests/fixtures/environment-candidate",
        resources / "specification-environment-candidate",
    )
    project_spec = importlib.util.spec_from_file_location(
        "stage_project_inputs", Path(__file__).with_name("stage_project_inputs.py")
    )
    project_helper = importlib.util.module_from_spec(project_spec)
    project_spec.loader.exec_module(project_helper)
    project_helper.stage(resources / "project-original")
    define_spec = importlib.util.spec_from_file_location(
        "stage_define_inputs", Path(__file__).with_name("stage_define_inputs.py")
    )
    define_helper = importlib.util.module_from_spec(define_spec)
    define_spec.loader.exec_module(define_helper)
    define_helper.stage(resources / "define-fixtures")
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
