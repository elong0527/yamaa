# Reference solution for the yamaa benchmark adam-adqs-missed-visit-locf (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

SCHEDULED = [("Week 8", 8), ("Week 16", 16), ("Week 24", 24)]


def main():
    with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
        adsl = list(csv.DictReader(f))
    efffl = {(r["STUDYID"], r["USUBJID"]): r["EFFFL"] for r in adsl}

    with open(f"{INPUT_DIR}/qs.csv", newline="") as f:
        qs = list(csv.DictReader(f))

    # Every QS record is kept, an empty score included.
    rows = []
    for r in qs:
        rows.append(
            {
                "STUDYID": r["STUDYID"],
                "USUBJID": r["USUBJID"],
                "PARAMCD": r["QSTESTCD"],
                "AVISIT": r["VISIT"],
                "AVISITN": r["VISITNUM"],
                "AVAL": r["QSSTRESN"],
                "DTYPE": "",
                "QSSEQ": r["QSSEQ"],
                "EFFFL": efffl.get((r["STUDYID"], r["USUBJID"]), ""),
            }
        )

    # Scores by subject and visit number. A score of zero counts.
    scored = {}
    for r in qs:
        if r["QSSTRESN"].strip() != "":
            key = (r["STUDYID"], r["USUBJID"])
            scored.setdefault(key, {})[int(r["VISITNUM"])] = r

    # A carried-forward record for each missed scheduled visit of an
    # efficacy subject, from the closest earlier post-baseline score.
    for (studyid, usubjid), flag in efffl.items():
        if flag != "Y":
            continue
        visits = scored.get((studyid, usubjid), {})
        for avisit, avisitn in SCHEDULED:
            if avisitn in visits:
                continue
            earlier = [n for n in visits if 0 < n < avisitn]
            if not earlier:
                continue
            source = visits[max(earlier)]
            rows.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "PARAMCD": "ACTOT",
                    "AVISIT": avisit,
                    "AVISITN": str(avisitn),
                    "AVAL": source["QSSTRESN"],
                    "DTYPE": "LOCF",
                    "QSSEQ": source["QSSEQ"],
                    "EFFFL": flag,
                }
            )

    rows.sort(key=lambda r: (r["USUBJID"], int(r["AVISITN"]), r["DTYPE"]))

    columns = [
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "AVISIT",
        "AVISITN",
        "AVAL",
        "DTYPE",
        "QSSEQ",
        "EFFFL",
    ]
    with open(f"{OUTPUT_DIR}/adqs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
