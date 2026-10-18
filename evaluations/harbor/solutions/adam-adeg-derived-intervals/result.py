# Reference solution for the yamaa benchmark adam-adeg-derived-intervals (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEG with derived QTcB, QTcF, and RR-rederived records.

Reads the collected ADEG dataset from /app/input, keeps every collected
record, and adds one record per subject and analysis visit for each
derived parameter whose source results are present:

- QTCBR: QT / SQRT(RR / 1000)      (needs QT and RR)
- QTCFR: QT / POWER(RR / 1000, 1/3) (needs QT and RR)
- RRR:   60000 / heart rate       (needs heart rate present and nonzero)

AVAL on the derived records is not rounded.
"""

import csv
import math
from collections import OrderedDict

INPUT_PATH = "/app/input/adeg.csv"
OUTPUT_PATH = "/app/output/adeg.csv"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "AVALU"]

DERIVED_PARAMS = {
    "QTCBR": "QTcB - Bazett's Correction Formula Rederived (ms)",
    "QTCFR": "QTcF - Fridericia's Correction Formula Rederived (ms)",
    "RRR": "RR Duration Rederived (ms)",
}


def parse_number(text):
    """Return the float value of a cell, or None when it is empty."""
    if text is None:
        return None
    text = text.strip()
    if text == "":
        return None
    return float(text)


def format_number(value):
    """Write a number without rounding; whole numbers without a fraction."""
    if value == int(value) and abs(value) < 1e15:
        return str(int(value))
    return repr(value)


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    # Group collected records by subject and analysis visit, keeping the
    # order in which each group first appears.
    groups = OrderedDict()
    for row in rows:
        key = (row["STUDYID"], row["USUBJID"], row["AVISIT"])
        groups.setdefault(key, []).append(row)

    out_rows = []
    for (studyid, usubjid, avisit), group in groups.items():
        out_rows.extend(group)

        # Source results for this subject and visit, by parameter.
        values = {}
        for row in group:
            number = parse_number(row["AVAL"])
            if number is not None:
                values[row["PARAMCD"]] = number

        derived = []
        qt = values.get("QT")
        rr = values.get("RR")
        if qt is not None and rr is not None:
            rr_seconds = rr / 1000.0
            derived.append(("QTCBR", qt / math.sqrt(rr_seconds)))
            derived.append(("QTCFR", qt / (rr_seconds ** (1.0 / 3.0))))
        hr = values.get("HR")
        if hr is not None and hr != 0:
            derived.append(("RRR", 60000.0 / hr))

        for paramcd, aval in derived:
            out_rows.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "PARAMCD": paramcd,
                    "PARAM": DERIVED_PARAMS[paramcd],
                    "AVISIT": avisit,
                    "AVAL": format_number(aval),
                    "AVALU": "ms",
                }
            )

    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
