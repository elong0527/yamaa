# Reference solution for the yamaa benchmark adam-adae-partial-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE from ADSL and AE, completing partial AE start dates.

ASTDT is the adverse event start date derived from the collected
AESTDTC character date:

- a full date is kept exactly as collected, even before treatment start;
- a year and month only is completed to the 15th of that month, except
  that when the 15th falls before TRTSDT within the same month ASTDT is
  TRTSDT, and when the whole month falls before TRTSDT ASTDT is missing;
- a year only, non-date text, or a missing value gives no ASTDT.

ASTDTF is "D" when a day was supplied to complete a year and month.
TRTEMFL is "Y" when ASTDT falls on or after TRTSDT.
"""

import calendar
import csv
import os
import re
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

FULL_DATE = re.compile(r"^(\d{4})-(\d{2})-(\d{2})$")
YEAR_MONTH = re.compile(r"^(\d{4})-(\d{2})$")


def parse_full_date(text):
    """Return a date for a strict YYYY-MM-DD string, else None."""
    match = FULL_DATE.match(text or "")
    if not match:
        return None
    try:
        return date(int(match.group(1)), int(match.group(2)), int(match.group(3)))
    except ValueError:
        return None


def derive_astdt(aestdtc, trtsdt):
    """Return (ASTDT as a date or None, day-imputed flag)."""
    aestdtc = (aestdtc or "").strip()
    full = parse_full_date(aestdtc)
    if full is not None:
        return full, False
    match = YEAR_MONTH.match(aestdtc)
    if match:
        year, month = int(match.group(1)), int(match.group(2))
        if not 1 <= month <= 12:
            return None, False
        candidate = date(year, month, 15)
        if trtsdt is None or candidate >= trtsdt:
            return candidate, True
        if trtsdt.year == year and trtsdt.month == month:
            return trtsdt, True
        last_day = date(year, month, calendar.monthrange(year, month)[1])
        if last_day < trtsdt:
            return None, False
        return candidate, True
    return None, False


def main():
    with open(os.path.join(INPUT_DIR, "adsl.csv"), newline="") as handle:
        adsl = list(csv.DictReader(handle))
    with open(os.path.join(INPUT_DIR, "ae.csv"), newline="") as handle:
        ae = list(csv.DictReader(handle))

    trtsdt_by_subject = {}
    for row in adsl:
        trtsdt_by_subject[row["USUBJID"]] = parse_full_date(row.get("TRTSDT"))

    columns = [
        "STUDYID", "USUBJID", "AESEQ", "AETERM", "AESTDTC",
        "ASTDT", "ASTDTC", "ASTDTF", "TRTSDT", "TRTEMFL",
    ]
    out_rows = []
    for row in ae:
        trtsdt = trtsdt_by_subject.get(row["USUBJID"])
        astdt, day_imputed = derive_astdt(row.get("AESTDTC"), trtsdt)
        astdtc = astdt.isoformat() if astdt is not None else ""
        astdtf = "D" if (day_imputed and astdt is not None) else ""
        trtemfl = ""
        if astdt is not None and trtsdt is not None and astdt >= trtsdt:
            trtemfl = "Y"
        out_rows.append({
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "AESEQ": row["AESEQ"],
            "AETERM": row["AETERM"],
            "AESTDTC": row["AESTDTC"],
            "ASTDT": astdt.isoformat() if astdt is not None else "",
            "ASTDTC": astdtc,
            "ASTDTF": astdtf,
            "TRTSDT": trtsdt.isoformat() if trtsdt is not None else "",
            "TRTEMFL": trtemfl,
        })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adae.csv"), "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
