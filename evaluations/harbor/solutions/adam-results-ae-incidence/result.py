# Reference solution for the yamaa benchmark adam-results-ae-incidence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the RESULTS dataset (AE incidence) from ADAE and ADSL.

One record per planned treatment group and operation (COUNT, PERCENT),
covering only safety subjects (SAFFL = 'Y' in ADSL).  A qualifying event
is an adverse event with TRTEMFL = 'Y'; a subject with at least one
qualifying event counts once.
"""

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def main():
    adsl = read_csv(f"{INPUT_DIR}/adsl.csv")
    adae = read_csv(f"{INPUT_DIR}/adae.csv")

    studyid = adsl[0]["STUDYID"] if adsl else ""

    # Planned treatment groups, ordered by the planned treatment code
    # (TRT01PN); GROUPLEVEL is the planned treatment name (TRT01P).
    groups = OrderedDict()  # TRT01PN -> {"name": ..., "subjects": set}
    safety_subjects = set()
    for row in adsl:
        if row["SAFFL"] != "Y":
            continue
        safety_subjects.add(row["USUBJID"])
        code = row["TRT01PN"]
        group = groups.setdefault(code, {"name": row["TRT01P"], "subjects": set()})
        group["subjects"].add(row["USUBJID"])

    # Safety subjects with at least one qualifying (TRTEMFL = 'Y') event.
    subjects_with_event = set()
    for row in adae:
        if row["TRTEMFL"] == "Y" and row["USUBJID"] in safety_subjects:
            subjects_with_event.add(row["USUBJID"])

    columns = [
        "STUDYID", "ANALYSISID", "ANALYSISSETID", "DATASET", "VARIABLE",
        "ANALYSISSET", "DATASUBSET", "GROUPINGID", "GROUPORDER",
        "GROUPLEVEL", "METHODID", "OPERATIONID", "RESULT",
    ]

    out_rows = []
    for code in sorted(groups, key=lambda c: (int(c) if c.lstrip("-").isdigit() else c, c)):
        group = groups[code]
        total = len(group["subjects"])
        count = len(group["subjects"] & subjects_with_event)
        percent = count / total * 100 if total else 0.0
        base = {
            "STUDYID": studyid,
            "ANALYSISID": "AEINC",
            "ANALYSISSETID": "SAFETY",
            "DATASET": "ADAE",
            "VARIABLE": "USUBJID",
            "ANALYSISSET": "SAFFL = 'Y'",
            "DATASUBSET": "TRTEMFL = 'Y'",
            "GROUPINGID": "TRT01P",
            "GROUPORDER": code,
            "GROUPLEVEL": group["name"],
            "METHODID": "AEINCIDENCE",
        }
        count_row = dict(base, OPERATIONID="COUNT", RESULT=str(count))
        percent_row = dict(base, OPERATIONID="PERCENT", RESULT=repr(percent))
        out_rows.extend([count_row, percent_row])

    with open(f"{OUTPUT_DIR}/results.csv", "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)

    print(f"Wrote {len(out_rows)} records to {OUTPUT_DIR}/results.csv")


if __name__ == "__main__":
    main()
