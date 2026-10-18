"""Authored golden file names, table shape and repository column-order style."""

import csv
from pathlib import Path, PurePosixPath

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.editorial import example_spec_paths

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

    Checks each authored spec's declared `columns`, its `output.columns`
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
        for spec_path in example_spec_paths(ex_dir):
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


def validate_examples_csv(root: Path):
    """Check golden CSV layout, leaving types, values and inheritance to native suites."""
    errors = []
    for example in sorted((root / "benchmarks").glob("*")):
        if not example.is_dir():
            continue
        declarations = {}
        inherited = False
        for path in example_spec_paths(example):
            try:
                document = yaml.load(
                    path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader
                )
            except (OSError, UnicodeError, yaml.YAMLError):
                continue
            if not isinstance(document, dict):
                continue
            inherited |= bool(document.get("parents"))
            output = document.get("output")
            if not isinstance(output, dict):
                continue
            for key in ("path", "report"):
                name = output.get(key)
                if isinstance(name, str):
                    declarations[PurePosixPath(name).name] = (
                        output.get("columns") if key == "path" else None
                    )
        for path in sorted((example / "expected").glob("*.csv")):
            label = path.relative_to(root)
            try:
                with path.open(encoding="utf-8", newline="") as stream:
                    rows = list(csv.reader(stream, strict=True))
            except (OSError, UnicodeError, csv.Error) as error:
                errors.append(f"ERROR: {label}: {error}")
                continue
            if not rows or not rows[0] or any(not name for name in rows[0]):
                errors.append(f"ERROR: {label}: missing or empty column header")
                continue
            header = rows[0]
            if len(set(header)) != len(header):
                errors.append(f"ERROR: {label}: duplicate column header")
            for number, row in enumerate(rows[1:], 2):
                if len(row) != len(header):
                    errors.append(
                        f"ERROR: {label}:{number}: expected {len(header)} fields, got {len(row)}"
                    )
            if inherited:
                # Resolving parents is a language operation owned by the Rust suite.
                continue
            if path.name not in declarations:
                errors.append(
                    f"ERROR: {label}: no authored output declares this artifact"
                )
            else:
                expected = declarations[path.name]
                if (
                    isinstance(expected, list)
                    and all(isinstance(name, str) for name in expected)
                    and header != expected
                ):
                    errors.append(
                        f"ERROR: {label}: header {header!r} does not match output columns {expected!r}"
                    )
    return errors
