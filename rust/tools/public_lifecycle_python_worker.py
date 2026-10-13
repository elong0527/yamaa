"""Isolated installed Python worker for public-lifecycle/1.

The worker has no expected report or expected frame. It transports actual copied
views and the held public Domain's raw observations for controller acceptance.
"""

from __future__ import annotations

import argparse
import gzip
import json
import platform
import struct
import sys
import time
import traceback
from pathlib import Path


def write_json(path, value):
    content = json.dumps(value, ensure_ascii=False, allow_nan=False, indent=2) + "\n"
    if path.suffix == ".gz":
        with gzip.open(path, "wt", encoding="utf-8", compresslevel=1) as target:
            target.write(content)
    else:
        path.write_text(content, encoding="utf-8")


def write_bytes(path, content):
    if path.suffix == ".gz":
        with gzip.open(path, "wb", compresslevel=1) as target:
            target.write(content)
    else:
        path.write_bytes(content)


def timed(durations, name, call):
    start = time.perf_counter_ns()
    try:
        return call()
    finally:
        end = time.perf_counter_ns()
        durations[name] = {"start_ns": start, "end_ns": end, "duration_ns": end - start}


def cell(value, kind):
    if value is None:
        return {"type": "missing", "value": None}
    if kind == "float":
        encoded = struct.pack(">d", value).hex()
    elif kind == "int":
        encoded = str(value)
    elif kind in {"date", "datetime"}:
        encoded = value.isoformat()
    else:
        encoded = value
    return {"type": kind, "value": encoded}


def frame(value, pl):
    if value is None:
        return None
    kinds = {pl.String: "str", pl.Int64: "int", pl.Float64: "float", pl.Date: "date"}
    types = []
    for dtype in value.dtypes:
        kind = kinds.get(dtype)
        if kind is None and isinstance(dtype, pl.Datetime):
            kind = "datetime"
        if kind is None:
            raise TypeError(f"unsupported actual copied dtype: {dtype}")
        types.append(kind)
    return {
        "columns": list(zip(value.columns, types)),
        "row_count": value.height,
        "rows": [
            [cell(item, kind) for item, kind in zip(row, types)] for row in value.rows()
        ],
        "host_schema": [[name, str(dtype)] for name, dtype in value.schema.items()],
    }


def issues(value, pl):
    kinds = {pl.String: "str", pl.List(pl.String): "list[str]"}
    schema = [[name, kinds[dtype]] for name, dtype in value.schema.items()]
    return {
        "columns": schema,
        "row_count": value.height,
        "rows": [list(row) for row in value.rows()],
    }


def metadata(yamaa):
    # Installed metadata follows Build. Core version requires source/build evidence.
    native = getattr(yamaa, "_native", None)
    if native is None:
        import importlib.util
        native_origin = importlib.util.find_spec("yamaa._native").origin
    else:
        native_origin = native.__file__
    return {
        "host": "python",
        "runtime_version": platform.python_version(),
        "report_runtime_version": sys.version,
        "package_version": yamaa.__version__,
        "engine_version": getattr(native, "_CORE_VERSION", None),
        "engine_version_availability": ("compiled metadata constant" if getattr(native, "_CORE_VERSION", None) else
                                        "compiled core metadata unavailable; source build attestation retained"),
        "installed_origins": {
            "package": str(Path(yamaa.__file__).resolve().parent),
            "native": str(Path(native_origin).resolve()),
        },
        "executable": str(Path(sys.executable).resolve()),
        "system": platform.system(),
        "machine": platform.machine(),
        "cpu_observation": platform.processor() or None,
        "sys_path": list(sys.path),
        "clock": {
            "provider": "time.perf_counter_ns",
            "unit": "ns",
            "implementation": time.get_clock_info("perf_counter").implementation,
            "monotonic": time.get_clock_info("perf_counter").monotonic,
            "adjustable": time.get_clock_info("perf_counter").adjustable,
        },
    }


def rss():
    system = platform.system()
    if system not in {"Linux", "Darwin"}:
        return None
    import resource

    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return {
        "provider": "resource.getrusage(RUSAGE_SELF).ru_maxrss",
        "raw": raw,
        "original_unit": "KiB" if system == "Linux" else "bytes",
        "bytes": raw * (1024 if system == "Linux" else 1),
        "scope": "whole-child-lifetime; cumulative across warm repetitions",
    }


def run(request):
    output = Path(request["output"])
    response = {
        "protocol": request["protocol"],
        "child": request["child"],
        "pid": __import__("os").getpid(),
        "import_duration": None,
        "repetitions": [],
        "failure": None,
    }
    write_json(output / "response.json", response)
    import_times = {}
    try:
        if "yamaa" in sys.modules:
            raise RuntimeError(
                "candidate package was already imported before the declared import boundary"
            )

        def load():
            import yamaa

            # Resolve the lazy public facade and its native module during import.
            # This performs no explicit native API call.
            domain = yamaa.yamaa_domain if request.get("evaluator") == "python-reference" else yamaa.domain
            import polars as pl

            return yamaa, domain, pl

        yamaa, domain, pl = timed(import_times, "package_import", load)
        response["import_duration"] = import_times["package_import"]
    except BaseException as error:
        response.update(
            import_duration=import_times.get("package_import"),
            failure={
                "class": type(error).__name__,
                "message": str(error),
                "traceback": traceback.format_exc(),
            },
        )
        write_json(output / "response.json", response)
        return 1
    for declaration in request["repetitions"]:
        row = {
            "index": declaration["index"],
            "declaration": declaration,
            "status": "failure",
            "durations": {},
            "metadata": None,
            "native_call_order": [],
            "unavailable": request["unavailable"],
            "rss": None,
            "failure": None,
            "reports": {},
            "saved_files": {},
            "save_returns": [],
            "evidence_capture_failures": [],
            "host_view_access_counts": {},
            "callback_observation": None,
            "views": f"rep-{declaration['index']}-views.json" + (".gz" if request.get("compress_evidence") else ""),
        }
        views = {}
        result = None
        response["repetitions"].append(row)
        write_json(output / row["views"], views)
        write_json(output / "response.json", response)
        prefix = f"rep-{declaration['index']}"
        target = Path(declaration["work_directory"]) / declaration["output_name"]
        capture_errors = []
        final_transport_failed = False

        def retain_reached(label, include_output=False):
            # Called once at the reached boundary, including from Save's finally.
            # A secondary transport error never replaces a pending public cause.
            try:
                filename = prefix + "-" + label + "-report.json" + (".gz" if request.get("compress_evidence") else "")
                raw = (None if request.get("evaluator") == "python-reference"
                       else result._native.observations())
                write_bytes(output / filename, b"null" if raw is None else raw.encode("utf-8"))
                row["reports"][label] = filename
            except BaseException as error:
                capture_errors.append(error)
                row["evidence_capture_failures"].append(
                    {
                        "location": label + "_report",
                        "class": type(error).__name__,
                        "message": str(error),
                        "traceback": traceback.format_exc(),
                    }
                )
            if include_output:
                try:
                    if target.exists():
                        filename = (
                            prefix + "-" + label + "-" + declaration["output_name"]
                            + (".gz" if request.get("compress_evidence") else "")
                        )
                        write_bytes(output / filename, target.read_bytes())
                        row["saved_files"][label] = filename
                    for name in declaration.get("artifact_names", []):
                        if (
                            name == declaration["output_name"]
                            and label in row["saved_files"]
                        ):
                            row.setdefault("saved_artifacts", {}).setdefault(label, {})[
                                name
                            ] = row["saved_files"][label]
                            continue
                        artifact = Path(declaration["work_directory"]) / name
                        if artifact.exists():
                            filename = prefix + "-" + label + "-" + name + (".gz" if request.get("compress_evidence") else "")
                            write_bytes(output / filename, artifact.read_bytes())
                            row.setdefault("saved_artifacts", {}).setdefault(label, {})[
                                name
                            ] = filename
                except BaseException as error:
                    capture_errors.append(error)
                    row["evidence_capture_failures"].append(
                        {
                            "location": label + "_output",
                            "class": type(error).__name__,
                            "message": str(error),
                            "traceback": traceback.format_exc(),
                        }
                    )

        def access(name):
            counts = row["host_view_access_counts"]
            counts[name] = counts.get(name, 0) + 1
            return getattr(result, name)

        callback_module = None
        original_callback = None
        callback_trace = []
        if request.get("observe_callbacks"):
            import functools
            import yamaa_benchmarks as callback_module
            original_callback = callback_module.project_value

            @functools.wraps(original_callback)
            def observed_callback(x):
                trace = {"parameters": {"x": cell(x, "float")},
                         "result": None, "failure": None}
                callback_trace.append(trace)
                try:
                    returned = original_callback(x=x)
                    trace["result"] = cell(returned, "float")
                    return returned
                except BaseException as error:
                    trace["failure"] = {"class": type(error).__name__, "message": str(error)}
                    raise

            callback_module.project_value = observed_callback

        try:
            if target.exists():
                raise RuntimeError(
                    "fresh public Build work directory already has an output"
                )
            row["native_call_order"].append("public_build")
            result = timed(
                row["durations"],
                "public_build",
                lambda: domain(
                    declaration["entry"],
                    **({"schema_root": request["reference_schema"]} if request.get("reference_schema") else {}),
                    **(
                        {"environment": declaration["environment"]}
                        if declaration.get("environment") and not request.get("reference_root_build")
                        else {}
                    ),
                ),
            )
            retain_reached("build")
            if request.get("reference_root_build"):
                import inspect
                row["build_invocation"] = {
                    "callable": domain.__module__ + "." + domain.__qualname__,
                    "signature": str(inspect.signature(domain)),
                    "parameters": list(inspect.signature(domain).parameters),
                    "source_file": str(Path(inspect.getsourcefile(domain)).resolve()),
                    "schema_root": request["reference_schema"],
                    "environment_passed": False,
                }
            if "artifact_names" in declaration:
                row["build_artifact_presence"] = {
                    name: (Path(declaration["work_directory"]) / name).exists()
                    for name in declaration["artifact_names"]
                }
            if capture_errors:
                raise capture_errors[0]
            row["native_call_order"].append("metadata_after_build")
            row["metadata"] = timed(
                row["durations"], "metadata_after_build", lambda: metadata(yamaa)
            )

            for name, call in (
                ("output", lambda: frame(access("output"), pl)),
                ("issues", lambda: issues(access("issues"), pl)),
                ("verification_log", lambda: frame(access("verification_log"), pl)),
                ("warning_log", lambda: frame(access("warning_log"), pl)),
            ):
                views[name] = timed(row["durations"], "copied_" + name, call)
                write_json(output / row["views"], views)
            for number in (() if request.get("reference_root_build") else (1, 2)):
                label = f"save_{number}"
                try:
                    returned = timed(row["durations"], "held_" + label, result.save)
                finally:
                    retain_reached(label, include_output=True)
                row["save_returns"].append(str(returned) if isinstance(returned, Path) else returned)
                if capture_errors:
                    raise capture_errors[0]
                write_json(output / "response.json", response)
            if request.get("reference_root_build"):
                views["output_after_build_repeat"] = frame(access("output"), pl)
                row["root_build_artifact_presence"] = {
                    name: (Path(declaration["work_directory"]) / name).exists()
                    for name in declaration["artifact_names"]
                }
            else:
                views["output_after_save_2"] = frame(access("output"), pl)
            write_json(output / row["views"], views)
            row["status"] = "success"
        except BaseException as error:
            row["failure"] = {
                "class": type(error).__name__,
                "message": str(error),
                "traceback": traceback.format_exc(),
            }
            print(row["failure"]["traceback"], file=sys.stderr)
        finally:
            if callback_module is not None:
                callback_module.project_value = original_callback
                row["callback_observation"] = {"provider": "once-delegating installed callable wrapper",
                    "scope": "activation tests and derivation combined; stage split unavailable",
                    "count": len(callback_trace), "trace": callback_trace}
            try:
                row["rss"] = rss()
            except BaseException as error:
                secondary = {
                    "location": "rss",
                    "class": type(error).__name__,
                    "message": str(error),
                    "traceback": traceback.format_exc(),
                }
                row["evidence_capture_failures"].append(secondary)
                if row["failure"] is None:
                    row["failure"] = secondary
                    row["status"] = "failure"
            if row["failure"] and row["failure"]["class"] in {
                "KeyboardInterrupt",
                "SystemExit",
            }:
                response["failure"] = row["failure"]
            try:
                write_json(output / "response.json", response)
            except BaseException as error:
                secondary = {
                    "location": "final_response",
                    "class": type(error).__name__,
                    "message": str(error),
                    "traceback": traceback.format_exc(),
                }
                row["evidence_capture_failures"].append(secondary)
                row["status"] = "failure"
                if row["failure"] is None:
                    row["failure"] = secondary
                response["failure"] = row["failure"]
                print("secondary final response transport failure:", file=sys.stderr)
                print(secondary["traceback"], file=sys.stderr)
                # The controller retains stderr and the last partial response.
                # Do not retry transport or enter another public lifecycle.
                final_transport_failed = True
            finally:
                result = None
        if final_transport_failed:
            return 1
        if row["failure"] and row["failure"]["class"] in {
            "KeyboardInterrupt",
            "SystemExit",
        }:
            return 1
    return int(any(row["status"] != "success" for row in response["repetitions"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", type=Path, required=True)
    args = parser.parse_args()
    return run(json.loads(args.request.read_text(encoding="utf-8")))


if __name__ == "__main__":
    raise SystemExit(main())
