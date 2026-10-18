# Reference solution for the yamaa benchmark adam-adsl-duration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import calendar
from datetime import date

IN_DIR = "/app/input"
OUT_DIR = "/app/output"


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def add_months(d, months):
    """Date `months` months after d, on the same day of month, or on the
    last day of the target month when it has no such day."""
    total = (d.year * 12 + (d.month - 1)) + months
    year, month0 = divmod(total, 12)
    month = month0 + 1
    day = min(d.day, calendar.monthrange(year, month)[1])
    return date(year, month, day)


def durations(start, end):
    """(DURW, DURM) from start to end; both blank if either is missing."""
    if start is None or end is None:
        return "", ""
    sign = 1
    lo, hi = start, end
    if hi < lo:
        sign = -1
        lo, hi = hi, lo
    durw = (hi - lo).days // 7
    durm = 0
    while add_months(lo, durm + 1) <= hi:
        durm += 1
    return sign * durw, sign * durm


def main():
    with open(f"{IN_DIR}/dm.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        stdt = parse_date(row.get("STDT"))
        endt = parse_date(row.get("ENDT"))
        durw, durm = durations(stdt, endt)
        out_rows.append(
            {
                "STUDYID": row.get("STUDYID", ""),
                "USUBJID": row.get("USUBJID", ""),
                "STDT": row.get("STDT", ""),
                "ENDT": row.get("ENDT", ""),
                "DURW": durw,
                "DURM": durm,
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "STDT", "ENDT", "DURW", "DURM"]
    with open(f"{OUT_DIR}/adsl.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
