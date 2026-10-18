# Reference solution for the yamaa benchmark adam-adsl-alive-date (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL with last-contact and last-alive dates.

Reads ADAE, ADSL and ADVS from /app/input and writes /app/output/adsl.csv
with one record per subject and columns:
STUDYID, USUBJID, TRTEDT, LSTCNTDC, LSTCNTDT, LSTALVDT.
"""

import csv
import re
from datetime import date
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")

FULL_DATE_RE = re.compile(r"^(\d{4})-(\d{2})-(\d{2})$")
YEAR_MONTH_RE = re.compile(r"^(\d{4})-(\d{2})$")
YEAR_RE = re.compile(r"^(\d{4})$")


def parse_full_date(text):
    """Parse a complete YYYY-MM-DD date; return None when unreadable."""
    if text is None:
        return None
    match = FULL_DATE_RE.match(text.strip())
    if not match:
        return None
    try:
        return date(int(match.group(1)), int(match.group(2)), int(match.group(3)))
    except ValueError:
        return None


def complete_date(text):
    """Complete a partial date text to a day.

    A full date keeps its day, a year-month takes the first day of that
    month, and a year alone takes the first day of January. Text that
    cannot be read as a date has no value.
    """
    if text is None:
        return None
    text = text.strip()
    if not text:
        return None
    full = parse_full_date(text)
    if full is not None:
        return full
    match = YEAR_MONTH_RE.match(text)
    if match:
        try:
            return date(int(match.group(1)), int(match.group(2)), 1)
        except ValueError:
            return None
    match = YEAR_RE.match(text)
    if match:
        try:
            return date(int(match.group(1)), 1, 1)
        except ValueError:
            return None
    return None


def fmt(value):
    return value.isoformat() if value is not None else ""


def main():
    with open(INPUT_DIR / "adsl.csv", newline="") as handle:
        adsl_rows = list(csv.DictReader(handle))
    with open(INPUT_DIR / "adae.csv", newline="") as handle:
        adae_rows = list(csv.DictReader(handle))
    with open(INPUT_DIR / "advs.csv", newline="") as handle:
        advs_rows = list(csv.DictReader(handle))

    latest_aendt = {}
    for row in adae_rows:
        value = parse_full_date(row.get("AENDT"))
        if value is None:
            continue
        key = (row["STUDYID"], row["USUBJID"])
        if key not in latest_aendt or value > latest_aendt[key]:
            latest_aendt[key] = value

    latest_adate = {}
    for row in advs_rows:
        value = parse_full_date(row.get("ADATE"))
        if value is None:
            continue
        key = (row["STUDYID"], row["USUBJID"])
        if key not in latest_adate or value > latest_adate[key]:
            latest_adate[key] = value

    out_rows = []
    for row in adsl_rows:
        key = (row["STUDYID"], row["USUBJID"])
        trtedt = parse_full_date(row.get("TRTEDT"))
        lstcntdt = complete_date(row.get("LSTCNTDC"))
        candidates = [
            value
            for value in (
                trtedt,
                lstcntdt,
                latest_aendt.get(key),
                latest_adate.get(key),
            )
            if value is not None
        ]
        lstalvdt = max(candidates) if candidates else None
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "TRTEDT": row.get("TRTEDT") or "",
                "LSTCNTDC": row.get("LSTCNTDC") or "",
                "LSTCNTDT": fmt(lstcntdt),
                "LSTALVDT": fmt(lstalvdt),
            }
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "TRTEDT", "LSTCNTDC", "LSTCNTDT", "LSTALVDT"]
    with open(OUTPUT_DIR / "adsl.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
