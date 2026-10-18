# Reference solution for the yamaa benchmark sdtm-ds-sequence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DS dataset from the collected DS_RAW dataset.

One output record per collected disposition record. Partial collection
dates are completed before use: a year and month becomes the 15th of
that month, and a year alone becomes June 15th of that year. DSSEQ
numbers each subject's records from 1 in order of the completed
collection date, earliest first; records with no date come after the
subject's dated records.
"""

import csv
import re
from datetime import date

INPUT_PATH = "/app/input/ds_raw.csv"
OUTPUT_PATH = "/app/output/ds.csv"

FULL_DATE = re.compile(r"^(\d{4})-(\d{2})-(\d{2})$")
YEAR_MONTH = re.compile(r"^(\d{4})-(\d{2})$")
YEAR_ONLY = re.compile(r"^(\d{4})$")


def complete_date(raw):
    """Return the completed ISO date string, or None if unreadable."""
    if raw is None:
        return None
    text = raw.strip()
    if not text:
        return None
    match = FULL_DATE.match(text)
    if match:
        year, month, day = (int(part) for part in match.groups())
    else:
        match = YEAR_MONTH.match(text)
        if match:
            year, month = (int(part) for part in match.groups())
            day = 15
        else:
            match = YEAR_ONLY.match(text)
            if match:
                year = int(match.group(1))
                month, day = 6, 15
            else:
                return None
    try:
        completed = date(year, month, day)
    except ValueError:
        return None
    return completed.isoformat()


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    records = []
    for position, row in enumerate(rows):
        outcome = (row.get("DSDECOD") or "").strip()
        dsdtc = complete_date(row.get("DSDTCOL"))
        records.append(
            {
                "DOMAIN": "DS",
                "STUDYID": row.get("STUDY") or "",
                "USUBJID": row.get("PATNUM") or "",
                "DSDECOD": outcome,
                "DSCAT": (
                    "PROTOCOL MILESTONE"
                    if outcome == "RANDOMIZED"
                    else "DISPOSITION EVENT"
                ),
                "DSDTC": dsdtc or "",
                "_sort_date": dsdtc,
                "_position": position,
            }
        )

    subjects = []
    for record in records:
        if record["USUBJID"] not in subjects:
            subjects.append(record["USUBJID"])

    output = []
    for subject in subjects:
        own = [r for r in records if r["USUBJID"] == subject]
        own.sort(
            key=lambda r: (
                r["_sort_date"] is None,
                r["_sort_date"] or "",
                r["_position"],
            )
        )
        for index, record in enumerate(own, start=1):
            record["DSSEQ"] = index
            output.append(record)

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "DSSEQ",
        "DSDECOD",
        "DSCAT",
        "DSDTC",
    ]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for record in output:
            writer.writerow({name: record[name] for name in fieldnames})


if __name__ == "__main__":
    main()
