"""Authored source controls over actual held native preparation and captures.

Only fixture scaffolding and codecs live here. The installed owner decides path
admission, graph/schema resolution, source binding, and snapshot reuse/refusal.
No execution, activation, publication, or Python language evaluator is requested.
"""
import _native_json

import csv
import io
import json
import subprocess
from contextlib import contextmanager
from pathlib import Path
from tempfile import NamedTemporaryFile

import _native_csv
import _native_odm
import _native_parquet
import _native_schema
import _native_specification
import _semantic_controls
import _stored_artifacts
import _typed_fixtures


class Finding(str):
    def __new__(cls, row, label):
        phase, condition, requirement, paths, encoded = row
        context = _native_json.loads(encoded)
        path = label + ("." + paths[0] if paths[0] else "")
        messages = {
            "resource_path_missing": "does not exist",
            "resource_path_symlink": "contains a symbolic link",
            "resource_path_not_regular_file": "is not a regular file",
            "resource_path_not_normalized": "is not a normalized path",
            "resource_path_uri_scheme": "uses a URI scheme",
        }
        detail = (
            repr(context["path"]) + " " + messages[condition]
            if condition in messages and "path" in context
            else _native_json.dumps(context, sort_keys=True)
        )
        value = super().__new__(cls, f"ERROR: {path}: {condition}: {detail}")
        value.path, value.condition, value.context = path, condition, context
        value.phase, value.requirement = phase, requirement
        return value


def findings(rows, label):
    return [
        Finding((phase, condition, requirement, [path], encoded), label)
        for phase, condition, requirement, paths, encoded in rows
        for path in paths or [""]
    ]


@contextmanager
def held(specification, path, *, scaffold=True):
    """Keep the authored declaring directory; only missing root scaffolding is added."""
    path = Path(path).absolute()
    if scaffold:
        specification, _ = _semantic_controls.complete(specification)
    # A distinct fixture file protects real repository source documents. All
    # written source/producer operands retain this same declaring directory.
    with NamedTemporaryFile(
        mode="w",
        suffix=".yaml",
        prefix="source-control-",
        dir=path.parent,
        encoding="utf-8",
        delete=True,
    ) as fixture:
        json.dump(specification, fixture)
        fixture.flush()
        yield _native_specification.prepare(Path(fixture.name))


def source_fields(owner, snapshots=None):
    sources = []
    errors = []
    before_capture_reads = owner.capture_reads() if snapshots is not None else 0
    for index, (identity, dataset, _declaring, _written, profile) in enumerate(
        owner.input_metadata()
    ):
        raw, issues = owner.capture_input_metadata(
            index, _native_specification.MAX_CAPTURE
        )
        if snapshots is not None:
            snapshots.reads = owner.capture_reads() - before_capture_reads
        if issues:
            errors.extend(issues)
            continue
        if profile == "csv":
            issues = _native_csv.profile_issues(raw)
            if issues:
                errors.extend(
                    (
                        row["phase"],
                        row["condition"],
                        row["requirement"],
                        [f"input.{dataset}.path"],
                        row["context"],
                    )
                    for row in issues
                )
                continue
            fields = [
                (name, "str")
                for name in next(
                    csv.reader(io.StringIO(raw.decode("utf-8"), newline="")), []
                )
            ]
        else:
            try:
                _, fields = _native_parquet.parquet_source_fields(raw)
            except _native_parquet.ParquetProfileFailure as error:
                errors.append(
                    (
                        "validation",
                        error.condition,
                        error.requirement,
                        [f"input.{dataset}.path"],
                        _native_json.dumps(error.context),
                    )
                )
                continue
        sources.append((identity, dataset, list(fields)))
    return sources, errors


def document(specification, label, path, _environment=None, *_args, snapshots=None, **_kwargs):
    with held(specification, path) as owner:
        rows = owner.check_issues()
        if not rows and owner.preparation_status() == "ready":
            sources, rows = source_fields(owner, snapshots)
            if not rows:
                rows = owner.check_source_metadata(sources)
        return findings(rows, label)


def physical(specification, path):
    with held(specification, path) as owner:
        if owner.preparation_status() != "ready":
            return {}
        sources, _errors = source_fields(owner)
        return {name: dict(fields) for _identity, name, fields in sources}


def catalog(specification, path, _environment=None):
    sources = []
    normalized, _ = _semantic_controls.complete(specification)
    normalized = _native_schema.decode_document(
        _semantic_controls._schema().normalize(
            _native_json.dumps(normalized).encode("utf-8"), ["root_class"], True
        )
    )
    published = {}
    for name, source in normalized.get("input", {}).items():
        if isinstance(source, dict) and isinstance(source.get("schema"), str):
            captured, condition = resource(source["schema"], Path(path).parent)
            if condition is not None:
                return {dataset: {} for dataset in normalized["input"]}
            owner = _native_specification.prepare(captured.resolve())
            if owner.preparation_status() != "ready":
                return {dataset: {} for dataset in normalized["input"]}
            published[name] = _native_schema.decode_document(owner.resolved_document())
    if published:
        observation = _typed_fixtures.observe(
            "source-catalog",
            _native_json.dumps(
                {
                    "specification": _native_json.dumps(normalized),
                    "sources": [],
                    "published": {
                        name: _native_json.dumps(document)
                        for name, document in published.items()
                    },
                }
            ),
        )
    else:
        with held(specification, path) as owner:
            if owner.preparation_status() != "ready":
                return {name: {} for name in normalized["input"]}
            sources, errors = source_fields(owner)
            if errors:
                return {name: {} for name in normalized["input"]}
            observation = _typed_fixtures.observe(
                "source-catalog",
                _native_json.dumps(
                    {
                        "specification": _native_json.dumps(normalized),
                        "sources": [
                            (name, fields) for _identity, name, fields in sources
                        ],
                    }
                ),
            )
    # A failed ordinal declaration must not erase the physical header that
    # caused its collision or claim a generated field was available.
    return observation.get("catalog") or {
        name: dict(fields) for _identity, name, fields in sources
    }


class ResourceOwner:
    """Drive the existing Rust component owner; no resolver/cache lives in Python."""

    def __init__(self, base):
        self.process = subprocess.Popen(
            [str(_typed_fixtures._binaries()["resource-capture"])],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
        )
        self.request({"operation": "open", "base": str(Path(base).absolute())})

    def request(self, request):
        self.process.stdin.write(_native_json.dumps(request) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(
                "native resource component refused transport: "
                + self.process.stderr.read()
            )
        return _native_json.loads(line)

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            self.process.wait(timeout=5)
        self.process.stdout.close()
        self.process.stderr.close()

    def __del__(self):
        if hasattr(self, "process"):
            self.close()


class Captured:
    """A decoded actual capture; canonical identity and Arc equality are Rust facts."""

    def __init__(self, owner, written, identity, observation):
        self.owner, self.written = owner, written
        self.path = Path(identity)
        self.content = bytes(observation["bytes"])
        self.same_snapshot = observation["same_snapshot"]
        self.capture_reads = observation["capture_reads"]

    def read_bytes(self):
        return self.content

    def csv_header(self):
        return next(
            csv.reader(io.StringIO(self.content.decode("utf-8"), newline="")), []
        )

    @property
    def name(self):
        return self.path.name

    def resolve(self):
        return self.path


def resource(written, base):
    owner = ResourceOwner(base)
    resolved = owner.request({"operation": "resolve", "written": written})
    if "issue" in resolved:
        owner.close()
        return None, resolved["issue"]["condition"]
    observation = owner.request({"operation": "capture", "written": written})
    if "issue" in observation:
        owner.close()
        return None, observation["issue"]["condition"]
    return Captured(owner, written, resolved["identity"], observation), None


def resource_findings(written, base, label):
    owner = ResourceOwner(base)
    observation = owner.request({"operation": "resolve", "written": written})
    owner.close()
    if "issue" not in observation:
        return []
    row = observation["issue"]
    return findings(
        [
            (
                row["phase"],
                row["condition"],
                row["requirement"],
                row["spec_paths"],
                row["context"],
            )
        ],
        label,
    )


class Captures:
    """Repeated captures remain with the one original native resource owner."""

    def __init__(self):
        self.first = None
        self.reads = 0

    def read(self, path):
        if self.first is None:
            self.first, condition = resource(Path(path).name, Path(path).parent)
            self.reads = 0 if self.first is None else self.first.capture_reads
            return self.first, condition
        observation = self.first.owner.request(
            {"operation": "capture", "written": self.first.written}
        )
        self.reads = observation["capture_reads"]
        if "issue" in observation:
            return None, observation["issue"]["condition"]
        return Captured(
            self.first.owner, self.first.written, self.first.path, observation
        ), None


def odm(specification, label, path, _environment=None):
    observation = _native_odm.observe(specification, path)
    return findings(
        [
            (
                row["phase"],
                row["condition"],
                row["requirement"],
                row["spec_paths"],
                _native_json.dumps(row["context"]),
            )
            for row in _native_odm.issues(observation)
        ],
        label,
    )


def retired_odm(specification, label, path, _environment=None):
    """Keep the original operands in the typed ODM retirement control."""
    observation = _native_odm.observe(specification, path)
    return findings([
        (row['phase'], row['condition'], row['requirement'], row['spec_paths'],
         _native_json.dumps(row['context']))
        for row in observation.get('binding', {}).get('issues', [])
    ], label)


def producing(specification, label, path, _environment=None, *_args):
    """Isolate producer declaration and retained artifact shape controls."""
    result = []
    for dataset, source in specification.get("input", {}).items():
        if not isinstance(source, dict) or "schema" not in source:
            continue
        schema, condition = resource(source["schema"], Path(path).parent)
        if condition is not None:
            for finding in resource_findings(
                source["schema"], Path(path).parent, label
            ):
                finding.path = f"{label}.input.{dataset}.schema"
                # Keep the actual Rust condition/context; only the authored
                # field location belongs to this focused schema capture.
                result.append(
                    Finding(
                        (
                            finding.phase,
                            finding.condition,
                            finding.requirement,
                            [f"input.{dataset}.schema"],
                            _native_json.dumps(finding.context),
                        ),
                        label,
                    )
                )
            continue
        producer = _native_specification.prepare(schema.resolve())
        result.extend(
            findings(producer.check_issues(), f"{label}.input.{dataset}.schema")
        )
        if producer.preparation_status() == "ready":
            sources, errors = source_fields(producer)
            result.extend(findings(errors, f"{label}.input.{dataset}.schema"))
            if not errors:
                result.extend(
                    findings(
                        producer.check_source_metadata(sources),
                        f"{label}.input.{dataset}.schema",
                    )
                )
        with held(specification, path) as graph:
            result.extend(
                finding
                for finding in findings(graph.check_issues(), label)
                if finding.condition
                in {
                    "redundant_field_type",
                    "invalid_producer_contract",
                    "producer_workflow_cycle",
                    "ordinal_field_collision",
                }
            )
        if producer.preparation_status() != "ready":
            continue
        document = _native_schema.decode_document(producer.resolved_document())
        output_columns = document.get("output", {}).get("columns", [])
        if isinstance(source.get("ordinal"), str):
            root, _ = _semantic_controls.complete(specification)
            root = _native_schema.decode_document(
                _semantic_controls._schema().normalize(
                    _native_json.dumps(root).encode("utf-8"), ["root_class"], True
                )
            )
            observation = _typed_fixtures.observe(
                "source-catalog",
                _native_json.dumps(
                    {
                        "specification": _native_json.dumps(root),
                        "sources": [],
                        "published": {dataset: _native_json.dumps(document)},
                    }
                ),
            )
            result.extend(
                findings(
                    [
                        (
                            row["phase"],
                            row["condition"],
                            row["requirement"],
                            row["spec_paths"],
                            _native_json.dumps(row["context"]),
                        )
                        for row in observation["issues"]
                    ],
                    label,
                )
            )
        stored, condition = resource(source["path"], Path(path).parent)
        if condition is None and source["path"].lower().endswith(".csv"):
            result.extend(
                _stored_artifacts.check_csv(
                    stored.read_bytes(),
                    output_columns,
                    f"{label}.input.{dataset}.schema",
                    source["path"],
                )
            )
    return list(dict.fromkeys(result))
