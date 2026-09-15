import pytest
import polars as pl
from cdiscbuilderv2.verifications import VerificationEngine


def test_verifications_unique_and_not_missing():
    df = pl.DataFrame({
        "STUDYID": ["S1", "S1", "S1"],
        "USUBJID": ["SUBJ1", "SUBJ2", "SUBJ3"],
        "AGE": [25, 30, 45]
    })

    spec = {
        "domain": "DM",
        "verifications": [
            {"unique": {"columns": ["STUDYID", "USUBJID"]}},
            {"row_count": {"min": 1, "max": 10}}
        ],
        "columns": [
            {
                "name": "USUBJID",
                "verifications": [{"not_missing": {}}]
            },
            {
                "name": "AGE",
                "verifications": [{"range": {"min": 18, "max": 120}}]
            }
        ]
    }

    report = VerificationEngine.verify(df, spec)
    assert report.is_valid
    assert report.pass_count == 4
    assert report.fail_count == 0


def test_verifications_failure():
    df = pl.DataFrame({
        "STUDYID": ["S1", "S1"],
        "USUBJID": ["SUBJ1", "SUBJ1"],  # Duplicate
        "SEX": ["M", "INVALID"]
    })

    spec = {
        "domain": "DM",
        "verifications": [
            {"unique": {"columns": ["STUDYID", "USUBJID"]}}
        ],
        "columns": [
            {
                "name": "SEX",
                "verifications": [{"allowed_values": {"values": ["M", "F", "U"]}}]
            }
        ]
    }

    report = VerificationEngine.verify(df, spec)
    assert not report.is_valid
    assert report.fail_count == 2
