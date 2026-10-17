"""Run repository checks shared by local Make targets and pull request CI."""

import argparse
import subprocess
import sys

from tools.paths import ROOT, WORKSPACE

LINT_PATHS = [
    "tools",
    "python/src",
    "python/tests",
    "python/yamaa_build_backend.py",
    "evaluations/harbor",
]
CHECKS = (
    "tools.packaging.check_shared_release_versions",
    "tools.packaging.generate_style_orders",
    "tools.packaging.generate_shipped_schema",
    "tools.packaging.generate_facade_api",
    "tools.packaging.generate_bindings",
    "tools.validation.validate_repository",
    "tools.validation.check_execution_manifest",
    "tools.validation.check_vocabulary_coverage",
    "tools.validation.check_rule_metadata",
    "tools.validation.check_documentation",
    "tools.validation.check_rule_rewrite",
    "tools.validation.check_rationale_phrases",
    "tools.validation.check_diagnostics",
)


def run(*arguments, cwd=ROOT):
    subprocess.run(arguments, cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("lint", "check"))
    command = parser.parse_args().command
    if command == "lint":
        for arguments in (("format", "--check"), ("check",)):
            run(sys.executable, "-m", "ruff", *arguments, *LINT_PATHS)
        run("cargo", "fmt", "--all", "--check", cwd=WORKSPACE)
        run(
            "cargo",
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
            cwd=WORKSPACE,
        )
    else:
        for module in CHECKS:
            run(sys.executable, "-m", module)
        run(
            sys.executable,
            "-m",
            "yamaa.style",
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
