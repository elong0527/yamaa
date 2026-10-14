"""Repository artifact shape against an explicitly held native producer contract.

This does not execute a producer or decide workflow paths/types. Native CSV
admission precedes the standard-library header codec and exact sequence check.
"""

import csv
import io

import _native_csv


def check_csv(raw, output_columns, label, written):
    """Compare original captured artifact bytes with admitted publication columns."""
    if not raw:
        return [f"ERROR: {label}: stored CSV artifact is empty: {written}"]
    issues = _native_csv.profile_issues(raw)
    if issues:
        return [
            f"ERROR: {label}: {row['condition']}: {row['context']}" for row in issues
        ]
    header = next(csv.reader(io.StringIO(raw.decode("utf-8"), newline="")), [])
    if header != output_columns:
        return [
            (
                f"ERROR: {label}.output.columns: producer output must "
                f"match the artifact header exactly; expected {output_columns!r}, "
                f"got {header!r}"
            )
        ]
    return []
