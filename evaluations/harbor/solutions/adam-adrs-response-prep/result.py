# Reference solution for the yamaa benchmark adam-adrs-response-prep (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADRS from ADRS_RAW and ADSL.

One output record per collected overall response assessment, with the
best-overall-response category (BORCAT), its priority (BORPRI), and the
per study-subject sequence (BORSEQ) of records that support a category.
"""

import csv
from datetime import date
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")

# Collected response -> supported category, before the day-42 and
# new-therapy rules are applied.
CATEGORY_BY_RESPONSE = {
    "CR": "CR",
    "PR": "PR",
    "SD": "SD",
    "NON-CR/NON-PD": "NON-CR/NON-PD",
    "PD": "PD",
    "NE": "NE",
}

PRIORITY_BY_CATEGORY = {
    "CR": 1,
    "PR": 2,
    "SD": 3,
    "NON-CR/NON-PD": 4,
    "PD": 5,
    "NE": 6,
}

# Categories that only count on or after day 42 after randomization.
DAY_LIMITED = {"SD", "NON-CR/NON-PD"}
MIN_DAY = 42


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def parse_int(text):
    text = (text or "").strip()
    if not text:
        return None
    return int(text)


def main():
    with open(INPUT_DIR / "adsl.csv", newline="", encoding="utf-8") as f:
        adsl_rows = list(csv.DictReader(f))
    new_therapy_start = {}
    for row in adsl_rows:
        key = (row["STUDYID"], row["USUBJID"])
        new_therapy_start[key] = parse_date(row.get("NTXSTDT"))

    with open(INPUT_DIR / "adrs_raw.csv", newline="", encoding="utf-8") as f:
        raw_rows = list(csv.DictReader(f))

    records = []
    for row in raw_rows:
        studyid = row["STUDYID"]
        usubjid = row["USUBJID"]
        adt = parse_date(row.get("ADT"))
        randdy = parse_int(row.get("RANDDY"))
        avalc = (row.get("AVALC") or "").strip()

        borcat = CATEGORY_BY_RESPONSE.get(avalc)
        if borcat is not None:
            start = new_therapy_start.get((studyid, usubjid))
            if start is not None and adt is not None and adt >= start:
                # On or after new anti-cancer therapy: supports nothing.
                borcat = None
            elif borcat in DAY_LIMITED and (randdy is None or randdy < MIN_DAY):
                borcat = "NE"

        borpri = PRIORITY_BY_CATEGORY[borcat] if borcat is not None else None
        records.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "ASEQ": row["ASEQ"],
                "ADT": (row.get("ADT") or "").strip(),
                "RANDDY": (row.get("RANDDY") or "").strip(),
                "AVALC": (row.get("AVALC") or "").strip(),
                "BORCAT": borcat or "",
                "BORPRI": borpri,
                "_adt_sort": adt,
                "_aseq_sort": int(row["ASEQ"]),
                "BORSEQ": None,
            }
        )

    groups = {}
    for rec in records:
        if rec["BORPRI"] is not None:
            groups.setdefault((rec["STUDYID"], rec["USUBJID"]), []).append(rec)
    for group in groups.values():
        ordered = sorted(
            group,
            key=lambda r: (
                r["BORPRI"],
                r["_adt_sort"] is None,
                r["_adt_sort"] or date.min,
                r["_aseq_sort"],
            ),
        )
        for number, rec in enumerate(ordered, start=1):
            rec["BORSEQ"] = number

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = [
        "STUDYID",
        "USUBJID",
        "ASEQ",
        "ADT",
        "RANDDY",
        "AVALC",
        "BORCAT",
        "BORPRI",
        "BORSEQ",
    ]
    with open(OUTPUT_DIR / "adrs.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for rec in records:
            writer.writerow(
                {
                    "STUDYID": rec["STUDYID"],
                    "USUBJID": rec["USUBJID"],
                    "ASEQ": rec["ASEQ"],
                    "ADT": rec["ADT"],
                    "RANDDY": rec["RANDDY"],
                    "AVALC": rec["AVALC"],
                    "BORCAT": rec["BORCAT"],
                    "BORPRI": "" if rec["BORPRI"] is None else rec["BORPRI"],
                    "BORSEQ": "" if rec["BORSEQ"] is None else rec["BORSEQ"],
                }
            )


if __name__ == "__main__":
    main()
