# Reference solution for the yamaa benchmark adam-adae-partial-times (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE from AE, completing partial start/end datetimes.

ASTDTM completes AESTDTC with the earliest value of each uncollected
time part; AENDTM completes AEENDTC with the latest value. The time
imputation flags record what was collected: "H" for a date alone,
"M" for collection to the hour, "S" for collection to the minute,
and no value when collected to the second or when no datetime can
be formed (day not collected).
"""

import csv
import re

INPUT_PATH = "/app/input/ae.csv"
OUTPUT_PATH = "/app/output/adae.csv"

SECOND = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$")
MINUTE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$")
HOUR = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def complete(value, end):
    """Return (datetime_text, flag) completing a partial ISO datetime.

    `end` selects latest-value completion instead of earliest-value.
    """
    value = (value or "").strip()
    if not value:
        return "", ""
    if SECOND.match(value):
        return value, ""
    if MINUTE.match(value):
        seconds = "59" if end else "00"
        return value + ":" + seconds, "S"
    if HOUR.match(value):
        tail = ":59:59" if end else ":00:00"
        return value + tail, "M"
    if DATE.match(value):
        tail = "T23:59:59" if end else "T00:00:00"
        return value + tail, "H"
    # Collected without its day (e.g. year-month only): no datetime.
    return "", ""


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        astdtm, asttmf = complete(row["AESTDTC"], end=False)
        aendtm, aentmf = complete(row["AEENDTC"], end=True)
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "AETERM": row["AETERM"],
                "AESTDTC": row["AESTDTC"],
                "ASTDTM": astdtm,
                "ASTTMF": asttmf,
                "AEENDTC": row["AEENDTC"],
                "AENDTM": aendtm,
                "AENTMF": aentmf,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AESTDTC",
        "ASTDTM",
        "ASTTMF",
        "AEENDTC",
        "AENDTM",
        "AENTMF",
    ]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
