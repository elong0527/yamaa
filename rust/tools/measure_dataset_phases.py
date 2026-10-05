"""Measure installed prototypes in fresh processes against unchanged benchmark CSVs.

This is an opt-in measurement tool, not a conformance runner or a timing gate.
Only native-profile samples use private instrumentation. Baseline native and
reference samples retain the ordinary entrypoints. Every sample checks truth.
"""

import argparse
import json
import platform
import statistics
import subprocess
import sys
import time
from contextlib import ExitStack
from importlib.metadata import version
from pathlib import Path
from unittest.mock import patch

CASES = (
    "adam-adlb-ordered-sum",
    "schema-window-functions",
    "schema-lookup",
    "schema-functions",
)
MODES = ("reference", "native", "native-profile")


def rss_bytes():
    """Read process lifetime peak RSS where the standard library exposes its units."""
    if sys.platform not in {"darwin", "linux"}:
        return None
    import resource

    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return peak if sys.platform == "darwin" else peak * 1024


def sample(root, case_name, mode):
    """Execute once with cold activation; stage timers never trigger extra work."""
    seconds = {}

    def timed(label, function, *args, **kwargs):
        """Accumulate a labeled boundary duration, including failures before propagation."""
        started = time.perf_counter_ns()
        try:
            return function(*args, **kwargs)
        finally:
            seconds[label] = (
                seconds.get(label, 0) + (time.perf_counter_ns() - started) / 1e9
            )

    started = time.perf_counter_ns()
    import yamaa
    from yamaa.io import ProjectResources, load_source_tables, render_artifact
    from yamaa.specification import load_specification

    if mode == "reference":
        from yamaa.functions.execution import execute_with_project_functions
        from yamaa.runtime import execute_with_source_provider
    else:
        import yamaa_native
        from yamaa.adapters import native_datasets
        from yamaa.adapters.native_datasets import (
            execute_with_project_functions,
            execute_with_source_provider,
        )
    seconds["package_imports"] = (time.perf_counter_ns() - started) / 1e9
    origins = {"yamaa": str(Path(yamaa.__file__).resolve())}
    if mode != "reference":
        origins["yamaa_native"] = str(Path(yamaa_native.__file__).resolve())
    if any(Path(origin).is_relative_to(root.resolve()) for origin in origins.values()):
        raise RuntimeError(
            "measurement requires installed packages outside the fixture checkout"
        )
    case = root / "benchmarks" / case_name
    schema = root / "yaml"
    specification = timed(
        "load_normalize_validate",
        load_specification,
        case / "spec.yaml",
        schema,
    ).specification

    def provider(declarations):
        """Time existing host source ingestion without changing declarations or tables."""
        return timed(
            "source_ingestion",
            load_source_tables,
            declarations,
            ProjectResources(case),
        )

    profiles = []

    def profile(request, source, secondary=None, callbacks=None):
        """Record one real native execution and return its original result pair."""
        table, outcome, encoded = yamaa_native._profile_dataset_functions(
            request,
            source,
            [] if secondary is None else secondary,
            [] if callbacks is None else callbacks,
        )
        profiles.append(json.loads(encoded))
        return table, outcome

    def wrap(label, function):
        """Capture the original callable before installing a measurement boundary."""

        def measured(*args, **kwargs):
            """Delegate exactly once and preserve the original return value or exception."""
            return timed(label, function, *args, **kwargs)

        return measured

    with ExitStack() as stack:
        if mode == "native-profile":
            for name, label in (
                ("admit", "specification_admission"),
                ("activate_project", "project_activation"),
                ("plan_execution", "host_planning"),
                ("lower", "host_lowering"),
                ("_source_ipc", "host_arrow_encoding"),
                ("_output_table", "host_output_materialization"),
                ("build_artifact", "host_artifact_construction"),
            ):
                stack.enter_context(
                    patch.object(
                        native_datasets,
                        name,
                        wrap(label, getattr(native_datasets, name)),
                    )
                )
            for name in (
                "execute_dataset",
                "execute_dataset_sources",
                "execute_dataset_functions",
            ):
                stack.enter_context(
                    patch.object(yamaa_native, name, wrap("native_call", profile))
                )
        if case_name == "schema-functions":
            run = timed(
                "run_total",
                execute_with_project_functions,
                specification,
                provider,
                case / "python",
                schema,
                cache=None,
            )
        else:
            run = timed(
                "run_total", execute_with_source_provider, specification, provider
            )
    result = run if mode == "reference" else run.result
    if result.status != "success":
        raise RuntimeError(f"{case_name}/{mode}: {result}")
    output = timed("render", render_artifact, result.artifact)
    peak = rss_bytes()
    expected = (case / "expected" / Path(specification.output.path).name).read_bytes()
    if output != expected:
        raise RuntimeError(f"{case_name}/{mode}: output differs from committed truth")
    if mode == "native-profile" and len(profiles) != 1:
        raise RuntimeError("expected exactly one instrumented dataset attempt")
    return {
        "case": case_name,
        "mode": mode,
        "seconds": seconds,
        "native_profiles": profiles,
        "peak_rss_bytes": peak,
        "expected_csv_equal": True,
        "output_rows": result.table.frame.height,
        "installed_versions": {
            name: version(name)
            for name in ("yamaa", "yamaa-native", "polars", "pyarrow")
        },
        "module_origins": origins,
    }


def measure(args):
    """Alternate modes across repeats, retaining every raw sample without trimming."""
    samples = []
    for repeat in range(args.repeats):
        for case in CASES:
            modes = MODES[repeat % len(MODES) :] + MODES[: repeat % len(MODES)]
            for mode in modes:
                started = time.perf_counter_ns()
                try:
                    completed = subprocess.run(
                        [
                            sys.executable,
                            "-I",
                            str(Path(__file__).resolve()),
                            "--root",
                            str(args.root.resolve()),
                            "--sample",
                            case,
                            mode,
                        ],
                        text=True,
                        capture_output=True,
                        check=True,
                    )
                except subprocess.CalledProcessError as error:
                    sys.stderr.write(error.stderr)
                    raise
                elapsed = (time.perf_counter_ns() - started) / 1e9
                record = json.loads(completed.stdout)
                record["repeat"] = repeat + 1
                record["seconds"]["process_wall"] = elapsed
                samples.append(record)
    summaries = []
    for case in CASES:
        for mode in MODES:
            selected = [s for s in samples if (s["case"], s["mode"]) == (case, mode)]
            summaries.append(
                {
                    "case": case,
                    "mode": mode,
                    "median_seconds": {
                        name: statistics.median(s["seconds"][name] for s in selected)
                        for name in selected[0]["seconds"]
                    },
                }
            )
    args.output.write_text(
        json.dumps(
            {
                "protocol": "dataset-measurement/1",
                "platform": platform.platform(),
                "machine": platform.machine(),
                "python": sys.version,
                "executable": sys.executable,
                "repeats": args.repeats,
                "activation_cache": "disabled; fresh process for every sample",
                "samples": samples,
                "summaries": summaries,
            },
            indent=2,
        )
        + "\n"
    )


def main():
    """Keep source fixtures explicit and import packages only from the installed environment."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument(
        "--sample", nargs=2, metavar=("CASE", "MODE"), help=argparse.SUPPRESS
    )
    args = parser.parse_args()
    if args.sample:
        case, mode = args.sample
        if case not in CASES or mode not in MODES:
            parser.error("unknown case or mode")
        print(json.dumps(sample(args.root, case, mode)))
    else:
        if args.output is None or args.repeats < 1:
            parser.error("--output and at least one repeat are required")
        measure(args)


if __name__ == "__main__":
    main()
