# Reference solution for the yamaa benchmark adam-advs-locf-record (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the ADVS LOCF dataset from PLAN and VS.

One output record per planned visit (per subject and parameter). For each
planned visit, AVAL is the latest non-missing result selected for analysis
(ANL01FL = "Y") at or before the planned visit within the same subject and
parameter; the latest observation is the one with the highest visit number,
then the highest sequence number within that visit. ADT and QSSEQ are the
date and source sequence number of that same observation. A result of zero
counts as a collected result. With no such observation, AVAL, ADT, and
QSSEQ are blank.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    plan = read_rows(os.path.join(INPUT_DIR, "plan.csv"))
    vs = read_rows(os.path.join(INPUT_DIR, "vs.csv"))

    # Candidate observations: selected for analysis with a collected result.
    candidates = {}
    for row in vs:
        if row["ANL01FL"] != "Y":
            continue
        if row["AVAL"] is None or row["AVAL"].strip() == "":
            continue
        key = (row["USUBJID"], row["PARAMCD"])
        candidates.setdefault(key, []).append(row)

    out_rows = []
    for planned in plan:
        key = (planned["USUBJID"], planned["PARAMCD"])
        planned_visit = int(planned["AVISITN"])
        eligible = [
            row
            for row in candidates.get(key, [])
            if int(row["AVISITN"]) <= planned_visit
        ]
        aval = adt = qsseq = ""
        if eligible:
            latest = max(
                eligible,
                key=lambda row: (int(row["AVISITN"]), int(row["QSSEQ"])),
            )
            aval = latest["AVAL"]
            adt = latest["ADT"]
            qsseq = latest["QSSEQ"]
        out_rows.append(
            {
                "USUBJID": planned["USUBJID"],
                "PARAMCD": planned["PARAMCD"],
                "AVISITN": planned["AVISITN"],
                "AVAL": aval,
                "ADT": adt,
                "QSSEQ": qsseq,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["USUBJID", "PARAMCD", "AVISITN", "AVAL", "ADT", "QSSEQ"]
    with open(
        os.path.join(OUTPUT_DIR, "advs.csv"), "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
