"""Time the performance cases against polars and dplyr: a measurement, run by
hand, never in CI. Results are evidence for an issue, never for a committed
file.

For each case and size it generates the inputs (generate.py), copies them
into one directory per engine, since a run refuses symlinked inputs (FILE-8),
and runs each engine as its own process. An engine's first output is checked
equal to yamaa's, sorted by the case's keys, before any of its times count.
It reports the median wall time of the runs (3, or 1 at 7.2M rows and over),
the peak resident memory from wait4, the output's rows and file size, and
yamaa's own phase table from `--verbose`.
"""

import argparse
import dataclasses
import datetime
import os
import platform
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

import polars as pl
from polars.testing import assert_frame_equal

from tools.paths import ROOT
from tools.performance import generate

HERE = Path(__file__).resolve().parent
CASES = HERE / "cases"
DEFAULT_CLI = ROOT / "rust" / "target" / "release" / "yamaa"
DEFAULT_WORK = ROOT / "build" / "performance"
TABLE_SIZES = ("18k", "180k", "1.8M", "7.2M")
FUNCTION_SIZES = ("1M",)
SINGLE_RUN_ROWS = 7_200_000
PHASES = (
    "preparation",
    "input",
    "admission",
    "derivation",
    "output_keys",
    "verification",
    "output",
)
LB_KEYS = ("STUDYID", "USUBJID", "LBTESTCD", "LBDTC")


@dataclasses.dataclass(frozen=True)
class Engine:
    """One program that builds a case's output from its staged inputs."""

    name: str
    # The command, given the run directory and the options.
    command: object
    # Extra environment variables of the command.
    environment: tuple = ()
    # The case files staged beside the inputs, relative to cases/<case>/.
    files: tuple = ()
    # Why it cannot run with these options, or None.
    unavailable: object = lambda options: None


@dataclasses.dataclass(frozen=True)
class Case:
    """One case: its inputs at a size, its output and keys, its engines; the
    first engine is yamaa, the reference the others are checked against."""

    name: str
    inputs: object
    output: str
    keys: tuple
    engines: tuple
    sizes: tuple


def _lb_inputs(rows):
    subjects = max(1, round(rows / generate.ROWS_PER_SUBJECT))
    return {"lb.parquet": lambda: generate.lb(subjects)}


def _bds_inputs(rows):
    subjects = max(1, round(rows / generate.ROWS_PER_SUBJECT))
    return {
        "lb.parquet": lambda: generate.lb(subjects),
        "adsl.parquet": lambda: generate.adsl(subjects),
    }


def _vitals_inputs(rows):
    return {"vitals.parquet": lambda: generate.vitals(rows)}


def _cli(directory, options):
    return [str(options.cli), "run", "spec.yaml", "--verbose"]


def _polars(case):
    def command(directory, options):
        return [sys.executable, str(HERE / "polars_cases.py"), case, str(directory)]

    return command


def _host(*arguments):
    def command(directory, options):
        return [str(options.python), str(HERE / "host_run.py"), *arguments]

    return command


def _no_cli(options):
    if not options.cli.is_file():
        return (
            f"no yamaa CLI at {options.cli}: build it with "
            "`cargo build --locked --release -p yamaa-cli` from rust/, "
            "or pass --cli"
        )
    return None


def _no_python(options):
    if options.python is None:
        return (
            "needs --python, an interpreter with a release build of yamaa "
            "installed (see tools/performance/README.md)"
        )
    return None


def _no_dplyr(options):
    rscript = shutil.which(options.rscript)
    if rscript is None:
        return f"R is not installed ({options.rscript} not found)"
    probe = subprocess.run(
        [rscript, "-e", "library(dplyr); library(arrow)"],
        capture_output=True,
        text=True,
    )
    if probe.returncode != 0:
        return "R lacks dplyr or arrow: " + probe.stderr.strip().splitlines()[-1]
    return None


def _cases():
    yamaa = Engine("yamaa", _cli, files=("spec.yaml",), unavailable=_no_cli)
    return {
        "bds": Case(
            "bds",
            _bds_inputs,
            "adlb.parquet",
            ("STUDYID", "USUBJID", "PARAMCD", "ADT", "ASEQ"),
            (
                yamaa,
                Engine("polars", _polars("bds")),
                Engine("polars-1", _polars("bds"), (("POLARS_MAX_THREADS", "1"),)),
                Engine(
                    "dplyr",
                    lambda directory, options: [
                        options.rscript,
                        str(HERE / "dplyr_bds.R"),
                        str(directory),
                    ],
                    unavailable=_no_dplyr,
                ),
            ),
            TABLE_SIZES,
        ),
        "copy": Case(
            "copy",
            _lb_inputs,
            "lb.parquet",
            LB_KEYS,
            (yamaa, Engine("polars", _polars("copy"))),
            TABLE_SIZES,
        ),
        "function": Case(
            "function",
            _vitals_inputs,
            "adsl.parquet",
            ("STUDYID", "USUBJID"),
            (
                Engine(
                    "yamaa",
                    _host("expression.yaml"),
                    files=("expression.yaml",),
                    unavailable=_no_python,
                ),
                Engine(
                    "yamaa-function",
                    _host("project.yaml", "ADSL"),
                    environment=(("PYTHONPATH", "."),),
                    files=(
                        "project.yaml",
                        "function.yaml",
                        "functions/bmi.yaml",
                        "yamaa_performance_bmi.py",
                    ),
                    unavailable=_no_python,
                ),
            ),
            FUNCTION_SIZES,
        ),
    }


def rows_of(size):
    """The row count a size names: `18k`, `1.8M` or a plain integer."""
    text = size.strip()
    factor = {"k": 1_000, "M": 1_000_000}.get(text[-1:], 1)
    number = float(text[:-1] if factor != 1 else text)
    rows = round(number * factor)
    if rows < 1:
        raise argparse.ArgumentTypeError(f"size {size!r} names no rows")
    return rows


def label(rows):
    """A row count as the table writes it: 18k, 1.8M, 7.2M."""
    for factor, suffix in ((1_000_000, "M"), (1_000, "k")):
        if rows >= factor:
            return f"{rows / factor:.4g}{suffix}"
    return str(rows)


def inputs(case, rows, work):
    """Generate the case's inputs at `rows`, once per work directory; the
    generator is deterministic, so a directory once written stays valid."""
    directory = work / "data" / f"{case.name}-{rows}"
    files = case.inputs(rows)
    if not all((directory / name).is_file() for name in files):
        partial = directory.with_name(directory.name + ".partial")
        shutil.rmtree(partial, ignore_errors=True)
        for name, frame in files.items():
            generate.write(frame(), partial / name)
        shutil.rmtree(directory, ignore_errors=True)
        partial.rename(directory)
    return directory


def stage(case, engine, data, directory):
    """A fresh run directory with copies of the inputs and the case files."""
    shutil.rmtree(directory, ignore_errors=True)
    (directory / "input").mkdir(parents=True)
    for source in data.iterdir():
        shutil.copyfile(source, directory / "input" / source.name)
    for name in engine.files:
        target = directory / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(CASES / case.name / name, target)


@dataclasses.dataclass
class Measure:
    seconds: float
    peak_bytes: int
    status: int
    stderr: str


def measure(command, directory, environment):
    """Run `command` in `directory` through launch.py; its wall time, peak
    RSS, exit status and standard error."""
    result = directory / "measure.txt"
    result.unlink(missing_ok=True)
    with (
        open(directory / "stdout.txt", "wb") as out,
        open(directory / "stderr.txt", "wb") as err,
    ):
        subprocess.run(
            [
                sys.executable,
                "-I",
                "-S",
                str(HERE / "launch.py"),
                str(result),
                *command,
            ],
            cwd=directory,
            stdout=out,
            stderr=err,
            env=environment,
            check=True,
        )
    seconds, peak, status = result.read_text().split()
    # ru_maxrss counts KiB on Linux and bytes on macOS.
    scale = 1 if sys.platform == "darwin" else 1024
    return Measure(
        float(seconds),
        int(peak) * scale,
        int(status),
        (directory / "stderr.txt").read_text(errors="replace"),
    )


def phases(stderr):
    """yamaa's phase table from `--verbose` standard error: phase -> ms."""
    table = {}
    lines = stderr.splitlines()
    for index, line in enumerate(lines):
        if line.startswith("name\tphase\t"):
            columns = line.split("\t")
            for row in lines[index + 1 :]:
                cells = row.split("\t")
                if len(cells) != len(columns):
                    break
                record = dict(zip(columns, cells))
                table[record["phase"]] = table.get(record["phase"], 0.0) + float(
                    record["elapsed_ms"]
                )
    return table


def difference(case, reference, candidate):
    """None when `candidate` holds `reference`'s rows, sorted by the case's
    keys, with every value equal; otherwise what differs."""
    expected = pl.read_parquet(reference)
    try:
        actual = pl.read_parquet(candidate)
        if actual.columns != expected.columns:
            return f"columns {actual.columns} differ from {expected.columns}"
        assert_frame_equal(
            expected.sort(case.keys),
            actual.cast(expected.schema).sort(case.keys),
            check_exact=True,
        )
    except Exception as error:  # noqa: BLE001 - reported, not raised
        return " ".join(str(error).split())[:300]
    return None


@dataclasses.dataclass
class Result:
    case: str
    rows: int
    engine: str
    note: str = ""
    seconds: float = 0.0
    peak_bytes: int = 0
    output_rows: int = 0
    output_bytes: int = 0
    phases: dict = dataclasses.field(default_factory=dict)


def run_case(case, rows, engines, options):
    """Every selected engine of `case` at `rows`; one Result each."""
    data = inputs(case, rows, options.work)
    repeat = options.repeat or (1 if rows >= SINGLE_RUN_ROWS else 3)
    results = []
    reference = None
    for engine in engines:
        result = Result(case.name, rows, engine.name)
        results.append(result)
        reason = engine.unavailable(options)
        if reason:
            result.note = f"skipped: {reason}"
            continue
        if engine is not case.engines[0] and reference is None:
            result.note = "skipped: yamaa produced no output to check it against"
            continue
        directory = options.work / "runs" / case.name / label(rows) / engine.name
        stage(case, engine, data, directory)
        output = directory / case.output
        environment = os.environ | dict(engine.environment)
        if engine.name == "polars" and options.polars_threads:
            environment["POLARS_MAX_THREADS"] = str(options.polars_threads)
        measures = []
        for _ in range(repeat):
            output.unlink(missing_ok=True)
            done = measure(engine.command(directory, options), directory, environment)
            if done.status != 0 or not output.is_file():
                printed = (directory / "stdout.txt").read_text(errors="replace")
                tail = " ".join((printed + done.stderr).split()[-40:])
                result.note = f"failed (exit {done.status}): {tail}"
                break
            if not measures and reference is not None:
                differs = difference(case, reference, output)
                if differs:
                    result.note = f"differs from yamaa: {differs}"
                    break
            measures.append(done)
        if not measures or result.note:
            continue
        if engine is case.engines[0]:
            reference = output
        middle = sorted(measures, key=lambda each: each.seconds)[len(measures) // 2]
        result.seconds = statistics.median(each.seconds for each in measures)
        result.peak_bytes = max(each.peak_bytes for each in measures)
        result.output_rows = pl.scan_parquet(output).select(pl.len()).collect().item()
        result.output_bytes = output.stat().st_size
        result.phases = phases(middle.stderr)
        print(
            f"  {case.name} {label(rows)} {engine.name}: {result.seconds:.2f} s",
            flush=True,
        )
    return results


def _megabytes(count):
    return (
        f"{count / 1_000_000:,.1f} MB"
        if count >= 1_000_000
        else f"{count / 1_000:,.0f} kB"
    )


def report(results):
    """The results as two markdown tables: times and memory, then yamaa's
    phases."""
    lines = [
        "| case | input rows | engine | output rows | wall s | peak RSS | output file |",
        "|---|---|---|---|---|---|---|",
    ]
    for each in results:
        if each.note:
            lines.append(
                f"| {each.case} | {label(each.rows)} | {each.engine} | {each.note} | | | |"
            )
            continue
        lines.append(
            f"| {each.case} | {label(each.rows)} | {each.engine} | "
            f"{each.output_rows:,} | {each.seconds:.2f} | "
            f"{_megabytes(each.peak_bytes)} | {_megabytes(each.output_bytes)} |"
        )
    timed = [each for each in results if each.phases]
    if timed:
        lines += [
            "",
            "yamaa phases (ms, the median run's):",
            "",
            "| case | input rows | engine | " + " | ".join(PHASES) + " |",
            "|---|---|---|" + "---|" * len(PHASES),
        ]
        for each in timed:
            cells = " | ".join(
                f"{each.phases[phase]:,.0f}" if phase in each.phases else ""
                for phase in PHASES
            )
            lines.append(
                f"| {each.case} | {label(each.rows)} | {each.engine} | {cells} |"
            )
    return "\n".join(lines)


def _output(command):
    try:
        return subprocess.run(
            command, capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def header(options):
    """What the numbers were measured on: revision, builds, tools, machine."""
    revision = _output(["git", "-C", str(ROOT), "rev-parse", "--short=9", "HEAD"])
    changed = _output(
        ["git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no"]
    )
    cli = _output([str(options.cli), "--version"]) if options.cli.is_file() else None
    lines = [
        f"revision: {revision}{' with local changes' if changed else ''}",
        f"date: {datetime.date.today().isoformat()}",
        f"machine: {platform.system()} {platform.machine()}, {os.cpu_count()} cores"
        + _memory(),
        f"cli: {_shown(options.cli)} ({cli.splitlines()[0] if cli else 'missing'})",
    ]
    if options.python:
        version = _output(
            [str(options.python), "-c", "import yamaa; print(yamaa.info()['version'])"]
        )
        lines.append(f"python host: {_shown(options.python)} (yamaa {version})")
    threads = options.polars_threads or os.environ.get("POLARS_MAX_THREADS")
    lines.append(
        f"polars: {pl.__version__}, {threads or 'default'} threads (polars-1: 1)"
    )
    rscript = shutil.which(options.rscript)
    if rscript:
        dplyr = _output([rscript, "-e", 'cat(format(packageVersion("dplyr")))'])
        lines.append(
            f"R: {_output([rscript, '-e', 'cat(R.version.string)'])}, "
            f"dplyr {dplyr or 'missing'}"
        )
    return "\n".join(lines)


def _shown(path):
    """`path` relative to the repository root, so a posted header names no
    machine's directories."""
    return os.path.relpath(path, ROOT)


def _memory():
    try:
        with open("/proc/meminfo") as meminfo:
            kib = int(meminfo.readline().split()[1])
        return f", {kib / 1024 / 1024:.0f} GB"
    except (OSError, ValueError, IndexError):
        return ""


def parse(argv):
    cases = _cases()
    parser = argparse.ArgumentParser(
        prog="python -m tools.performance",
        description=__doc__.split("\n\n")[0],
    )
    parser.add_argument(
        "--case",
        action="append",
        choices=sorted(cases),
        help="a case to run (repeatable; default: all)",
    )
    parser.add_argument(
        "--size",
        action="append",
        type=rows_of,
        help="input rows, such as 18k or 1.8M (repeatable; default: "
        f"{', '.join(TABLE_SIZES)} for bds and copy, {FUNCTION_SIZES[0]} for function)",
    )
    parser.add_argument(
        "--engine",
        action="append",
        help="an engine to run besides yamaa, which always runs as the "
        "reference: polars, polars-1, dplyr, yamaa-function (repeatable; "
        "default: all); `--engine yamaa` runs yamaa alone",
    )
    parser.add_argument(
        "--cli",
        type=Path,
        default=DEFAULT_CLI,
        help=f"the yamaa command line to time (default: {DEFAULT_CLI.relative_to(ROOT)})",
    )
    parser.add_argument(
        "--python",
        type=Path,
        help="an interpreter with yamaa installed, for the function case",
    )
    parser.add_argument("--rscript", default="Rscript", help="Rscript for dplyr")
    parser.add_argument(
        "--polars-threads", type=int, help="POLARS_MAX_THREADS for the polars engine"
    )
    parser.add_argument(
        "--repeat",
        type=int,
        help="runs per engine (default: 3, 1 at 7.2M rows and over)",
    )
    parser.add_argument(
        "--work",
        type=Path,
        default=DEFAULT_WORK,
        help=f"where inputs and outputs go (default: {DEFAULT_WORK.relative_to(ROOT)})",
    )
    options = parser.parse_args(argv)
    options.cli = options.cli.resolve()
    options.work = options.work.resolve()
    if options.python:
        options.python = Path(shutil.which(options.python) or options.python).absolute()
    return cases, options


def main(argv=None):
    cases, options = parse(argv)
    print(header(options), flush=True)
    results = []
    for name in options.case or sorted(cases, key=["bds", "copy", "function"].index):
        case = cases[name]
        engines = [
            engine
            for index, engine in enumerate(case.engines)
            if index == 0 or options.engine is None or engine.name in options.engine
        ]
        for rows in options.size or [rows_of(size) for size in case.sizes]:
            results += run_case(case, rows, engines, options)
    print()
    print(report(results))
    return 0
