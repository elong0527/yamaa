"""Run the benchmark corpus, written as 1.0, through a `yamaa` binary (#3060).

The command line reads only 1.x files: it refuses a file that declares
`schema_version` as `unsupported_language_version` (#3030 D122, D125). Until
the corpus moves to 1.0 (P2-5 to P2-8), its conformance runs converted
copies, as the 1.0 conversion gate does (`tools.commands.conversion`). This
writes every benchmark as 1.0 under OUTPUT/benchmarks, excludes each
benchmark the gate excepts, with the gate's reason, plans the converted
corpus into OUTPUT/plan, and runs the command line's runner,
`rust/tests/public_conformance_cli.py`, over it into OUTPUT/observed. Unlike
the gate, it keeps what it writes, so the records and the converted cases
they ran can be read afterwards.
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
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--source-revision",
        help="the revision the records name (default: the checkout's HEAD)",
    )
    args = parser.parse_args(argv)
    output = args.output.resolve()
    if output.exists():
        parser.error("output must be a fresh directory")
    revision = (
        args.source_revision
        or subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
    )
    fixtures, plan = prepare(output)
    return subprocess.run(
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
        ],
        check=False,
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
