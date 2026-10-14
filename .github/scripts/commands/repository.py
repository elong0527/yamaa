"""Run the owned repository checks used by both Make and YAML validation CI."""

import argparse
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VALIDATION = ".github/scripts/yaml-validation/"
LINT_PATHS = [
    "python/src/yamaa",
    "python/tests",
    ".github/scripts/ci",
    ".github/scripts/docs",
    ".github/scripts/commands",
]
CHECKS = {
    "validator-tests": ["test_validate_repository.py"],
    "validator": ["validate_repository.py"],
    "manifest": ["check_execution_manifest.py"],
    "vocabulary": ["check_vocabulary_coverage.py"],
    "metadata": ["check_rule_metadata.py"],
    "documentation": ["check_documentation.py"],
    "rule-references": [
        "test_rule_rewrite.py",
        "check_rule_rewrite.py",
        "generate_rule_reference.py --check",
    ],
    "rules": [
        "test_rationale_phrases.py",
        "check_rationale_phrases.py",
        "test_requirement_registry.py",
        "check_requirement_registry.py",
    ],
    "blockers": ["check_validation_blockers.py"],
}


def run(*arguments):
    subprocess.run(arguments, cwd=ROOT, check=True)


def project(*arguments):
    run("uv", "run", "--project", "python", "--locked", "--no-sync", *arguments)


def check(name):
    if name == "fast":
        check("lint")
        check("ci-tests")
    elif name == "repository":
        check("style")
        for group in CHECKS:
            # The network-backed blocker check is an explicit separate target.
            if group != "blockers":
                check(group)
    elif name == "lint":
        project("ruff", "format", "--check", *LINT_PATHS)
        project("ruff", "check", *LINT_PATHS)
    elif name == "ci-tests":
        project("python", "-m", "unittest", "discover", "-s", ".github/scripts/ci")
    elif name == "style":
        project("python", "rust/tools/generate_style_orders.py")
        project(
            "python",
            "-m",
            "yamaa.style",
            *sorted(map(str, ROOT.glob("benchmarks/*/spec*.yaml"))),
        )
    else:
        for command in CHECKS[name]:
            script, *arguments = command.split()
            project("python", VALIDATION + script, *arguments)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "check", choices=("fast", "repository", "lint", "ci-tests", "style", *CHECKS)
    )
    check(parser.parse_args().check)
