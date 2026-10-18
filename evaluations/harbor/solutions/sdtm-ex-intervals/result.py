# Reference solution for the yamaa benchmark sdtm-ex-intervals (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the EX dataset (one record per constant-dose interval) from EC_RAW.

Reads /app/input/ec_raw.csv and writes /app/output/ex.csv with columns:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXDOSFRQ,
EXSTDTC, EXENDTC, EXADJ.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def main():
    with open(os.path.join(INPUT_DIR, "ec_raw.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    # Administrations of one subject and treatment, in date order.
    rows.sort(key=lambda r: (r["USUBJID"], r["ECTRT"], r["ECSTDTC"]))

    # Consecutive administrations at one dose, dose unit, and dosing
    # frequency form one interval. A dose interruption (dose 0 with a
    # recorded reason) is its own interval like any other dose level.
    intervals = []
    for r in rows:
        key = (
            r["STUDYID"],
            r["USUBJID"],
            r["ECTRT"],
            r["ECDOSE"],
            r["ECDOSU"],
            r["ECDOSFRQ"],
        )
        if intervals and intervals[-1]["key"] == key:
            iv = intervals[-1]
            iv["end"] = r["ECSTDTC"]
            if not iv["adj"] and r["ECADJ"]:
                iv["adj"] = r["ECADJ"]
        else:
            intervals.append(
                {
                    "key": key,
                    "start": r["ECSTDTC"],
                    "end": r["ECSTDTC"],
                    "adj": r["ECADJ"],
                }
            )

    # EXSEQ numbers each subject's intervals from 1 in the order they
    # started, by start date then treatment.
    by_subject = {}
    for iv in intervals:
        by_subject.setdefault(iv["key"][1], []).append(iv)

    out_rows = []
    for usubjid in sorted(by_subject):
        subject_intervals = sorted(
            by_subject[usubjid], key=lambda iv: (iv["start"], iv["key"][2])
        )
        for seq, iv in enumerate(subject_intervals, start=1):
            studyid, _, trt, dose, dosu, dosfrq = iv["key"]
            out_rows.append(
                {
                    "DOMAIN": "EX",
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "EXSEQ": seq,
                    "EXTRT": trt,
                    "EXDOSE": dose,
                    "EXDOSU": dosu,
                    "EXDOSFRQ": dosfrq,
                    "EXSTDTC": iv["start"],
                    "EXENDTC": iv["end"],
                    "EXADJ": iv["adj"],
                }
            )

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "EXSEQ",
        "EXTRT",
        "EXDOSE",
        "EXDOSU",
        "EXDOSFRQ",
        "EXSTDTC",
        "EXENDTC",
        "EXADJ",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "ex.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
