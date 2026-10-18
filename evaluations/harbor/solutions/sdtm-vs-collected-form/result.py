# Reference solution for the yamaa benchmark sdtm-vs-collected-form (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM VS dataset from the collected vital signs form (ODM)
and the VS_TESTS test list.

One record per test per visit form. Reads /app/input, writes
/app/output/vs.csv.
"""

import csv
import os
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VISIT", "VSTESTCD",
    "VSTEST", "VSORRES", "VSORRESU", "VSSTRESN", "VSSTRESC", "VSSTRESU",
    "VSPOS", "VSMETHOD", "VSSTAT", "VSREASND", "VSDTC",
]

BP_TESTS = {"SYSBP", "DIABP"}
POSITION_TESTS = {"SYSBP", "DIABP", "PULSE"}
METHOD_TESTS = {"TEMP"}


def num_text(value):
    """Write a collected numeric result as text: integers without a
    decimal point, other values in their plain decimal form."""
    number = float(value)
    if number == int(number):
        return str(int(number))
    return repr(number)


def main():
    with open(os.path.join(INPUT_DIR, "vs_tests.csv"), newline="") as fh:
        tests = sorted(
            csv.DictReader(fh), key=lambda row: int(row["TESTORD"])
        )

    with open(os.path.join(INPUT_DIR, "odm.csv"), newline="") as fh:
        odm_rows = list(csv.DictReader(fh))

    # Group the form items by subject and visit form occurrence,
    # keeping first-seen order.
    forms = OrderedDict()
    for row in odm_rows:
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            row["StudyEventOID"],
            row["StudyEventRepeatKey"],
            row["FormOID"],
            row["FormRepeatKey"],
            row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        form = forms.setdefault(key, {})
        item = row["ItemOID"].rsplit(".", 1)[-1]
        form[item] = row["Value"]

    # Collect one entry per subject: list of (visit date, test records).
    subjects = OrderedDict()
    for (studyid, subjid, visit, *_rest), form in forms.items():
        vsdtc = form.get("VSDTC", "")
        position = form.get("POSITION", "")
        method = form.get("TEMPMETHOD", "")
        bp_not_done = form.get("BPNOTDONE", "") == "Y"
        bp_reason = form.get("BPREASND", "") if bp_not_done else ""

        records = []
        for test in tests:
            code = test["VSTESTCD"]
            result = form.get(code, "")
            unit = form.get(code + "U", "") if result else ""
            if result:
                stresn = num_text(result)
                stresc = stresn
            else:
                stresn = ""
                stresc = ""
            records.append({
                "DOMAIN": "VS",
                "STUDYID": studyid,
                "USUBJID": subjid,
                "VISIT": visit,
                "VSTESTCD": code,
                "VSTEST": test["VSTEST"],
                "VSORRES": result,
                "VSORRESU": unit,
                "VSSTRESN": stresn,
                "VSSTRESC": stresc,
                "VSSTRESU": unit,
                "VSPOS": position
                if (code in POSITION_TESTS and result) else "",
                "VSMETHOD": method
                if (code in METHOD_TESTS and result) else "",
                "VSSTAT": "NOT DONE"
                if (code in BP_TESTS and bp_not_done) else "",
                "VSREASND": bp_reason if code in BP_TESTS else "",
                "VSDTC": vsdtc,
            })
        subjects.setdefault(subjid, []).append((vsdtc, records))

    out_rows = []
    for subjid, visits in subjects.items():
        visits.sort(key=lambda entry: entry[0])
        seq = 0
        for _vsdtc, records in visits:
            for record in records:
                seq += 1
                record["VSSEQ"] = seq
                out_rows.append(record)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "vs.csv"), "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=COLUMNS)
        writer.writeheader()
        for record in out_rows:
            writer.writerow({col: record[col] for col in COLUMNS})


if __name__ == "__main__":
    main()
