"""Stage a standalone R source package without maintaining duplicate Rust code."""

import argparse
import shutil
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
REPOSITORY = WORKSPACE.parent


def stage(destination: Path):
    """Copy shared sources and independent installed-test truth into a new archive tree."""
    # copytree rejects an existing destination, protecting previous builds.
    shutil.copytree(REPOSITORY / "R/yamaanative", destination)
    rust = destination / "src/rust"
    rust.mkdir()
    for name in ("Cargo.toml", "rust-toolchain.toml"):
        shutil.copy2(WORKSPACE / name, rust / name)
    shutil.copytree(
        WORKSPACE / "crates",
        rust / "crates",
        ignore=shutil.ignore_patterns("target", "__pycache__", "*.pyc", "Cargo.lock"),
    )
    resources = destination / "inst"
    resources.mkdir(exist_ok=True)
    for fixture in (
        "aggregate_syntax.tsv",
        "numeric_syntax.tsv",
        "predicate_syntax.tsv",
        "schema_transport.tsv",
        "schema_windows.tsv",
        "schema_composition.tsv",
        "schema_layer_admission.tsv",
        "schema_inheritance_dependencies.tsv",
        "yaml_transport.tsv",
        "regex_transport.tsv",
        "scalar_transport.tsv",
        "numeric_transport.tsv",
        "reference_binding.tsv",
        "reference_scope.tsv",
        "reference_intermediate.tsv",
        "reference_keys.tsv",
        "reference_match_values.tsv",
        "reference_relations.tsv",
    ):
        shutil.copy2(
            WORKSPACE / "crates/yamaa-adapters/tests/fixtures" / fixture,
            resources / fixture,
        )
    shutil.copytree(
        WORKSPACE / "crates/yamaa-adapters/tests/fixtures/tables",
        resources / "tables",
    )
    shutil.copytree(
        WORKSPACE / "crates/yamaa-adapters/tests/fixtures/datasets",
        resources / "datasets",
    )
    shutil.copy2(
        WORKSPACE / "crates/yamaa-engine/tests/fixtures/function_invocation.tsv",
        resources / "function_invocation.tsv",
    )
    shutil.copy2(
        WORKSPACE / "crates/yamaa-core/tests/fixtures/dependency_analysis.tsv",
        resources / "dependency_analysis.tsv",
    )
    shutil.copy2(
        WORKSPACE / "crates/yamaa-core/tests/fixtures/column_dependencies.tsv",
        resources / "column_dependencies.tsv",
    )
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
