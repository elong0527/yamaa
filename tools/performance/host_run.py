"""Run one specification through the Python host, as the command line runs
it: build, save, and write the timings table to standard error in the
command line's `--verbose` form.

Usage: python host_run.py SPEC, or python host_run.py PROJECT NAME for a
specification its project gives functions to. This file imports nothing from
the repository, so it runs in any interpreter with yamaa installed.
"""

import sys

import yamaa


def main():
    if len(sys.argv) == 3:
        run = yamaa.project(sys.argv[1]).spec(sys.argv[2]).run()
    else:
        run = yamaa.spec(sys.argv[1]).run()
    rows = run.timings.to_pylist()
    if rows:
        print("\t".join(rows[0]), file=sys.stderr)
        for row in rows:
            print("\t".join(str(value) for value in row.values()), file=sys.stderr)
    if run.status != "succeeded":
        print(run, file=sys.stderr)
        for issue in run.issues.to_pylist():
            print(issue, file=sys.stderr)
        sys.exit(1)
    for path in run.save():
        print(path)


if __name__ == "__main__":
    main()
