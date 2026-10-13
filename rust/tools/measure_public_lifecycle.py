"""Opt-in installed public lifecycle collection; no release budgets or phase attribution.

This stdlib controller never imports the candidate. Acceptance uses unchanged Git
reports, typed cells and CSV bytes. Source/artifact association is attested by a
retained installation record; workers independently confirm actual loaded origins
and versions. A one-repeat correctness smoke is explicitly not characterization.
"""

from __future__ import annotations

import argparse
import copy
import functools
import gzip
import json
import math
import os
import platform
import shutil
import statistics
import struct
import subprocess
import sys
import tarfile
import traceback
import zipfile
from pathlib import Path, PureWindowsPath


CASES = ("adam-adlb-ordered-sum", "schema-window-functions", "schema-lookup")
OUTPUTS = dict(zip(CASES, ("adlb.csv", "advs.csv", "adsl.csv")))
FORMS = {"python": ("wheel", "source"), "r": ("source",)}
PROTOCOL = "public-lifecycle/1"
BOUNDARIES = (
    "package_import",
    "public_build",
    "copied_output",
    "copied_issues",
    "copied_verification_log",
    "copied_warning_log",
    "metadata_after_build",
    "held_save_1",
    "held_save_2",
)
UNAVAILABLE = {
    "phase_durations": None,
    "actual_callback_counts": None,
    "isolated_process_startup": None,
    "shipped_schema_initialization": None,
    "os_filesystem_cache": None,
}
HERE = Path(__file__).resolve().parent
WORKERS = {
    "python": "public_lifecycle_python_worker.py",
    "r": "public_lifecycle_r_worker.R",
}


def write_json(path, value):
    path.write_text(
        json.dumps(value, ensure_ascii=False, allow_nan=False, indent=2) + "\n",
        encoding="utf-8",
    )


def retained_bytes(path):
    return gzip.decompress(path.read_bytes()) if path.suffix == ".gz" else path.read_bytes()


def read_json(path):
    return json.loads(retained_bytes(path).decode("utf-8"))


@functools.lru_cache(maxsize=2048)
def git_bytes(repository, revision, path):
    return subprocess.run(
        ["git", "show", f"{revision}:{path}"],
        cwd=repository,
        check=True,
        capture_output=True,
    ).stdout


def git_paths(repository, revision, prefix):
    output = subprocess.run(
        ["git", "ls-tree", "-rz", "--name-only", revision, "--", prefix],
        cwd=repository,
        check=True,
        capture_output=True,
    ).stdout
    return [p.decode("utf-8") for p in output.split(b"\0") if p]


def surrounding_git_head(directory):
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=directory, capture_output=True, text=True
    )
    return result.stdout.strip() if result.returncode == 0 else None


def checkout_roots(repository):
    lines = subprocess.run(
        ["git", "worktree", "list", "--porcelain"],
        cwd=repository,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    return [Path(line[9:]).resolve() for line in lines if line.startswith("worktree ")]


def outside_checkout(path, repository, roots=None):
    resolved = Path(path).resolve(strict=True)
    if any(
        resolved.is_relative_to(root) for root in (roots or checkout_roots(repository))
    ):
        raise ValueError(f"installed origin is inside a checkout: {path}")
    return str(resolved)


def artifact_bytes(record, repository):
    """Tie the supplied archive to actual installed bytes/original native source.

    A source installation's final binary association still needs its retained
    build/installation attestation. Archive filenames alone are never proof.
    """
    artifact = Path(record["artifact_path"])
    comparisons = []
    if record["package_form"] == "wheel":
        with zipfile.ZipFile(artifact) as archive:
            names = [
                name
                for name in archive.namelist()
                if Path(name).parent.as_posix() == "yamaa"
                and Path(name).name.startswith("_native.")
                and Path(name).suffix in {".so", ".pyd", ".dylib"}
            ]
            if len(names) != 1:
                raise ValueError("wheel has no unique native library")
            data = archive.read(names[0])
            if data != Path(record["installed_origins"]["native"]).read_bytes():
                raise ValueError(
                    "actual installed library differs from declared wheel bytes"
                )
            comparisons.append({"member": names[0], "bytes": len(data), "equal": True})
    else:
        paths = [
            p
            for p in git_paths(repository, record["source_revision"], "rust")
            if (p.endswith(".rs") and "/src/" in p)
            or p.endswith("Cargo.toml")
            or p in {"rust/Cargo.lock", "rust/rust-toolchain.toml"}
        ]
        if not paths:
            raise ValueError("original native source tree is absent")
        with tarfile.open(artifact) as archive:
            members = {
                member.name: member
                for member in archive.getmembers()
                if member.isfile()
            }
            for path in paths:
                names = [name for name in members if name.endswith("/" + path)]
                if len(names) != 1:
                    raise ValueError(
                        "source archive lacks unique native source: " + path
                    )
                data = archive.extractfile(members[names[0]]).read()
                if data != git_bytes(repository, record["source_revision"], path):
                    raise ValueError(
                        "source archive differs from pinned Git bytes: " + path
                    )
                comparisons.append(
                    {
                        "member": names[0],
                        "original": path,
                        "bytes": len(data),
                        "equal": True,
                    }
                )
    return comparisons


def clock_evidence(record, repository, retained):
    """Reject microbenchmark's wall-clock fallback rather than infer a provider.

    The caller retains the selected source-build config and the built library.
    Direct library equality ties that attestation to the library the R child uses.
    The installed version is pinned because these are 1.5.0's selection branches.
    """
    collectors = record["collectors"]
    if set(collectors) != {"jsonlite", "microbenchmark"}:
        raise ValueError("R requires declared jsonlite and microbenchmark collectors")
    for name, spec in collectors.items():
        if not isinstance(spec.get("version"), str) or not spec["version"]:
            raise ValueError(f"missing collector version: {name}")
        spec["origin"] = outside_checkout(spec["origin"], repository)
    if collectors["microbenchmark"]["version"] != "1.5.0":
        raise ValueError("clock build admission currently covers microbenchmark 1.5.0")
    evidence = record["clock_evidence"]
    built = Path(evidence["built_library"]).resolve(strict=True)
    installed = Path(evidence["installed_library"]).resolve(strict=True)
    if not installed.is_relative_to(Path(collectors["microbenchmark"]["origin"])):
        raise ValueError("clock library is outside the declared collector")
    outside_checkout(installed, repository)
    if built.read_bytes() != installed.read_bytes():
        raise ValueError("installed timer library differs from retained build")
    retained.mkdir()
    shutil.copyfile(built, retained / built.name)
    system = record["system"]
    if system == "Windows":
        # 1.5.0's WIN32 branch precedes all config branches. The caller must
        # retain its actual build log with the WIN32 compilation declaration.
        config = Path(evidence["build_config"]).read_bytes()
        if b"WIN32" not in config or evidence["provider"] != "QueryPerformanceCounter":
            raise ValueError("missing Windows timer build declaration")
    else:
        config = Path(evidence["build_config"]).read_bytes()
        definitions = {}
        for line in config.decode("utf-8").splitlines():
            parts = line.split()
            if len(parts) >= 3 and parts[0] == "#define":
                definitions[parts[1]] = parts[2]
        if definitions.get("MB_HAVE_MACH_TIME") == "1" and system == "Darwin":
            provider = "mach_absolute_time"
        elif definitions.get("MB_HAVE_CLOCK_GETTIME") == "1":
            clockid = definitions.get("MB_CLOCKID_T")
            if clockid not in {
                "CLOCK_MONOTONIC",
                "CLOCK_MONOTONIC_RAW",
                "CLOCK_MONOTONIC_PRECISE",
            }:
                raise ValueError("timer build lacks an admitted monotonic clockid")
            provider = "clock_gettime/" + clockid
        else:
            raise ValueError("timer build has no admitted monotonic provider")
        if evidence["provider"] != provider:
            raise ValueError("declared timer differs from selected build config")
    (retained / "build-config.txt").write_bytes(config)
    record["clock_evidence"]["installed_library"] = str(installed)


def provenance(path, repository, host, retained):
    record = read_json(path)
    required = (
        "source_revision",
        "artifact_reference",
        "artifact_path",
        "package_form",
        "host",
        "runtime_version",
        "package_version",
        "engine_version",
        "toolchain",
        "installed_origins",
        "system",
        "cpu",
        "installation_evidence",
    )
    if any(key not in record for key in required):
        raise ValueError("missing declared installation provenance fields")
    if record["host"] != host or record["package_form"] not in FORMS[host]:
        raise ValueError("invalid installation host or package form")
    for key in (
        "source_revision",
        "artifact_reference",
        "runtime_version",
        "package_version",
        "engine_version",
        "system",
    ):
        if not isinstance(record[key], str) or not record[key]:
            raise ValueError(f"missing nonempty identity: {key}")
    if set(record["installed_origins"]) != {"package", "native"}:
        raise ValueError("declare both package and native-library origins")
    record["installed_origins"] = {
        key: outside_checkout(value, repository)
        for key, value in record["installed_origins"].items()
    }
    if not Path(record["installed_origins"]["native"]).is_relative_to(
        Path(record["installed_origins"]["package"])
    ):
        raise ValueError("native library is outside the declared package")
    if (
        not Path(record["installed_origins"]["package"]).is_dir()
        or not Path(record["installed_origins"]["native"]).is_file()
    ):
        raise ValueError("installed package/native origins have the wrong file kind")
    attestation = read_json(Path(record["installation_evidence"]))
    evidence, install = attestation["evidence"], attestation["installation"]
    for key in ("source_revision", "artifact_reference", "package_form"):
        if evidence[key] != record[key]:
            raise ValueError(f"installation evidence differs: {key}")
    for left, right in (
        ("runtime_version", "runtime_version"),
        ("package_version", "host_package_version"),
        ("engine_version", "core_version"),
    ):
        if record[left] != install[right]:
            raise ValueError(f"installed version evidence differs: {left}")
    installation_origin = "native" if host == "python" else "package"
    if (
        str(Path(install["package_location"]).resolve(strict=True))
        != record["installed_origins"][installation_origin]
    ):
        raise ValueError("installation evidence differs from package origin")
    retained.mkdir()
    shutil.copyfile(path, retained / "declared.json")
    shutil.copyfile(record["installation_evidence"], retained / "installation.json")
    native = Path(record["installed_origins"]["native"])
    shutil.copyfile(native, retained / "installed-native-library.bin")
    artifact = Path(record["artifact_path"]).resolve(strict=True)
    shutil.copyfile(artifact, retained / artifact.name)
    if (retained / artifact.name).read_bytes() != artifact.read_bytes():
        raise ValueError("retained artifact bytes differ")
    record["artifact_path"] = str(artifact)
    write_json(
        retained / "artifact-byte-comparisons.json", artifact_bytes(record, repository)
    )
    if host == "r":
        clock_evidence(record, repository, retained / "clock-build")
    return record


def stage_originals(repository, revision, destination):
    destination.mkdir(parents=True, exist_ok=False)
    paths = git_paths(repository, revision, "yaml")
    if not paths:
        raise ValueError("original schema tree is absent")
    for case in CASES:
        paths += git_paths(repository, revision, f"benchmarks/{case}")
        paths.append(f"rust/crates/yamaa-adapters/tests/fixtures/specs/{case}.json")
    comparisons = []
    for path in sorted(set(paths)):
        original = git_bytes(repository, revision, path)
        if (repository / path).read_bytes() != original:
            raise ValueError(f"fixture checkout differs from original source: {path}")
        target = destination / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(original)
        comparisons.append({"path": path, "bytes": len(original), "equal": True})
    verify_originals(repository, revision, destination, comparisons)
    return comparisons


def verify_originals(repository, revision, destination, comparisons):
    for row in comparisons:
        if (destination / row["path"]).read_bytes() != git_bytes(
            repository, revision, row["path"]
        ):
            raise ValueError(f"retained original bytes changed: {row['path']}")


def execution_plan(repeats, warmups):
    if repeats < 1 or warmups < 1:
        raise ValueError("at least one repeat and one retained warm-up are required")
    order = []
    for case in CASES:
        for host in ("python", "r"):
            for repeat in range(repeats):
                order.append(
                    {
                        "host": host,
                        "case": case,
                        "mode": "cold",
                        "child": f"{host}-{case}-cold-{repeat}",
                        "repetitions": [{"index": repeat, "warmup": False}],
                    }
                )
            order.append(
                {
                    "host": host,
                    "case": case,
                    "mode": "warm",
                    "child": f"{host}-{case}-warm",
                    "repetitions": [
                        {"index": i, "warmup": i < warmups}
                        for i in range(warmups + repeats)
                    ],
                }
            )
    return order


def worker_command(host, executable, worker, request):
    if host == "python":
        return [str(executable), "-I", str(worker), "--request", str(request)]
    return [str(executable), "--vanilla", str(worker), str(request)]


def worker_executable(path, repository):
    # Keep the invocation spelling: a venv's python symlink finds pyvenv.cfg
    # through that path. Audit its real target separately from its argv spelling.
    invoked = Path(os.path.abspath(path))
    roots = checkout_roots(repository)
    if any(invoked.is_relative_to(root) for root in roots):
        raise ValueError("worker executable invocation is inside a checkout")
    outside_checkout(invoked, repository, roots)
    if not invoked.is_file():
        raise ValueError("worker executable has the wrong file kind")
    return invoked


def specification_identity(path):
    # Match the native caller-path identity, including Windows verbatim/UNC
    # prefixes, while retaining literal backslashes in Unix filenames.
    text = path.as_posix()
    if isinstance(path, PureWindowsPath):
        if text.startswith("//?/UNC/"):
            return "//" + text.removeprefix("//?/UNC/")
        return text.removeprefix("//?/")
    return text


def compilation_probe():
    """A conservative process inventory plus an explicit caller attestation.

    The inventory is not an atomic global lock. Users must keep other compilation
    stopped throughout collection. An unavailable inventory refuses collection.
    """
    compiler_names = {
        "cargo",
        "rustc",
        "maturin",
        "cc",
        "c++",
        "gcc",
        "g++",
        "clang",
        "clang++",
        "ld",
        "ld64",
        "lld",
        "link",
        "cl",
        "gfortran",
        "f95",
        "make",
        "ninja",
        "cmake",
    }
    if os.name == "nt":
        command = [
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Process | Select-Object Id,ProcessName | ConvertTo-Json -Compress",
        ]
        raw = subprocess.run(command, check=True, capture_output=True, text=True).stdout
        rows = json.loads(raw)
        rows = rows if isinstance(rows, list) else [rows]
        processes = [(row["Id"], row["ProcessName"].lower()) for row in rows]
    else:
        raw = subprocess.run(
            ["ps", "-axo", "pid=,comm="], check=True, capture_output=True, text=True
        ).stdout
        processes = []
        for line in raw.splitlines():
            pid, command = line.strip().split(None, 1)
            processes.append((int(pid), Path(command).name.lower()))
    matches = [
        {"pid": pid, "name": name}
        for pid, name in processes
        if name.removesuffix(".exe") in compiler_names
    ]
    return {
        "provider": "process-name inventory",
        "compilers": matches,
        "clear": not matches,
        "atomic_lock": False,
    }


def expected_report(original, observed, entry, host, saved):
    expected = copy.deepcopy(original)
    expected.update(
        runtime=host,
        runtime_version=observed["report_runtime_version"],
        engine_version=observed["engine_version"],
        example="domain",
    )
    for field in ("nodes", "tables", "verifications", "callbacks"):
        for item in expected[field]:
            if item.get("specification") == "spec.yaml":
                item["specification"] = entry
    if not saved:
        expected["artifacts"] = []
    return expected


def expected_frame(original):
    (artifact,) = original["artifacts"]
    (table,) = [t for t in original["tables"] if t["stage"] == "derived"]
    projection = [table["columns"].index(name) for name in artifact["columns"]]
    return {
        "columns": list(zip(artifact["columns"], artifact["types"])),
        "row_count": len(table["rows"]),
        "rows": [[row[index] for index in projection] for row in table["rows"]],
    }


def expected_issues(original):
    return {
        "columns": [
            ["phase", "str"],
            ["condition", "str"],
            ["requirement", "str"],
            ["spec_paths", "list[str]"],
            ["context", "str"],
        ],
        "row_count": len(original["diagnostics"]),
        "rows": [
            [
                row["phase"],
                row["condition"],
                row["requirement"],
                row["spec_paths"],
                json.dumps(
                    row["context"],
                    ensure_ascii=False,
                    sort_keys=True,
                    separators=(",", ":"),
                ),
            ]
            for row in original["diagnostics"]
        ],
    }


def assert_equal(actual, expected, label):
    # JSON canonicalization preserves arrays/order and compares typed cell values
    # directly. It also rejects NaN instead of silently tolerating it.
    if json.dumps(actual, sort_keys=True, allow_nan=False) != json.dumps(
        expected, sort_keys=True, allow_nan=False
    ):
        raise ValueError(f"independent truth mismatch: {label}")


def actual_origins(observed, declared, repository):
    for field in (
        "host",
        "runtime_version",
        "package_version",
        "system",
    ):
        if observed[field] != declared[field]:
            raise ValueError(f"actual installed identity differs: {field}")
    if observed.get("engine_version") is not None and observed["engine_version"] != declared["engine_version"]:
        raise ValueError("actual installed core version differs from build attestation")
    roots = checkout_roots(repository)
    origins = {
        key: outside_checkout(value, repository, roots)
        for key, value in observed["installed_origins"].items()
    }
    assert_equal(origins, declared["installed_origins"], "actual installed origins")
    report_version = observed["report_runtime_version"]
    if (
        report_version.split()[0] if declared["host"] == "python" else report_version
    ) != declared["runtime_version"]:
        raise ValueError(
            "complete-report runtime label differs from the actual runtime"
        )
    if declared["host"] == "r":
        collectors = {
            name: dict(spec, origin=outside_checkout(spec["origin"], repository, roots))
            for name, spec in observed["collectors"].items()
        }
        assert_equal(
            collectors,
            declared["collectors"],
            "actual collector origins/versions",
        )
        if (
            str(Path(observed["clock_library"]).resolve(strict=True))
            != declared["clock_evidence"]["installed_library"]
        ):
            raise ValueError("loaded R timer library differs")
        if (
            observed["clock"]["provider"] != declared["clock_evidence"]["provider"]
            or observed["clock"]["api"] != "microbenchmark::get_nanotime"
        ):
            raise ValueError("actual R clock call differs from admitted raw provider")
    elif (
        observed["clock"]["monotonic"] is not True
        or observed["clock"]["adjustable"] is not False
    ):
        raise ValueError(
            "actual Python clock is not the declared monotonic boundary provider"
        )


def retained_file(child, filename):
    path = (child / filename).resolve(strict=True)
    if not path.is_relative_to(child.resolve()):
        raise ValueError("worker evidence escaped its child directory")
    return path


def binary64_value(value):
    if not isinstance(value, dict) or set(value) != {"decimal", "binary64_bits"}:
        raise ValueError("missing lossless binary64 timestamp fields")
    decimal, bits = value["decimal"], value["binary64_bits"]
    if (
        not isinstance(decimal, str)
        or not isinstance(bits, str)
        or len(bits) != 16
        or any(c not in "0123456789abcdef" for c in bits)
    ):
        raise ValueError("invalid lossless binary64 timestamp encoding")
    raw = bytes.fromhex(bits)
    number = struct.unpack(">d", raw)[0]
    if struct.pack(">d", float(decimal)) != raw:
        raise ValueError("timestamp decimal and binary64 bytes disagree")
    return number


def validate_duration(value, host=None):
    if not isinstance(value, dict):
        raise ValueError("missing reached raw boundary duration")
    start, end, duration = (value.get(k) for k in ("start_ns", "end_ns", "duration_ns"))
    binary64 = value.get("representation") == "binary64-nanoseconds"
    if host == "r" and not binary64:
        raise ValueError("R clock requires lossless raw binary64 timestamps")
    if host == "python" and (
        value.get("representation") is not None
        or any(type(number) is not int for number in (start, end, duration))
    ):
        raise ValueError("Python clock requires raw integer nanoseconds")
    if binary64:
        start, end, duration = map(binary64_value, (start, end, duration))
    elif value.get("representation") is not None:
        raise ValueError("unknown raw timestamp representation")
    if any(
        isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x)
        for x in (start, end, duration)
    ):
        raise ValueError("nonfinite or unavailable reached boundary duration")
    if (
        end < start
        or duration != end - start
        or (binary64 and struct.pack(">d", duration) != struct.pack(">d", end - start))
    ):
        raise ValueError("raw timer moved backwards or was adjusted")
    return duration


def validate_rss(rss, host, system):
    if host == "r" or system not in {"Linux", "Darwin"}:
        if rss is not None:
            raise ValueError("unvalidated RSS provider must remain null")
        return
    unit, scale = ("KiB", 1024) if system == "Linux" else ("bytes", 1)
    if (
        not isinstance(rss, dict)
        or rss.get("provider") != "resource.getrusage(RUSAGE_SELF).ru_maxrss"
    ):
        raise ValueError("missing actual Python child RSS provider")
    raw = rss.get("raw")
    if (
        isinstance(raw, bool)
        or not isinstance(raw, (int, float))
        or not math.isfinite(raw)
        or raw <= 0
    ):
        raise ValueError("invalid child peak RSS")
    if (
        rss.get("original_unit") != unit
        or rss.get("bytes") != raw * scale
        or rss.get("scope")
        != "whole-child-lifetime; cumulative across warm repetitions"
    ):
        raise ValueError("invalid RSS units or scope")


def validate_repetition(repository, originals, request, child, row, declared):
    if row.get("status") != "success":
        raise ValueError("worker repetition failed: " + str(row.get("failure")))
    if row["evidence_capture_failures"]:
        raise ValueError("secondary evidence transport failed")
    (wanted,) = [r for r in request["repetitions"] if r["index"] == row["index"]]
    assert_equal(row["declaration"], wanted, "declared repetition/work directory")
    observed = row["metadata"]
    actual_origins(observed, declared, repository)
    if row["native_call_order"] != ["public_build", "metadata_after_build"]:
        raise ValueError(
            "public Build must precede the first explicit native metadata call"
        )
    assert_equal(row["unavailable"], UNAVAILABLE, "unavailable metrics")
    validate_rss(row["rss"], request["host"], observed["system"])
    for boundary in BOUNDARIES[1:]:
        validate_duration(row["durations"][boundary], request["host"])
    original = read_json(
        originals
        / f"rust/crates/yamaa-adapters/tests/fixtures/specs/{request['case']}.json"
    )
    views = read_json(retained_file(child, row["views"]))
    for label in ("output", "output_after_save_2"):
        frame = views[label]
        # Keep physical host schema separately, while comparing portable declared
        # types, order and exact cell tags/float bits against the original report.
        assert_equal(
            {k: frame[k] for k in ("columns", "row_count", "rows")},
            expected_frame(original),
            label,
        )
        if not frame.get("host_schema"):
            raise ValueError("actual host output schema was not retained")
    assert_equal(views["issues"], expected_issues(original), "issues")
    assert_equal(views["verification_log"], None, "verification log")
    assert_equal(views["warning_log"], None, "warning log")
    for label, saved in (("build", False), ("save_1", True), ("save_2", True)):
        actual = read_json(retained_file(child, row["reports"][label]))
        expected = expected_report(
            original, observed, wanted["entry"], request["host"], saved
        )
        assert_equal(actual, expected, "complete " + label + " report")
    expected_csv = (
        originals / f"benchmarks/{request['case']}/expected/{OUTPUTS[request['case']]}"
    ).read_bytes()
    (artifact,) = original["artifacts"]
    if artifact["content"].encode("utf-8") != expected_csv or artifact[
        "byte_length"
    ] != len(expected_csv):
        raise ValueError("independent original report and CSV disagree")
    saved_bytes = [
        retained_file(child, row["saved_files"][key]).read_bytes()
        for key in ("save_1", "save_2")
    ]
    if (
        any(value != expected_csv for value in saved_bytes)
        or saved_bytes[0] != saved_bytes[1]
    ):
        raise ValueError("saved or held repeated bytes differ from original CSV")
    work = Path(wanted["work_directory"])
    if (work / wanted["output_name"]).read_bytes() != saved_bytes[1]:
        raise ValueError("actual published output differs from retained Save2 bytes")
    source_root = originals / "benchmarks" / request["case"]
    for source in source_root.rglob("*"):
        if (
            source.is_file()
            and (work / source.relative_to(source_root)).read_bytes()
            != source.read_bytes()
        ):
            raise ValueError(
                "original repetition fixture bytes changed during the public lifecycle"
            )
    assert_equal(
        row["save_returns"], [True, True], "original public Save return contract"
    )


def launch(command, child):
    request = read_json(child / "request.json")
    environment = os.environ.copy()
    if request["host"] == "r":
        libraries = [
            str(Path(request["provenance"]["installed_origins"]["package"]).parent)
        ]
        libraries += [
            str(Path(spec["origin"]).parent)
            for spec in request["provenance"]["collectors"].values()
        ]
        environment["R_LIBS"] = os.pathsep.join(dict.fromkeys(libraries))
    with (
        (child / "stdout.txt").open("wb") as out,
        (child / "stderr.txt").open("wb") as err,
    ):
        broker = [sys.executable, "-I", str(HERE / "child_peak_rss.py"),
                  "--command", json.dumps(command), "--record", str(child / "process.json")]
        process = subprocess.Popen(
            broker, cwd=child, env=environment, stdout=out, stderr=err
        )
        try:
            code = process.wait()
        except BaseException:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            else:
                process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            raise
    if code:
        raise ValueError("child RSS broker failed; retain stdout/stderr and unavailable peak")
    return read_json(child / "process.json")


def prepare(args):
    repository = args.repository.resolve(strict=True)
    revision = subprocess.run(
        ["git", "rev-parse", args.revision],
        cwd=repository,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if revision != args.revision:
        raise ValueError("revision must be the complete immutable Git commit identity")
    output = args.output.resolve()
    if any(output.is_relative_to(root) for root in checkout_roots(repository)):
        raise ValueError("measurement evidence must be outside all checkouts")
    output.mkdir(parents=True, exist_ok=False)
    plan = {
        "protocol": PROTOCOL,
        "source_revision": revision,
        "truth_repository_head": surrounding_git_head(repository),
        "purpose": args.purpose,
        "state": "preparing",
        "accepted": False,
        "compilation_clear_attested": args.compilation_clear,
        "execution_order": execution_plan(args.repeats, args.warmups),
        "children": [],
        "unavailable": UNAVAILABLE,
    }
    write_json(output / "collection.json", plan)
    records = {}
    for host in ("python", "r"):
        records[host] = provenance(
            getattr(args, f"{host}_provenance"),
            repository,
            host,
            output / (host + "-installation"),
        )
        if records[host]["source_revision"] != revision:
            raise ValueError(
                "installed artifact source differs from requested originals"
            )
        if records[host]["system"] != platform.system():
            raise ValueError("declared installed system differs from controller")
    plan["provenance"] = records
    plan["original_byte_comparisons"] = stage_originals(
        repository, revision, output / "originals"
    )
    harness = output / "harness"
    harness.mkdir()
    for name in (
        Path(__file__).name,
        "public_lifecycle_worker_protocol.json",
        "child_peak_rss.py",
        *WORKERS.values(),
    ):
        shutil.copyfile(HERE / name, harness / name)
    plan["harness_source"] = {
        "checkout_head": surrounding_git_head(HERE),
        "retained_direct_bytes": str(harness),
        "qualification_source_revision": revision,
        "identity_limit": "Retained executed bytes are authoritative. A surrounding checkout head does not prove untracked or modified harness bytes.",
    }
    for step in plan["execution_order"]:
        child = output / step["child"]
        child.mkdir()
        request = dict(
            step,
            protocol=PROTOCOL,
            output=str(child),
            provenance=records[step["host"]],
            unavailable=UNAVAILABLE,
        )
        repetitions = []
        for repetition in step["repetitions"]:
            work = child / f"work-{repetition['index']}"
            source_root = output / "originals" / "benchmarks" / step["case"]
            shutil.copytree(source_root, work)
            for source in source_root.rglob("*"):
                if (
                    source.is_file()
                    and (work / source.relative_to(source_root)).read_bytes()
                    != source.read_bytes()
                ):
                    raise ValueError(
                        "fresh repetition input differs from original bytes"
                    )
            repetitions.append(
                dict(
                    repetition,
                    work_directory=str(work),
                    entry=specification_identity(work / "spec.yaml"),
                    output_name=OUTPUTS[step["case"]],
                )
            )
        request["repetitions"] = repetitions
        write_json(child / "request.json", request)
    plan["state"] = "prepared"
    write_json(output / "collection.json", plan)
    return repository, output, plan


def collect(args, *, launcher=launch, probe=compilation_probe):
    if not args.compilation_clear:
        raise ValueError("explicit --compilation-clear attestation is required")
    if args.purpose == "correctness-smoke" and (args.repeats, args.warmups) != (1, 1):
        raise ValueError(
            "non-performance correctness smoke requires one repeat and one warm-up"
        )
    output = args.output.resolve()
    if output.exists():
        raise ValueError("output must be a fresh evidence directory")
    try:
        repository, output, plan = prepare(args)
        plan["state"] = "collecting"
        write_json(output / "collection.json", plan)
        for step in plan["execution_order"]:
            child = output / step["child"]
            row = dict(step, status="rejected", acceptance=[], failure=None)
            plan["children"].append(row)
            try:
                guard = probe()
                write_json(child / "compilation-probe.json", guard)
                if guard.get("clear") is not True or guard.get("compilers") != []:
                    raise ValueError("compilation is active or cannot be ruled out")
                verify_originals(
                    repository,
                    args.revision,
                    output / "originals",
                    plan["original_byte_comparisons"],
                )
                request = read_json(child / "request.json")
                executable = worker_executable(
                    getattr(args, f"{step['host']}_executable"), repository
                )
                command = worker_command(
                    step["host"],
                    executable,
                    output / "harness" / WORKERS[step["host"]],
                    child / "request.json",
                )
                row["command"] = command
                row["process"] = launcher(command, child)
                response = read_json(child / "response.json")
                assert_equal(response["protocol"], PROTOCOL, "worker protocol")
                assert_equal(response["child"], step["child"], "worker child identity")
                validate_duration(response["import_duration"], step["host"])
                records = response["repetitions"]
                assert_equal(
                    [r["index"] for r in records],
                    [r["index"] for r in request["repetitions"]],
                    "retained repetition order including warm-ups",
                )
                for rep in records:
                    acceptance = {
                        "index": rep["index"],
                        "warmup": rep["declaration"]["warmup"],
                        "accepted": False,
                    }
                    row["acceptance"].append(acceptance)
                    try:
                        validate_repetition(
                            repository,
                            output / "originals",
                            request,
                            child,
                            rep,
                            plan["provenance"][step["host"]],
                        )
                        acceptance["accepted"] = True
                    except Exception as error:
                        acceptance["failure"] = {
                            "class": type(error).__name__,
                            "message": str(error),
                        }
                if (
                    row["process"]["exit_code"] != 0
                    or response.get("failure") is not None
                    or not all(r["accepted"] for r in row["acceptance"])
                ):
                    raise ValueError("child or independent acceptance failed")
                row["status"] = "accepted"
            except Exception as error:
                row["failure"] = {
                    "class": type(error).__name__,
                    "message": str(error),
                    "traceback": traceback.format_exc(),
                }
            write_json(output / "collection.json", plan)
        verify_originals(
            repository,
            args.revision,
            output / "originals",
            plan["original_byte_comparisons"],
        )
        for host, record in plan["provenance"].items():
            if (
                Path(record["installed_origins"]["native"]).read_bytes()
                != (
                    output / (host + "-installation") / "installed-native-library.bin"
                ).read_bytes()
            ):
                raise ValueError("installed native library changed during collection")
        plan["accepted"] = all(row["status"] == "accepted" for row in plan["children"])
        plan["state"] = "accepted" if plan["accepted"] else "rejected"
        if plan["accepted"] and args.purpose == "characterization":
            summaries = []
            for row in plan["children"]:
                response = read_json(output / row["child"] / "response.json")
                summaries.append(
                    {
                        "host": row["host"],
                        "case": row["case"],
                        "mode": row["mode"],
                        "boundary": "package_import",
                        "duration_ns": validate_duration(
                            response["import_duration"], row["host"]
                        ),
                    }
                )
                for rep in response["repetitions"]:
                    if rep["declaration"]["warmup"]:
                        continue
                    for boundary, timing in rep["durations"].items():
                        summaries.append(
                            {
                                "host": row["host"],
                                "case": row["case"],
                                "mode": row["mode"],
                                "boundary": boundary,
                                "duration_ns": validate_duration(timing, row["host"]),
                            }
                        )
            # Raw records remain authoritative. No release threshold is assigned.
            groups = {}
            for item in summaries:
                key = tuple(item[k] for k in ("host", "case", "mode", "boundary"))
                groups.setdefault(key, []).append(item["duration_ns"])
            plan["distribution"] = [
                dict(
                    zip(("host", "case", "mode", "boundary"), key),
                    count=len(values),
                    min_ns=min(values),
                    median_ns=statistics.median(values),
                    max_ns=max(values),
                    quartiles_ns=statistics.quantiles(values, n=4, method="inclusive")
                    if len(values) > 1
                    else None,
                )
                for key, values in groups.items()
            ]
        write_json(output / "collection.json", plan)
        return plan
    except BaseException as error:
        if output.exists():
            path = output / "collection.json"
            plan = read_json(path) if path.exists() else {"protocol": PROTOCOL}
            plan.update(
                state="interrupted"
                if isinstance(error, KeyboardInterrupt)
                else "rejected",
                accepted=False,
                failure={
                    "class": type(error).__name__,
                    "message": str(error),
                    "traceback": traceback.format_exc(),
                },
            )
            write_json(path, plan)
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "repository",
        "output",
        "python-provenance",
        "r-provenance",
        "python-executable",
        "r-executable",
    ):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument(
        "--purpose", choices=("correctness-smoke", "characterization"), required=True
    )
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--compilation-clear", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = collect(args)
        print(
            json.dumps(
                {
                    "state": result["state"],
                    "purpose": result["purpose"],
                    "evidence": str(args.output.resolve()),
                }
            )
        )
        return 0 if result["accepted"] else 1
    except Exception as error:
        print(f"{type(error).__name__}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
