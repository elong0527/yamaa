# Reference solution for the yamaa benchmark sdtm-lb-ranges (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from DM, LBRANGE, and LB_RAW.

One record per collected test result in LB_RAW. The reference range
(unit and limits) is selected by the record's test code together with
the subject's sex from DM. LBNRIND flags the result against the limits.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_rows(name):
    with open(os.path.join(INPUT_DIR, name), newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    dm = read_rows("dm.csv")
    lbrange = read_rows("lbrange.csv")
    lb_raw = read_rows("lb_raw.csv")

    sex_by_subject = {row["USUBJID"]: row["SEX"] for row in dm}
    range_by_key = {
        (row["LBTESTCD"], row["SEX"]): row for row in lbrange
    }

    records = []
    for row in lb_raw:
        sex = sex_by_subject.get(row["USUBJID"], "")
        entry = range_by_key.get((row["LBTESTCD"], sex))

        if entry is None:
            unit = ""
            nrlo = ""
            nrhi = ""
        else:
            unit = entry["UNIT"]
            nrlo = entry["NRLO"]
            nrhi = entry["NRHI"]

        result = row["LBSTRESN"]
        nrind = ""
        if result != "" and nrlo != "" and nrhi != "":
            value = float(result)
            if value < float(nrlo):
                nrind = "LOW"
            elif value > float(nrhi):
                nrind = "HIGH"
            else:
                nrind = "NORMAL"

        records.append(
            {
                "DOMAIN": "LB",
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "LBSEQ": row["LBSEQ"],
                "LBTESTCD": row["LBTESTCD"],
                "LBSTRESN": result,
                "LBSTRESU": unit,
                "LBSTNRLO": nrlo,
                "LBSTNRHI": nrhi,
                "LBNRIND": nrind,
            }
        )

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBSTRESN",
        "LBSTRESU",
        "LBSTNRLO",
        "LBSTNRHI",
        "LBNRIND",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "lb.csv"), "w", newline=""
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
