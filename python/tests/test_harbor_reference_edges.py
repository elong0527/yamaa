"""Independent prompt witnesses for #1889, using temporary input copies only."""

from __future__ import annotations

import csv
import functools
import os
import re
import shutil
import subprocess
import sys
from decimal import Decimal
from itertools import pairwise
from pathlib import Path

import pytest

ROOT = Path(__file__).parents[2]
SOLUTIONS = ROOT / "evaluations/harbor/solutions"


def _read(path):
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream)
        return reader.fieldnames, list(reader)


def _input(name, file):
    return _read(ROOT / "benchmarks" / name / "input" / file)[1]


@functools.cache
def _r_has(packages):
    if shutil.which("Rscript") is None:
        return False
    script = ";".join(f"library({p})" for p in packages)
    return (
        subprocess.run(
            ["Rscript", "-e", script], capture_output=True, check=False
        ).returncode
        == 0
    )


@pytest.fixture(params=["python", "r", "opensas"])
def language(request):
    return request.param


def _run(tmp_path, name, language, changes):
    script_name = {"python": "result.py", "r": "result.R", "opensas": "result.sas"}[
        language
    ]
    text = (SOLUTIONS / name / script_name).read_text()
    if language == "r":
        packages = tuple(sorted(set(re.findall(r"library\(([\w.]+)", text))))
        if not _r_has(packages):
            if os.environ.get("YAMAA_REQUIRE_R"):
                pytest.fail(f"R reference dependencies unavailable: {packages}")
            pytest.skip("R reference dependencies unavailable")
    if language == "opensas" and not shutil.which("opensas"):
        if os.environ.get("YAMAA_REQUIRE_OPENSAS"):
            pytest.fail("opensas reference runtime unavailable")
        pytest.skip("opensas reference runtime unavailable")
    source = tmp_path / "input"
    shutil.copytree(ROOT / "benchmarks" / name / "input", source)
    for file, rows in changes.items():
        header, _ = _read(source / file)
        with (source / file).open("w", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=header)
            writer.writeheader()
            writer.writerows(rows)
    output = tmp_path / "output"
    output.mkdir()
    script = output / script_name
    script.write_text(text.replace("/app/", f"{tmp_path.as_posix()}/"))
    runner = {"python": sys.executable, "r": "Rscript", "opensas": "opensas"}[language]
    completed = subprocess.run(
        [runner, str(script)], cwd=tmp_path, capture_output=True, text=True, check=False
    )
    assert completed.returncode == 0, completed.stdout + completed.stderr
    return {p.name: _read(p)[1] for p in output.glob("*.csv")}


@pytest.fixture(params=["yamaa", "python", "r", "opensas"])
def worsening_track(request):
    return request.param


def _worsening_rows(tmp_path, track, ae=None, fa=None):
    name = "adam-adae-worsening-emergence"
    changes = {
        "ae.csv": ae or _input(name, "ae.csv"),
        "fa.csv": fa or _input(name, "fa.csv"),
    }
    if track != "yamaa":
        return _run(tmp_path, name, track, changes)["adae.csv"]
    from yamaa import yamaa_domain

    study = tmp_path / "study"
    shutil.copytree(ROOT / "benchmarks" / name, study)
    for file, rows in changes.items():
        header, _ = _read(study / "input" / file)
        with (study / "input" / file).open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=header)
            writer.writeheader()
            writer.writerows(rows)
    result = yamaa_domain(study / "spec.yaml").output
    result.write_csv(study / "actual.csv", datetime_format="%Y-%m-%dT%H:%M:%S")
    return _read(study / "actual.csv")[1]


def _event_rows(rows, subject="P7-101", sequence="1"):
    return sorted(
        [row for row in rows if row["USUBJID"] == subject and row["AESEQ"] == sequence],
        key=lambda row: row["ASTDTM"],
    )


def test_one_source_ae_expands_to_continuous_analysis_periods(
    tmp_path, worsening_track
):
    rows = _worsening_rows(tmp_path, worsening_track)
    assert len(rows) == 14 > len(_input("adam-adae-worsening-emergence", "ae.csv"))
    headache = _event_rows(rows)
    assert [row["TRTEMFL"] for row in headache] == ["", "Y", "Y", "Y"]
    assert [row["ASEV"] for row in headache] == ["MILD", "MODERATE", "SEVERE", "MILD"]
    assert all(row["AESEV"] == "SEVERE" for row in headache)
    assert [int(row["ASEQ"]) for row in headache] == [1, 2, 3, 4]
    for earlier, later in pairwise(headache):
        assert earlier["AENDTM"] == later["ASTDTM"]
    assert headache[0]["ASTDTM"] == headache[0]["AESTDTC"]
    assert headache[-1]["AENDTM"] == headache[-1]["AEENDTC"]
    # Improvement alone does not start TE; improvement after worsening keeps TE.
    assert [row["TRTEMFL"] for row in _event_rows(rows, "P7-102")] == ["", "", "Y"]
    cough = _event_rows(rows, "P7-105")
    assert all(row["TRTEMFL"] == "" for row in cough)
    assert cough[-1]["AENDTM"] == ""
    assert all(row["TRTEMFL"] == "" for row in _event_rows(rows, "P7-104"))
    assert [row["TRTEMFL"] for row in _event_rows(rows, "P7-103")] == ["", "Y"]


def test_moving_a_collected_change_moves_both_period_boundaries(
    tmp_path, worsening_track
):
    fa = _input("adam-adae-worsening-emergence", "fa.csv")
    for row in fa:
        if row["USUBJID"] == "P7-101" and row["FADTC"] == "2025-03-04T11:00:00":
            row["FADTC"] = "2025-03-04T11:45"
    rows = _event_rows(_worsening_rows(tmp_path, worsening_track, fa=fa))
    assert rows[0]["AENDTM"] == "2025-03-04T11:45:00"
    assert rows[1]["ASTDTM"] == rows[0]["AENDTM"]
    assert rows[1]["TRTEMFL"] == "Y"


def test_different_source_events_with_the_same_term_do_not_share_worsening(
    tmp_path, worsening_track
):
    name = "adam-adae-worsening-emergence"
    ae, fa = _input(name, "ae.csv"), _input(name, "fa.csv")
    for row in ae:
        if row["USUBJID"] == "P7-101" and row["AESEQ"] == "2":
            row.update(AESTDTC="2025-03-03T06:00:00", AEENDTC="2025-03-04T06:00:00")
    for row in fa:
        if row["USUBJID"] == "P7-101" and row["FALNKID"] == "AE-2":
            row["FADTC"] = "2025-03-03T06:00:00"
    rows = _worsening_rows(tmp_path, worsening_track, ae, fa)
    assert all(row["TRTEMFL"] == "" for row in _event_rows(rows, sequence="2"))
    assert [row["TRTEMFL"] for row in _event_rows(rows)] == ["", "Y", "Y", "Y"]


@pytest.mark.parametrize(
    ("subject", "dimension"), [("P7-101", "SEV"), ("P7-103", "TOXGR")]
)
def test_missing_pretreatment_dimension_cannot_establish_worsening(
    tmp_path, worsening_track, subject, dimension
):
    fa = _input("adam-adae-worsening-emergence", "fa.csv")
    for row in fa:
        if (
            row["USUBJID"] == subject
            and row["FALNKID"] == "AE-1"
            and row["FATESTCD"] == dimension
            and row["FADTC"] < "2025-03-04T09:00:00"
        ):
            row["FASTRESC"] = ""
    rows = _worsening_rows(tmp_path, worsening_track, fa=fa)
    assert all(row["TRTEMFL"] == "" for row in _event_rows(rows, subject))


def test_worsening_uses_the_last_pretreatment_state(tmp_path, worsening_track):
    fa = _input("adam-adae-worsening-emergence", "fa.csv")
    for row in fa:
        if row["USUBJID"] != "P7-101" or row["FALNKID"] != "AE-1":
            continue
        if row["FADTC"] == "2025-03-04T11:00:00":
            row.update(FADTC="2025-03-04T08:00:00", FASTRESC="SEVERE")
        elif row["FADTC"] == "2025-03-04T13:00:00":
            row["FASTRESC"] = "MODERATE"
    rows = _event_rows(_worsening_rows(tmp_path, worsening_track, fa=fa))
    assert all(row["TRTEMFL"] == "" for row in rows)


def test_worsening_at_first_exposure_starts_treatment_emergence(
    tmp_path, worsening_track
):
    fa = _input("adam-adae-worsening-emergence", "fa.csv")
    for row in fa:
        if row["USUBJID"] == "P7-101" and row["FADTC"] == "2025-03-04T11:00:00":
            row["FADTC"] = "2025-03-04T09:00:00"
    rows = _event_rows(_worsening_rows(tmp_path, worsening_track, fa=fa))
    assert [row["TRTEMFL"] for row in rows] == ["", "Y", "Y", "Y"]
    assert rows[1]["ASTDTM"] == rows[1]["TRTSDTM"]


def test_simultaneous_severity_and_grade_changes_create_one_period(
    tmp_path, worsening_track
):
    name = "adam-adae-worsening-emergence"
    ae, fa = _input(name, "ae.csv"), _input(name, "fa.csv")
    update = next(
        row
        for row in fa
        if row["USUBJID"] == "P7-101" and row["FADTC"] == "2025-03-04T11:00:00"
    )
    fa.append({**update, "FASEQ": "8", "FATESTCD": "TOXGR", "FASTRESC": "2"})
    ae[0]["AETOXGR"] = "2"
    rows = _worsening_rows(tmp_path, worsening_track, ae, fa)
    assert len(rows) == 14
    headache = _event_rows(rows)
    assert len(headache) == 4
    assert headache[1]["ASEV"] == "MODERATE"
    assert headache[1]["ATOXGRN"] == "2"
    assert all(row["ATOXGRN"] == "2" for row in headache[1:])


def test_missing_age_has_no_group_and_ranks_last(tmp_path, language):
    name = "adam-adsl-flag-chain"
    dm = _input(name, "dm.csv")
    dm.append({**dm[0], "USUBJID": "AGE-MISSING", "AGE": ""})
    rows = _run(tmp_path, name, language, {"dm.csv": dm})["adsl.csv"]
    missing = next(r for r in rows if r["USUBJID"] == "AGE-MISSING")
    assert missing["AGEGR1"] == ""
    assert int(missing["AGERNK"]) == len(rows)


@pytest.mark.parametrize("only_temperature", [False, True])
def test_temperature_never_becomes_fa(tmp_path, language, only_temperature):
    name = "sdtm-fa-reactogenicity-diary"
    diary = _input(name, "diary.csv")
    temperature = {**diary[0], "REACTION": "TEMPERATURE", "TPT": "TEMP-EDGE"}
    changes = [temperature] if only_temperature else [*diary, temperature]
    rows = _run(tmp_path, name, language, {"diary.csv": changes})["fa.csv"]
    assert all(r["FAOBJ"] != "TEMPERATURE" for r in rows)
    if only_temperature:
        assert rows == []


def test_blank_completion_is_a_missed_day(tmp_path, language):
    name = "sdtm-fa-reactogenicity-diary"
    diary = _input(name, "diary.csv")
    diary.append({**diary[0], "COMPLETED": "", "TPT": "COMPLETION-EDGE"})
    rows = _run(tmp_path, name, language, {"diary.csv": diary})["fa.csv"]
    missed = [r for r in rows if r["FATPT"] == "COMPLETION-EDGE"]
    assert len(missed) == 1
    assert missed[0]["FATESTCD"] == "OCCUR"
    assert missed[0]["FASTAT"] == "NOT DONE"
    assert missed[0]["FAORRES"] == ""


def test_unoccurred_reaction_severity_is_none(tmp_path, language):
    name = "sdtm-fa-reactogenicity-diary"
    diary = _input(name, "diary.csv")
    diary.append({**diary[0], "OCCUR": "N", "SEV": "SEVERE", "TPT": "NONE-EDGE"})
    rows = _run(tmp_path, name, language, {"diary.csv": diary})["fa.csv"]
    severity = next(
        r for r in rows if r["FATPT"] == "NONE-EDGE" and r["FATESTCD"] == "SEV"
    )
    assert severity["FAORRES"] == severity["FASTRESC"] == "NONE"


def test_multiform_repeats_use_numeric_occurrence_and_result(tmp_path, language):
    name = "sdtm-lb-multiform"
    odm = _input(name, "odm.csv")
    result = next(r for r in odm if r["ItemOID"].endswith(".VITD"))
    date = next(r for r in odm if r["ItemOID"].endswith(".LBDTC"))
    records = []
    for repeat, value in [("1", "001.20"), ("10", "0"), ("11", "text")]:
        base = {
            "SubjectKey": "REPEAT-EDGE",
            "StudyEventOID": "SE.UNSCH",
            "ItemGroupRepeatKey": repeat,
        }
        records.extend([{**date, **base}, {**result, **base, "Value": value}])
    rows = _run(tmp_path, name, language, {"odm.csv": records})["lb.csv"]
    by_visit = {Decimal(r["VISITNUM"]): r for r in rows}
    assert len(rows) == 3
    assert set(by_visit) == {Decimal("2.01"), Decimal("2.1"), Decimal("2.11")}
    assert by_visit[Decimal("2.01")]["LBORRES"] == "001.20"
    assert Decimal(by_visit[Decimal("2.01")]["LBSTRESN"]) == Decimal("1.2")
    assert Decimal(by_visit[Decimal("2.10")]["LBSTRESN"]) == 0
    assert by_visit[Decimal("2.11")]["LBSTRESN"] == ""


def test_unknown_event_keeps_reported_lab_without_visit(tmp_path, language):
    name = "sdtm-lb-multiform"
    odm = _input(name, "odm.csv")
    records = [
        {**r, "StudyEventOID": "SE.OTHER"}
        for r in odm
        if r["ItemOID"].endswith((".VITD", ".LBDTC"))
    ]
    rows = _run(tmp_path, name, language, {"odm.csv": records})["lb.csv"]
    assert rows
    assert all(r["VISIT"] == r["VISITNUM"] == "" for r in rows)


def test_missing_seriousness_does_not_mean_nonserious(tmp_path, language):
    name = "adam-adae-prior-serious-event"
    ae = _input(name, "ae.csv")
    ae.append({**ae[1], "AESEQ": "99", "AESER": ""})
    rows = _run(tmp_path, name, language, {"ae.csv": ae})["adae.csv"]
    row = next(r for r in rows if r["AESEQ"] == "99")
    assert all(
        row[c] == ""
        for c in [
            "PRIOR_SAEFL",
            "PRIOR_SAEDECOD",
            "PRIOR_SAESEQ",
            "FIRST_SAEDECOD",
            "FIRST_SAESEQ",
        ]
    )
    assert row["PREV_AEDECOD"]


@pytest.mark.parametrize("answer", ["N", "", "UNKNOWN"])
def test_repeated_checklist_uses_last_collected_answer(tmp_path, language, answer):
    name = "sdtm-mh-prespecified-conditions"
    odm = _input(name, "odm.csv")
    items = _input(name, "mh_items.csv")
    first = next(r for r in odm if r["ItemOID"] == items[0]["ItemOID"])
    odm.append({**first, "Value": answer})
    rows = _run(tmp_path, name, language, {"odm.csv": odm})["mh.csv"]
    selected = [
        r
        for r in rows
        if r["USUBJID"] == first["SubjectKey"] and r["MHTERM"] == items[0]["MHTERM"]
    ]
    assert len(selected) == 1
    assert selected[0]["MHOCCUR"] == (answer if answer == "N" else "")
    assert selected[0]["MHSTAT"] == ("" if answer == "N" else "NOT DONE")


@pytest.mark.parametrize(
    "name,file,seq,severity,flag",
    [
        ("adam-adae-worst-severity", "adae_raw.csv", "AESEQ", "AESEV", "AWSEVFL"),
        ("adam-adce-worst-toxicity", "ce.csv", "CESEQ", "CESEV", "AOCCFL"),
    ],
)
def test_missing_date_does_not_win_a_severity_tie(
    tmp_path, language, name, file, seq, severity, flag
):
    template = _input(name, file)[0]
    records = [
        {**template, seq: "1", severity: "SEVERE", "ASTDT": ""},
        {**template, seq: "2", severity: "SEVERE", "ASTDT": "2025-01-01"},
    ]
    output = "adae.csv" if seq == "AESEQ" else "adce.csv"
    rows = _run(tmp_path, name, language, {file: records})[output]
    assert next(r for r in rows if r[seq] == "1")[flag] == ""
    assert next(r for r in rows if r[seq] == "2")[flag] == "Y"


def test_missing_censor_date_follows_dated_reason(tmp_path, language):
    name = "adam-adtte-pro-deterioration"
    ds = _input(name, "ds.csv")
    ds.append({**ds[0], "DSDTC": "", "DSSEQ": "0", "DSDECOD": "WITHDRAWAL OF CONSENT"})
    rows = _run(tmp_path, name, language, {"ds.csv": ds})["adtte.csv"]
    row = next(r for r in rows if r["USUBJID"] == ds[0]["USUBJID"])
    assert row["CNSDTDSC"] == "DISEASE PROGRESSION"
    assert row["ADT"] == "2026-03-12"


def test_missing_exposure_date_sorts_last(tmp_path, language):
    name = "sdtm-ex-from-ec"
    odm = _input(name, "odm.csv")
    date = next(r for r in odm if r["ItemOID"] == "IT.EC.ECSTDTC")
    date["Value"] = ""
    rows = _run(tmp_path, name, language, {"odm.csv": odm})["ex.csv"]
    subject = [r for r in rows if r["USUBJID"] == date["SubjectKey"]]
    missing = next(r for r in subject if r["EXSTDTC"] == "")
    assert int(missing["EXSEQ"]) == len(subject)


def test_missing_window_target_sorts_after_known_distance(tmp_path, language):
    name = "adam-advs-window-table"
    raw = _input(name, "advs_raw.csv")[0]
    window = _input(name, "awindow.csv")[2]
    records = [{**raw, "VSSEQ": "1", "ADY": "2"}, {**raw, "VSSEQ": "2", "ADY": "15"}]
    windows = [
        {**window, "AWLO": "2", "AWHI": "2", "AWTARGET": ""},
        {**window, "AWLO": "15", "AWHI": "15", "AWTARGET": "15"},
    ]
    rows = _run(
        tmp_path, name, language, {"advs_raw.csv": records, "awindow.csv": windows}
    )["advs.csv"]
    assert next(r for r in rows if r["VSSEQ"] == "1")["ANL01FL"] == ""
    assert next(r for r in rows if r["VSSEQ"] == "2")["ANL01FL"] == "Y"


@pytest.mark.parametrize("boundary", ["age", "effective_date"])
@pytest.mark.parametrize("reversed_ranges", [False, True])
def test_range_boundary_selects_the_starting_band_and_new_limits(
    tmp_path, language, boundary, reversed_ranges
):
    name = "sdtm-lb-lab-specific-ranges"
    odm = _input(name, "odm.csv")
    first = odm[0]["SubjectKey"]
    odm = [r for r in odm if r["SubjectKey"] == first]
    for r in odm:
        if r["ItemOID"] == "IT.LB.LBDTC":
            r["Value"] = "2025-05-01" if boundary == "age" else "2025-07-01"
    old = _input(name, "lbrange.csv")[0]
    if boundary == "age":
        ranges = [{**old, "AGEHI": "45"}, {**old, "AGELO": "45", "NRHI": "55"}]
    else:
        ranges = [
            {**old, "EFFENDT": "2025-07-01"},
            {**old, "EFFSTDT": "2025-07-01", "EFFENDT": "", "NRHI": "55"},
        ]
    if reversed_ranges:
        ranges.reverse()
    rows = _run(tmp_path, name, language, {"odm.csv": odm, "lbrange.csv": ranges})[
        "lb.csv"
    ]
    assert len(rows) == 1
    assert Decimal(rows[0]["LBSTNRHI"]) == 55


def test_before_exposure_compares_dates_then_sequences(tmp_path, language):
    name = "sdtm-lb-last-observation-before-exposure"
    odm = _input(name, "odm.csv")
    result = next(r for r in odm if r["ItemOID"] == "IT.LB.ALT_SERUM")
    date = next(r for r in odm if r["ItemOID"] == "IT.LB.LBDTC")
    records = []
    for repeat, time in [("1", "23:00"), ("2", "01:00")]:
        base = {"ItemGroupRepeatKey": repeat}
        records.extend(
            [{**result, **base}, {**date, **base, "Value": f"2026-01-10T{time}"}]
        )
    dm = _input(name, "dm.csv")
    for r in dm:
        if r["USUBJID"] == result["SubjectKey"]:
            r["RFSTDTC"] = "2026-01-10T00:00"
    rows = _run(tmp_path, name, language, {"odm.csv": records, "dm.csv": dm})["lb.csv"]
    assert next(r for r in rows if r["LBSEQ"] == "1")["LBLOBXFL"] == ""
    assert next(r for r in rows if r["LBSEQ"] == "2")["LBLOBXFL"] == "Y"


def test_identical_range_starts_keep_the_first_collected_range(tmp_path, language):
    name = "sdtm-lb-lab-specific-ranges"
    odm = _input(name, "odm.csv")
    odm = [r for r in odm if r["SubjectKey"] == odm[0]["SubjectKey"]]
    first = _input(name, "lbrange.csv")[0]
    ranges = [first, {**first, "NRHI": "55"}]
    rows = _run(tmp_path, name, language, {"odm.csv": odm, "lbrange.csv": ranges})[
        "lb.csv"
    ]
    assert len(rows) == 1
    assert Decimal(rows[0]["LBSTNRHI"]) == 40


@pytest.mark.parametrize("consent", ["2025-01", "2025-01-02T09:30"])
def test_informed_consent_text_is_preserved(tmp_path, language, consent):
    name = "sdtm-dm-dates"
    dm = _input(name, "dm_raw.csv")
    dm[0]["RFICDTC"] = consent
    rows = _run(tmp_path, name, language, {"dm_raw.csv": dm})["dm.csv"]
    assert (
        next(r for r in rows if r["USUBJID"] == dm[0]["USUBJID"])["RFICDTC"] == consent
    )


def test_race_keeps_the_subject_key_in_dm_and_suppdm(tmp_path, language):
    name = "sdtm-dm-race-ethnicity"
    odm = _input(name, "odm.csv")
    first = odm[0]["SubjectKey"]
    for r in odm:
        if r["SubjectKey"] == first:
            r["SubjectKey"] = "COLLECTED-SUBJECT"
    output = _run(tmp_path, name, language, {"odm.csv": odm})
    assert any(r["USUBJID"] == "COLLECTED-SUBJECT" for r in output["dm.csv"])
    for r in output["suppdm.csv"]:
        if "COLLECTED-SUBJECT" in r["USUBJID"]:
            assert r["USUBJID"] == r["IDVARVAL"] == "COLLECTED-SUBJECT"


def test_disposition_always_joins_study_and_subject(tmp_path, language):
    name = "sdtm-ds-multi-form-disposition"
    odm = _input(name, "odm.csv")
    first = odm[0]["SubjectKey"]
    study = odm[0]["StudyOID"]
    for r in odm:
        if r["SubjectKey"] == first:
            r["SubjectKey"] = f"{study}-ALREADY-PREFIXED"
    rows = _run(tmp_path, name, language, {"odm.csv": odm})["ds.csv"]
    assert any(r["USUBJID"] == f"{study}-{study}-ALREADY-PREFIXED" for r in rows)


@pytest.mark.parametrize("only_nondemographic", [False, True])
def test_subject_without_dm_items_still_gets_dm(
    tmp_path, language, only_nondemographic
):
    name = "sdtm-dm-basic"
    odm = _input(name, "odm.csv")
    extra = {
        **odm[0],
        "SubjectKey": "NO-DEMOGRAPHICS",
        "ItemGroupOID": "IG.VS",
        "ItemOID": "IT.VS.WEIGHT",
        "Value": "70",
    }
    records = [extra] if only_nondemographic else [*odm, extra]
    rows = _run(tmp_path, name, language, {"odm.csv": records})["dm.csv"]
    subject = next(r for r in rows if r["USUBJID"] == "NO-DEMOGRAPHICS")
    assert subject["SEX"] == "U"
    assert subject["AGE"] == subject["ARM"] == subject["ACTARM"] == ""
    assert subject["ARMNRS"] == "Not assigned to treatment arm"


def test_expected_visit_sequences_follow_all_subject_parameters(tmp_path, language):
    name = "adam-advs-windows"
    vs = _input(name, "vs.csv")
    subject = vs[0]["USUBJID"]
    vs = [
        r
        for r in vs
        if not (
            r["USUBJID"] == subject
            and r["VSTESTCD"] == "SYSBP"
            and r["VSDY"] in {str(day) for day in range(22, 43)}
        )
    ]
    vs.append({**vs[0], "VSTESTCD": "DIABP", "VSSEQ": "99"})
    rows = _run(tmp_path, name, language, {"vs.csv": vs})["advs.csv"]
    expected = [
        r
        for r in rows
        if r["USUBJID"] == subject and r["AVISIT"] == "WEEK 4" and r["ADY"] == ""
    ]
    assert expected
    assert min(int(r["VSSEQ"]) for r in expected) == 100
    assert all(r["ANL01FL"] == "" for r in expected)


def test_prior_result_visit_ties_keep_input_order(tmp_path, language):
    name = "adam-advs-prior-result"
    row = _input(name, "vs.csv")[0]
    records = [
        {**row, "VSSEQ": "9", "AVISITN": "1", "AVALC": "FIRST-COLLECTED"},
        {**row, "VSSEQ": "1", "AVISITN": "1", "AVALC": "SECOND-COLLECTED"},
    ]
    rows = _run(tmp_path, name, language, {"vs.csv": records})["advs.csv"]
    assert next(r for r in rows if r["VSSEQ"] == "9")["PREVAVALC"] == ""
    assert next(r for r in rows if r["VSSEQ"] == "1")["PREVAVALC"] == "FIRST-COLLECTED"


def test_fever_numbers_by_assessment_date_then_sequence(tmp_path, language):
    name = "adam-adfa-fever"
    row = _input(name, "vs.csv")[0]
    records = [
        {**row, "VSSEQ": "1", "VSDTC": "2025-03-02T23:00"},
        {**row, "VSSEQ": "2", "VSDTC": "2025-03-02T01:00"},
    ]
    rows = _run(tmp_path, name, language, {"vs.csv": records})["adfa.csv"]
    assert next(r for r in rows if r["SRCSEQ"] == "1")["ASEQ"] == "1"
    assert next(r for r in rows if r["SRCSEQ"] == "2")["ASEQ"] == "2"


def test_site_parse_accepts_any_prefix_and_exactly_two_dashes(tmp_path, language):
    name = "adam-adsl-site-parse"
    row = _input(name, "dm.csv")[0]
    records = [
        {**row, "USUBJID": "OTHER-SITE-1234", "SITEID": "FALLBACK"},
        {**row, "USUBJID": "OTHER-EXTRA-SITE-1234", "SITEID": "FALLBACK"},
    ]
    rows = _run(tmp_path, name, language, {"dm.csv": records})["adsl.csv"]
    valid = next(r for r in rows if r["USUBJID"] == "OTHER-SITE-1234")
    assert valid["SITEIDP"] == valid["SITEID"] == "SITE"
    invalid = next(r for r in rows if r["USUBJID"] == "OTHER-EXTRA-SITE-1234")
    assert invalid["SITEIDP"] == ""
    assert invalid["SITEID"] == "FALLBACK"
