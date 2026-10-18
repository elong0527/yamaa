# Reference solution for the yamaa benchmark adam-adsl-new-therapy (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL with new anticancer therapy (NACT) derivations.

Reads ADSL, CM and PR from /app/input and writes /app/output/adsl.csv
with one record per subject and columns STUDYID, USUBJID, TRTSDT,
NACTDT, NACTDY, NACTFL.

A medication record qualifies when CMCAT is "ON TREATMENT"; a procedure
record qualifies when PRCAT is "CANCER RELATED" and PRSCAT is
"ON TREATMENT". A qualifying record counts only when its start date is
on or after the subject's TRTSDT, and a qualifying record with no start
date never counts. A medication start recorded as year and month only
is taken as the 15th of that month; a full recorded start date stands
as recorded. NACTDT is the earlier of the earliest qualifying
medication and procedure start dates, NACTDY its study day counted
from TRTSDT, and NACTFL is "Y" exactly when NACTDT has a value.
"""

import os
from datetime import date

import pandas as pd


def parse_med_start(raw):
    """Medication start: full date as recorded; YYYY-MM -> the 15th."""
    raw = (raw or "").strip()
    if not raw:
        return None
    parts = raw.split("-")
    if len(parts) == 3:
        return date(int(parts[0]), int(parts[1]), int(parts[2]))
    if len(parts) == 2:
        return date(int(parts[0]), int(parts[1]), 15)
    return None


def parse_full_date(raw):
    """Full ISO date, or None when absent or not a full date."""
    raw = (raw or "").strip()
    if not raw:
        return None
    parts = raw.split("-")
    if len(parts) == 3:
        return date(int(parts[0]), int(parts[1]), int(parts[2]))
    return None


def main():
    adsl = pd.read_csv("/app/input/adsl.csv", dtype=str, keep_default_na=False)
    cm = pd.read_csv("/app/input/cm.csv", dtype=str, keep_default_na=False)
    pr = pd.read_csv("/app/input/pr.csv", dtype=str, keep_default_na=False)

    trtsdt = {}
    for _, row in adsl.iterrows():
        trtsdt[(row["STUDYID"], row["USUBJID"])] = parse_full_date(row["TRTSDT"])

    earliest = {}

    def consider(key, start):
        if key not in earliest or start < earliest[key]:
            earliest[key] = start

    for _, row in cm.iterrows():
        if row["CMCAT"] != "ON TREATMENT":
            continue
        key = (row["STUDYID"], row["USUBJID"])
        start = parse_med_start(row["CMSTDTC"])
        ref = trtsdt.get(key)
        if start is not None and ref is not None and start >= ref:
            consider(key, start)

    for _, row in pr.iterrows():
        if row["PRCAT"] != "CANCER RELATED" or row["PRSCAT"] != "ON TREATMENT":
            continue
        key = (row["STUDYID"], row["USUBJID"])
        start = parse_full_date(row["PRSTDTC"])
        ref = trtsdt.get(key)
        if start is not None and ref is not None and start >= ref:
            consider(key, start)

    rows = []
    for _, row in adsl.iterrows():
        key = (row["STUDYID"], row["USUBJID"])
        nactdt = earliest.get(key)
        if nactdt is None:
            rows.append(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "TRTSDT": row["TRTSDT"],
                    "NACTDT": "",
                    "NACTDY": "",
                    "NACTFL": "",
                }
            )
        else:
            ref = trtsdt[key]
            if nactdt >= ref:
                nactdy = (nactdt - ref).days + 1
            else:
                nactdy = (nactdt - ref).days
            rows.append(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "TRTSDT": row["TRTSDT"],
                    "NACTDT": nactdt.isoformat(),
                    "NACTDY": nactdy,
                    "NACTFL": "Y",
                }
            )

    out = pd.DataFrame(
        rows,
        columns=["STUDYID", "USUBJID", "TRTSDT", "NACTDT", "NACTDY", "NACTFL"],
    )
    os.makedirs("/app/output", exist_ok=True)
    out.to_csv("/app/output/adsl.csv", index=False)


if __name__ == "__main__":
    main()
