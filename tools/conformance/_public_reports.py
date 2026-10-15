"""Tool-private complete-report truth and M1 comparison; executes no evaluator.

The observation models and comparison rules are preserved from the reviewed M1
protocol. Public owners supply actual reports; committed fixtures supply truth.
Only expected-artifact decoding uses independent Polars/PyArrow/YAML libraries.
This module is not installed as a yamaa API or runtime implementation.
"""

from __future__ import annotations

import datetime
import io
import itertools
import json
import struct
from collections.abc import Sequence
from pathlib import Path
from typing import Literal, TypeAlias

import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq
import yaml12
from pydantic import BaseModel, ConfigDict, Field, JsonValue

REPORT_VERSION = "0.3.0-draft"
RUNTIME: Literal["python"] = "python"
BACKEND: Literal["python"] = "python"
Outcome: TypeAlias = Literal["success", "failure", "unsupported", "error"]
EXPECTED_DIR = "expected"
ERROR_CONTRACT = "error.yaml"


class Observation(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class ScalarObservation(Observation):
    """Exact scalar transport; float bits and i64 values never pass through JSON numbers."""

    type: Literal["missing", "bool", "int", "float", "str", "date", "datetime"]
    value: str | bool | None


def observe_scalar(value: object) -> ScalarObservation:
    if value is None:
        return ScalarObservation(type="missing", value=None)
    if isinstance(value, bool):
        return ScalarObservation(type="bool", value=value)
    if isinstance(value, int):
        return ScalarObservation(type="int", value=str(value))
    if isinstance(value, float):
        return ScalarObservation(type="float", value=struct.pack(">d", value).hex())
    if isinstance(value, datetime.datetime):
        return ScalarObservation(type="datetime", value=value.isoformat())
    if isinstance(value, datetime.date):
        return ScalarObservation(type="date", value=value.isoformat())
    if isinstance(value, str):
        return ScalarObservation(type="str", value=value)
    raise TypeError(f"no portable scalar encoding for {type(value).__name__}")


class TableObservation(Observation):
    specification: str
    stage: Literal["source", "derived"]
    name: str
    columns: tuple[str, ...]
    types: tuple[str, ...]
    rows: tuple[tuple[ScalarObservation, ...], ...]


class CallbackObservation(Observation):
    """One actual study callback; activation vectors and short circuits are excluded."""

    specification: str
    function: str
    contract_version: str
    arguments: tuple[tuple[str, ScalarObservation], ...]


class VerificationObservation(Observation):
    specification: str
    spec_path: str
    check: str
    target: str | None
    requirement: str
    verification_id: str | None
    severity: str
    evaluated_count: int
    # Retain warning details which VerificationFailure.model_dump excludes.
    failure: dict[str, JsonValue] | None


class SourceReadObservation(Observation):
    """One resource capture request during study ingestion, including cached reads."""

    base_directory: str
    path: str
    outcome: Literal["captured", "failure"]
    condition: str | None
    snapshots_created: int


def _expected_parquet(payload):
    """Read committed artifact truth independently; this is no source-profile evaluator."""
    table = pq.read_table(io.BytesIO(payload))
    kinds = {
        pa.string(): "str",
        pa.large_string(): "str",
        pa.int64(): "int",
        pa.float64(): "float",
        pa.date32(): "date",
        pa.timestamp("us"): "datetime",
    }
    try:
        types = tuple(kinds[field.type] for field in table.schema)
    except KeyError as error:
        raise ValueError(
            "committed Parquet output has an unrecognized logical type"
        ) from error
    return pl.from_arrow(table), types


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class ArtifactObservation(_FrozenModel):
    """One published artifact exactly as the run rendered it."""

    name: str = Field(min_length=1)
    profile: Literal["csv", "parquet"]
    columns: tuple[str, ...]
    types: tuple[str, ...]
    row_count: int = Field(ge=0)
    # The CSV profile contract's complete bytes split on the U+000A terminator
    # REQ-0723 writes. A missing value and a quoted empty string render
    # differently and are kept apart here; `content` below decides equality
    # so that a newline inside a quoted field cannot make this split the
    # deciding view. A parquet artifact carries one JSON document per
    # record instead: REQ-0742 denies parquet a byte guarantee, so its
    # comparison runs over what the bytes read back as (REQ-0740), and JSON
    # keeps a missing value null rather than the empty string.
    records: tuple[str, ...]
    byte_length: int = Field(ge=0)
    # The rendered text of a CSV artifact, which is what a comparison
    # decides on: carrying it is what lets equality be the bytes themselves
    # rather than two identities that are only believed to stand for them.
    # A parquet artifact carries the empty string: its comparison is the
    # logical one `records` above encodes, never the bytes.
    content: str = ""


class DiagnosticObservation(_FrozenModel):
    """One structured failure in the shape the committed contracts use."""

    phase: str = Field(min_length=1)
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str | None
    context: dict[str, JsonValue]


class UnsupportedObservation(_FrozenModel):
    """One valid declaration this engine does not execute yet."""

    operation: str = Field(min_length=1)
    spec_path: str = Field(min_length=1)


class HandlerObservation(_FrozenModel):
    """How often one declared handler path fired, zero included (REQ-0361)."""

    spec_path: str = Field(min_length=1)
    handler: str = Field(min_length=1)
    count: int = Field(ge=0)


class NodeObservation(_FrozenModel):
    specification: str
    outcome: Literal["success", "failure", "unsupported"]
    diagnostics: tuple[DiagnosticObservation, ...] = ()
    unsupported: tuple[UnsupportedObservation, ...] = ()
    handler_counts: tuple[HandlerObservation, ...] = ()


class ExampleReport(_FrozenModel):
    """What one runtime observed running one example, and nothing more."""

    report_version: Literal["0.3.0-draft"] = REPORT_VERSION
    runtime: Literal["python", "r"] = RUNTIME
    backend: Literal["python", "rust"] = BACKEND
    runtime_version: str
    engine_version: str
    example: str = Field(min_length=1)
    outcome: Outcome
    artifacts: tuple[ArtifactObservation, ...] = ()
    diagnostics: tuple[DiagnosticObservation, ...] = ()
    unsupported: tuple[UnsupportedObservation, ...] = ()
    handler_counts: tuple[HandlerObservation, ...] = ()
    nodes: tuple[NodeObservation, ...]
    tables: tuple[TableObservation, ...]
    verifications: tuple[VerificationObservation, ...]
    callbacks: tuple[CallbackObservation, ...]
    source_reads: tuple[SourceReadObservation, ...]
    # Held for whoever reads a broken run. #101 keeps implementation error
    # text out of the portable comparison, so `compare_example` reads the
    # `error` outcome and never this string.
    error: str | None = None


class ComparisonFinding(_FrozenModel):
    """One difference between what an example committed and what ran."""

    kind: str = Field(min_length=1)
    detail: str = Field(min_length=1)
    expected: JsonValue = None
    actual: JsonValue = None


class ComparisonVerdict(_FrozenModel):
    """Whether one report satisfies the artifacts its example committed."""

    example: str = Field(min_length=1)
    runtime: Literal["python", "r"] = RUNTIME
    backend: Literal["python", "rust"] = BACKEND
    kind: Literal["positive", "negative", "parity"]
    passed: bool
    findings: tuple[ComparisonFinding, ...] = ()


class ConformanceError(ValueError):
    """Raised when an example or its output location cannot be used."""


def _records(payload: bytes) -> tuple[str, ...]:
    """Split R020's bytes on the U+000A terminator it writes after each."""
    text = payload.decode("utf-8")
    text = text.removesuffix("\n")
    return tuple(text.split("\n")) if text else ()


def _parquet_records(frame: pl.DataFrame) -> tuple[str, ...]:
    """Render one parquet artifact's logical content as canonical records.

    REQ-0742 denies parquet a byte guarantee, so the comparison REQ-0740
    requires runs over what the bytes read back as: the same field names in
    the same order, the same logical types, the same rows in the same order,
    the same nulls, and the same values. One JSON document per record keeps
    every value distinct from its text rendering: a missing value is null
    rather than the empty string, and a float renders through repr, the
    shortest text its bits round-trip through, so bit-identical DOUBLEs
    compare equal and only then. The logical types ride alongside rather
    than inside the records, compared field for field.
    """

    def canonical(value: object) -> JsonValue:
        if value is None:
            return None
        if isinstance(value, float):
            # json.dumps renders floats with repr: shortest round-trip text.
            return value
        if isinstance(value, (datetime.date, datetime.datetime, datetime.time)):
            return value.isoformat()
        if isinstance(value, (bytes, bytearray)):
            return bytes(value).hex()
        if isinstance(value, (str, int, bool)):
            return value
        return str(value)

    header = json.dumps(list(frame.columns), ensure_ascii=True)
    rows = (
        json.dumps([canonical(value) for value in row], ensure_ascii=True)
        for row in frame.iter_rows(named=False)
    )
    return (header, *rows)


def _finding(
    kind: str, detail: str, expected: object, actual: object
) -> ComparisonFinding:
    return ComparisonFinding(
        kind=kind,
        detail=detail,
        expected=expected,  # type: ignore[arg-type]
        actual=actual,  # type: ignore[arg-type]
    )


def expected_kind(example: str | Path) -> Literal["positive", "negative"]:
    """Say which committed artifact an example is judged against."""
    contract = Path(example) / EXPECTED_DIR / ERROR_CONTRACT
    return "negative" if contract.is_file() else "positive"


def _record_findings(
    observation: ArtifactObservation,
    committed: tuple[str, ...],
) -> list[ComparisonFinding]:
    """Say where one artifact's records first differ from the committed ones."""
    produced = observation.records
    findings: list[ComparisonFinding] = []
    if committed and produced and committed[0] != produced[0]:
        findings.append(
            _finding(
                "artifact.columns",
                f"{observation.name}: header record differs",
                committed[0],
                produced[0],
            )
        )
    if len(committed) != len(produced):
        findings.append(
            _finding(
                "artifact.row_count",
                f"{observation.name}: record count differs",
                max(len(committed) - 1, 0),
                max(len(produced) - 1, 0),
            )
        )
    for index, (want, got) in enumerate(zip(committed[1:], produced[1:]), start=1):
        if want != got:
            findings.append(
                _finding(
                    "artifact.record",
                    f"{observation.name}: record {index} differs",
                    want,
                    got,
                )
            )
    return findings


def _artifact_findings(
    observation: ArtifactObservation,
    payload: bytes,
) -> tuple[ComparisonFinding, ...]:
    """Compare one artifact, then say where it first differs.

    A CSV artifact compares byte for byte under the csv profile's byte
    guarantee; when the records agree but the bytes do not, the bytes decide.
    A parquet artifact has no byte guarantee (REQ-0742), so its comparison is
    the logical one REQ-0740 requires: the committed bytes are read back
    through the closed parquet profile and compared on field names in order,
    logical types, row order, nulls, and values, never as bytes.
    """
    if observation.profile == "parquet":
        frame, committed_types = _expected_parquet(payload)
        findings: list[ComparisonFinding] = []
        if committed_types != observation.types:
            findings.append(
                _finding(
                    "artifact.types",
                    f"{observation.name}: logical types differ",
                    list(committed_types),
                    list(observation.types),
                )
            )
        findings.extend(_record_findings(observation, _parquet_records(frame)))
        return tuple(findings)
    if observation.content.encode("utf-8") == payload:
        return ()

    findings = _record_findings(observation, _records(payload))
    if not findings:
        # The records agree yet the bytes do not: a terminator, an encoding,
        # or a newline inside a quoted field. The bytes decide.
        findings.append(
            _finding(
                "artifact.bytes",
                f"{observation.name}: rendered bytes differ",
                len(payload),
                observation.byte_length,
            )
        )
    return tuple(findings)


def _positive_findings(
    report: ExampleReport,
    example: Path,
) -> tuple[ComparisonFinding, ...]:
    if report.outcome != "success":
        return (
            _finding(
                "outcome",
                f"{report.example}: a positive example must execute",
                "success",
                report.outcome,
            ),
        )

    committed = {
        path.stem: path
        for path in sorted(
            itertools.chain(
                (example / EXPECTED_DIR).glob("*.csv"),
                (example / EXPECTED_DIR).glob("*.parquet"),
            )
        )
    }
    produced = {item.name: item for item in report.artifacts}
    findings: list[ComparisonFinding] = []
    for name in sorted(set(committed) | set(produced)):
        if name not in produced:
            findings.append(
                _finding("artifact.missing", f"{name}: not produced", name, None)
            )
        elif name not in committed:
            findings.append(
                _finding("artifact.unexpected", f"{name}: not committed", None, name)
            )
        else:
            findings.extend(
                _artifact_findings(produced[name], committed[name].read_bytes())
            )
    return tuple(findings)


def _negative_findings(
    report: ExampleReport,
    example: Path,
) -> tuple[ComparisonFinding, ...]:
    contract = yaml12.parse_yaml(
        (example / EXPECTED_DIR / ERROR_CONTRACT).read_text(encoding="utf-8")
    )
    if not isinstance(contract, dict):
        raise ConformanceError(f"{example.name}: {ERROR_CONTRACT} is not a mapping")

    # Unsupported execution and an engine crash are not the semantic failure
    # a negative example commits, so neither one passes here.
    if report.outcome != "failure":
        return (
            _finding(
                "outcome",
                f"{report.example}: a negative example must fail semantically",
                "failure",
                report.outcome,
            ),
        )

    observed = report.diagnostics[0]
    findings: list[ComparisonFinding] = []
    for field in ("phase", "condition"):
        want = contract[field]
        got = getattr(observed, field)
        if want != got:
            findings.append(
                _finding(f"diagnostic.{field}", f"{report.example}: {field}", want, got)
            )

    paths = tuple(contract["spec_paths"])
    if paths != observed.spec_paths:
        findings.append(
            _finding(
                "diagnostic.spec_paths",
                f"{report.example}: spec_paths",
                list(paths),
                list(observed.spec_paths),
            )
        )

    if "requirement" in contract and contract["requirement"] != observed.requirement:
        findings.append(
            _finding(
                "diagnostic.requirement",
                f"{report.example}: requirement",
                contract["requirement"],
                observed.requirement,
            )
        )

    # The contract states the context a portable failure must carry. A
    # runtime may observe more; it may not observe a different value.
    for key in sorted(contract.get("context") or {}):
        want = contract["context"][key]
        if key not in observed.context:
            findings.append(
                _finding(
                    "diagnostic.context",
                    f"{report.example}: context.{key} absent",
                    want,
                    None,
                )
            )
        elif observed.context[key] != want:
            findings.append(
                _finding(
                    "diagnostic.context",
                    f"{report.example}: context.{key}",
                    want,
                    observed.context[key],
                )
            )
    return tuple(findings)


def _handler_findings(
    report: ExampleReport,
    expected: Sequence[HandlerObservation],
) -> tuple[ComparisonFinding, ...]:
    produced = {
        (item.spec_path, item.handler): item.count for item in report.handler_counts
    }
    committed = {(item.spec_path, item.handler): item.count for item in expected}
    findings: list[ComparisonFinding] = []
    for path, handler in sorted(set(committed) | set(produced)):
        key = (path, handler)
        detail = f"{report.example}: {path} {handler}"
        if key not in produced:
            findings.append(
                _finding(
                    "handler.missing", f"{detail} not reported", committed[key], None
                )
            )
        elif key not in committed:
            findings.append(
                _finding(
                    "handler.unexpected", f"{detail} not committed", None, produced[key]
                )
            )
        elif committed[key] != produced[key]:
            findings.append(
                _finding("handler.count", detail, committed[key], produced[key])
            )
    return tuple(findings)


def compare_example(
    report: ExampleReport,
    example: str | Path,
    *,
    expected_handler_counts: Sequence[HandlerObservation] | None = None,
) -> ComparisonVerdict:
    """Judge one finished report against the artifacts its example committed.

    This is the only half that reads `expected/`, and it reads a report
    rather than an engine. Nothing here normalizes: column order, record
    order, a missing value, and a quoted empty string are compared as
    rendered, and the artifact verdict is the complete bytes.

    REQ-0361 requires a run to report every declared handler path. The counts
    themselves become a comparison only when a caller states them, because
    no example commits them yet; #101's cross-runtime comparator is where R
    and Python counts meet.
    """
    example_path = Path(example).resolve()
    kind = expected_kind(example_path)
    findings = (
        _negative_findings(report, example_path)
        if kind == "negative"
        else _positive_findings(report, example_path)
    )
    if expected_handler_counts is not None:
        findings = findings + _handler_findings(report, expected_handler_counts)
    return ComparisonVerdict(
        example=report.example,
        runtime=report.runtime,
        backend=report.backend,
        kind=kind,
        passed=not findings,
        findings=findings,
    )


def _portable_report(report: ExampleReport) -> dict[str, JsonValue]:
    payload = report.model_dump(
        mode="json",
        exclude={
            "report_version",
            "runtime",
            "runtime_version",
            "backend",
            "engine_version",
            "error",
        },
    )
    for artifact in payload["artifacts"]:
        if artifact["profile"] == "parquet":
            artifact.pop("byte_length")
    # Host-language exception spelling is not part of a portable condition.
    diagnostics = list(payload["diagnostics"])
    diagnostics.extend(
        item for node in payload["nodes"] for item in node["diagnostics"]
    )
    for diagnostic in diagnostics:
        if diagnostic["condition"] == "function_call_failed":
            for key in ("call", "host_error", "host_message"):
                diagnostic["context"].pop(key, None)
    return payload


def compare_reports(
    reference: ExampleReport, candidate: ExampleReport
) -> ComparisonVerdict:
    """Compare independent runs without changing goldens or invoking either engine.

    Unsupported and infrastructure-error outcomes never establish parity. Float
    observations are exact IEEE-754 encodings; no tolerance hides differences.
    Version validation is repeated because model_copy can bypass model validation.
    """
    reference = ExampleReport.model_validate_json(reference.model_dump_json())
    candidate = ExampleReport.model_validate_json(candidate.model_dump_json())
    findings = []
    for label, report in (("reference", reference), ("candidate", candidate)):
        if report.outcome in ("unsupported", "error"):
            findings.append(
                _finding("outcome", label, "success or failure", report.outcome)
            )

    def compare(path, expected, actual):
        if type(expected) is not type(actual):
            findings.append(_finding(path, "different types", expected, actual))
        elif isinstance(expected, dict):
            for key in sorted(expected.keys() | actual.keys()):
                if key not in expected or key not in actual:
                    findings.append(
                        _finding(
                            path + "." + key,
                            "missing field",
                            expected.get(key),
                            actual.get(key),
                        )
                    )
                else:
                    compare(path + "." + key, expected[key], actual[key])
        elif isinstance(expected, list):
            if len(expected) != len(actual):
                findings.append(
                    _finding(
                        path + ".count", "different lengths", len(expected), len(actual)
                    )
                )
            for index, (left, right) in enumerate(zip(expected, actual)):
                compare(f"{path}[{index}]", left, right)
        elif expected != actual:
            findings.append(_finding(path, "different values", expected, actual))

    compare("report", _portable_report(reference), _portable_report(candidate))
    return ComparisonVerdict(
        example=candidate.example,
        runtime=candidate.runtime,
        backend=candidate.backend,
        kind="parity",
        passed=not findings,
        findings=tuple(findings),
    )


def parse_report(text: str) -> ExampleReport:
    """Require every complete observation field before strict JSON model admission."""
    raw = json.loads(text)

    def complete(value, model):
        if not isinstance(value, dict) or set(value) != set(model.model_fields):
            raise ValueError("complete report fields differ: " + model.__name__)

    complete(raw, ExampleReport)
    for group, model in (
        ("artifacts", ArtifactObservation),
        ("diagnostics", DiagnosticObservation),
        ("unsupported", UnsupportedObservation),
        ("handler_counts", HandlerObservation),
        ("nodes", NodeObservation),
        ("tables", TableObservation),
        ("verifications", VerificationObservation),
        ("callbacks", CallbackObservation),
        ("source_reads", SourceReadObservation),
    ):
        if not isinstance(raw[group], list):
            raise TypeError("complete report ledger is not an array: " + group)
        for item in raw[group]:
            complete(item, model)
    for node in raw["nodes"]:
        for group, model in (
            ("diagnostics", DiagnosticObservation),
            ("unsupported", UnsupportedObservation),
            ("handler_counts", HandlerObservation),
        ):
            for item in node[group]:
                complete(item, model)
    for table in raw["tables"]:
        for row in table["rows"]:
            for scalar in row:
                complete(scalar, ScalarObservation)
    for callback in raw["callbacks"]:
        for _, scalar in callback["arguments"]:
            complete(scalar, ScalarObservation)
    return ExampleReport.model_validate_json(text)


def read_report(path: str | Path) -> ExampleReport:
    """Read this exact complete protocol; never silently upgrade missing observations."""
    return parse_report(Path(path).read_text(encoding="utf-8"))


def write_report(report: ExampleReport, report_dir: Path) -> Path:
    """Write one report where a runner collects it, deterministically."""
    report_dir.mkdir(parents=True, exist_ok=True)
    path = report_dir / f"{report.example}.{report.runtime}.{report.backend}.json"
    payload = json.dumps(report.model_dump(mode="json"), indent=2)
    path.write_text(f"{payload}\n", encoding="utf-8")
    return path
