# Reference solution for the yamaa benchmark adam-adcm-end-date-bound (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADCM from ADSL and CM.

AENDT is the analysis end date formed from the collected end date
CMENDTC, bounded by the subject's date of death for partial dates.
"""

import calendar

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adcm.csv"


def parse_partial(value):
    """Return (kind, year, month) for a collected ISO date string.

    kind is "full", "month" (YYYY-MM), "year" (YYYY) or "missing".
    """
    text = (value or "").strip()
    if not text:
        return ("missing", None, None)
    parts = text.split("-")
    if len(parts) == 3:
        return ("full", int(parts[0]), int(parts[1]))
    if len(parts) == 2:
        return ("month", int(parts[0]), int(parts[1]))
    return ("year", int(parts[0]), None)


def derive_aendt(cmendtc, dthdt):
    """Return (AENDT, AENDTF) as ISO strings, "" when no value."""
    kind, year, month = parse_partial(cmendtc)
    if kind == "missing":
        return "", ""
    if kind == "full":
        return cmendtc.strip(), ""

    if kind == "month":
        last_day = calendar.monthrange(year, month)[1]
        completed = pd.Timestamp(year=year, month=month, day=last_day)
        period_start = pd.Timestamp(year=year, month=month, day=1)
        flag = "D"
    else:  # year only
        completed = pd.Timestamp(year=year, month=12, day=31)
        period_start = pd.Timestamp(year=year, month=1, day=1)
        flag = "M"

    if dthdt:
        death = pd.Timestamp(dthdt)
        if completed > death:
            if period_start > death:
                # The whole collected month or year begins after death.
                return "", ""
            # The collected month or year contains the death date.
            return dthdt, flag
    return completed.strftime("%Y-%m-%d"), flag


def main():
    adsl = pd.read_csv(f"{INPUT_DIR}/adsl.csv", dtype=str, keep_default_na=False)
    cm = pd.read_csv(f"{INPUT_DIR}/cm.csv", dtype=str, keep_default_na=False)

    death_by_subject = {}
    for _, row in adsl.iterrows():
        death_by_subject[(row["STUDYID"], row["USUBJID"])] = row["DTHDT"].strip()

    records = []
    for _, row in cm.iterrows():
        key = (row["STUDYID"], row["USUBJID"])
        dthdt = death_by_subject.get(key, "")
        aendt, aendtf = derive_aendt(row["CMENDTC"], dthdt)
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "CMSEQ": row["CMSEQ"],
                "CMTRT": row["CMTRT"],
                "CMENDTC": row["CMENDTC"],
                "AENDT": aendt,
                "AENDTF": aendtf,
                "DTHDT": dthdt,
            }
        )

    columns = [
        "STUDYID",
        "USUBJID",
        "CMSEQ",
        "CMTRT",
        "CMENDTC",
        "AENDT",
        "AENDTF",
        "DTHDT",
    ]
    adcm = pd.DataFrame(records, columns=columns)
    adcm.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
