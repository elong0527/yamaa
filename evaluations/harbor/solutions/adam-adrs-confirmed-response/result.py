# Reference solution for the yamaa benchmark adam-adrs-confirmed-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(text):
    if text is None or text.strip() == "":
        return None
    return date.fromisoformat(text.strip())


def main():
    with open(INPUT_DIR + "/rs.csv", newline="") as fh:
        rows = list(csv.DictReader(fh))

    records = []
    for row in rows:
        records.append(
            {
                "studyid": row["STUDYID"],
                "usubjid": row["USUBJID"],
                "rsseq": int(row["RSSEQ"]),
                "adt": row["RSDTC"].strip() if row["RSDTC"] else "",
                "date": parse_date(row["RSDTC"]),
                "avalc": row["RSSTRESC"].strip() if row["RSSTRESC"] else "",
            }
        )

    by_subject = {}
    for rec in records:
        by_subject.setdefault(rec["usubjid"], []).append(rec)

    for subject_records in by_subject.values():
        subject_records.sort(key=lambda r: (r["date"], r["rsseq"]))
        for i, rec in enumerate(subject_records):
            nxt = subject_records[i + 1] if i + 1 < len(subject_records) else None
            if rec["avalc"] == "PD":
                rec["confirmed"] = "Y"
            elif rec["avalc"] in ("PR", "CR"):
                if (
                    nxt is not None
                    and nxt["avalc"] in ("PR", "CR")
                    and (nxt["date"] - rec["date"]).days >= 28
                ):
                    rec["confirmed"] = "Y"
                else:
                    rec["confirmed"] = "N"
            else:
                rec["confirmed"] = "N"

    out_rows = []
    for rec in records:
        out_rows.append(
            {
                "STUDYID": rec["studyid"],
                "USUBJID": rec["usubjid"],
                "PARAMCD": "CONFRESP",
                "RSSEQ": rec["rsseq"],
                "ADT": rec["adt"],
                "AVALC": rec["avalc"],
                "CONFIRMED": rec["confirmed"],
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "PARAMCD", "RSSEQ", "ADT", "AVALC", "CONFIRMED"]
    with open(OUTPUT_DIR + "/adrs.csv", "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
