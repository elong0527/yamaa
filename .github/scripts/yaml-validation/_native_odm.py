"""Development-only ODM partial controls over a typed Rust declaration fixture.

Complete repository specifications use the held native engine owner. These
controls isolate ODM declaration/header facts and retain later binding refusals
separately; they grant no execution or publication authority.
"""

import copy
import json
from functools import lru_cache
from pathlib import Path

import _native_csv
import _typed_fixtures


@lru_cache(maxsize=1)
def fields():
    return tuple(_typed_fixtures.observe("odm", json.dumps(["{}", []]))["fields"])


def observe(specification, spec_path=None, sources=None):
    document = copy.deepcopy(specification)
    # This fixture isolates the ODM family, including the overridden-column
    # control whose original domain/input name collision belongs to full check.
    document["domain"] = "VALIDATORFIXTURE"
    headers = []
    for dataset, source in (document.get("input") or {}).items():
        written = (
            source
            if isinstance(source, str)
            else (source.get("path") if isinstance(source, dict) else None)
        )
        path = (sources or {}).get((dataset, "path"))
        if path is None and spec_path is not None and isinstance(written, str):
            path = Path(spec_path).parent / written
        # Missing source IO is outside this partial declaration/header control.
        # Its authored complete fixture provides the fixed schema as metadata.
        header = list(fields())
        if path is not None:
            try:
                raw = Path(path).read_bytes()
                if _native_csv.source_issue(raw) is None:
                    rows = _native_csv.records(raw.decode("utf-8"))
                    header = rows[0] if rows else []
            except (OSError, UnicodeError, _native_csv.CsvProfileFailure):
                pass
        headers.append(header)
    return _typed_fixtures.observe("odm", json.dumps([json.dumps(document), headers]))


def inputs(observation):
    return frozenset(observation.get("odm_inputs", []))


def issues(observation):
    # Only this partial family's intrinsic/header issues are accepted here.
    # The complete production validator never applies this family projection.
    return [
        issue
        for issue in (observation.get("issues") or [])
        if issue["condition"]
        in {
            "invalid_odm_context",
            "odm_schema_field_type",
            "odm_schema_field_missing",
            "odm_schema_field_ambiguous",
        }
        or (
            issue["condition"] == "unknown_field"
            and any(".odm" in path for path in issue["spec_paths"])
        )
    ]
