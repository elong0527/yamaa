# Reference solution for the yamaa benchmark adam-adsl-new-therapy (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADSL dataset with new anticancer therapy variables.

Reads the ADSL, CM and PR datasets from /app/input and writes
/app/output/adsl.csv with one record per subject and the columns
STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL.

A medication record qualifies when its CMCAT is "ON TREATMENT". A
procedure record qualifies when its PRCAT is "CANCER RELATED" and its
PRSCAT is "ON TREATMENT". A qualifying record counts only when its
start date is on or after TRTSDT; a qualifying record with no start
date does not count. A start recorded as a year and month only is
imputed to the 15th of that month; a full recorded start date stands
as recorded.

NACTDT is the earlier of the subject's earliest qualifying medication
start date and earliest qualifying procedure start date, and has no
value when no qualifying record counts. NACTDY is the study day of
NACTDT counted from TRTSDT (TRTSDT itself is day 1). NACTFL is "Y"
when NACTDT has a value and has no value otherwise.
"""

import csv
from datetime import date
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")


def parse_start_date(value):
    """Return a date from an ISO 8601 date string, or None.

    A full date (YYYY-MM-DD) stands as recorded. A year-and-month
    value (YYYY-MM) is imputed to the 15th of that month. A missing
    value returns None.
    """
    value = (value or "").strip()
    if not value:
        return None
    parts = value.split("-")
    if len(parts) == 3:
        year, month, day = (int(part) for part in parts)
        return date(year, month, day)
    if len(parts) == 2:
        year, month = (int(part) for part in parts)
        return date(year, month, 15)
    return None


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_csv(INPUT_DIR / "adsl.csv")
    cm = read_csv(INPUT_DIR / "cm.csv")
    pr = read_csv(INPUT_DIR / "pr.csv")

    trtsdt_by_subject = {
        row["USUBJID"]: parse_start_date(row["TRTSDT"]) for row in adsl
    }

    # Earliest counting start date per subject across both sources.
    earliest = {}

    def consider(usubjid, start):
        if start is None:
            return
        trtsdt = trtsdt_by_subject.get(usubjid)
        if trtsdt is None or start < trtsdt:
            return
        if usubjid not in earliest or start < earliest[usubjid]:
            earliest[usubjid] = start

    for row in cm:
        if (row.get("CMCAT") or "").strip() != "ON TREATMENT":
            continue
        consider(row["USUBJID"], parse_start_date(row.get("CMSTDTC", "")))

    for row in pr:
        if (row.get("PRCAT") or "").strip() != "CANCER RELATED":
            continue
        if (row.get("PRSCAT") or "").strip() != "ON TREATMENT":
            continue
        consider(row["USUBJID"], parse_start_date(row.get("PRSTDTC", "")))

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "TRTSDT", "NACTDT", "NACTDY", "NACTFL"]
    with open(OUTPUT_DIR / "adsl.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in adsl:
            usubjid = row["USUBJID"]
            trtsdt = trtsdt_by_subject[usubjid]
            nactdt = earliest.get(usubjid)
            if nactdt is None:
                nactdt_text, nactdy_text, nactfl = "", "", ""
            else:
                nactdt_text = nactdt.isoformat()
                nactdy_text = str((nactdt - trtsdt).days + 1)
                nactfl = "Y"
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": usubjid,
                    "TRTSDT": row["TRTSDT"],
                    "NACTDT": nactdt_text,
                    "NACTDY": nactdy_text,
                    "NACTFL": nactfl,
                }
            )


if __name__ == "__main__":
    main()
