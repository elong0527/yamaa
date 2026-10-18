# Reference solution for the yamaa benchmark adam-adqs-subscale-score (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT_QS = "/app/input/qs.csv"
OUTPUT_ADQS = "/app/output/adqs.csv"

ITEM_CODES = ("PF01", "PF02", "PF03", "PF04")
SCORE_PARAMCD = "PFSCORE"
SCORE_PARAM = "Physical Functioning Subscale Score"


def fmt(value):
    if value is None:
        return ""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(INPUT_QS, newline="") as f:
        rows = list(csv.DictReader(f))

    # Item records: the physical functioning records (PF01..PF04).
    items = [r for r in rows if r["QSTESTCD"] in ITEM_CODES]

    # Group item records by subject and visit, preserving input order.
    groups = OrderedDict()
    for r in items:
        key = (r["STUDYID"], r["USUBJID"], r["VISIT"])
        groups.setdefault(key, []).append(r)

    out = []
    for (studyid, usubjid, visit), recs in groups.items():
        answered = []
        for r in recs:
            raw = (r["QSSTRESN"] or "").strip()
            aval = float(raw) if raw != "" else None
            if aval is not None:
                answered.append(aval)
            out.append(
                [studyid, usubjid, visit, r["QSTESTCD"], r["QSTEST"], fmt(aval)]
            )
        # Every visit with a PF01 item record also has a score record.
        if any(r["QSTESTCD"] == "PF01" for r in recs):
            if len(answered) >= 3:
                score = sum(answered) / len(answered) * 25
            else:
                score = None
            out.append(
                [studyid, usubjid, visit, SCORE_PARAMCD, SCORE_PARAM, fmt(score)]
            )

    with open(OUTPUT_ADQS, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL"])
        writer.writerows(out)


if __name__ == "__main__":
    main()
