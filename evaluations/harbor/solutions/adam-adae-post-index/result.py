# Reference solution for the yamaa benchmark adam-adae-post-index (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE from AE: flag records strictly after each subject's
index event (first COVID-19 record by ASTDT, AESEQ breaking ties)."""

import csv
from datetime import date

INPUT_AE = "/app/input/ae.csv"
OUTPUT_ADAE = "/app/output/adae.csv"

COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "ASTDT", "AFTIDXFL"]


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def main():
    with open(INPUT_AE, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    for row in rows:
        row["_astdt"] = parse_date(row["AESTDTC"])
        row["_aeseq"] = int(row["AESEQ"])

    # Index event per subject: first COVID-19 record ordered by ASTDT,
    # AESEQ breaking ties; records with no start date order last.
    index = {}
    for row in rows:
        if row["AEDECOD"] != "COVID-19":
            continue
        key = (
            row["_astdt"] is None,
            row["_astdt"] or date.min,
            row["_aeseq"],
        )
        subject = row["USUBJID"]
        if subject not in index or key < index[subject][0]:
            index[subject] = (key, row)

    out = []
    for row in rows:
        flag = ""
        subject = row["USUBJID"]
        if subject in index and row["_astdt"] is not None:
            _, idx = index[subject]
            if row["_astdt"] > idx["_astdt"] or (
                row["_astdt"] == idx["_astdt"]
                and row["_aeseq"] > idx["_aeseq"]
            ):
                flag = "Y"
        out.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "AEDECOD": row["AEDECOD"],
                "ASTDT": row["_astdt"].isoformat() if row["_astdt"] else "",
                "AFTIDXFL": flag,
            }
        )

    with open(OUTPUT_ADAE, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
