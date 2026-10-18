# Reference solution for the yamaa benchmark adam-adsl-analysis-age (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os
from datetime import date


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    year, month, day = text.split("-")
    return date(int(year), int(month), int(day))


def anniversary_in_year(birth, year):
    """Yearly anniversary of `birth` within `year`.

    A February 29 birthday falls on February 28 in a common year.
    """
    month, day = birth.month, birth.day
    if month == 2 and day == 29:
        try:
            return date(year, 2, 29)
        except ValueError:
            return date(year, 2, 28)
    return date(year, month, day)


def count_anniversaries(start, end):
    """Count of yearly anniversaries of `start` falling on or before `end`.

    Assumes end >= start.
    """
    years = end.year - start.year
    if anniversary_in_year(start, end.year) > end:
        years -= 1
    return years


def analysis_age(brthdt, randdt):
    if brthdt is None or randdt is None:
        return ""
    if randdt >= brthdt:
        return count_anniversaries(brthdt, randdt)
    return -count_anniversaries(randdt, brthdt)


def main():
    with open("/app/input/dm.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        brthdt = parse_date(row["BRTHDT"])
        randdt = parse_date(row["RANDDT"])
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "BRTHDT": row["BRTHDT"],
                "RANDDT": row["RANDDT"],
                "AAGE": analysis_age(brthdt, randdt),
                "AAGEU": "YEARS",
            }
        )

    os.makedirs("/app/output", exist_ok=True)
    with open("/app/output/adsl.csv", "w", newline="") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=["STUDYID", "USUBJID", "BRTHDT", "RANDDT", "AAGE", "AAGEU"],
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
