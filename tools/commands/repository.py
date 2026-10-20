"""Run repository checks shared by local Make targets and pull request CI."""

import argparse
import os
import subprocess
import sys

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
    "tools.packaging.generate_style_orders",
    "tools.packaging.generate_shipped_schema",
    "tools.packaging.generate_json_schema",
    "tools.packaging.generate_facade_api",
    "tools.packaging.generate_bindings",
    "tools.validation.validate_repository",
    "tools.validation.check_execution_manifest",
    "tools.validation.check_vocabulary_coverage",
    "tools.rules.check",
    "tools.validation.check_documentation",
    "tools.validation.check_diagnostics",
)


def run(*arguments, cwd=ROOT):
    subprocess.run(arguments, cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("lint", "check"))
    parser.add_argument(
        "--without-r",
        action="store_true",
        help="leave the R binding, which needs R to build, out of Clippy",
    )
    args = parser.parse_args()
    if args.without_r and args.command != "lint":
        parser.error("--without-r is supported only for lint")
    if args.command == "lint":
        for arguments in (("format", "--check"), ("check",)):
            run(sys.executable, "-m", "ruff", *arguments, *LINT_PATHS)
        # Type-check the package with the configuration in python/pyproject.toml.
        run(sys.executable, "-m", "pyrefly", "check", cwd=ROOT / "python")
        run("cargo", "fmt", "--all", "--check", cwd=WORKSPACE)
        # The outside-workspace check and the fuzz targets are workspaces of
        # their own, out of reach of `--all`. CI's minimum-toolchain job builds
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
        run(
            "cargo",
            "clippy",
            "--locked",
            "--workspace",
            *(("--exclude", "yamaa-r") if args.without_r else ()),
            "--all-targets",
            "--all-features",
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
