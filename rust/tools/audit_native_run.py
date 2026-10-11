"""Audit retained native CI evidence without running an engine or inventing passes.

The JSON/Markdown describe evidence completeness, not whole-corpus semantic parity.
Run IDs fetch an exact Actions attempt; --input replays its downloaded snapshot.
Package identity is its artifact reference, and equality compares actual bytes.
"""

from __future__ import annotations

import argparse
import base64
import io
import itertools
import json
import re
import subprocess
import tarfile
import zipfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from check_cargo_lock import check_archive as check_cargo_archive
from check_math_portability import FUNCTIONS, compare, observations, validate_hosts
from check_python_package_hygiene import check_archive as check_python_archive
from math_corpus import validate_corpus

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
FORMS = ("wheel", "source")
SOURCE_FILES = (
    "rust/Cargo.lock",
    "rust/Cargo.toml",
    "rust/rust-toolchain.toml",
    "python/uv.lock",
    "benchmarks/execution-manifest.yaml",
    "rust/qualification/supplemental.json",
)
COHORT = (
    "adam-adlb-ordered-sum",
    "schema-lookup",
    "schema-window-functions",
    "schema-inheritance",
    "negative-zero-division",
    "negative-integer-overflow",
    "negative-formula-flag",
    "negative-row-aggregate",
    "negative-row-no-prior",
    "negative-source-missing-field",
    "negative-source-trivial-filter",
    "schema-verification-log",
)
SYSTEMS = {"ubuntu-24.04": "Linux", "macos-15": "Darwin", "windows-latest": "Windows"}


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def require(condition, message):
    if not condition:
        raise ValueError(message)


def gh_json(endpoint, *, pages=False):
    command = ["gh", "api", endpoint]
    if pages:
        command += ["--paginate", "--slurp"]
    return json.loads(subprocess.check_output(command, text=True))


def snapshot(repository, run_id, destination):
    """Retain exact source and attempt metadata; exclude API digest fields."""
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository),
        "invalid repository",
    )
    destination.mkdir(parents=True)
    prefix = f"repos/{repository}"
    raw = gh_json(f"{prefix}/actions/runs/{run_id}")
    run = {
        key: raw[key]
        for key in (
            "id",
            "head_sha",
            "head_branch",
            "event",
            "run_attempt",
            "path",
            "status",
            "conclusion",
            "created_at",
            "updated_at",
            "html_url",
        )
    }
    run["repository"] = repository
    write(destination / "run.json", run)
    source = gh_json(f"{prefix}/commits/{run['head_sha']}")
    write(
        destination / "source.json",
        {"sha": source["sha"], "parents": [p["sha"] for p in source["parents"]]},
    )
    attempt = run["run_attempt"]
    pages = gh_json(
        f"{prefix}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100",
        pages=True,
    )
    jobs = [
        {
            key: job.get(key)
            for key in (
                "id",
                "name",
                "status",
                "conclusion",
                "started_at",
                "completed_at",
                "html_url",
                "runner_name",
                "runner_group_name",
                "labels",
                "steps",
            )
        }
        for page in pages
        for job in page["jobs"]
    ]
    write(destination / "jobs.json", jobs)
    pages = gh_json(
        f"{prefix}/actions/runs/{run_id}/artifacts?per_page=100", pages=True
    )
    artifacts = [
        {
            key: item.get(key)
            for key in (
                "id",
                "name",
                "size_in_bytes",
                "expired",
                "created_at",
                "expires_at",
            )
        }
        for page in pages
        for item in page["artifacts"]
        if item["name"] != "native-run-audit"
    ]
    write(destination / "artifacts.json", artifacts)
    errors = []
    for name in SOURCE_FILES:
        content = gh_json(f"{prefix}/contents/{name}?ref={run['head_sha']}")
        target = destination / "source" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(base64.b64decode(content["content"]))

    def download(item):
        try:
            require(not item["expired"], "artifact is expired")
            name = item["name"]
            require(re.fullmatch(r"[A-Za-z0-9_.-]+", name), "unsafe artifact name")
            data = subprocess.check_output(
                ["gh", "api", f"{prefix}/actions/artifacts/{item['id']}/zip"]
            )
            target = destination / "artifacts" / name
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                require(
                    len(archive.namelist()) == len(set(archive.namelist())),
                    "duplicate ZIP members",
                )
                for member in archive.infolist():
                    path = Path(member.filename)
                    require(
                        not path.is_absolute()
                        and ".." not in path.parts
                        and "\\" not in member.filename,
                        "unsafe ZIP member",
                    )
                    require(
                        (member.external_attr >> 16) & 0o170000 != 0o120000,
                        "artifact ZIP links are unsupported",
                    )
                archive.extractall(target)
        except (
            OSError,
            ValueError,
            subprocess.CalledProcessError,
            zipfile.BadZipFile,
        ) as exc:
            return {"artifact": item["name"], "error": str(exc)}
        return None

    with ThreadPoolExecutor(max_workers=6) as executor:
        errors.extend(error for error in executor.map(download, artifacts) if error)
    write(destination / "download-errors.json", errors)
    return destination


class Audit:
    def __init__(self, directory):
        self.directory = directory
        self.global_checks = []
        defaults = {
            "run.json": {
                "id": None,
                "head_sha": None,
                "head_branch": None,
                "event": None,
                "run_attempt": None,
                "path": None,
                "status": "unavailable",
                "conclusion": None,
                "html_url": None,
            },
            "source.json": {"sha": None, "parents": []},
            "jobs.json": [],
            "artifacts.json": [],
        }
        values = {}
        for name, default in defaults.items():
            path = directory / name
            if path.is_file():
                values[name] = read(path)
            else:
                values[name] = default
                self.global_checks.append(
                    {
                        "name": name,
                        "result": "missing",
                        "detail": "snapshot metadata unavailable: " + name,
                    }
                )
        self.run, self.source = values["run.json"], values["source.json"]
        self.jobs, self.artifacts = values["jobs.json"], values["artifacts.json"]
        self.rows = []
        self.packages = {}
        self.math = []
        if not self.global_checks:
            self.check(self.global_checks, "source", self.check_source)

    def check_source(self):
        require(
            self.source["sha"] == self.run["head_sha"],
            "source differs from Actions head",
        )
        require(
            bool(self.source["parents"])
            and all(
                isinstance(p, str) and re.fullmatch(r"[0-9a-f]{40}", p)
                for p in self.source["parents"]
            ),
            "exact source parents are missing",
        )
        require(
            self.run["path"] == ".github/workflows/native.yml", "run is not native.yml"
        )
        require(
            self.run["event"] in ("push", "workflow_dispatch"),
            "sampled PR cannot qualify full matrix",
        )
        require(
            len({a["name"] for a in self.artifacts}) == len(self.artifacts),
            "duplicate artifact names from different attempts",
        )
        for name in SOURCE_FILES:
            require(
                (self.directory / "source" / name).is_file(),
                f"exact source bytes missing: {name}",
            )
        return {"revision": self.source["sha"], "parents": self.source["parents"]}

    def check(self, checks, name, function):
        try:
            detail = function()
            checks.append({"name": name, "result": "pass", "detail": detail})
        except FileNotFoundError as exc:
            checks.append({"name": name, "result": "missing", "detail": str(exc)})
        except (
            ValueError,
            KeyError,
            TypeError,
            AssertionError,
            OSError,
            zipfile.BadZipFile,
            tarfile.TarError,
        ) as exc:
            checks.append({"name": name, "result": "fail", "detail": str(exc)})

    def artifact(self, name):
        entries = [a for a in self.artifacts if a["name"] == name]
        if not entries:
            raise FileNotFoundError(f"artifact not advertised: {name}")
        require(
            len(entries) == 1 and not entries[0]["expired"],
            f"duplicate or expired artifact: {name}",
        )
        directory = self.directory / "artifacts" / name
        if not directory.is_dir():
            raise FileNotFoundError(f"artifact bytes unavailable: {name}")
        return directory

    def job(self, name):
        entries = [j for j in self.jobs if j["name"] == name]
        if not entries:
            raise FileNotFoundError(f"job missing: {name}")
        require(len(entries) == 1, f"duplicate job: {name}")
        job = entries[0]
        if job["status"] != "completed":
            raise FileNotFoundError(f"job incomplete ({job['status']}): {name}")
        require(job["conclusion"] == "success", f"job {name}: {job['conclusion']}")
        return {
            key: job.get(key)
            for key in ("id", "name", "html_url", "runner_name", "labels")
        }

    def lane(self, kind, platform, version, form, job_name, checkers):
        row = {
            "kind": kind,
            "os": platform,
            "runtime_version": version,
            "package_form": form,
            "checks": [],
        }
        self.check(row["checks"], "job", lambda: self.job(job_name))
        for name, function in checkers:
            self.check(row["checks"], name, function)
        statuses = {c["result"] for c in row["checks"] + self.global_checks}
        row["result"] = (
            "fail"
            if "fail" in statuses
            else "missing"
            if "missing" in statuses
            else "pass"
        )
        self.rows.append(row)
        return row

    def single(self, directory, pattern):
        matches = list(directory.glob(pattern))
        require(
            len(matches) == 1, f"expected exactly one {pattern} in {directory.name}"
        )
        return matches[0]

    def package(self, platform, runtime):
        name = (
            f"native-python-{platform}-3.14"
            if runtime == "python"
            else f"native-r-{platform}"
        )
        directory = self.artifact(name)
        source = self.single(directory, "*.tar.gz")
        canonical = self.directory / "source/rust"
        check_cargo_archive(
            source,
            (canonical / "Cargo.lock").read_bytes(),
            (canonical / "Cargo.toml").read_bytes(),
            (canonical / "rust-toolchain.toml").read_bytes(),
        )
        detail = {
            "source_artifact": name,
            "source_archive": source.name,
            "canonical_cargo_bytes": "equal",
        }
        if runtime == "python":
            check_python_archive(source)
            wheel = self.single(directory, "*.whl")
            check_python_archive(wheel)
            reference_name = f"native-reference-{platform}-3.14"
            reference = self.single(self.artifact(reference_name), "*.whl")
            require(
                reference.read_bytes() == wheel.read_bytes(),
                "reference wheel differs from installed candidate wheel",
            )
            project_name = f"native-project-{platform}-3.14"
            project = self.single(self.artifact(project_name), "*.whl")
            check_python_archive(project)
            detail.update(
                wheel=wheel.name,
                reference_artifact=reference_name,
                reference_wheel_bytes="equal",
                project_artifact=project_name,
            )
        self.packages[(platform, runtime)] = detail
        return detail

    def toolchain(self, platform, runtime):
        suffix = f"r-{platform}" if runtime == "r" else f"dist-{platform}-3.14"
        directory = self.artifact("rust-dependencies-" + suffix)
        metadata = read(directory / "cargo-metadata.json")
        require(
            metadata["packages"] and metadata["resolve"]["nodes"],
            "locked Cargo graph is empty",
        )
        return {
            "cargo": (directory / "cargo-version.txt").read_text().strip(),
            "rustc": (directory / "rustc-version.txt").read_text().strip(),
            "packages": [
                {"name": p["name"], "version": p["version"], "source": p["source"]}
                for p in metadata["packages"]
            ],
        }

    def evidence(self, record, runtime, platform, version, form):
        evidence = record["evidence"]
        require(
            evidence["source_revision"] == self.source["sha"],
            "stale installed evidence source",
        )
        require(
            evidence["run"] == self.run["html_url"],
            "installed evidence refers to another run",
        )
        require(evidence["package_form"] == form, "wrong installed package form")
        artifact = (
            "native-python-" + platform + "-3.14"
            if runtime == "python"
            else "native-r-" + platform
        )
        prefix = f"github-actions:{self.run['id']}/attempt-{self.run['run_attempt']}/{artifact}/"
        reference = evidence["artifact_reference"]
        require(
            reference.startswith(prefix) and "/" not in reference.removeprefix(prefix),
            "wrong installed artifact/attempt reference",
        )
        require(
            (self.artifact(artifact) / reference.removeprefix(prefix)).is_file(),
            "installed package artifact bytes missing",
        )
        installation = record["installation"]
        require(
            all(
                installation.get(k)
                for k in (
                    "runtime_version",
                    "host_package_version",
                    "core_version",
                    "package_location",
                )
            ),
            "incomplete installation metadata",
        )
        require(
            runtime != "python"
            or installation["runtime_version"].startswith(version + "."),
            "wrong installed Python version",
        )
        expected_system = "macOS" if platform == "macos-15" else SYSTEMS[platform]
        require(
            record["platform"].startswith(expected_system + "-"),
            "wrong installed OS/image",
        )
        return {
            "installation": installation,
            "os_image": record["platform"],
            "artifact_reference": reference,
        }

    def verified_package(self, platform, runtime):
        if (platform, runtime) not in self.packages:
            raise FileNotFoundError(
                "package byte verification did not pass; see packages-"
                + runtime
                + " lane"
            )
        return self.packages[platform, runtime]

    def supplemental(self, platform, runtime, version, form):
        name = (
            f"native-qualification-{platform}-{version}-{form}"
            if runtime == "python"
            else f"native-r-qualification-{platform}"
        )
        root = self.artifact(name) / (form if runtime == "python" else "r-source")
        directory = root / "supplemental" if runtime == "python" else root
        record = read(directory / "supplemental.json")
        detail = self.evidence(record, runtime, platform, version, form)
        expected = {
            s["id"]
            for s in read(
                self.directory / "source/rust/qualification/supplemental.json"
            )["suites"]
            if s["runtime"] == runtime
        }
        require(
            len(record["suites"]) == len(expected)
            and {s["id"] for s in record["suites"]} == expected,
            "supplemental suite denominator differs",
        )
        failed = [
            s["id"]
            for s in record["suites"]
            if s["result"] != "pass" or s["exit_code"] != 0
        ]
        require(not failed, "supplemental suites not passed: " + ", ".join(failed))
        for suite in record["suites"]:
            relative = Path(suite["log"])
            require(
                not relative.is_absolute()
                and ".." not in relative.parts
                and "\\" not in str(relative),
                "unsafe suite log path",
            )
            log = directory / relative
            require(log.is_file(), f"missing suite log: {suite['id']}")
            if runtime == "python" and suite["id"] in (
                "python/original-specifications",
                "python/verification-logs",
                "python/producer-cohort",
            ):
                require(
                    "skipped=" not in log.read_text(),
                    "Python installed suite skipped tests",
                )
        detail["suites"] = len(expected)
        if runtime == "python":
            inventory = read(root / "coverage.json")
            require(
                inventory["source_revision"] == self.source["sha"]
                and not inventory["errors"],
                "assisted inventory source/errors differ",
            )
            detail["assisted_summary"] = inventory["summary"]
        return detail

    def r_check(self, platform):
        directory = self.artifact("native-r-check-" + platform)
        require(
            (directory / "source-revision.txt").read_text().strip()
            == self.source["sha"],
            "R package check source differs",
        )
        require(
            "Status: OK"
            in (directory / "yamaa.Rcheck/00check.log").read_text().splitlines(),
            "R package check is not OK",
        )
        return "Status: OK"

    def math_report(self, platform, version):
        report = read(
            self.artifact(f"math-assessment-{platform}-{version}")
            / "math-assessment.json"
        )
        host = report["host"]
        require(
            host["system"] == SYSTEMS[platform]
            and host["python"].startswith(version + "."),
            "math host differs",
        )
        values = observations(report)
        validate_corpus(values)
        require(
            all(sum(k[1] == name for k in values) == 10011 for name in FUNCTIONS),
            "incomplete per-function math corpus",
        )
        self.math.append(report)
        return {"host": host, "policy": report["policy"], "observations": len(values)}

    def math_comparison(self):
        if len(self.math) != 9:
            raise FileNotFoundError("complete nine-host math evidence is unavailable")
        validate_hosts(
            [
                (
                    r["host"]["system"],
                    r["host"]["machine"],
                    ".".join(r["host"]["python"].split(".")[:2]),
                )
                for r in self.math
            ]
        )
        return {
            "observations": compare(self.math),
            "scope": "sample portability; no historical parity or whole accuracy claim",
        }

    def cohort(self, platform, version, form, producer):
        root = self.artifact("native-public-cohort") / platform / version / form
        directory = root / "producer-cohort" if producer else root
        inventory = read(directory / "coverage.json")
        require(
            inventory["source_revision"] == self.source["sha"]
            and not inventory["errors"],
            "cohort source/errors differ",
        )
        names = ("shared-producer-parquet",) if producer else COHORT
        expected = set(
            itertools.product(
                names,
                (
                    ("python", "python", "reference_run"),
                    ("python", "rust", "shared_run"),
                    ("r", "rust", "shared_run"),
                ),
            )
        )
        observed = {
            (row["example"], (row["runtime"], row["backend"], row["level"]))
            for row in inventory["coverage"]
            if row["example"] in names
        }
        require(observed == expected, "cohort required tuples missing")
        relevant = [r for r in inventory["coverage"] if r["example"] in names]
        require(
            len(relevant) == len(expected)
            and all(
                r["result"] == "pass" and not r["findings"] and not r["error"]
                for r in relevant
            ),
            "cohort does not pass",
        )
        require(
            len(inventory["batches"]) == 3,
            "cohort must retain three independently qualified routes",
        )
        routes = {
            (b["runtime"], b["backend"], b["level"]) for b in inventory["batches"]
        }
        require(
            routes
            == {
                ("python", "python", "reference_run"),
                ("python", "rust", "shared_run"),
                ("r", "rust", "shared_run"),
            },
            "cohort batch routes differ",
        )
        for batch in inventory["batches"]:
            require(
                batch["source_revision"] == self.source["sha"]
                and batch["evidence"] == self.run["html_url"],
                "cohort batch provenance differs",
            )
            artifact = (
                f"native-reference-{platform}-3.14"
                if batch["backend"] == "python"
                else f"native-python-{platform}-3.14"
                if batch["runtime"] == "python"
                else f"native-r-{platform}"
            )
            prefix = f"github-actions:{self.run['id']}/attempt-{self.run['run_attempt']}/{artifact}/"
            require(
                batch["artifact_reference"].startswith(prefix)
                and "/" not in batch["artifact_reference"].removeprefix(prefix),
                "cohort batch artifact attempt/OS differs",
            )
            require(
                (
                    self.artifact(artifact)
                    / batch["artifact_reference"].removeprefix(prefix)
                ).is_file(),
                "cohort package bytes missing",
            )
        # Reports are retained under portable names, never read stale absolute paths.
        for name, (host, backend, _) in expected:
            filename = f"{name}.{host}.{backend}.json"
            paths = list(directory.rglob(filename))
            require(
                len(paths) == 1, "cohort retained report missing/duplicate: " + filename
            )
            report = read(paths[0])
            require(
                (report["example"], report["runtime"], report["backend"])
                == (name, host, backend),
                "retained report identity differs",
            )
            require(
                report["report_version"] == "0.3.0-draft"
                and report["outcome"] in ("success", "failure")
                and report["error"] is None,
                "retained report protocol/outcome differs",
            )
            batch = next(
                b
                for b in inventory["batches"]
                if (b["runtime"], b["backend"]) == (host, backend)
            )
            version_key = (
                "host_package_version" if backend == "python" else "core_version"
            )
            require(
                report["engine_version"] == batch[version_key],
                "retained report engine version differs from installed batch",
            )
            require(
                all(
                    isinstance(report[k], list)
                    for k in (
                        "nodes",
                        "tables",
                        "verifications",
                        "callbacks",
                        "source_reads",
                        "artifacts",
                        "diagnostics",
                        "unsupported",
                        "handler_counts",
                    )
                ),
                "retained complete report observations missing",
            )
        if producer:
            comparisons = read(directory / "complete-comparisons.json")
            require(
                len(comparisons) == 4 and all(c["passed"] for c in comparisons),
                "producer complete comparisons failed/missing",
            )
            for runtime in ("python", "r"):
                owner = directory / "raw" / runtime / "shared-producer-parquet"
                before = read(owner / "before.json")
                require(
                    "publication" not in before, "Build contains a publication attempt"
                )
                snapshots = []
                for attempt in (1, 2):
                    saved = owner / f"after-{attempt}"
                    after = read(saved / "report.json")
                    require(
                        after.pop("publication")
                        == {
                            "status": "success",
                            "error": None,
                            "entered": [
                                {"node": 1, "published": True},
                                {"node": 0, "published": True},
                            ],
                        },
                        "producer publication prefix differs",
                    )
                    require(after == before, "Save changed original execution evidence")
                    payloads = {
                        name: (saved / name).read_bytes()
                        for name in ("produced.parquet", "out.csv")
                    }
                    published = (
                        directory / "published" / runtime / str(attempt) / "expected"
                    )
                    require(
                        all(
                            (published / name).read_bytes() == value
                            for name, value in payloads.items()
                        ),
                        "published producer bytes differ from held save",
                    )
                    snapshots.append(payloads)
                require(
                    snapshots[0] == snapshots[1],
                    "repeat Save changed held producer bytes",
                )
        return {
            "required_tuples": len(expected),
            "summary": inventory["summary"],
            "scope": "retained comparator verdict and reports; no engine execution",
        }

    def public_inventory(self, platform, runtime, version, form):
        name = (
            f"public-conformance-python-{platform}-{version}-{form}"
            if runtime == "python"
            else f"public-conformance-r-{platform}"
        )
        inventory = read(self.artifact(name) / "inventory.json")
        require(
            inventory["protocol"] == "public-conformance/1"
            and inventory["runtime"] == runtime,
            "public inventory protocol/runtime differs",
        )
        require(
            inventory["source_revision"] == self.source["sha"]
            and inventory["evidence"] == self.run["html_url"],
            "public inventory provenance differs",
        )
        require(
            inventory["package_form"] == form, "public inventory package form differs"
        )
        expected_system = "macOS" if platform == "macos-15" else SYSTEMS[platform]
        require(
            inventory["platform"].startswith(expected_system + "-"),
            "public inventory OS/image differs",
        )
        require(
            all(
                inventory.get(k)
                for k in ("runtime_version", "host_package_version", "engine_version")
            ),
            "public inventory versions missing",
        )
        require(
            runtime != "python"
            or inventory["runtime_version"].startswith(version + "."),
            "public inventory Python version differs",
        )
        # Extract only top-level manifest entries without depending on a host evaluator.
        manifest = (
            self.directory / "source/benchmarks/execution-manifest.yaml"
        ).read_text()
        expected = set(re.findall(r"^  ([A-Za-z0-9_-]+):\s*$", manifest, re.MULTILINE))
        rows = inventory["benchmarks"]
        require(
            expected
            and len(rows) == len(expected)
            and {r["example"] for r in rows} == expected,
            "public corpus denominator differs",
        )
        counts = dict(Counter(r["verdict"] for r in rows))
        require(
            set(counts) <= {"pass", "mismatch", "unsupported", "error", "excluded"},
            "unknown public verdict",
        )
        require(
            counts == {k: v for k, v in inventory["summary"].items() if v},
            "public inventory summary differs from records",
        )
        return {
            "runtime_version": inventory["runtime_version"],
            "host_package_version": inventory["host_package_version"],
            "core_version": inventory["engine_version"],
            "os_image": inventory["platform"],
            "summary": counts,
            "benchmarks": len(rows),
            "unsupported_operations": inventory["unsupported_operations"],
            "scope": "report-only inventory completeness; counts retain semantic nonpasses",
        }

    def shared_crates(self, platform):
        directory = self.artifact(f"shared-crates-check-{platform}-3.14")
        log = directory / "shared-crates-check.log"
        require(log.is_file() and log.stat().st_size > 0, "shared crates log missing")
        return {"log": log.name}

    def collect(self):
        for platform in PLATFORMS:
            self.lane(
                "shared-crates",
                platform,
                None,
                None,
                f"native-rust ({platform}, 3.14)",
                [("retained_log", lambda p=platform: self.shared_crates(p))],
            )
            for runtime in ("python", "r"):
                job_name = (
                    f"native-dist ({platform}, 3.14)"
                    if runtime == "python"
                    else f"native-r ({platform})"
                )
                self.lane(
                    "packages-" + runtime,
                    platform,
                    None,
                    "wheel+source" if runtime == "python" else "source",
                    job_name,
                    [
                        (
                            "package_bytes",
                            lambda p=platform, r=runtime: self.package(p, r),
                        ),
                        (
                            "toolchain",
                            lambda p=platform, r=runtime: self.toolchain(p, r),
                        ),
                    ],
                )
            for version, form in itertools.product(PYTHONS, FORMS):
                job = f"native-qualify ({platform}, {version}, {form})"
                self.lane(
                    "python",
                    platform,
                    version,
                    form,
                    job,
                    [
                        (
                            "package_bytes",
                            lambda p=platform: self.verified_package(p, "python"),
                        ),
                        (
                            "installed_suites",
                            lambda p=platform, v=version, f=form: self.supplemental(
                                p, "python", v, f
                            ),
                        ),
                    ],
                )
                self.lane(
                    "public-conformance-python",
                    platform,
                    version,
                    form,
                    job,
                    [
                        (
                            "inventory",
                            lambda p=platform, v=version, f=form: self.public_inventory(
                                p, "python", v, f
                            ),
                        )
                    ],
                )
                for producer in (False, True):
                    self.lane(
                        "producer" if producer else "public-cohort",
                        platform,
                        version,
                        form,
                        "public-cohort-complete-runs",
                        [
                            (
                                "cohort",
                                lambda p=platform, v=version, f=form, pr=producer: (
                                    self.cohort(p, v, f, pr)
                                ),
                            )
                        ],
                    )
            self.lane(
                "r",
                platform,
                None,
                "source",
                f"native-r ({platform})",
                [
                    ("package_bytes", lambda p=platform: self.verified_package(p, "r")),
                    (
                        "installed_suites",
                        lambda p=platform: self.supplemental(p, "r", None, "source"),
                    ),
                    ("package_check", lambda p=platform: self.r_check(p)),
                ],
            )
            self.lane(
                "public-conformance-r",
                platform,
                None,
                "source",
                f"native-r ({platform})",
                [
                    (
                        "inventory",
                        lambda p=platform: self.public_inventory(
                            p, "r", None, "source"
                        ),
                    )
                ],
            )
            for version in PYTHONS:
                self.lane(
                    "math",
                    platform,
                    version,
                    "installed",
                    f"native-qualify ({platform}, {version}, wheel)",
                    [
                        (
                            "observations",
                            lambda p=platform, v=version: self.math_report(p, v),
                        )
                    ],
                )
        self.lane(
            "math-portability",
            "all",
            None,
            None,
            "portable-math-exact",
            [("exact_comparison", self.math_comparison)],
        )
        summary = dict(Counter(r["result"] for r in self.rows))
        return {
            "protocol": "native-run-audit/1",
            "run": self.run,
            "source": self.source,
            "scope": "evidence completeness; public report-only counts are not whole-corpus qualification",
            "checks": self.global_checks,
            "summary": summary,
            "lanes": self.rows,
            "download_errors": read(self.directory / "download-errors.json")
            if (self.directory / "download-errors.json").is_file()
            else [],
            "complete": all(r["result"] == "pass" for r in self.rows),
        }


def markdown(record):
    lines = [
        f"# Native run {record['run']['id']} evidence audit",
        "",
        f"Source: `{record['source']['sha']}`",
        "",
        "Parents: " + ", ".join(f"`{p}`" for p in record["source"]["parents"]),
        "",
        f"Run: {record['run']['html_url']}",
        "",
        record["scope"],
        "",
        f"Complete: {record['complete']}; summary: {json.dumps(record['summary'], sort_keys=True)}",
        "",
        "| Lane | OS | Runtime | Form | Evidence |",
        "| --- | --- | --- | --- | --- |",
    ]
    for row in record["lanes"]:
        lines.append(
            "| "
            + " | ".join(
                str(row.get(k) or "-")
                for k in ("kind", "os", "runtime_version", "package_form", "result")
            )
            + " |"
        )
    lines += ["", "## Findings", ""]
    for row in record["lanes"]:
        for check in row["checks"]:
            if check["result"] != "pass":
                lines.append(
                    f"- {row['kind']} / {row['os']} / {row['runtime_version']} / {row['package_form']}: {check['name']} {check['result']}: {str(check['detail']).replace(chr(10), ' ')}"
                )
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--run-id", type=int)
    source.add_argument(
        "--input",
        type=Path,
        help="downloaded snapshot containing metadata, source/ and artifacts/",
    )
    parser.add_argument("--repository", default="elong0527/yamaa")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail on any failed/missing evidence; reports are always written",
    )
    args = parser.parse_args(argv)
    require(not args.output.exists(), "output must be a fresh evidence directory")
    args.output.mkdir(parents=True)
    directory = args.input if args.input else args.output / "input"
    if args.run_id:
        try:
            snapshot(args.repository, args.run_id, directory)
        except (OSError, ValueError, subprocess.CalledProcessError) as exc:
            write(
                directory / "download-errors.json",
                [{"stage": "metadata/source snapshot", "error": str(exc)}],
            )
    record = Audit(directory).collect()
    write(args.output / "qualification.json", record)
    (args.output / "qualification.md").write_text(markdown(record), encoding="utf-8")
    print(json.dumps(record["summary"], sort_keys=True))
    return int(args.check and not record["complete"])


if __name__ == "__main__":
    raise SystemExit(main())
