# Reference solution for the yamaa benchmark adam-adrs-response-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADRS (one record per investigator-scored overall response
assessment) from ADSL and RS, following CDISC ADaM standards."""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

AVAL_MAP = {
    "CR": 1,
    "PR": 2,
    "SD": 3,
    "NON-CR/NON-PD": 4,
    "PD": 5,
    "NE": 6,
}


def complete_date(rsdtc):
    """Complete a possibly partial ISO date: a year and month without a
    day complete to the first of the month; a year alone completes to
    1 January."""
    parts = rsdtc.split("-")
    year = int(parts[0])
    month = int(parts[1]) if len(parts) > 1 else 1
    day = int(parts[2]) if len(parts) > 2 else 1
    return date(year, month, day)


def parse_full_date(text):
    year, month, day = (int(p) for p in text.split("-"))
    return date(year, month, day)


def main():
    with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
        adsl = {row["USUBJID"]: row for row in csv.DictReader(f)}

    with open(f"{INPUT_DIR}/rs.csv", newline="") as f:
        rs_rows = list(csv.DictReader(f))

    records = []
    for row in rs_rows:
        if row["RSTESTCD"] != "OVRLRESP" or row["RSEVAL"] != "INVESTIGATOR":
            continue
        trtsdt = parse_full_date(adsl[row["USUBJID"]]["TRTSDT"])
        adt = complete_date(row["RSDTC"])
        ady = (adt - trtsdt).days + 1
        avalc = row["RSSTRESC"]
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "RSSEQ": row["RSSEQ"],
                "PARAMCD": "OVR",
                "PARAM": "Overall Response by Investigator",
                "RSDTC": row["RSDTC"],
                "ADT": adt.isoformat(),
                "ADY": ady,
                "AVALC": avalc,
                "AVAL": AVAL_MAP[avalc],
                "ANL01FL": "",
            }
        )

    # ANL01FL: one record per subject per assessment date (ADT) -- the
    # record carrying the worst response that day (largest AVAL), with
    # the lowest RSSEQ breaking ties.
    groups = {}
    for rec in records:
        key = (rec["USUBJID"], rec["ADT"])
        groups.setdefault(key, []).append(rec)
    for group in groups.values():
        flagged = min(group, key=lambda r: (-r["AVAL"], int(r["RSSEQ"])))
        flagged["ANL01FL"] = "Y"

    fieldnames = [
        "STUDYID", "USUBJID", "RSSEQ", "PARAMCD", "PARAM", "RSDTC",
        "ADT", "ADY", "AVALC", "AVAL", "ANL01FL",
    ]
    with open(f"{OUTPUT_DIR}/adrs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
