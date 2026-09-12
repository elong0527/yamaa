import argparse
from pathlib import Path

from .repository import RegexEngineUnavailable, check_yaml_files
from .repository import require_regex_engine


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Validate yamaa repository structure and specs."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[4],
        help="Repository root directory",
    )
    parser.add_argument(
        "--warnings-as-errors",
        action="store_true",
        help="Treat warnings as errors",
    )
    args = parser.parse_args(argv)

    try:
        require_regex_engine()
    except RegexEngineUnavailable as exc:
        print(f"ERROR: {exc}")
        return 1

    errors, warnings = check_yaml_files(args.root)

    for warning in warnings:
        print(warning)

    if errors:
        for error in errors:
            print(error)
        return 1

    if warnings and args.warnings_as_errors:
        return 1

    print("PASS: Repository looks clean.")
    return 0
