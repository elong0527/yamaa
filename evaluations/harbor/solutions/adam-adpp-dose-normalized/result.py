# Reference solution for the yamaa benchmark adam-adpp-dose-normalized (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADPP from EX and PP: collected PK parameters plus dose-normalized
records for CMAX, AUCLST, and AUCIFO, normalized by the dose (mg) of the
subject's administration starting at the profile's reference time."""

import csv
import os

INPUT = "/app/input"
OUTPUT = "/app/output"

NORMALIZED = {
    "CMAX": ("CMAXD", "Max Conc Norm by Dose", "ng/mL/mg"),
    "AUCLST": ("AUCLSTD", "AUC to Last Nonzero Conc Norm by Dose", "h*ng/mL/mg"),
    "AUCIFO": ("AUCIFOD", "AUC Infinity Obs Norm by Dose", "h*ng/mL/mg"),
}

COLUMNS = ["STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL", "AVALU"]


def read_csv(name):
    with open(os.path.join(INPUT, name), newline="") as f:
        return list(csv.DictReader(f))


def dose_mg(ex_row):
    dose = float(ex_row["EXDOSE"])
    unit = ex_row["EXDOSU"].strip()
    if unit == "mg":
        return dose
    if unit in ("ug", "\u00b5g", "mcg"):
        return dose / 1000.0
    return dose


def main():
    ex = read_csv("ex.csv")
    pp = read_csv("pp.csv")

    # One administration per subject per start time.
    dose_by_key = {}
    for row in ex:
        dose_by_key[(row["USUBJID"], row["EXSTDTC"])] = dose_mg(row)

    out = []
    for rec in pp:
        base = {
            "STUDYID": rec["STUDYID"],
            "USUBJID": rec["USUBJID"],
            "VISIT": rec["VISIT"],
            "PARAMCD": rec["PPTESTCD"],
            "PARAM": rec["PPTEST"],
            "AVAL": rec["PPSTRESN"],
            "AVALU": rec["PPSTRESU"],
        }
        out.append(base)

        spec = NORMALIZED.get(rec["PPTESTCD"])
        if spec is None or rec["PPSTRESN"] == "":
            continue
        dose = dose_by_key.get((rec["USUBJID"], rec["PPRFTDTC"]))
        if dose is None or dose == 0:
            continue
        paramcd, param, avalu = spec
        aval = float(rec["PPSTRESN"]) / dose
        added = dict(base)
        added["PARAMCD"] = paramcd
        added["PARAM"] = param
        added["AVAL"] = repr(aval)
        added["AVALU"] = avalu
        out.append(added)

    os.makedirs(OUTPUT, exist_ok=True)
    with open(os.path.join(OUTPUT, "adpp.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
