# Reference solution for the yamaa benchmark adam-adex-cumulative-dose (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEX: one record per subject per treatment, with cumulative
dose, number of cycles, and relative dose intensity."""

import csv
from collections import defaultdict
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

COLUMNS = ["STUDYID", "USUBJID", "EXTRT", "EXDOSU", "DOSECUM", "NCYCLES", "RDI"]


def fmt(value):
    """Write a number without a trailing .0 when it is whole."""
    if value is None:
        return ""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(INPUT / "subject_treatment.csv", newline="") as f:
        treatments = list(csv.DictReader(f))
    with open(INPUT / "ex.csv", newline="") as f:
        exposure = list(csv.DictReader(f))

    doses = defaultdict(list)
    for row in exposure:
        key = (row["STUDYID"], row["USUBJID"], row["EXTRT"])
        doses[key].append(row["EXDOSE"])

    out_rows = []
    for trt in treatments:
        key = (trt["STUDYID"], trt["USUBJID"], trt["EXTRT"])
        records = doses.get(key)
        if records:
            ncycles = len(records)
            dosecum = sum(float(d) for d in records if d.strip() != "")
        else:
            ncycles = None
            dosecum = None
        planned_total = float(trt["PLANDOSE"]) * float(trt["PLANCYC"])
        if dosecum is None or planned_total == 0:
            rdi = None
        else:
            rdi = dosecum / planned_total * 100
        out_rows.append(
            {
                "STUDYID": trt["STUDYID"],
                "USUBJID": trt["USUBJID"],
                "EXTRT": trt["EXTRT"],
                "EXDOSU": trt["EXDOSU"],
                "DOSECUM": fmt(dosecum),
                "NCYCLES": "" if ncycles is None else str(ncycles),
                "RDI": fmt(rdi),
            }
        )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "adex.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
