# Reference solution for the yamaa benchmark sdtm-vs-planned-time-points (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date, datetime
from pathlib import Path

import polars as pl


def parse_date(text: str | None) -> date | None:
    if text is None or text == "":
        return None
    try:
        return datetime.fromisoformat(text).date()
    except ValueError:
        try:
            return date.fromisoformat(text[:10])
        except ValueError:
            return None


def study_day(ref: date | None, coll: date | None) -> int | None:
    if ref is None or coll is None:
        return None
    delta = (coll - ref).days
    return delta + 1 if delta >= 0 else delta


def time_point(form: str) -> tuple[str, int, str] | None:
    # The planned time point for each collection form, in schedule order.
    name = form.upper()
    if "PREDOSE" in name:
        return ("PRE-DOSE", 1, "-PT15M")
    if "30MIN" in name:
        return ("30 MIN POST-DOSE", 2, "PT30M")
    if "1H" in name:
        return ("1 H POST-DOSE", 3, "PT1H")
    if "4H" in name:
        return ("4 H POST-DOSE", 4, "PT4H")
    return None


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
mapping = pl.read_csv("/app/input/vs_mapping.csv", infer_schema=False)

ref_map = {row["USUBJID"]: parse_date(row["RFSTDTC"]) for row in dm.to_dicts()}
test_map = {
    row["ItemOID"]: (row["VSTESTCD"], row["UNIT"]) for row in mapping.to_dicts()
}
# Within each time point: pulse, systolic, then diastolic.
test_order = {"PULSE": 1, "SYSBP": 2, "DIABP": 3}

# One form per subject per planned time point: its datetime and results.
forms: dict[tuple[str, str], dict] = {}
for row in odm.to_dicts():
    if row["StudyEventOID"] != "SE.D1":
        continue
    if not row["FormOID"].startswith("FO.VS_"):
        continue
    key = (row["SubjectKey"], row["FormOID"])
    entry = forms.setdefault(
        key,
        {
            "subject": row["SubjectKey"],
            "study": row["StudyOID"],
            "form": row["FormOID"],
            "dt": None,
            "results": {},
        },
    )
    if row["ItemOID"] == "IT.VS.VSDTC":
        entry["dt"] = row["Value"]
    elif row["ItemOID"] in test_map:
        entry["results"][row["ItemOID"]] = row["Value"]

records = []
for entry in forms.values():
    point = time_point(entry["form"])
    if point is None or entry["dt"] in (None, ""):
        continue
    vstpt, vstptnum, vseltm = point
    coll = parse_date(entry["dt"])
    vsdy = study_day(ref_map.get(entry["subject"]), coll)
    for item, value in entry["results"].items():
        if value is None or value == "":
            continue
        testcd, unit = test_map[item]
        records.append(
            {
                "DOMAIN": "VS",
                "STUDYID": entry["study"],
                "USUBJID": entry["subject"],
                "VISIT": "DAY 1",
                "VSTPT": vstpt,
                "VSTPTNUM": vstptnum,
                "VSELTM": vseltm,
                "VSTESTCD": testcd,
                "_tord": test_order.get(testcd, 9),
                "VSORRES": value,
                "VSORRESU": unit,
                "VSSTRESN": float(value),
                "VSSTRESU": unit,
                "VSDTC": entry["dt"],
                "VSDY": vsdy,
            }
        )

vs = (
    pl.DataFrame(records)
    .sort(["USUBJID", "VSTPTNUM", "_tord"])
    .with_columns(
        VSSEQ=pl.int_range(1, pl.len() + 1).over("USUBJID"),
    )
    .sort(["USUBJID", "VSSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "VSSEQ",
        "VISIT",
        "VSTPT",
        "VSTPTNUM",
        "VSELTM",
        "VSTESTCD",
        "VSORRES",
        "VSORRESU",
        "VSSTRESN",
        "VSSTRESU",
        "VSDTC",
        "VSDY",
    )
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
