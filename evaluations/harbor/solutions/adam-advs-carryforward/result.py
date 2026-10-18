# Reference solution for the yamaa benchmark adam-advs-carryforward (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS with one record per planned measurement.

Reads ADSL, PLAN and VS from /app/input and writes /app/output/advs.csv.
A collected VS record belongs to a planned measurement when study,
subject, test code (= planned parameter code) and collection date
(= ADT) all match. Uncollected planned measurements carry forward the
most recent earlier collected value for the same subject and parameter
(LOCF). HEIGHTBL is the latest height collected on or before the
subject's treatment start date.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    with open(os.path.join(INPUT_DIR, name), newline="") as f:
        return list(csv.DictReader(f))


def main():
    adsl = read_csv("adsl.csv")
    plan = read_csv("plan.csv")
    vs = read_csv("vs.csv")

    trtsdt = {row["USUBJID"]: row["TRTSDT"] for row in adsl}

    # Collected records keyed by (study, subject, test code).
    collected = {}
    for row in vs:
        key = (row["STUDYID"], row["USUBJID"], row["VSTESTCD"])
        collected.setdefault(key, []).append(row)
    for rows in collected.values():
        rows.sort(key=lambda r: (r["VSDTC"], int(r["VSSEQ"])))

    def heightbl(usubjid):
        start = trtsdt.get(usubjid, "")
        best = None
        for row in vs:
            if (
                row["USUBJID"] == usubjid
                and row["VSTESTCD"] == "HEIGHT"
                and row["VSDTC"] <= start
            ):
                if best is None or (row["VSDTC"], int(row["VSSEQ"])) > (
                    best["VSDTC"],
                    int(best["VSSEQ"]),
                ):
                    best = row
        return best["VSSTRESN"] if best is not None else ""

    out_rows = []
    for p in plan:
        key = (p["STUDYID"], p["USUBJID"], p["PARAMCD"])
        records = collected.get(key, [])
        match = None
        earlier = None
        for row in records:
            if row["VSDTC"] == p["ADT"]:
                match = row
            elif row["VSDTC"] < p["ADT"]:
                earlier = row
        if match is not None:
            vsseq = match["VSSEQ"]
            aval = match["VSSTRESN"]
        else:
            vsseq = ""
            aval = earlier["VSSTRESN"] if earlier is not None else ""
        out_rows.append(
            {
                "STUDYID": p["STUDYID"],
                "USUBJID": p["USUBJID"],
                "ASEQ": p["ASEQ"],
                "VSSEQ": vsseq,
                "PARAMCD": p["PARAMCD"],
                "ADT": p["ADT"],
                "AVAL": aval,
                "TRTSDT": trtsdt.get(p["USUBJID"], ""),
                "HEIGHTBL": heightbl(p["USUBJID"]),
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = [
        "STUDYID",
        "USUBJID",
        "ASEQ",
        "VSSEQ",
        "PARAMCD",
        "ADT",
        "AVAL",
        "TRTSDT",
        "HEIGHTBL",
    ]
    with open(os.path.join(OUTPUT_DIR, "advs.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
