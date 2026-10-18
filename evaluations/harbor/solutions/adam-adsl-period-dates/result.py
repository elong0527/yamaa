# Reference solution for the yamaa benchmark adam-adsl-period-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL period dates from DM and EX.

One record per subject (from DM) with the analysis-period dates:
AP01SDT/AP02SDT are the earliest exposure start dates in the
"TREATMENT 1" / "TREATMENT 2" epochs; AP01EDT is the day before
period two started, or 30 days after the last exposure end date when
the subject never entered period two; AP02EDT is 30 days after the
last exposure end date.
"""

import csv
from datetime import date, timedelta

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(text):
    """Return a date for an ISO date string, or None when blank."""
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def fmt(value):
    return value.isoformat() if value is not None else ""


def main():
    with open(f"{INPUT_DIR}/dm.csv", newline="") as handle:
        dm_rows = list(csv.DictReader(handle))
    with open(f"{INPUT_DIR}/ex.csv", newline="") as handle:
        ex_rows = list(csv.DictReader(handle))

    ex_by_subject = {}
    for row in ex_rows:
        ex_by_subject.setdefault(row["USUBJID"], []).append(row)

    out_rows = []
    for dm in dm_rows:
        records = ex_by_subject.get(dm["USUBJID"], [])

        def earliest_start(epoch):
            starts = [
                parse_date(r["EXSTDTC"])
                for r in records
                if r["EPOCH"] == epoch
            ]
            starts = [s for s in starts if s is not None]
            return min(starts) if starts else None

        ap01sdt = earliest_start("TREATMENT 1")
        ap02sdt = earliest_start("TREATMENT 2")

        ends = [parse_date(r["EXENDTC"]) for r in records]
        ends = [e for e in ends if e is not None]
        last_end = max(ends) if ends else None

        entered_period_two = ap02sdt is not None
        if entered_period_two:
            ap01edt = ap02sdt - timedelta(days=1)
            ap02edt = (
                last_end + timedelta(days=30) if last_end is not None else None
            )
        else:
            ap01edt = (
                last_end + timedelta(days=30) if last_end is not None else None
            )
            ap02edt = None

        out_rows.append(
            {
                "STUDYID": dm["STUDYID"],
                "USUBJID": dm["USUBJID"],
                "AP01SDT": fmt(ap01sdt),
                "AP01EDT": fmt(ap01edt),
                "AP02SDT": fmt(ap02sdt),
                "AP02EDT": fmt(ap02edt),
            }
        )

    columns = ["STUDYID", "USUBJID", "AP01SDT", "AP01EDT", "AP02SDT", "AP02EDT"]
    with open(f"{OUTPUT_DIR}/adsl.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
