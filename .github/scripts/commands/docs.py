"""Run canonical-page tests, dashboard generation or the strict site build."""

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("test", "dashboards", "build"))
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    if args.output_dir and args.command != "dashboards":
        parser.error("--output-dir is supported only for dashboards")
    if args.command == "test":
        command = [
            "-m",
            "unittest",
            "discover",
            "-s",
            ".github/scripts/docs",
            "-p",
            "test_*.py",
        ]
    elif args.command == "dashboards":
        command = [".github/scripts/benchmark-docs/generate.py", "--all", "--quiet"]
        if args.output_dir:
            command += ["--output-dir", str(args.output_dir.resolve())]
    else:
        # Generated references and the benchmark index are built here, at
        # docs build time, instead of being committed: materialize the rule
        # references from rules/ + yaml/ and render the full benchmark index
        # beside the generated gallery (excluded from the rendered site).
        for generation in (
            [".github/scripts/yaml-validation/generate_rule_reference.py"],
            [
                ".github/scripts/benchmark-docs/build_index.py",
                "--output",
                "docs/benchmark/readme-index.md",
            ],
        ):
            subprocess.run([sys.executable, *generation], cwd=ROOT, check=True)
        command = ["-m", "mkdocs", "build", "--strict"]
    subprocess.run([sys.executable, *command], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
