# Reference solution for the yamaa benchmark adam-adeg-replicate-qtc-average (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT = "/app/input"
OUTPUT = "/app/output"

PARAMS = [
    ("QTAG", "QT Interval, Aggregate (msec)"),
    ("RRAG", "RR Interval, Aggregate (msec)"),
    ("QTCF", "QTcF Interval, Aggregate (msec)"),
]


def main():
    with open(f"{INPUT}/eg.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    # Group rows by subject and planned time point, preserving input order.
    groups = OrderedDict()
    for row in rows:
        key = (row["STUDYID"], row["USUBJID"], row["EGTPT"], row["EGTPTNUM"])
        groups.setdefault(key, []).append(row)

    out = []
    for (studyid, usubjid, atpt, atptn), grows in groups.items():
        qt = {}  # EGREPNUM -> QT result (msec)
        rr = {}  # EGREPNUM -> RR result (msec)
        for row in grows:
            value = row["EGSTRESN"].strip()
            if value == "":
                # A test that was not done has no result.
                continue
            if row["EGTESTCD"] == "QTAG":
                qt[row["EGREPNUM"]] = float(value)
            elif row["EGTESTCD"] == "RRAG":
                rr[row["EGREPNUM"]] = float(value)

        values = {}
        if qt:
            values["QTAG"] = sum(qt.values()) / len(qt)
        if rr:
            values["RRAG"] = sum(rr.values()) / len(rr)
        corrected = []
        for repnum, qtval in qt.items():
            if repnum in rr and rr[repnum] != 0:
                corrected.append(qtval / (rr[repnum] / 1000.0) ** (1.0 / 3.0))
        if corrected:
            values["QTCF"] = sum(corrected) / len(corrected)

        for paramcd, param in PARAMS:
            if paramcd not in values:
                # A parameter with no value at a time point has no record.
                continue
            out.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "PARAMCD": paramcd,
                    "PARAM": param,
                    "ATPT": atpt,
                    "ATPTN": atptn,
                    "DTYPE": "AVERAGE",
                    "AVAL": repr(values[paramcd]),
                }
            )

    columns = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "ATPT", "ATPTN",
               "DTYPE", "AVAL"]
    with open(f"{OUTPUT}/adeg.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
