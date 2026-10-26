"""Run repository checks shared by local Make targets and pull request CI."""

import argparse
import os
import subprocess
import sys
import tomllib

from tools.ci import fuzz
from tools.paths import ROOT, WORKSPACE

LINT_PATHS = [
    "tools",
    "python/src",
    "python/tests",
    "evaluations/harbor",
]
CHECKS = (
    "tools.packaging.check_shared_release_versions",
    "tools.packaging.generate_third_party_notices",
    "tools.packaging.check_minimum_rust",
    "tools.packaging.check_cargo_lock",
    "tools.packaging.check_crate_purposes",
    "tools.packaging.generate_component_inventory",
    "tools.packaging.generate_style_orders",
    "tools.packaging.generate_shipped_schema",
    "tools.packaging.generate_message_templates",
    "tools.packaging.generate_cdisc_vocabulary",
    "tools.packaging.generate_expression_table",
    "tools.packaging.generate_json_schema",
    "tools.packaging.generate_facade_api",
    "tools.packaging.generate_selftest_cases",
    "tools.packaging.generate_bindings",
    "tools.validation.validate_repository",
    "tools.validation.check_expression_conformance",
    "tools.validation.check_execution_manifest",
    "tools.validation.check_selftest_cases",
    "tools.validation.check_vocabulary_coverage",
    "tools.validation.check_cdisc_vocabulary",
    "tools.validation.check_cdisc_tables",
    "tools.validation.check_cdisc_copies",
    "tools.validation.check_parquet_profile",
    "tools.rules.check",
    "tools.validation.check_documentation",
    "tools.validation.check_site_examples",
    "tools.validation.check_supported_platforms",
    "tools.validation.check_diagnostics",
    "tools.validation.check_architecture",
    "tools.validation.check_test_support",
    "tools.validation.check_source_size",
    "tools.validation.check_interface_stability",
)
DOCS_CHECKS = (
    "tools.rules.check",
    "tools.validation.check_documentation",
    "tools.validation.check_site_examples",
)


def run(*arguments, cwd=ROOT):
    subprocess.run(arguments, cwd=cwd, check=True)


def featured_members():
    """The workspace members with default features, but the R binding, in
    manifest order: the crates whose feature-less build differs (#3966)."""
    workspace = tomllib.loads((WORKSPACE / "Cargo.toml").read_text(encoding="utf-8"))
    names = []
    for member in workspace["workspace"]["members"]:
        manifest = tomllib.loads(
            (WORKSPACE / member / "Cargo.toml").read_text(encoding="utf-8")
        )
        name = manifest["package"]["name"]
        if manifest.get("features", {}).get("default") and name != "yamaa-r":
            names.append(name)
    return names


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=("lint", "lint-python", "lint-rust", "lint-r-binding", "check", "docs"),
        help="lint-r-binding runs Clippy over the R binding alone, which needs R",
    )
    args = parser.parse_args(argv)
    if args.command == "docs":
        for module in DOCS_CHECKS:
            run(sys.executable, "-m", module)
    elif args.command == "lint-r-binding":
        # Building the R binding needs R, so `make test-r` runs this beside
        # the R package check, rather than `lint`. `--no-deps` leaves the
        # workspace crates that it depends on to `lint`.
        run(
            "cargo",
            "clippy",
            "--locked",
            "-p",
            "yamaa-r",
            "--all-targets",
            "--",
            "--no-deps",
            "-D",
            "warnings",
            cwd=WORKSPACE,
        )
    elif args.command in ("lint", "lint-python", "lint-rust"):
        if args.command != "lint-rust":
            for arguments in (("format", "--check"), ("check",)):
                run(sys.executable, "-m", "ruff", *arguments, *LINT_PATHS)
            # Use the configuration in python/pyproject.toml.
            run(sys.executable, "-m", "pyrefly", "check", cwd=ROOT / "python")
        if args.command == "lint-python":
            return
        run("cargo", "fmt", "--all", "--check", cwd=WORKSPACE)
        # The outside-workspace check and the fuzz targets are workspaces of
        # their own, out of reach of `--all`. Release compatibility builds
        # and tests the first; the Fuzz workflow builds and runs the second,
        # and Clippy below also lints it on every pull request.
        for manifest in ("tests/embedder/Cargo.toml", "fuzz/Cargo.toml"):
            run(
                "cargo",
                "fmt",
                "--check",
                "--manifest-path",
                str(WORKSPACE / manifest),
                cwd=WORKSPACE,
            )
        # Every crate but the R binding, whose extendr build scripts run R
        # (see lint-r-binding). Clippy links nothing, so PyO3's build script
        # needs no more than the Python interpreter that runs this.
        run(
            "cargo",
            "clippy",
            "--locked",
            "--workspace",
            "--exclude",
            "yamaa-r",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
            cwd=WORKSPACE,
        )
        # The same crates without their default features, so test code that a
        # disabled feature leaves unused fails here too (#3966). No member
        # enables another's features, so one build checks each feature-less.
        run(
            "cargo",
            "clippy",
            "--locked",
            *(argument for name in featured_members() for argument in ("-p", name)),
            "--all-targets",
            "--no-default-features",
            "--",
            "-D",
            "warnings",
            cwd=WORKSPACE,
        )
        # Clippy type-checks the fuzz targets as `cargo check` does, so an
        # engine change that breaks a target fails here, not only in the
        # nightly fuzz run. Like the Fuzz workflow, it builds on a copy of the
        # canonical lock, with the workspace toolchain instead of the fuzzing
        # nightly, and shares the workspace's build directory unless the caller
        # chose one.
        fuzz.copy_lock()
        run(
            "cargo",
            "clippy",
            "--manifest-path",
            str(fuzz.FUZZ / "Cargo.toml"),
            "--all-targets",
            *(
                ()
                if "CARGO_TARGET_DIR" in os.environ
                else ("--target-dir", str(WORKSPACE / "target"))
            ),
            "--",
            "-D",
            "warnings",
            cwd=WORKSPACE,
        )
    else:
        for module in CHECKS:
            run(sys.executable, "-m", module)
        # No pull request job installs the Harbor evaluation's own project, so
        # its lock is checked here against its manifest and the packages it
        # takes from the checkout. `uv run` names its own binary in UV.
        run(
            os.environ.get("UV", "uv"),
            "lock",
            "--check",
            "--project",
            "evaluations/harbor",
        )
        run(
            sys.executable,
            "-m",
            "tools.commands.lint",
            *sorted(map(str, ROOT.glob("benchmarks/*/spec*.yaml"))),
        )
        run(sys.executable, "-m", "tools.commands.docs", "dashboards")
        run(sys.executable, "-m", "tools.commands.docs", "build")
        run(
            sys.executable,
            "-m",
            "pytest",
            "evaluations/harbor/tests/test_harbor_evaluation.py"
            "::test_every_sdtm_and_adam_benchmark_has_its_prompts_with_the_evaluation",
            "-q",
        )


if __name__ == "__main__":
    main()
