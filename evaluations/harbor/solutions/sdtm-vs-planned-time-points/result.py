# Reference solution for the yamaa benchmark sdtm-vs-planned-time-points (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM VS dataset from DM, ODM, and VS_MAPPING.

One record per collected vital-signs result. Each vital-signs form fixes
the planned time point of every result on it; the form's datetime item
gives the actual collection datetime for all results on that form.
"""

import csv
from datetime import date, datetime
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

# Form OID -> (VSTPT, VSTPTNUM, VSELTM)
FORM_TIMEPOINTS = {
    "FO.VS_PREDOSE": ("PRE-DOSE", 1, "-PT15M"),
    "FO.VS_30MIN": ("30 MIN POST-DOSE", 2, "PT30M"),
    "FO.VS_1H": ("1 H POST-DOSE", 3, "PT1H"),
    "FO.VS_4H": ("4 H POST-DOSE", 4, "PT4H"),
}

# Test order within a time point for VSSEQ.
TEST_ORDER = {"PULSE": 0, "SYSBP": 1, "DIABP": 2}

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VISIT", "VSTPT",
    "VSTPTNUM", "VSELTM", "VSTESTCD", "VSORRES", "VSORRESU",
    "VSSTRESN", "VSSTRESU", "VSDTC", "VSDY",
]


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def study_day(collection, reference):
    """SDTM study day: reference day is day 1, no day zero."""
    if not collection or not reference:
        return ""
    delta = (collection - reference).days
    return delta + 1 if delta >= 0 else delta


def numeric_text(value):
    number = float(value)
    return str(int(number)) if number == int(number) else str(number)


def main():
    dm_rows = read_csv(INPUT / "dm.csv")
    odm_rows = read_csv(INPUT / "odm.csv")
    mapping_rows = read_csv(INPUT / "vs_mapping.csv")

    reference_start = {}
    for row in dm_rows:
        value = (row.get("RFSTDTC") or "").strip()
        reference_start[row["USUBJID"]] = (
            datetime.strptime(value[:10], "%Y-%m-%d").date() if value else None
        )

    dictionary = {
        row["ItemOID"]: (row["VSTESTCD"], row["UNIT"]) for row in mapping_rows
    }

    # Group ODM item rows by subject and form occurrence.
    forms = {}
    for row in odm_rows:
        form_oid = row["FormOID"]
        if form_oid not in FORM_TIMEPOINTS:
            continue
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            row["StudyEventOID"],
            row["StudyEventRepeatKey"],
            form_oid,
            row["FormRepeatKey"],
            row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        forms.setdefault(key, {})[row["ItemOID"]] = row["Value"]

    records = []
    for key, items in forms.items():
        study_id, subject = key[0], key[1]
        vstpt, vstptnum, vseltm = FORM_TIMEPOINTS[key[4]]
        vsdtc = (items.get("IT.VS.VSDTC") or "").strip()
        collection_date = (
            datetime.strptime(vsdtc[:10], "%Y-%m-%d").date() if vsdtc else None
        )
        vsdy = study_day(collection_date, reference_start.get(subject))
        for item_oid, (testcd, unit) in dictionary.items():
            if item_oid not in items:
                continue
            value = items[item_oid]
            records.append(
                {
                    "DOMAIN": "VS",
                    "STUDYID": study_id,
                    "USUBJID": subject,
                    "VISIT": "DAY 1",
                    "VSTPT": vstpt,
                    "VSTPTNUM": vstptnum,
                    "VSELTM": vseltm,
                    "VSTESTCD": testcd,
                    "VSORRES": value,
                    "VSORRESU": unit,
                    "VSSTRESN": numeric_text(value),
                    "VSSTRESU": unit,
                    "VSDTC": vsdtc,
                    "VSDY": vsdy,
                    "_test_order": TEST_ORDER[testcd],
                }
            )

    records.sort(
        key=lambda r: (r["USUBJID"], r["VSTPTNUM"], r["_test_order"])
    )
    counters = {}
    for record in records:
        subject = record["USUBJID"]
        counters[subject] = counters.get(subject, 0) + 1
        record["VSSEQ"] = counters[subject]
        del record["_test_order"]

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "vs.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
