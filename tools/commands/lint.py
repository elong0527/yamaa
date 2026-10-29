"""Lint specification files with the engine's Rust lint (`yamaa.lint`).

Prints one `file:line:column: finding message` line per finding, then the
count of findings and of the files that hold them, and exits 1 while a
finding remains and 2 when a file cannot be read or parsed. `--fix` applies
the proved layout fixes first. `make check-repository` runs it over every
benchmark specification.
"""

import argparse
import sys

import yamaa


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", metavar="PATH")
    parser.add_argument(
        "--fix", action="store_true", help="apply the proved layout fixes in place"
    )
    arguments = parser.parse_args(argv)
    try:
        result = yamaa.lint(arguments.paths, fix=arguments.fix)
    except yamaa.Error as error:
        print(error, file=sys.stderr)
        for issue in error.issues.to_pylist():
            print(f"  {issue['condition']}: {issue['context']}", file=sys.stderr)
        return 2
    rows = result.findings.to_pylist()
    for row in rows:
        print(
            f"{row['file']}:{row['line']}:{row['column']}: "
            f"{row['finding']} {row['message']}"
        )
    print(result, file=sys.stderr)
    return 1 if result.status == "failed" else 0


if __name__ == "__main__":
    sys.exit(main())
