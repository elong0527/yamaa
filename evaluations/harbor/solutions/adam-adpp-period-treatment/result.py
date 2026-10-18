# Reference solution for the yamaa benchmark adam-adpp-period-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os
import re

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

OUT_FIELDS = [
    "STUDYID", "USUBJID", "PARCAT1", "PARAMCD", "PARAM", "PPRFTDTC",
    "APERIOD", "TRTA", "DOSEA", "DOSEU", "AVAL", "AVALU",
]


def main():
    with open(os.path.join(INPUT_DIR, "ex.csv"), newline="") as f:
        ex_rows = list(csv.DictReader(f))
    with open(os.path.join(INPUT_DIR, "pp.csv"), newline="") as f:
        pp_rows = list(csv.DictReader(f))

    # Index each subject's doses by their exact start date and time.
    doses = {}
    for ex in ex_rows:
        key = (ex["STUDYID"], ex["USUBJID"], ex["EXSTDTC"])
        doses[key] = ex

    out_rows = []
    for pp in pp_rows:
        ex = doses.get((pp["STUDYID"], pp["USUBJID"], pp["PPRFTDTC"]))
        if ex is not None:
            match = re.search(r"\d+", ex["EPOCH"])
            aperiod = match.group(0) if match else ""
            trta = ex["EXTRT"]
            dosea = ex["EXDOSE"]
            doseu = ex["EXDOSU"]
        else:
            aperiod = trta = dosea = doseu = ""
        out_rows.append({
            "STUDYID": pp["STUDYID"],
            "USUBJID": pp["USUBJID"],
            "PARCAT1": pp["PPCAT"],
            "PARAMCD": pp["PPTESTCD"],
            "PARAM": pp["PPTEST"],
            "PPRFTDTC": pp["PPRFTDTC"],
            "APERIOD": aperiod,
            "TRTA": trta,
            "DOSEA": dosea,
            "DOSEU": doseu,
            "AVAL": pp["PPSTRESN"],
            "AVALU": pp["PPSTRESU"],
        })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adpp.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=OUT_FIELDS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
