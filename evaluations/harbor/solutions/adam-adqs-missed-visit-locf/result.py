# Reference solution for the yamaa benchmark adam-adqs-missed-visit-locf (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import defaultdict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

SCHEDULED = [("Week 8", 8), ("Week 16", 16), ("Week 24", 24)]


def main():
    with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
        adsl = list(csv.DictReader(f))
    efffl = {(r["STUDYID"], r["USUBJID"]): r["EFFFL"] for r in adsl}

    with open(f"{INPUT_DIR}/qs.csv", newline="") as f:
        qs = list(csv.DictReader(f))

    # Collected records: a QS record with a non-empty score. A score of
    # zero counts as a score.
    collected = [r for r in qs if r["AVAL"].strip() != ""]

    # Scores by subject and analysis visit number, for LOCF lookup.
    scores = defaultdict(dict)
    for r in collected:
        scores[(r["STUDYID"], r["USUBJID"])][int(r["AVISITN"])] = r["AVAL"]

    rows = []
    for r in collected:
        rows.append(
            {
                "STUDYID": r["STUDYID"],
                "USUBJID": r["USUBJID"],
                "PARAMCD": r["PARAMCD"],
                "AVISIT": r["AVISIT"],
                "AVISITN": r["AVISITN"],
                "AVAL": r["AVAL"],
                "DTYPE": "",
                "EFFFL": efffl.get((r["STUDYID"], r["USUBJID"]), ""),
            }
        )

    # Added LOCF records for missed scheduled visits, efficacy
    # population only.
    for (studyid, usubjid), flag in efffl.items():
        if flag != "Y":
            continue
        subject_scores = scores.get((studyid, usubjid), {})
        for avisit, avisitn in SCHEDULED:
            if avisitn in subject_scores:
                continue
            earlier = [n for n in subject_scores if n < avisitn]
            aval = subject_scores[max(earlier)] if earlier else ""
            rows.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "PARAMCD": "ACTOT",
                    "AVISIT": avisit,
                    "AVISITN": str(avisitn),
                    "AVAL": aval,
                    "DTYPE": "LOCF",
                    "EFFFL": flag,
                }
            )

    rows.sort(key=lambda r: (r["USUBJID"], int(r["AVISITN"])))

    columns = [
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "AVISIT",
        "AVISITN",
        "AVAL",
        "DTYPE",
        "EFFFL",
    ]
    with open(f"{OUTPUT_DIR}/adqs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
