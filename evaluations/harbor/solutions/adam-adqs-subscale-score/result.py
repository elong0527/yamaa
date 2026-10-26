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
        reasons = set()
        for r in recs:
            raw = (r["QSSTRESN"] or "").strip()
            aval = float(raw) if raw != "" else None
            if aval is not None:
                answered.append(aval)
            reason = (r["QSREASND"] or "").strip()
            if reason:
                reasons.add(reason)
            out.append(
                [studyid, usubjid, visit, r["QSTESTCD"], r["QSTEST"], fmt(aval), reason]
            )
        # Every visit with a PF01 item record also has a score record. A score
        # without a value takes the items' reason when none was answered, and
        # reads NOT CALCULABLE otherwise.
        if any(r["QSTESTCD"] == "PF01" for r in recs):
            if len(answered) >= 3:
                score = sum(answered) / len(answered) * 25
                areasnd = ""
            elif not answered and len(reasons) == 1:
                score = None
                areasnd = next(iter(reasons))
            else:
                score = None
                areasnd = "NOT CALCULABLE"
            out.append(
                [
                    studyid,
                    usubjid,
                    visit,
                    SCORE_PARAMCD,
                    SCORE_PARAM,
                    fmt(score),
                    areasnd,
                ]
            )

    with open(OUTPUT_ADQS, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(
            ["STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL", "AREASND"]
        )
        writer.writerows(out)


if __name__ == "__main__":
    main()
