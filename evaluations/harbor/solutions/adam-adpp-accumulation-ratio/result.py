# Reference solution for the yamaa benchmark adam-adpp-accumulation-ratio (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

STEADY_STATE_VISIT = {"A": "DAY 7", "B": "DAY 5"}
FIRST_DOSE_VISIT = "DAY 1"

RATIO_PARAMS = [
    ("AUCTAU", "ARAUC", "Accumulation Ratio AUCTAU"),
    ("CMAX", "ARCMAX", "Accumulation Ratio Cmax"),
]


def read_csv(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def main():
    adsl = read_csv(f"{INPUT_DIR}/adsl.csv")
    pp = read_csv(f"{INPUT_DIR}/pp.csv")

    arm_by_subject = {row["USUBJID"]: row["ARMCD"] for row in adsl}

    # Index collected values: (subject, testcd, visit) -> (value, studyid)
    values = {}
    for row in pp:
        raw = (row.get("PPSTRESN") or "").strip()
        value = float(raw) if raw else None
        values[(row["USUBJID"], row["PPTESTCD"], row["VISIT"])] = (
            value,
            row["STUDYID"],
        )

    out_rows = []

    # Collected records, one per PP record, in input order, grouped by
    # subject so each subject's ratio records can follow its own records.
    subjects_in_order = []
    for row in pp:
        if row["USUBJID"] not in subjects_in_order:
            subjects_in_order.append(row["USUBJID"])

    pp_by_subject = OrderedDict()
    for row in pp:
        pp_by_subject.setdefault(row["USUBJID"], []).append(row)

    for subject in subjects_in_order:
        for row in pp_by_subject[subject]:
            raw = (row.get("PPSTRESN") or "").strip()
            out_rows.append(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": subject,
                    "PARAMCD": row["PPTESTCD"],
                    "PARAM": row["PPTEST"],
                    "AVISIT": row["VISIT"],
                    "AVAL": raw if raw else "",
                    "AVALU": row["PPSTRESU"] if raw else row["PPSTRESU"],
                }
            )

        # Ratio records for this subject.
        arm = arm_by_subject.get(subject)
        ss_visit = STEADY_STATE_VISIT.get(arm)
        if ss_visit is None:
            continue
        for testcd, ratio_cd, ratio_name in RATIO_PARAMS:
            ss = values.get((subject, testcd, ss_visit))
            d1 = values.get((subject, testcd, FIRST_DOSE_VISIT))
            if ss is None or d1 is None:
                continue
            ss_value, studyid = ss
            d1_value, _ = d1
            if ss_value is None or d1_value is None:
                continue
            if d1_value == 0:
                continue
            ratio = ss_value / d1_value
            out_rows.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": subject,
                    "PARAMCD": ratio_cd,
                    "PARAM": ratio_name,
                    "AVISIT": ss_visit,
                    "AVAL": repr(ratio),
                    "AVALU": "RATIO",
                }
            )

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "AVALU"]
    with open(f"{OUTPUT_DIR}/adpp.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
