# Reference solution for the yamaa benchmark adam-adpp-geometric-mean-ratio (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import math
import statistics

PP_CSV = "/app/input/pp.csv"
PC_CSV = "/app/input/pc.csv"
OUTPUT_CSV = "/app/output/adpp.csv"


def read(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def number(text):
    text = text.strip()
    return float(text) if text else None


def cell(value):
    return "" if value is None else repr(value)


def main():
    pp = read(PP_CSV)
    pc = read(PC_CSV)
    subjects = []
    for row in pp + pc:
        key = (row["STUDYID"], row["USUBJID"])
        if key not in subjects:
            subjects.append(key)

    out_rows = []
    for study, subject in subjects:
        ratios = [
            number(r["PPSTRESN"])
            for r in pp
            if (r["STUDYID"], r["USUBJID"]) == (study, subject)
            and r["PPTESTCD"] == "MRCMAX"
        ]
        ratios = [x for x in ratios if x is not None and x > 0]
        geomean = (
            math.exp(sum(math.log(x) for x in ratios) / len(ratios)) if ratios else None
        )
        out_rows.append(
            {
                "STUDYID": study,
                "USUBJID": subject,
                "PARAMCD": "MRCMAXGM",
                "PARAM": "Geometric Mean Metabolite Ratio Cmax",
                "AVAL": cell(geomean),
                "AVALU": "RATIO",
            }
        )
        replicates = [
            number(r["PCSTRESN"])
            for r in pc
            if (r["STUDYID"], r["USUBJID"]) == (study, subject) and r["PCTPT"] == "2H"
        ]
        replicates = [x for x in replicates if x is not None]
        sd = statistics.stdev(replicates) if len(replicates) >= 2 else None
        out_rows.append(
            {
                "STUDYID": study,
                "USUBJID": subject,
                "PARAMCD": "C2HREPSD",
                "PARAM": "Replicate SD of 2 H Concentration",
                "AVAL": cell(sd),
                "AVALU": "ng/mL",
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "AVALU"]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
