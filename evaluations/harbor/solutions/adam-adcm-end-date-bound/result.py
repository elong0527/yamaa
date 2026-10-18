# Reference solution for the yamaa benchmark adam-adcm-end-date-bound (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADCM from ADSL and CM, bounding partial end dates at death."""

import calendar
import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def parse_full(text):
    year, month, day = (int(part) for part in text.split("-"))
    return date(year, month, day)


def derive_end(cmendtc, dthdt):
    """Return (aendt, aendtf) for one collected end date."""
    if not cmendtc:
        return "", ""
    parts = cmendtc.split("-")
    if len(parts) == 3:
        # A full date is used exactly as collected, even after death.
        return cmendtc, ""
    if len(parts) == 2:
        year, month = int(parts[0]), int(parts[1])
        last_day = calendar.monthrange(year, month)[1]
        completed = date(year, month, last_day)
        flag = "D"
        period_start = date(year, month, 1)
        period_end = completed
    else:
        year = int(parts[0])
        completed = date(year, 12, 31)
        flag = "M"
        period_start = date(year, 1, 1)
        period_end = completed
    if dthdt and completed > dthdt:
        if period_start <= dthdt <= period_end:
            # The collected month or year also contains the death date;
            # the flag still records what was supplied.
            return dthdt.isoformat(), flag
        # The whole collected month or year begins after death.
        return "", ""
    return completed.isoformat(), flag


def main():
    adsl = read_csv(f"{INPUT_DIR}/adsl.csv")
    death = {}
    for row in adsl:
        death[row["USUBJID"]] = parse_full(row["DTHDT"]) if row["DTHDT"] else None

    cm = read_csv(f"{INPUT_DIR}/cm.csv")
    out_rows = []
    for row in cm:
        dthdt = death.get(row["USUBJID"])
        aendt, aendtf = derive_end(row["CMENDTC"], dthdt)
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "CMSEQ": row["CMSEQ"],
                "CMTRT": row["CMTRT"],
                "CMENDTC": row["CMENDTC"],
                "AENDT": aendt,
                "AENDTF": aendtf,
                "DTHDT": dthdt.isoformat() if dthdt else "",
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "CMSEQ",
        "CMTRT",
        "CMENDTC",
        "AENDT",
        "AENDTF",
        "DTHDT",
    ]
    with open(f"{OUTPUT_DIR}/adcm.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
