"""The warning and verification logs on the edges the benchmarks leave open.

The four log benchmarks pin one violated warning each. These pin the rest:
a declared log with nothing to report (REQ-0391/REQ-1173), the sidecar
declarations (REQ-0389/REQ-0391/REQ-0756/REQ-1179/REQ-1180), the canonical
JSON fields (REQ-0394), complete evidence rather than a sample (REQ-0393),
the unit each count counts (REQ-1176), a failed run's log (REQ-1177/1181),
and a sidecar's own profile.
"""

import csv
import io
import os

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import yaml

from yamaa import YamaaError, derive, derive_artifacts
from yamaa.logs import canonical_json

DM = "USUBJID,AGE,ARM\nS1,30,A\nS2,214,A\nS3,45,B\n"


def spec(output=None, columns=None, verifications=None):
    s = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": "input/dm.csv"},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "AGE", "ARM"]},
        "columns": columns
        or [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "AGE", "type": "int", "derivation": "DM.AGE"},
            {"name": "ARM", "type": "str", "derivation": "DM.ARM"},
        ],
    }
    s["output"].update(output or {})
    if verifications is not None:
        s["verifications"] = verifications
    return s


def write(tmp_path, s, dm=DM):
    os.makedirs(tmp_path / "input", exist_ok=True)
    (tmp_path / "input" / "dm.csv").write_text(dm, encoding="utf-8")
    path = tmp_path / "spec.yaml"
    path.write_text(yaml.safe_dump(s), encoding="utf-8")
    return str(path)


def artifacts(tmp_path, s, dm=DM):
    return derive_artifacts(write(tmp_path, s, dm))


def failure(tmp_path, s, dm=DM):
    with pytest.raises(YamaaError) as ei:
        artifacts(tmp_path, s, dm)
    return ei.value


def records(text):
    return list(csv.DictReader(io.StringIO(text)))


WARNING_HEADER = (
    "LOG_VERSION,ARTIFACT,SEVERITY,CONDITION,REQUIREMENT,SPEC_PATH,"
    "VERIFICATION_ID,FAILURE_COUNT,OFFENDING_KEYS,DETAILS\n"
)
LOGS = {"warning_log": "warnings.csv", "verification_log": "checks.csv"}


def age_range(severity="warning", low=18):
    return [{"range": {"min": low, "max": 250, "severity": severity}}]


def with_age_checks(checks):
    s = spec(output=LOGS)
    s["columns"][1]["verifications"] = checks
    return s


# -- a declared log with nothing to report ------------------------------------


def test_a_warning_log_without_a_violated_warning_is_its_header(tmp_path):
    # REQ-0391: the empty case is a header-only dataset, so publication
    # replaces a stale log; REQ-1173: every declared check still has a row.
    got = artifacts(tmp_path, with_age_checks(age_range()))
    assert list(got) == ["checks.csv", "warnings.csv", "adsl.csv"]  # REQ-1181
    assert got["warnings.csv"] == WARNING_HEADER
    (row,) = records(got["checks.csv"])
    assert (row["OUTCOME"], row["CONDITION"], row["FAILURE_COUNT"]) == ("held", "", "0")
    assert (row["EVALUATED_COUNT"], row["DETAILS"]) == ("3", "{}")
    assert got["adsl.csv"] == derive(write(tmp_path, with_age_checks(age_range())))


def test_a_verification_log_is_independent_of_severity(tmp_path):
    # REQ-1173: all-error checks may declare the log, with no warning log.
    s = spec(
        output={"verification_log": "checks.csv"},
        verifications=[{"unique": ["ARM"]}, {"row_count": {"min": 1}}],
    )
    e = failure(tmp_path, s)
    assert (e.condition, e.spec_paths) == ("unique_failed", ["verifications[0].unique"])
    # REQ-1181: a failed run replaces the verification log alone, and the
    # check the stopped run never reached has no row (REQ-1177).
    assert list(e.artifacts) == ["checks.csv"]
    (row,) = records(e.artifacts["checks.csv"])
    assert (row["SPEC_PATH"], row["SEVERITY"], row["OUTCOME"]) == (
        "verifications[0].unique",
        "error",
        "violated",
    )
    # REQ-1176: `unique` counts distinct combinations, not rows.
    assert (row["EVALUATED_COUNT"], row["FAILURE_COUNT"]) == ("2", "1")
    assert row["DETAILS"] == '{"columns":["ARM"]}'


def test_a_failed_run_keeps_its_warnings_out_of_a_warning_log(tmp_path):
    s = spec(
        output=LOGS,
        verifications=[
            {"row_count": {"max": 1, "severity": "warning"}},
            {"assert": {"expr": "AGE < 100"}},
        ],
    )
    e = failure(tmp_path, s)
    assert e.condition == "assert_failed"
    assert list(e.artifacts) == ["checks.csv"]
    rows = records(e.artifacts["checks.csv"])
    assert [(r["CHECK"], r["SEVERITY"], r["OUTCOME"]) for r in rows] == [
        ("row_count", "warning", "violated"),
        ("assert", "error", "violated"),
    ]


def test_a_run_that_never_executed_writes_no_log(tmp_path):
    s = with_age_checks(age_range())
    s["output"]["columns"].append("NOPE")
    assert failure(tmp_path, s).artifacts == {}


# -- the sidecar declarations --------------------------------------------------


@pytest.mark.parametrize(
    ("output", "paths", "requirement"),
    [
        (
            {"verification_log": "adsl.csv"},
            ["output.path", "output.verification_log"],
            "REQ-1180",
        ),
        (
            {"warning_log": "logs/w.csv", "verification_log": "logs/./w.csv"},
            ["output.warning_log", "output.verification_log"],
            "REQ-1180",
        ),
        (
            {"warning_log": "./adsl.csv"},
            ["output.path", "output.warning_log"],
            "REQ-0756",
        ),
    ],
)
def test_a_sidecar_path_must_not_collide(tmp_path, output, paths, requirement):
    # REQ-1179: the three files are different files, however a path is spelled.
    s = with_age_checks(age_range())
    s["output"].update(output)
    e = failure(tmp_path, s)
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "artifact_path_collision",
        requirement,
    )
    assert e.spec_paths == paths


@pytest.mark.parametrize("field", ["warning_log", "verification_log"])
def test_a_sidecar_extension_selects_a_profile(tmp_path, field):
    s = with_age_checks(age_range())
    s["output"][field] = "log.json"
    e = failure(tmp_path, s)
    assert (e.condition, e.requirement, e.spec_paths) == (
        "unknown_artifact_profile",
        "REQ-0760",
        [f"output.{field}"],
    )
    assert e.context == {"path": "log.json", "permitted": [".csv", ".parquet"]}


def test_a_warning_needs_a_warning_log(tmp_path):
    s = spec()
    s["columns"][1]["verifications"] = age_range()
    e = failure(tmp_path, s)
    assert (e.condition, e.requirement, e.spec_paths) == (
        "missing_warning_log",
        "REQ-0391",
        ["output.warning_log"],
    )
    assert e.context == {"warnings": ["columns.AGE.verifications[0].range.severity"]}


def test_a_severity_is_error_or_warning(tmp_path):
    e = failure(tmp_path, with_age_checks(age_range(severity="warn")))
    assert (e.condition, e.requirement, e.spec_paths) == (
        "value_not_permitted",
        "REQ-0389",
        ["columns.AGE.verifications[0].range.severity"],
    )


# -- canonical JSON and complete evidence ---------------------------------------


def test_canonical_json_is_compact_sorted_ascii():
    # REQ-0394.
    value = {
        "b": [1, 2.5, 1e21, None, True, False],
        "a": '\u00e9\U0001f600"\n',
        "Z": {},
    }
    assert canonical_json(value) == (
        '{"Z":{},"a":"\\u00e9\\ud83d\\ude00\\"\\n",'
        '"b":[1,2.5,1000000000000000000000,null,true,false]}'
    )
    assert (canonical_json([]), canonical_json({})) == ("[]", "{}")


def test_offending_keys_are_complete_and_canonical(tmp_path):
    # REQ-0393: every offending key, not an error report's bounded sample;
    # a date key renders as its text and a name outside ASCII escapes.
    subjects = [f"S\u00e9{i}" for i in range(7)]
    dm = "USUBJID,AGE,ARM\n" + "".join(f"{u},10,A\n" for u in subjects)
    s = with_age_checks(age_range())
    s["keys"] = ["USUBJID", "VISITDT"]
    s["columns"].append(
        {"name": "VISITDT", "type": "date", "derivation": {"literal": "2020-01-31"}}
    )
    s["output"]["columns"].append("VISITDT")
    (row,) = records(artifacts(tmp_path, s, dm)["warnings.csv"])
    assert row["FAILURE_COUNT"] == "7"
    assert row["OFFENDING_KEYS"] == (
        "["
        + ",".join(
            f'{{"USUBJID":"S\\u00e9{i}","VISITDT":"2020-01-31"}}' for i in range(7)
        )
        + "]"
    )
    assert row["DETAILS"] == '{"column":"AGE"}'


def test_a_grouped_row_count_reports_counts_even_for_one_group(tmp_path):
    # REQ-0393: grouped `row_count` details carry the aligned counts.
    s = spec(
        output=LOGS,
        verifications=[
            {
                "row_count": {
                    "group_by": ["ARM"],
                    "filter": "AGE > 40",
                    "max_fraction": 0.6,
                    "severity": "warning",
                }
            }
        ],
    )
    got = artifacts(tmp_path, s)
    (warning,) = records(got["warnings.csv"])
    # Arm A has 1 of 2 over 40 and holds; arm B has 1 of 1.
    assert warning["OFFENDING_KEYS"] == '[{"ARM":"B"}]'
    assert warning["DETAILS"] == '{"counts":[1],"denominators":[1]}'
    (check,) = records(got["checks.csv"])
    assert (check["EVALUATED_COUNT"], check["FAILURE_COUNT"]) == ("2", "1")
    assert check["DETAILS"] == warning["DETAILS"]  # REQ-1176: one encoding


def test_a_unique_warning_lists_every_row_of_a_repeated_combination(tmp_path):
    s = spec(
        output=LOGS,
        verifications=[
            {"unique": {"columns": ["ARM"], "id": "arm", "severity": "warning"}}
        ],
    )
    (row,) = records(artifacts(tmp_path, s)["warnings.csv"])
    assert (row["VERIFICATION_ID"], row["FAILURE_COUNT"]) == ("arm", "1")
    assert row["OFFENDING_KEYS"] == '[{"USUBJID":"S1"},{"USUBJID":"S2"}]'


def test_max_length_names_the_requirement_defining_it(tmp_path):
    s = spec(output=LOGS)
    s["columns"][0]["verifications"] = [
        {"max_length": {"max": 1, "severity": "warning"}}
    ]
    (row,) = records(artifacts(tmp_path, s)["checks.csv"])
    assert (row["TARGET"], row["REQUIREMENT"]) == ("USUBJID", "REQ-0378")
    assert row["DETAILS"] == '{"column":"USUBJID","max":1}'


# -- each file's own profile -----------------------------------------------------


def test_each_sidecar_takes_the_profile_its_extension_selects(tmp_path):
    # REQ-1180: a parquet verification log beside a csv primary and warning log.
    s = with_age_checks(age_range(low=40))
    s["output"]["verification_log"] = "checks.PARQUET"
    got = artifacts(tmp_path, s)
    assert isinstance(got["adsl.csv"], str) and isinstance(got["warnings.csv"], str)
    table = pq.ParquetFile(pa.BufferReader(got["checks.PARQUET"])).read()
    assert [str(f.type) for f in table.schema][-3:] == ["int64", "int64", "string"]
    (row,) = table.to_pylist()
    assert (row["VERIFICATION_ID"], row["TARGET"], row["OUTCOME"]) == (
        None,
        "AGE",
        "violated",
    )
    assert (row["EVALUATED_COUNT"], row["FAILURE_COUNT"]) == (3, 1)
