# Reference solution for the yamaa benchmark adam-adlbc-window-chain (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def fmt(value):
    if value is None:
        return ""
    return repr(float(value))


def main():
    with open(f"{INPUT_DIR}/lb.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    # Keep only albumin records with a numeric result.
    records = []
    for row in rows:
        if row["LBTESTCD"] != "ALB":
            continue
        raw = (row["LBSTRESN"] or "").strip()
        if raw == "":
            continue
        try:
            aval = float(raw)
        except ValueError:
            continue
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AVISITN": int(row["VISITNUM"]),
                "AVAL": aval,
            }
        )

    # Order by subject (first appearance) then visit number.
    subject_order = {}
    for rec in records:
        if rec["USUBJID"] not in subject_order:
            subject_order[rec["USUBJID"]] = len(subject_order)
    records.sort(key=lambda r: (subject_order[r["USUBJID"]], r["AVISITN"]))

    out = []
    prev_aval = {}
    prev_chg = {}
    for rec in records:
        subj = rec["USUBJID"]
        p_aval = prev_aval.get(subj)
        chg = None if p_aval is None else rec["AVAL"] - p_aval
        p_chg = prev_chg.get(subj)
        out.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": subj,
                "PARAMCD": "_ALB",
                "AVISITN": rec["AVISITN"],
                "AVAL": rec["AVAL"],
                "PREV_AVAL": p_aval,
                "CHG": chg,
                "PREV2": p_chg,
            }
        )
        prev_aval[subj] = rec["AVAL"]
        prev_chg[subj] = chg

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "AVISITN", "AVAL",
                  "PREV_AVAL", "CHG", "PREV2"]
    with open(f"{OUTPUT_DIR}/adlbc.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in out:
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "PARAMCD": row["PARAMCD"],
                    "AVISITN": row["AVISITN"],
                    "AVAL": fmt(row["AVAL"]),
                    "PREV_AVAL": fmt(row["PREV_AVAL"]),
                    "CHG": fmt(row["CHG"]),
                    "PREV2": fmt(row["PREV2"]),
                }
            )


if __name__ == "__main__":
    main()
