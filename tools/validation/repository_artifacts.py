"""Repository stored artifact shapes and native profile codec checks."""

import csv
import math
import re
from pathlib import Path, PurePosixPath

import yaml
from tools.validation import _native_csv
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.editorial import example_spec_paths
from tools.validation.repository_schema import build_schema_env, prepare_spec_document
from tools.validation.validation_diagnostics import CSV_PROFILE


def example_entry_specs(example_dir: Path):
    paths = example_spec_paths(example_dir)
    named = set()
    for path in paths:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                spec = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if not isinstance(spec, dict):
            continue
        parents = spec.get("parents", [])
        if isinstance(parents, str):
            parents = [parents]
        for parent in parents:
            if isinstance(parent, str) and parent:
                named.add(Path(parent).name)
        inputs = spec.get("input", {})
        if isinstance(inputs, dict):
            for source in inputs.values():
                if (
                    isinstance(source, dict)
                    and isinstance(source.get("schema"), str)
                    and source["schema"]
                ):
                    named.add(Path(source["schema"]).name)
    return [path for path in paths if path.name not in named]


def example_artifact_owners(example_dir: Path):
    """Map each artifact an example's specs declare to the files declaring it.

    Only an entry and a producer kept beside it own a golden: a spec another
    reads through `input.*.schema` is part of the example, so the artifact it
    produces sits in `expected/` next to the entry's. An inheritance level
    owns nothing of its own; the entry that resolves it does.
    """
    documents = {}
    for path in example_spec_paths(example_dir):
        try:
            with open(path, "r", encoding="utf-8") as handle:
                documents[path.name] = yaml.load(handle, Loader=UniqueKeyLoader)
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
    owners_of = {path.name for path in example_entry_specs(example_dir)}
    for spec in documents.values():
        inputs = spec.get("input") if isinstance(spec, dict) else None
        if not isinstance(inputs, dict):
            continue
        for source in inputs.values():
            if isinstance(source, dict) and isinstance(source.get("schema"), str):
                owners_of.add(Path(source["schema"]).name)
    artifacts = {}
    for name in sorted(owners_of):
        spec = documents.get(name)
        output = spec.get("output") if isinstance(spec, dict) else None
        if not isinstance(output, dict):
            continue
        for field in ("path", "warning_log", "verification_log"):
            if isinstance(output.get(field), str):
                artifact = PurePosixPath(output[field]).name
                artifacts.setdefault(artifact, set()).add(name)
    return artifacts


def expected_resolved_path(example_dir, spec_path):
    if not (example_dir / "spec.yaml").is_file():
        return example_dir / "expected" / "spec_resolved.yaml"
    suffix = spec_path.stem.removeprefix("spec")
    return example_dir / "expected" / f"resolved{suffix}.yaml"


def validate_expected_resolved_fixture(spec, spec_label, spec_path, example_dir, env):
    """Compare a current resolved fixture, excluding retained reference history."""
    if example_dir.name in HISTORICAL_RESOLVED_FIXTURES:
        return []
    if not isinstance(spec, dict) or "parents" not in spec:
        return []
    resolved, resolution_errors, _ = prepare_spec_document(
        spec, spec_label, spec_path, env
    )
    if resolution_errors or not isinstance(resolved, dict):
        return []
    expected_path = expected_resolved_path(example_dir, spec_path)
    if not expected_path.is_file():
        return [
            f"ERROR: {spec_label}: inherited positive example requires "
            f"{expected_path.name}"
        ]
    try:
        with open(expected_path, "r", encoding="utf-8") as handle:
            expected = yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        return [f"ERROR: {expected_path}: cannot read resolved fixture: {exc}"]
    if resolved != expected:
        return [
            f"ERROR: {spec_label}: resolved specification differs from "
            f"expected/{expected_path.name}"
        ]
    return []


HISTORICAL_RESOLVED_FIXTURES = frozenset(
    {
        "schema-column-composition",
        "schema-inheritance",
        "schema-inheritance-project-root",
        "schema-inherited-output",
    }
)


BOM_UTF8 = "\ufeff"


CANONICAL_INT = re.compile(r"0|-?[1-9][0-9]*")


render_records = None


scan_records = None


def _ensure_csv_binding():
    """Retained repository CSV layout policy with native source admission."""
    global CsvProfileFailure, render_records, scan_records
    CsvProfileFailure = _native_csv.CsvProfileFailure
    render_records = _native_csv.render_records
    scan_records = _native_csv.records


def canonical_float_text(value: str, decimals=None):
    """Return why value is not R020's text for its float, or None.

    Static validation reads a golden file rather than running a derivation,
    so it checks the form of the text and not the value behind it. The form
    itself comes from the runtime: R011's shortest text with no declared
    precision, R020's exact display rounding with one.
    """
    _ensure_csv_binding()
    try:
        number = float(value)
    except ValueError:
        return "not a number"
    if math.isnan(number) or math.isinf(number):
        return "a non-finite float is the missing value"
    if decimals is None:
        canonical = _native_csv.scalar_text(value, "float")
        if value != canonical:
            return f"expected the shortest round-trip text {canonical}"
        return None
    canonical = _native_csv.scalar_text(value, "float", decimals)
    if value != canonical:
        return (
            f"expected exactly {decimals} digit(s) after the decimal point "
            "in positional notation"
        )
    return None


CANONICAL_DATE = re.compile(r"(\d{4})-(\d{2})-(\d{2})")


CANONICAL_DATETIME = re.compile(r"(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})")


def canonical_temporal_text(value: str, declared: str):
    """Return why value is not R016's canonical text for its type, or None.

    R016 fixes exactly one written form per temporal type. The verdict comes
    from the runtime's strict parsers; the shape patterns below only route
    the message, telling a misspelled field from a date no calendar admits.
    """
    pattern = CANONICAL_DATE if declared == "date" else CANONICAL_DATETIME
    expected = (
        "expected YYYY-MM-DD" if declared == "date" else "expected YYYY-MM-DDThh:mm:ss"
    )
    if pattern.fullmatch(value) is None:
        return expected
    try:
        if _native_csv.scalar_text(value, declared) != value:
            return expected
    except ValueError as exc:
        return f"not a {declared} on the calendar: {exc}"

    return None


def validate_csv_artifact(csv_path: Path, label: str, spec):
    """Check one expected artifact against R020's csv profile."""
    _ensure_csv_binding()
    errors = []
    try:
        raw = csv_path.read_bytes()
    except OSError as exc:
        return [f"ERROR: {label}: cannot read artifact: {exc}"]
    try:
        data = raw.decode("utf-8")
    except UnicodeError as exc:
        return [f"ERROR: {label}: invalid_text: {exc}"]
    if data.startswith(BOM_UTF8):
        return [f"ERROR: {label}: csv carries a byte-order mark"]
    try:
        records = scan_records(data)
    except ValueError as exc:
        return [f"ERROR: {label}: csv: {exc}"]
    if not records:
        return [f"ERROR: {label}: csv carries no header record"]
    if render_records(records[0], records[1:]).decode("utf-8") != data:
        errors.append(
            f"ERROR: {label}: csv quoting is not the exact condition R020 "
            "states, or a record is not terminated by U+000A"
        )

    output = spec.get("output") if isinstance(spec, dict) else None
    output = output if isinstance(output, dict) else {}
    decimals = output.get("decimals")
    if isinstance(decimals, bool) or not isinstance(decimals, int):
        decimals = None
    types = {}
    for column in (spec.get("columns") if isinstance(spec, dict) else None) or []:
        if isinstance(column, dict) and isinstance(column.get("name"), str):
            types[column["name"]] = column.get("type")

    header = records[0]
    for number, record in enumerate(records[1:], 2):
        for name, text in zip(header, record):
            if text is None:
                continue
            declared = types.get(name)
            if declared == "int":
                if not CANONICAL_INT.fullmatch(text):
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R020's int text: {text!r}"
                    )
            elif declared == "float":
                problem = canonical_float_text(text, decimals)
                if problem is not None:
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R020's float text: {text!r}, {problem}"
                    )
            elif declared in ("date", "datetime"):
                problem = canonical_temporal_text(text, declared)
                if problem is not None:
                    errors.append(
                        f"ERROR: {label}: record {number}: {name} is not "
                        f"R016's {declared} text: {text!r}, {problem}"
                    )
            if len(errors) >= 6:
                errors.append(f"ERROR: {label}: further csv errors elided")
                return errors
    return errors


ARTIFACT_PROFILES = {".csv": "csv", ".parquet": "parquet"}


WARNING_LOG_TYPES = {
    "LOG_VERSION": "str",
    "ARTIFACT": "str",
    "SEVERITY": "str",
    "CONDITION": "str",
    "REQUIREMENT": "str",
    "SPEC_PATH": "str",
    "VERIFICATION_ID": "str",
    "FAILURE_COUNT": "int",
    "OFFENDING_KEYS": "str",
    "DETAILS": "str",
}


VERIFICATION_LOG_TYPES = {
    "REPORT_VERSION": "str",
    "ARTIFACT": "str",
    "SPEC_PATH": "str",
    "VERIFICATION_ID": "str",
    "CHECK": "str",
    "TARGET": "str",
    "REQUIREMENT": "str",
    "SEVERITY": "str",
    "OUTCOME": "str",
    "CONDITION": "str",
    "EVALUATED_COUNT": "int",
    "FAILURE_COUNT": "int",
    "DETAILS": "str",
}


def artifact_profile(output):
    """Return R020's profile for a declared output.path, or None.

    The mapping is closed: an extension outside it names no profile, which is
    a validation failure rather than a fallback to a default.
    """
    if not isinstance(output, dict):
        return None
    declared = output.get("path")
    if not isinstance(declared, str):
        return None
    return ARTIFACT_PROFILES.get(PurePosixPath(declared).suffix.lower())


SourceProfileError = CSV_PROFILE.CsvProfileFailure


parse_source_profile = CSV_PROFILE.records


source_coordinates = CSV_PROFILE.coordinates


SOURCE_PROFILE_CONDITIONS = {
    "source_profile_unknown": "an extension R023 does not map",
    "source_byte_order_mark": "source carries a byte-order mark",
    "source_header_absent": "source has no header record",
    "source_field_name_empty": "header contains an empty name",
    "source_field_name_duplicate": "duplicate header(s)",
    "source_record_width": "record width is not the header width",
    "source_quote_unterminated": "a quoted field has no closing quote",
    "source_quote_in_bare_field": "a bare field carries U+0022",
    "source_text_after_quote": "a closing quote is followed by ordinary text",
    "source_carriage_return": "U+000D does not begin a record terminator",
}


SOURCE_READ_CONDITIONS = (
    set(SOURCE_PROFILE_CONDITIONS) - {"source_profile_unknown"}
) | {"invalid_text"}


def check_source_file(csv_path: Path):
    """Report every way one fixture departs from R023's source profile.

    Each finding is a condition, the record it was decided at, and the
    detail a reader needs to find it.
    """
    raw = csv_path.read_bytes()
    try:
        data = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        # Everything before the offending byte decoded, so the reader's
        # position in the source is exactly what that prefix spells.
        record, field = source_coordinates(raw[: exc.start].decode("utf-8"))
        return [
            (
                "invalid_text",
                record,
                f"ill-formed encoded text at field {field}: {exc.reason}",
            )
        ]
    if data.startswith(BOM_UTF8):
        return [("source_byte_order_mark", 1, "source carries a byte-order mark")]
    try:
        records = parse_source_profile(data)
    except SourceProfileError as exc:
        return [
            (
                exc.condition,
                exc.record,
                f"{SOURCE_PROFILE_CONDITIONS[exc.condition]} at field {exc.field}",
            )
        ]
    if not records:
        return [("source_header_absent", 1, "source has no header record")]

    findings = []
    header = records[0]
    if any(not name for name in header):
        findings.append(("source_field_name_empty", 1, "header contains an empty name"))
    duplicates = sorted({name for name in header if name and header.count(name) > 1})
    if duplicates:
        findings.append(
            (
                "source_field_name_duplicate",
                1,
                "duplicate header(s): " + ", ".join(duplicates),
            )
        )
    for number, record in enumerate(records[1:], 2):
        if len(record) != len(header):
            findings.append(
                (
                    "source_record_width",
                    number,
                    f"expected {len(header)} fields, got {len(record)}",
                )
            )
    return findings


def declared_source_condition(example_dir: Path):
    """The source condition a negative example declares, if any."""
    if not example_dir.name.startswith("negative-"):
        return None
    error_path = example_dir / "expected" / "error.yaml"
    if not error_path.is_file():
        return None
    try:
        contract = yaml.safe_load(error_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, yaml.YAMLError):
        return None
    if not isinstance(contract, dict):
        return None
    condition = contract.get("condition")
    return condition if condition in SOURCE_READ_CONDITIONS else None


def validate_csv_shapes(root: Path):
    """Check every fixture against R023's csv source profile.

    The reader preserves quoting, so a bare empty field stays distinct from
    a quoted empty one rather than being normalized to the same text. A
    negative example that declares a source condition may carry the fixture
    that provokes it, and must actually provoke it: a malformed fixture no
    example asked for is still an error, and a declared condition no fixture
    reports means the example has stopped failing the way it claims to.
    """
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors
    for example_dir in sorted(path for path in examples_dir.iterdir() if path.is_dir()):
        declared = declared_source_condition(example_dir)
        reported = set()
        csv_paths = []
        for directory in ("input", "expected"):
            csv_paths.extend(
                sorted(
                    path
                    for path in (example_dir / directory).rglob("*")
                    if path.is_file() and path.suffix.lower() == ".csv"
                )
            )
        for csv_path in csv_paths:
            label = csv_path.relative_to(root)
            try:
                findings = check_source_file(csv_path)
            except OSError as exc:
                errors.append(f"ERROR: {label}: cannot read source: {exc}")
                continue
            for condition, number, detail in findings:
                reported.add(condition)
                if condition == declared:
                    continue
                errors.append(f"ERROR: {label}:{number}: {condition}: {detail}")
        if declared is not None and declared not in reported:
            errors.append(
                f"ERROR: {example_dir.relative_to(root)}: {declared} is "
                "declared but no source fixture reports it"
            )
    return errors


KEY_COLUMN_ORDER = ("DOMAIN", "STUDYID", "USUBJID")


def _key_column_order_errors(names, label):
    """Check populated key columns keep DOMAIN, STUDYID, USUBJID in order.

    Only key columns present in `names` constrain the order; an absent key
    column imposes nothing.
    """
    present = [column for column in KEY_COLUMN_ORDER if column in names]
    actual = [column for column in names if column in present]
    if actual != present:
        return [
            f"ERROR: {label}: key columns out of order: "
            f"{', '.join(actual)}; populated key columns keep the order "
            f"{', '.join(present)}"
        ]
    return []


def validate_key_column_order(root: Path):
    """Keep DOMAIN, STUDYID, USUBJID in that order across benchmark artifacts.

    Checks each entry spec's declared `columns`, its `output.columns`
    artifact order, and the golden CSV header, so the column order the
    GitHub Action verifies stays consistent everywhere a benchmark's
    columns are populated.
    """
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue
        for spec_path in example_entry_specs(ex_dir):
            try:
                with open(spec_path, "r", encoding="utf-8") as handle:
                    spec = yaml.load(handle, Loader=UniqueKeyLoader)
            except Exception:
                continue
            if not isinstance(spec, dict):
                continue
            declared = spec.get("columns")
            if isinstance(declared, list):
                names = [
                    column.get("name") if isinstance(column, dict) else column
                    for column in declared
                ]
                names = [name for name in names if isinstance(name, str)]
                errors.extend(
                    _key_column_order_errors(
                        names,
                        f"{ex_dir.name}/{spec_path.name}: declared columns",
                    )
                )
            artifact_name = None
            output = spec.get("output")
            if isinstance(output, dict):
                artifact_columns = output.get("columns")
                if isinstance(artifact_columns, list) and all(
                    isinstance(c, str) for c in artifact_columns
                ):
                    errors.extend(
                        _key_column_order_errors(
                            artifact_columns,
                            f"{ex_dir.name}/{spec_path.name}: output columns",
                        )
                    )
                artifact_path = output.get("path")
                if isinstance(artifact_path, str) and artifact_path:
                    artifact_name = PurePosixPath(artifact_path).name
            if artifact_name:
                golden = ex_dir / "expected" / artifact_name
                # Parquet goldens carry no header contract; only CSV
                # goldens pin the column order.
                if golden.suffix.lower() != ".csv" or not golden.is_file():
                    continue
                try:
                    with open(golden, "r", encoding="utf-8", newline="") as f:
                        header = next(csv.reader(f))
                except (StopIteration, UnicodeDecodeError, OSError):
                    # Empty, non-UTF-8, or unreadable goldens are
                    # reported by the CSV shape checks; skip them here.
                    continue
                errors.extend(
                    _key_column_order_errors(
                        header,
                        f"{ex_dir.name}/expected/{artifact_name}: header",
                    )
                )
    return errors


def validate_examples_csv(root: Path, env=None):
    errors = []
    warnings = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors, warnings
    if env is None:
        candidate_env, _ = build_schema_env(root)
        env = candidate_env

    profile_checked = set()
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        owners = example_artifact_owners(ex_dir)
        for spec_path in example_entry_specs(ex_dir):
            try:
                with open(spec_path, "r", encoding="utf-8") as f:
                    spec = yaml.load(f, Loader=UniqueKeyLoader)
            except Exception:
                continue

            if env is not None and isinstance(spec, dict) and "parents" in spec:
                spec, resolution_errors, _ = prepare_spec_document(
                    spec, f"{ex_dir.name}/{spec_path.name}", spec_path, env
                )
                if resolution_errors:
                    continue
            if not isinstance(spec, dict) or "columns" not in spec:
                continue

            output = spec.get("output")
            expected_cols = output.get("columns") if isinstance(output, dict) else None
            if not isinstance(expected_cols, list):
                continue
            primary_path = output.get("path")
            primary_name = (
                PurePosixPath(primary_path).name
                if isinstance(primary_path, str)
                else None
            )
            violation_path = output.get("warning_log")
            violation_name = (
                PurePosixPath(violation_path).name
                if isinstance(violation_path, str)
                else None
            )
            verification_path = output.get("verification_log")
            verification_name = (
                PurePosixPath(verification_path).name
                if isinstance(verification_path, str)
                else None
            )

            expected_dir = ex_dir / "expected"
            if not expected_dir.exists():
                continue
            for csv_file in sorted(expected_dir.glob("*.csv")):
                # Each golden CSV must be one of the three artifacts the
                # specification names. The logs have their own fixed schemas:
                # R009's for the warning log, REQ-1173's for the verification
                # log.
                if csv_file.name == primary_name:
                    file_columns = expected_cols
                    file_output = output
                    file_spec = spec
                elif csv_file.name == violation_name:
                    file_columns = list(WARNING_LOG_TYPES)
                    file_output = {"path": violation_path}
                    file_spec = {
                        "output": file_output,
                        "columns": [
                            {"name": name, "type": column_type}
                            for name, column_type in WARNING_LOG_TYPES.items()
                        ],
                    }
                elif csv_file.name == verification_name:
                    file_columns = list(VERIFICATION_LOG_TYPES)
                    file_output = {"path": verification_path}
                    file_spec = {
                        "output": file_output,
                        "columns": [
                            {"name": name, "type": column_type}
                            for name, column_type in VERIFICATION_LOG_TYPES.items()
                        ],
                    }
                elif owners.get(csv_file.name, set()) - {spec_path.name}:
                    # Another spec of this example produces this golden, and
                    # its own pass checks the header.
                    continue
                else:
                    permitted = [
                        name
                        for name in (primary_name, violation_name, verification_name)
                        if name is not None
                    ]
                    errors.append(
                        f"ERROR: {ex_dir.name}/{csv_file.name}: expected "
                        f"artifact name in {permitted!r}, which output "
                        "declares"
                    )
                    continue
                try:
                    with open(csv_file, "r", encoding="utf-8") as f:
                        reader = csv.reader(f)
                        header = next(reader)
                        if header != file_columns:
                            errors.append(
                                f"ERROR: {ex_dir.name}/{csv_file.name}: "
                                f"header mismatch for {spec_path.name}. "
                                f"Expected {file_columns}, got {header}"
                            )
                except StopIteration:
                    if file_columns:
                        errors.append(
                            f"ERROR: {ex_dir.name}/{csv_file.name} is "
                            f"empty for {spec_path.name}"
                        )
                except (OSError, UnicodeError, csv.Error):
                    continue

                profile = artifact_profile(file_output)
                if profile == "csv" and csv_file not in profile_checked:
                    profile_checked.add(csv_file)
                    errors.extend(
                        validate_csv_artifact(
                            csv_file,
                            f"{ex_dir.name}/{csv_file.name}",
                            file_spec,
                        )
                    )

    return errors, warnings
