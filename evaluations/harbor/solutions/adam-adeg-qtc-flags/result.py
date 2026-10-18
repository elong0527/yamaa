# Reference solution for the yamaa benchmark adam-adeg-qtc-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADEG dataset with QTcF criterion flags from ADEG_RAW.

Only records with PARAMCD "QTCF" and ONTRTFL "Y" are assessed. The
absolute criteria compare AVAL against 450, 480 and 500 ms; the
increase criteria compare CHG against 30 and 60 ms. A flag is Y only
when the value is strictly above its limit, N otherwise, and each
criterion and flag has no value when the record is not assessed or
the compared value has no value. Values are compared as they stand,
without rounding.
"""

import csv

INPUT_PATH = "/app/input/adeg_raw.csv"
OUTPUT_PATH = "/app/output/adeg.csv"

COLUMNS = [
    "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL",
    "ABLFL", "ONTRTFL", "BASE", "CHG",
    "CRIT1", "CRIT1FL", "CRIT2", "CRIT2FL", "CRIT3", "CRIT3FL",
    "CRIT4", "CRIT4FL", "CRIT5", "CRIT5FL",
]

ABSOLUTE = [
    ("CRIT1", "CRIT1FL", "QTcF > 450 ms", 450.0),
    ("CRIT2", "CRIT2FL", "QTcF > 480 ms", 480.0),
    ("CRIT3", "CRIT3FL", "QTcF > 500 ms", 500.0),
]

INCREASE = [
    ("CRIT4", "CRIT4FL", "QTcF increase from baseline > 30 ms", 30.0),
    ("CRIT5", "CRIT5FL", "QTcF increase from baseline > 60 ms", 60.0),
]


def number(text):
    """The numeric value of a cell, or None when it has no value."""
    if text is None or text.strip() == "":
        return None
    return float(text)


def main():
    with open(INPUT_PATH, newline="") as handle:
        rows = list(csv.DictReader(handle))

    out_rows = []
    for row in rows:
        out = {column: row.get(column, "") for column in COLUMNS[:10]}
        assessed = row.get("PARAMCD") == "QTCF" and row.get("ONTRTFL") == "Y"
        aval = number(row.get("AVAL"))
        chg = number(row.get("CHG"))
        for crit, flag, label, limit in ABSOLUTE:
            if assessed and aval is not None:
                out[crit] = label
                out[flag] = "Y" if aval > limit else "N"
            else:
                out[crit] = ""
                out[flag] = ""
        for crit, flag, label, limit in INCREASE:
            if assessed and chg is not None:
                out[crit] = label
                out[flag] = "Y" if chg > limit else "N"
            else:
                out[crit] = ""
                out[flag] = ""
        out_rows.append(out)

    with open(OUTPUT_PATH, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
