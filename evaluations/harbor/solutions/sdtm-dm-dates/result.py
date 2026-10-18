# Reference solution for the yamaa benchmark sdtm-dm-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the DM dataset (one record per enrolled subject) from DM_RAW,
EX, DS, and AE source datasets, following CDISC SDTM conventions.

Reads /app/input/dm_raw.csv, ex.csv, ds.csv, ae.csv and writes
/app/output/dm.csv with columns:
DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC,
RFPENDTC, RFENDTC.
"""

import csv
import os
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    path = os.path.join(INPUT_DIR, name)
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def clean(value):
    """Return the stripped string, or None when the value is missing."""
    if value is None:
        return None
    value = value.strip()
    return value if value else None


def parse_date(value):
    """Parse an ISO date string to a date for comparison; None if absent."""
    value = clean(value)
    if value is None:
        return None
    try:
        return date.fromisoformat(value)
    except ValueError:
        return None


def main():
    dm_raw = read_csv("dm_raw.csv")
    ex = read_csv("ex.csv")
    ds = read_csv("ds.csv")
    ae = read_csv("ae.csv")

    def key(row):
        return (clean(row.get("STUDYID")), clean(row.get("USUBJID")))

    # Per-subject exposure dates.
    ex_starts = {}
    ex_ends = {}
    for row in ex:
        k = key(row)
        start = parse_date(row.get("EXSTDTC"))
        if start is not None:
            ex_starts.setdefault(k, []).append((start, clean(row.get("EXSTDTC"))))
        end = parse_date(row.get("EXENDTC"))
        if end is not None:
            ex_ends.setdefault(k, []).append((end, clean(row.get("EXENDTC"))))

    # Per-subject latest disposition event date (category DISPOSITION
    # EVENT; the date is the record's start date).
    ds_event_dates = {}
    for row in ds:
        if clean(row.get("DSCAT")) != "DISPOSITION EVENT":
            continue
        d = parse_date(row.get("DSSTDTC"))
        if d is not None:
            ds_event_dates.setdefault(key(row), []).append(
                (d, clean(row.get("DSSTDTC")))
            )

    # Per-subject latest adverse event end date.
    ae_end_dates = {}
    for row in ae:
        d = parse_date(row.get("AEENDTC"))
        if d is not None:
            ae_end_dates.setdefault(key(row), []).append(
                (d, clean(row.get("AEENDTC")))
            )

    def earliest(pairs):
        return min(pairs)[1] if pairs else ""

    def latest(pairs):
        return max(pairs)[1] if pairs else ""

    out_rows = []
    for row in dm_raw:
        k = key(row)
        rfxstdtc = earliest(ex_starts.get(k, []))
        rfxendtc = latest(ex_ends.get(k, []))
        rfstdtc = rfxstdtc

        candidates = []
        if ex_ends.get(k):
            candidates.append(max(ex_ends[k]))
        if ds_event_dates.get(k):
            candidates.append(max(ds_event_dates[k]))
        if ae_end_dates.get(k):
            candidates.append(max(ae_end_dates[k]))
        rfpendtc = max(candidates)[1] if candidates else ""

        # The reference end date exists only for a subject who entered
        # the reference period (RFSTDTC present).
        rfendtc = rfpendtc if rfstdtc else ""

        out_rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": clean(row.get("STUDYID")) or "",
                "USUBJID": clean(row.get("USUBJID")) or "",
                "RFICDTC": clean(row.get("RFICDTC")) or "",
                "RFXSTDTC": rfxstdtc,
                "RFXENDTC": rfxendtc,
                "RFSTDTC": rfstdtc,
                "RFPENDTC": rfpendtc,
                "RFENDTC": rfendtc,
            }
        )

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "RFICDTC",
        "RFXSTDTC",
        "RFXENDTC",
        "RFSTDTC",
        "RFPENDTC",
        "RFENDTC",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "dm.csv")
    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
