# Reference solution for the yamaa benchmark sdtm-vs-replicates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import re
from collections import OrderedDict

INPUT = "/app/input"
OUTPUT = "/app/output"

TESTS = {
    "SYSBP": "Systolic Blood Pressure",
    "DIABP": "Diastolic Blood Pressure",
}

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VISIT", "VSTESTCD", "VSTEST",
    "VSREPNUM", "VSORRES", "VSORRESU", "VSSTRESN", "VSSTRESC", "VSDRVFL",
]


def fmt(value):
    """Write a number as text, dropping trailing zeros (82.50 -> 82.5)."""
    if value == int(value):
        return str(int(value))
    return repr(value)


def visit_name(event_oid):
    name = event_oid.split(".", 1)[1] if "." in event_oid else event_oid
    m = re.match(r"^(.*?)(\d+)$", name)
    if m:
        return m.group(1) + " " + m.group(2)
    return name


def main():
    with open(INPUT + "/odm.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    studyid = rows[0]["StudyOID"]

    # readings[(subject, visit, testcd)] = [(repnum, value), ...]
    readings = OrderedDict()
    subjects = []
    for row in rows:
        subject = row["SubjectKey"]
        if subject not in subjects:
            subjects.append(subject)
        if row["ItemGroupOID"] != "IG.VS":
            continue
        testcd = row["ItemOID"].rsplit(".", 1)[-1]
        if testcd not in TESTS:
            continue
        value = (row["Value"] or "").strip()
        if value == "":
            continue  # reading not taken: no reading record
        key = (subject, visit_name(row["StudyEventOID"]), testcd)
        readings.setdefault(key, []).append(
            (int(row["ItemGroupRepeatKey"]), float(value))
        )

    out = []
    for subject in subjects:
        records = []
        keys = [k for k in readings if k[0] == subject]
        for key in sorted(keys, key=lambda k: (k[1], k[2])):
            _, visit, testcd = key
            reps = sorted(readings[key])
            for repnum, value in reps:
                records.append({
                    "DOMAIN": "VS",
                    "STUDYID": studyid,
                    "USUBJID": subject,
                    "VISIT": visit,
                    "VSTESTCD": testcd,
                    "VSTEST": TESTS[testcd],
                    "VSREPNUM": str(repnum),
                    "VSORRES": fmt(value),
                    "VSORRESU": "mmHg",
                    "VSSTRESN": fmt(value),
                    "VSSTRESC": fmt(value),
                    "VSDRVFL": "",
                })
            if reps:
                mean = sum(v for _, v in reps) / len(reps)
                records.append({
                    "DOMAIN": "VS",
                    "STUDYID": studyid,
                    "USUBJID": subject,
                    "VISIT": visit,
                    "VSTESTCD": testcd,
                    "VSTEST": TESTS[testcd],
                    "VSREPNUM": "",
                    "VSORRES": fmt(mean),
                    "VSORRESU": "mmHg",
                    "VSSTRESN": fmt(mean),
                    "VSSTRESC": fmt(mean),
                    "VSDRVFL": "Y",
                })
        for i, rec in enumerate(records, start=1):
            rec["VSSEQ"] = str(i)
            out.append(rec)

    with open(OUTPUT + "/vs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
