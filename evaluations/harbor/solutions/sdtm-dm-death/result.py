# Reference solution for the yamaa benchmark sdtm-dm-death (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the SDTM DM dataset with death date and death flag.

One record per subject (from DM_RAW). DTHDTC is the latest start date
among the subject's disposition records with decoded term "DEATH"
that carry a start date; if the subject has no such record, it is the
latest end date among the subject's adverse events with outcome
"FATAL" that carry an end date. DTHFL is "Y" when either source
supplies a death date, and blank otherwise.
"""

import csv
from collections import OrderedDict
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")


def read_rows(name):
    with open(INPUT_DIR / name, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def latest_date(dates):
    """Latest non-blank ISO date string, or "" when none is present."""
    present = [d.strip() for d in dates if d and d.strip()]
    return max(present) if present else ""


def main():
    subjects = read_rows("dm_raw.csv")
    ds_rows = read_rows("ds.csv")
    ae_rows = read_rows("ae.csv")

    ds_death_dates = {}
    for row in ds_rows:
        if row["DSDECOD"] == "DEATH":
            ds_death_dates.setdefault(row["USUBJID"], []).append(row["DSSTDTC"])

    ae_fatal_dates = {}
    for row in ae_rows:
        if row["AEOUT"] == "FATAL":
            ae_fatal_dates.setdefault(row["USUBJID"], []).append(row["AEENDTC"])

    out_rows = []
    for subject in subjects:
        usubjid = subject["USUBJID"]
        dthdtc = latest_date(ds_death_dates.get(usubjid, []))
        if not dthdtc:
            dthdtc = latest_date(ae_fatal_dates.get(usubjid, []))
        out_rows.append(
            OrderedDict(
                DOMAIN="DM",
                STUDYID=subject["STUDYID"],
                USUBJID=usubjid,
                DTHDTC=dthdtc,
                DTHFL="Y" if dthdtc else "",
            )
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / "dm.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle, fieldnames=["DOMAIN", "STUDYID", "USUBJID", "DTHDTC", "DTHFL"]
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
