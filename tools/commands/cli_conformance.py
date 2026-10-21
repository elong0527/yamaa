"""Run the benchmark corpus, written as 1.0, through a `yamaa` binary (#3060).

The command line reads only 1.x files: it refuses a file that declares
`schema_version` as `unsupported_language_version` (#3030 D122, D125). Until
the corpus moves to 1.0 (P2-5 to P2-8), its conformance runs converted
copies, as the 1.0 conversion gate does (`tools.commands.conversion`). This
writes every benchmark as 1.0 under OUTPUT/benchmarks, excludes each
benchmark the gate excepts, with the gate's reason, plans the converted
corpus into OUTPUT/plan, runs the command line's runner,
`rust/tests/public_conformance_cli.py`, over it into OUTPUT/observed, and
judges the records against the converted expected files into
OUTPUT/inventory.json. It exits 1 unless every benchmark the command line
runs passes. Unlike the gate, it keeps what it writes, so the records and the
converted cases they ran can be read afterwards.

With --prepare-only it writes the converted copies and their plan, and stops.
Native qualification prepares them while the network is up, then runs the
runner over them with the network disabled and judges the records itself with
`python -m tools.conformance.public_conformance compare`
(`.github/workflows/native.yml`).
"""

import argparse
import subprocess
import sys
from pathlib import Path

from tools.commands import conversion
from tools.conformance import public_conformance
from tools.paths import ROOT

RUNNER = ROOT / "rust/tests/public_conformance_cli.py"


def prepare(output):
    """Write the corpus as 1.0 under `output`, excluding what the conversion
    gate excepts, and plan it; return the converted benchmarks directory and
    the plan's directory."""
    fixtures = conversion.convert(output)
    conversion.except_benchmarks(fixtures)
    plan = output / "plan"
    public_conformance.main(
        ["plan", "--fixtures", str(fixtures), "--output", str(plan)]
    )
    return fixtures, plan


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--binary", type=Path)
    mode.add_argument(
        "--prepare-only",
        action="store_true",
        help="write the converted copies and their plan, and run nothing",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--source-revision",
        help="the revision the records name (default: the checkout's HEAD)",
    )
    parser.add_argument(
        "--evidence", help="where the run is recorded, such as its Actions run"
    )
    args = parser.parse_args(argv)
    output = args.output.resolve()
    if output.exists():
        parser.error("output must be a fresh directory")
    if args.prepare_only:
        if args.source_revision is not None or args.evidence is not None:
            parser.error(
                "--prepare-only writes no records: no --source-revision or --evidence"
            )
        prepare(output)
        return 0
    revision = (
        args.source_revision
        or subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
    )
    fixtures, plan = prepare(output)
    evidence = [] if args.evidence is None else ["--evidence", args.evidence]
    # The runner writes to the same streams, after what was printed so far.
    sys.stdout.flush()
    ran = subprocess.run(
        [
            sys.executable,
            "-I",
            str(RUNNER),
            "--plan",
            str(plan),
            "--fixtures",
            str(fixtures),
            "--binary",
            str(args.binary),
            "--output",
            str(output / "observed"),
            "--source-revision",
            revision,
            *evidence,
        ],
        check=False,
    )
    if ran.returncode:
        return ran.returncode
    return public_conformance.compare(
        fixtures, output / "observed", output / "inventory.json"
    )


if __name__ == "__main__":
    raise SystemExit(main())
